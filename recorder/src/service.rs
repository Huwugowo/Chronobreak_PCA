use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chronobreak_replay_time::{
    CaptureAnchorsV2, FinalizationExpectationsV2, MediaId, ProducerEvidenceV2,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant as TokioInstant;
use tracing::{error, info, warn};

use crate::config::{CodecPreference, Config, RecordingConfig, RecordingProfile};
#[cfg(not(target_os = "windows"))]
use crate::encoder::RecordingSession;
use crate::encoder::{
    AudioSource, CAPTURE_DIAGNOSTIC_ABI, EncoderKind, Ffmpeg, RecordingEvidence, RecordingPlan,
    VideoCodec,
};
use crate::finalizer::{CompletedVideoCandidate, publish_validated_candidate, validate_candidate};
#[cfg(target_os = "windows")]
use crate::native::{
    NATIVE_ENCODER_SLOT_COUNT, NativeRecordingSession, NativeRecordingStartupFailureDisposition,
};
#[cfg(not(target_os = "windows"))]
use crate::platform::fallback_capture_target;
use crate::platform::{
    CaptureTarget, CaptureTargetStateCache, CaptureTargetVisibility, capture_target_for_process,
    query_capture_target_state,
};
use crate::poller::{CaptureMetadata, PollerSession, RecordingDetails, RecordingMetadata};
use crate::storage::{create_game_directory, unix_timestamp_now};
use crate::watcher::{
    DEFAULT_PROCESS_NAME, LeagueProcess, POLL_INTERVAL, ProcessTransition, ProcessWatcher,
    ProcessWatcherTelemetry, transition,
};

#[derive(Debug, Clone)]
pub enum ServiceEvent {
    Idle,
    Recording { directory: PathBuf },
    Error { message: String },
    ShutdownComplete,
}

#[derive(Debug, Clone, Copy)]
pub enum ServiceCommand {
    Shutdown,
}

pub type EventSink = Arc<dyn Fn(ServiceEvent) + Send + Sync>;

struct ActiveRecording {
    media_id: MediaId,
    ffprobe_path: PathBuf,
    session: VideoRecordingSession,
    target: CaptureTarget,
    target_state_cache: CaptureTargetStateCache,
    diagnostics: watch::Receiver<RecordingEvidence>,
    progress_watchdog: CaptureProgressWatchdog,
    progress_report_due: Instant,
    poller: PollerSession,
    details: RecordingDetails,
}

struct StartingRecording {
    process: LeagueProcess,
    cancellation: watch::Sender<bool>,
    task: JoinHandle<RecordingStartupResult<ActiveRecording>>,
}

struct RecordingStartup {
    ffmpeg: Ffmpeg,
    recording_config: RecordingConfig,
    hevc_playback_supported: bool,
    audio: AudioSource,
    output_path: PathBuf,
    process: LeagueProcess,
    target: CaptureTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordingStartupDisposition {
    Cancelled,
    Retryable,
    Terminal,
}

#[derive(Debug)]
struct RecordingStartupFailure {
    disposition: RecordingStartupDisposition,
    error: anyhow::Error,
}

impl std::fmt::Display for RecordingStartupFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}

type RecordingStartupResult<T> = std::result::Result<T, RecordingStartupFailure>;

impl RecordingStartupFailure {
    fn cancelled(error: anyhow::Error) -> Self {
        Self {
            disposition: RecordingStartupDisposition::Cancelled,
            error,
        }
    }

    fn retryable(error: anyhow::Error) -> Self {
        Self {
            disposition: RecordingStartupDisposition::Retryable,
            error,
        }
    }

    fn terminal(error: anyhow::Error) -> Self {
        Self {
            disposition: RecordingStartupDisposition::Terminal,
            error,
        }
    }
}

trait ClassifyRecordingStartup<T> {
    fn retryable_startup(self) -> RecordingStartupResult<T>;
    fn terminal_startup(self) -> RecordingStartupResult<T>;
}

impl<T> ClassifyRecordingStartup<T> for Result<T> {
    fn retryable_startup(self) -> RecordingStartupResult<T> {
        self.map_err(RecordingStartupFailure::retryable)
    }

    fn terminal_startup(self) -> RecordingStartupResult<T> {
        self.map_err(RecordingStartupFailure::terminal)
    }
}

const STARTUP_RETRY_DELAYS: [Duration; 4] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

#[derive(Debug, Default)]
struct RecordingStartupRetryState {
    process: Option<LeagueProcess>,
    failures: u32,
    next_attempt_at: Option<TokioInstant>,
    terminal: bool,
    retry_alert_sent: bool,
    terminal_alert_sent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecordingStartupFailureAction {
    notify_user: bool,
    retry_at: Option<TokioInstant>,
}

impl RecordingStartupRetryState {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn permits(&self, process: LeagueProcess, now: TokioInstant) -> bool {
        if self.process != Some(process) {
            return true;
        }
        !self.terminal && self.next_attempt_at.is_none_or(|deadline| now >= deadline)
    }

    fn record_failure(
        &mut self,
        process: LeagueProcess,
        disposition: RecordingStartupDisposition,
        now: TokioInstant,
    ) -> RecordingStartupFailureAction {
        if self.process != Some(process) {
            self.reset();
            self.process = Some(process);
        }
        match disposition {
            RecordingStartupDisposition::Cancelled => {
                self.reset();
                RecordingStartupFailureAction {
                    notify_user: false,
                    retry_at: None,
                }
            }
            RecordingStartupDisposition::Retryable => {
                self.failures = self.failures.saturating_add(1);
                let delay = startup_retry_delay(self.failures);
                let retry_at = now + delay;
                self.next_attempt_at = Some(retry_at);
                let notify_user = !self.retry_alert_sent;
                self.retry_alert_sent = true;
                RecordingStartupFailureAction {
                    notify_user,
                    retry_at: Some(retry_at),
                }
            }
            RecordingStartupDisposition::Terminal => {
                self.terminal = true;
                self.next_attempt_at = None;
                let notify_user = !self.terminal_alert_sent;
                self.terminal_alert_sent = true;
                RecordingStartupFailureAction {
                    notify_user,
                    retry_at: None,
                }
            }
        }
    }

    fn block_active_process(&mut self, process: Option<LeagueProcess>) {
        self.reset();
        self.process = process;
        self.terminal = process.is_some();
    }
}

fn startup_retry_delay(failure_count: u32) -> Duration {
    let index = failure_count.saturating_sub(1) as usize;
    STARTUP_RETRY_DELAYS[index.min(STARTUP_RETRY_DELAYS.len() - 1)]
}

const STARTUP_CANCELLATION_TIMEOUT: Duration = Duration::from_secs(20);
const CAPTURE_PROGRESS_STALL_TIMEOUT: Duration = Duration::from_secs(15);
const CONTROL_TELEMETRY_INTERVAL: Duration = Duration::from_secs(60);
#[cfg(target_os = "windows")]
const WINDOWS_RECORDING_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

enum VideoRecordingSession {
    #[cfg(not(target_os = "windows"))]
    Ffmpeg(Box<RecordingSession>),
    #[cfg(target_os = "windows")]
    Native(NativeRecordingSession),
}

impl VideoRecordingSession {
    fn directory(&self) -> &std::path::Path {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Ffmpeg(session) => session.directory(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.directory(),
        }
    }

    fn video_started_at(&self) -> Instant {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Ffmpeg(session) => session.video_started_at(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.video_started_at(),
        }
    }

    fn recorded_at(&self) -> std::time::SystemTime {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Ffmpeg(session) => session.recorded_at(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.recorded_at(),
        }
    }

    fn evidence_receiver(&self) -> watch::Receiver<RecordingEvidence> {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Ffmpeg(session) => session.evidence_receiver(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.evidence_receiver(),
        }
    }

    fn has_exited(&mut self) -> Result<bool> {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Ffmpeg(session) => session.has_exited(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => Ok(session.has_exited()),
        }
    }

    async fn stop(self) -> Result<CompletedVideoCandidate> {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Ffmpeg(session) => (*session).stop().await,
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.stop().await,
        }
    }

    async fn stop_with_failure(self, reason: String) -> Result<CompletedVideoCandidate> {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Ffmpeg(session) => (*session).stop_with_failure(reason).await,
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.stop_with_failure(reason).await,
        }
    }
}

#[derive(Debug, Default)]
struct RecorderControlTelemetry {
    startup_attempts: u64,
    startup_ready: u64,
    startup_errors: u64,
    startup_task_failures: u64,
    startup_retries_scheduled: u64,
    startup_retry_alerts_suppressed: u64,
    startup_terminal_failures: u64,
    target_state_queries: u64,
    target_state_successes: u64,
    target_adapter_validations: u64,
    visible_target_checks: u64,
    paused_target_checks: u64,
    target_failures: u64,
    watchdog_stalls: u64,
    backend_exits: u64,
}

struct CaptureProgressWatchdog {
    state: CaptureProgressState,
    source_qpc: i64,
    source_advanced_at: Instant,
    encoded_frames: u64,
    encoded_advanced_at: Instant,
    muxed_bytes: u64,
    muxed_advanced_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureProgressState {
    Monitoring,
    PausedByWindowVisibility,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureProgressObservation {
    Healthy,
    PausedByWindowVisibility,
    ResumedAfterWindowVisibility,
    Stalled(String),
}

impl CaptureProgressWatchdog {
    fn new(evidence: &RecordingEvidence, now: Instant) -> Self {
        Self {
            state: CaptureProgressState::Monitoring,
            source_qpc: evidence.latest_qpc.unwrap_or_default(),
            source_advanced_at: now,
            encoded_frames: evidence.encoded_frames,
            encoded_advanced_at: now,
            muxed_bytes: evidence.muxed_bytes,
            muxed_advanced_at: now,
        }
    }

    fn observe(
        &mut self,
        evidence: &RecordingEvidence,
        now: Instant,
        visibility: CaptureTargetVisibility,
    ) -> CaptureProgressObservation {
        if let Some(error) = evidence.protocol_error.as_deref() {
            return CaptureProgressObservation::Stalled(format!(
                "capture diagnostics failed: {error}"
            ));
        }

        self.observe_progress(evidence, now);
        match (self.state, visibility) {
            (_, CaptureTargetVisibility::PausedByWindowVisibility) => {
                self.reset_stall_deadlines(now);
                if self.state == CaptureProgressState::Monitoring {
                    self.state = CaptureProgressState::PausedByWindowVisibility;
                    return CaptureProgressObservation::PausedByWindowVisibility;
                }
                return CaptureProgressObservation::Healthy;
            }
            (CaptureProgressState::PausedByWindowVisibility, CaptureTargetVisibility::Visible) => {
                self.state = CaptureProgressState::Monitoring;
                self.reset_stall_deadlines(now);
                return CaptureProgressObservation::ResumedAfterWindowVisibility;
            }
            (CaptureProgressState::Monitoring, CaptureTargetVisibility::Visible) => {}
        }

        for (label, last_advanced) in [
            ("WGC source timestamp", self.source_advanced_at),
            ("encoded frame count", self.encoded_advanced_at),
            ("muxed byte count", self.muxed_advanced_at),
        ] {
            if now.saturating_duration_since(last_advanced) >= CAPTURE_PROGRESS_STALL_TIMEOUT {
                return CaptureProgressObservation::Stalled(format!(
                    "{label} did not advance for {} seconds",
                    CAPTURE_PROGRESS_STALL_TIMEOUT.as_secs()
                ));
            }
        }
        CaptureProgressObservation::Healthy
    }

    fn observe_progress(&mut self, evidence: &RecordingEvidence, now: Instant) {
        let source_qpc = evidence.latest_qpc.unwrap_or_default();
        if source_qpc > self.source_qpc {
            self.source_qpc = source_qpc;
            self.source_advanced_at = now;
        }
        if evidence.encoded_frames > self.encoded_frames {
            self.encoded_frames = evidence.encoded_frames;
            self.encoded_advanced_at = now;
        }
        if evidence.muxed_bytes > self.muxed_bytes {
            self.muxed_bytes = evidence.muxed_bytes;
            self.muxed_advanced_at = now;
        }
    }

    fn reset_stall_deadlines(&mut self, now: Instant) {
        self.source_advanced_at = now;
        self.encoded_advanced_at = now;
        self.muxed_advanced_at = now;
    }
}

pub async fn run(
    config: Config,
    mut commands: mpsc::UnboundedReceiver<ServiceCommand>,
    events: EventSink,
) -> Result<()> {
    let output_path = config.output_path()?;
    fs::create_dir_all(output_path.join("games")).with_context(|| {
        format!(
            "failed to create recorder output at {}",
            output_path.display()
        )
    })?;

    #[cfg(target_os = "windows")]
    let _ = native_recording_plan(&config.recording)
        .context("unsupported Windows recorder configuration")?;
    let ffmpeg = Ffmpeg::resolve().await?;
    let audio = ffmpeg.detect_audio_source().await;
    #[cfg(target_os = "windows")]
    info!(
        backend = "native-wgc-d3d11-nvenc",
        "selected sole Windows recorder backend"
    );
    info!(
        ffmpeg = %ffmpeg.path().display(),
        media_runtime = ffmpeg.runtime_id(),
        audio = %audio.description(),
        output = %output_path.display(),
        "recorder initialized; capture plan awaits the exact League HWND and adapter"
    );

    events(ServiceEvent::Idle);

    let process_name =
        env::var("LEAGUE_REPLAY_PROCESS_NAME").unwrap_or_else(|_| DEFAULT_PROCESS_NAME.to_owned());
    let mut watcher = ProcessWatcher::new(&process_name);
    let mut previous_process: Option<LeagueProcess> = None;
    let mut active: Option<ActiveRecording> = None;
    let mut starting: Option<StartingRecording> = None;
    let mut startup_retry = RecordingStartupRetryState::default();
    let mut control_telemetry = RecorderControlTelemetry::default();
    let mut control_report_due = Instant::now() + CONTROL_TELEMETRY_INTERVAL;
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            command = commands.recv() => {
                if matches!(command, Some(ServiceCommand::Shutdown) | None) {
                    cancel_starting(starting.take(), &events).await;
                    if let Some(recording) = active.take() {
                        stop_recording(recording, &events).await;
                    }
                    log_control_telemetry(watcher.telemetry(), &control_telemetry, true);
                    events(ServiceEvent::ShutdownComplete);
                    return Ok(());
                }
            }
            _ = interval.tick() => {
                let current_process = watcher.refresh();
                match transition(previous_process, current_process) {
                    ProcessTransition::Appeared(process) => {
                        startup_retry.reset();
                        info!(pid = process.pid, "League process appeared; waiting for its capture window");
                    }
                    ProcessTransition::Replaced(process) => {
                        warn!(pid = process.pid, "League process was replaced between watcher polls");
                        cancel_starting(starting.take(), &events).await;
                        if let Some(recording) = active.take() {
                            stop_recording(recording, &events).await;
                        }
                        startup_retry.reset();
                    }
                    ProcessTransition::Disappeared => {
                        cancel_starting(starting.take(), &events).await;
                        if let Some(recording) = active.take() {
                            stop_recording(recording, &events).await;
                            events(ServiceEvent::Idle);
                        }
                        startup_retry.reset();
                    }
                    ProcessTransition::Unchanged => {}
                }

                if starting
                    .as_ref()
                    .is_some_and(|attempt| attempt.task.is_finished())
                {
                    let attempt = starting.take().expect("finished startup attempt exists");
                    let process = attempt.process;
                    match attempt.task.await {
                        Ok(Ok(recording)) if current_process == Some(process) => {
                            control_telemetry.startup_ready =
                                control_telemetry.startup_ready.saturating_add(1);
                            startup_retry.reset();
                            events(ServiceEvent::Recording {
                                directory: recording.session.directory().to_path_buf(),
                            });
                            active = Some(recording);
                        }
                        Ok(Ok(recording)) => {
                            warn!(pid = process.pid, "recording became ready after its League process changed");
                            stop_recording(recording, &events).await;
                        }
                        Ok(Err(start_error)) => {
                            control_telemetry.startup_errors =
                                control_telemetry.startup_errors.saturating_add(1);
                            let now = TokioInstant::now();
                            let action = startup_retry.record_failure(
                                process,
                                start_error.disposition,
                                now,
                            );
                            match start_error.disposition {
                                RecordingStartupDisposition::Cancelled => {
                                    info!(pid = process.pid, "recording startup was cancelled after cleanup");
                                }
                                RecordingStartupDisposition::Retryable => {
                                    control_telemetry.startup_retries_scheduled = control_telemetry
                                        .startup_retries_scheduled
                                        .saturating_add(1);
                                    let retry_in_ms = action.retry_at.map_or(0, |retry_at| {
                                        retry_at.saturating_duration_since(now).as_millis()
                                    });
                                    warn!(
                                        pid = process.pid,
                                        retry_in_ms,
                                        error = %start_error.error,
                                        "recording startup failed after cleanup; retry scheduled"
                                    );
                                }
                                RecordingStartupDisposition::Terminal => {
                                    control_telemetry.startup_terminal_failures = control_telemetry
                                        .startup_terminal_failures
                                        .saturating_add(1);
                                    error!(
                                        pid = process.pid,
                                        error = %start_error.error,
                                        "recording startup failed terminally for this process"
                                    );
                                }
                            }
                            if action.notify_user {
                                let message = match start_error.disposition {
                                    RecordingStartupDisposition::Cancelled => {
                                        "Recording startup was cancelled".to_owned()
                                    }
                                    RecordingStartupDisposition::Retryable => format!(
                                        "Recording temporarily failed to start; retrying automatically: {:#}",
                                        start_error.error
                                    ),
                                    RecordingStartupDisposition::Terminal => format!(
                                        "Recording failed to start: {:#}",
                                        start_error.error
                                    ),
                                };
                                events(ServiceEvent::Error {
                                    message,
                                });
                            } else if start_error.disposition == RecordingStartupDisposition::Retryable {
                                control_telemetry.startup_retry_alerts_suppressed = control_telemetry
                                    .startup_retry_alerts_suppressed
                                    .saturating_add(1);
                            }
                        }
                        Err(join_error) => {
                            control_telemetry.startup_task_failures =
                                control_telemetry.startup_task_failures.saturating_add(1);
                            let action = startup_retry.record_failure(
                                process,
                                RecordingStartupDisposition::Terminal,
                                TokioInstant::now(),
                            );
                            control_telemetry.startup_terminal_failures = control_telemetry
                                .startup_terminal_failures
                                .saturating_add(1);
                            error!(error = %join_error, "recording startup task failed");
                            if action.notify_user {
                                events(ServiceEvent::Error {
                                    message: format!("Recording failed to start: {join_error}"),
                                });
                            }
                        }
                    }
                }

                let mut visibility = CaptureTargetVisibility::Visible;
                if let Some(recording) = active.as_mut() {
                    control_telemetry.target_state_queries =
                        control_telemetry.target_state_queries.saturating_add(1);
                    let target_state = query_capture_target_state(
                        &recording.target,
                        &mut recording.target_state_cache,
                    );
                    match target_state {
                        Ok(state) => {
                            control_telemetry.target_state_successes = control_telemetry
                                .target_state_successes
                                .saturating_add(1);
                            if state.adapter_validated {
                                control_telemetry.target_adapter_validations = control_telemetry
                                    .target_adapter_validations
                                    .saturating_add(1);
                            }
                            visibility = state.visibility;
                            match state.visibility {
                                CaptureTargetVisibility::Visible => {
                                    control_telemetry.visible_target_checks = control_telemetry
                                        .visible_target_checks
                                        .saturating_add(1);
                                }
                                CaptureTargetVisibility::PausedByWindowVisibility => {
                                    control_telemetry.paused_target_checks = control_telemetry
                                        .paused_target_checks
                                        .saturating_add(1);
                                }
                            }
                        }
                        Err(target_error) => {
                            control_telemetry.target_failures =
                                control_telemetry.target_failures.saturating_add(1);
                            let recording = active.take().expect("active recording exists");
                            warn!(error = %target_error, "the active League capture target became invalid");
                            stop_recording_with_failure(
                                recording,
                                &events,
                                format!("the exact League capture target became invalid: {target_error:#}"),
                            )
                            .await;
                            startup_retry.block_active_process(current_process);
                        }
                    }
                }

                if let Some(recording) = active.as_mut() {
                    let evidence = recording.diagnostics.borrow().clone();
                    let now = Instant::now();
                    match recording.progress_watchdog.observe(&evidence, now, visibility) {
                        CaptureProgressObservation::Stalled(stall_reason) => {
                            control_telemetry.watchdog_stalls =
                                control_telemetry.watchdog_stalls.saturating_add(1);
                            let recording = active.take().expect("active recording exists");
                            warn!(reason = %stall_reason, "the active GPU capture graph stopped advancing");
                            stop_recording_with_failure(recording, &events, stall_reason).await;
                            startup_retry.block_active_process(current_process);
                        }
                        CaptureProgressObservation::PausedByWindowVisibility => {
                            info!("capture progress watchdog paused while the League window is hidden or minimized");
                        }
                        CaptureProgressObservation::ResumedAfterWindowVisibility => {
                            info!("capture progress watchdog resumed after the League window became visible");
                            recording.progress_report_due = now + Duration::from_secs(10);
                        }
                        CaptureProgressObservation::Healthy if now >= recording.progress_report_due => {
                            log_capture_progress(
                                &evidence,
                                recording.session.video_started_at().elapsed(),
                                false,
                            );
                            recording.progress_report_due = now + Duration::from_secs(10);
                        }
                        CaptureProgressObservation::Healthy => {}
                    }
                }

                if let Some(recording) = active.as_mut() {
                    match recording.session.has_exited() {
                        Ok(true) => {
                            control_telemetry.backend_exits =
                                control_telemetry.backend_exits.saturating_add(1);
                            let recording = active.take().expect("active recording exists");
                            warn!("recorder backend exited while League was still running");
                            events(ServiceEvent::Error {
                                message: "recorder backend exited unexpectedly; preserving the partial recording".to_owned(),
                            });
                            stop_recording(recording, &events).await;
                            startup_retry.block_active_process(current_process);
                        }
                        Ok(false) => {}
                        Err(inspect_error) => {
                            error!(error = %inspect_error, "could not inspect recorder backend");
                        }
                    }
                }

                if let Some(process) = startup_candidate(
                    current_process,
                    active.is_some(),
                    starting.is_some(),
                    &startup_retry,
                    TokioInstant::now(),
                ) {
                    match capture_target_for_process(process.pid) {
                        Ok(target) => {
                            control_telemetry.startup_attempts =
                                control_telemetry.startup_attempts.saturating_add(1);
                            starting = Some(spawn_recording_start(RecordingStartup {
                                ffmpeg: ffmpeg.clone(),
                                recording_config: config.recording.clone(),
                                hevc_playback_supported: config.app.hevc_playback_supported,
                                audio: audio.clone(),
                                output_path: output_path.clone(),
                                process,
                                target,
                            }));
                        }
                        Err(target_error) => {
                            tracing::debug!(
                                pid = process.pid,
                                error = %target_error,
                                "League process has no capturable window yet; retrying on the next watcher tick"
                            );
                        }
                    }
                }

                let now = Instant::now();
                if now >= control_report_due {
                    log_control_telemetry(watcher.telemetry(), &control_telemetry, false);
                    control_report_due = now + CONTROL_TELEMETRY_INTERVAL;
                }

                previous_process = current_process;
            }
        }
    }
}

fn startup_candidate(
    current: Option<LeagueProcess>,
    has_active: bool,
    has_starting: bool,
    retry: &RecordingStartupRetryState,
    now: TokioInstant,
) -> Option<LeagueProcess> {
    current.filter(|process| !has_active && !has_starting && retry.permits(*process, now))
}

fn log_control_telemetry(
    watcher: ProcessWatcherTelemetry,
    telemetry: &RecorderControlTelemetry,
    terminal: bool,
) {
    info!(
        terminal,
        process_refreshes = watcher.refreshes,
        refreshed_process_records = watcher.refreshed_process_records,
        total_process_refresh_100ns = watcher.total_refresh_100ns,
        maximum_process_refresh_100ns = watcher.maximum_refresh_100ns,
        known_processes = watcher.known_processes,
        maximum_known_processes = watcher.maximum_known_processes,
        startup_attempts = telemetry.startup_attempts,
        startup_ready = telemetry.startup_ready,
        startup_errors = telemetry.startup_errors,
        startup_task_failures = telemetry.startup_task_failures,
        startup_retries_scheduled = telemetry.startup_retries_scheduled,
        startup_retry_alerts_suppressed = telemetry.startup_retry_alerts_suppressed,
        startup_terminal_failures = telemetry.startup_terminal_failures,
        target_state_queries = telemetry.target_state_queries,
        target_state_successes = telemetry.target_state_successes,
        target_adapter_validations = telemetry.target_adapter_validations,
        visible_target_checks = telemetry.visible_target_checks,
        paused_target_checks = telemetry.paused_target_checks,
        target_failures = telemetry.target_failures,
        watchdog_stalls = telemetry.watchdog_stalls,
        backend_exits = telemetry.backend_exits,
        "recorder control-plane telemetry"
    );
}

fn spawn_recording_start(startup: RecordingStartup) -> StartingRecording {
    let (cancellation, receiver) = watch::channel(false);
    let process = startup.process;
    let task = tokio::spawn(async move { start_recording(startup, receiver).await });
    StartingRecording {
        process,
        cancellation,
        task,
    }
}

async fn cancel_starting(starting: Option<StartingRecording>, events: &EventSink) {
    cancel_starting_with_timeout(starting, events, STARTUP_CANCELLATION_TIMEOUT).await;
}

async fn cancel_starting_with_timeout(
    starting: Option<StartingRecording>,
    events: &EventSink,
    timeout: Duration,
) {
    let Some(mut starting) = starting else {
        return;
    };
    let _ = starting.cancellation.send(true);
    match tokio::time::timeout(timeout, &mut starting.task).await {
        Ok(Ok(Ok(recording))) => {
            stop_recording(recording, events).await;
        }
        Ok(Ok(Err(_))) | Ok(Err(_)) => {}
        Err(_) => {
            #[cfg(target_os = "windows")]
            {
                warn!(
                    pid = starting.process.pid,
                    "native recording startup cancellation exceeded its deadline; waiting for the offloaded worker join because abort cannot contain a hung driver call"
                );
                if let Ok(Ok(recording)) = starting.task.await {
                    stop_recording(recording, events).await;
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                warn!(
                    pid = starting.process.pid,
                    "recording startup cancellation exceeded its deadline; aborting the task"
                );
                starting.task.abort();
                let _ = starting.task.await;
            }
        }
    }
}

pub async fn diagnose(config: &Config) -> Result<DiagnosticReport> {
    let ffmpeg = Ffmpeg::resolve().await?;
    #[cfg(target_os = "windows")]
    let plan = native_recording_plan(&config.recording)
        .context("unsupported Windows recorder configuration")?;
    #[cfg(not(target_os = "windows"))]
    let plan = {
        let source_dimensions = fallback_capture_target()
            .ok()
            .and_then(|target| target.dimensions());
        ffmpeg
            .select_recording_plan(
                &config.recording,
                config.app.hevc_playback_supported,
                source_dimensions,
            )
            .await?
    };
    let audio = ffmpeg.detect_audio_source().await;
    Ok(DiagnosticReport {
        config_path: crate::config::default_config_path()?,
        output_path: config.output_path()?,
        ffmpeg_path: ffmpeg.path().to_path_buf(),
        media_runtime_id: ffmpeg.runtime_id().to_owned(),
        plan,
        audio,
    })
}

pub struct DiagnosticReport {
    pub config_path: PathBuf,
    pub output_path: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub media_runtime_id: String,
    pub plan: RecordingPlan,
    pub audio: AudioSource,
}

async fn start_recording(
    startup: RecordingStartup,
    cancellation: watch::Receiver<bool>,
) -> RecordingStartupResult<ActiveRecording> {
    #[cfg(not(target_os = "windows"))]
    let mut cancellation = cancellation;
    let RecordingStartup {
        ffmpeg,
        recording_config,
        hevc_playback_supported: _hevc_playback_supported,
        audio,
        output_path,
        process,
        target,
    } = startup;
    let mut target_state_cache = CaptureTargetStateCache::default();
    let initial_target_state = query_capture_target_state(&target, &mut target_state_cache)
        .context("capture target changed before recording startup")
        .retryable_startup()?;
    if initial_target_state.visibility == CaptureTargetVisibility::PausedByWindowVisibility {
        return Err(RecordingStartupFailure::retryable(anyhow::anyhow!(
            "capture target became hidden or minimized before recording startup"
        )));
    }
    #[cfg(not(target_os = "windows"))]
    let plan = tokio::select! {
        result = ffmpeg.select_recording_plan(
            &recording_config,
            _hevc_playback_supported,
            target.dimensions(),
        ) => result.terminal_startup()?,
        _ = startup_cancelled(&mut cancellation) => {
            return Err(RecordingStartupFailure::cancelled(anyhow::anyhow!(
                "recording startup was cancelled"
            )));
        }
    };

    if *cancellation.borrow() {
        return Err(RecordingStartupFailure::cancelled(anyhow::anyhow!(
            "recording startup was cancelled"
        )));
    }
    let timestamp = unix_timestamp_now().terminal_startup()?;
    let directory = create_game_directory(&output_path, timestamp).terminal_startup()?;
    let media_id = MediaId::new_v4();

    #[cfg(target_os = "windows")]
    let (session, plan) = {
        let plan = native_recording_plan(&recording_config).terminal_startup()?;
        let session_result = NativeRecordingSession::start(
            directory.clone(),
            target.clone(),
            ffmpeg.path().to_path_buf(),
            audio.clone(),
            cancellation.clone(),
            WINDOWS_RECORDING_STARTUP_TIMEOUT,
        )
        .await;
        let session = match session_result {
            Ok(session) => session,
            Err(error) => {
                let disposition = match error.disposition() {
                    NativeRecordingStartupFailureDisposition::Cancelled => {
                        RecordingStartupDisposition::Cancelled
                    }
                    NativeRecordingStartupFailureDisposition::Retryable => {
                        RecordingStartupDisposition::Retryable
                    }
                    NativeRecordingStartupFailureDisposition::Terminal => {
                        RecordingStartupDisposition::Terminal
                    }
                };
                return Err(RecordingStartupFailure {
                    disposition,
                    error: error.into_error(),
                });
            }
        };
        (VideoRecordingSession::Native(session), plan)
    };

    #[cfg(not(target_os = "windows"))]
    let (session, plan) = {
        let session_result = ffmpeg
            .start_recording(directory.clone(), &target, plan, &audio)
            .await;
        let session = match session_result {
            Ok(session) => session,
            Err(error) if *cancellation.borrow() => {
                return Err(RecordingStartupFailure::cancelled(error));
            }
            Err(error) => return Err(RecordingStartupFailure::retryable(error)),
        };
        (VideoRecordingSession::Ffmpeg(Box::new(session)), plan)
    };

    if *cancellation.borrow() {
        let _ = session
            .stop_with_failure("recording startup was cancelled".to_owned())
            .await;
        return Err(RecordingStartupFailure::cancelled(anyhow::anyhow!(
            "recording startup was cancelled"
        )));
    }

    let recording_resolution = plan
        .output_dimensions(target.dimensions())
        .map(|(width, height)| format!("{width}x{height}"))
        .unwrap_or_else(|| "source".to_owned());
    let diagnostics = session.evidence_receiver();
    let initial_diagnostics = diagnostics.borrow().clone();

    #[cfg(target_os = "windows")]
    let capture_details_result: Result<_> = (|| {
        let capture_adapter_luid_value = target
            .windows_adapter_luid()
            .context("optimized Windows target has no adapter LUID")?;
        let capture_adapter_luid = format!("{capture_adapter_luid_value:016x}");
        let capture_adapter_name = target
            .windows_adapter_name()
            .unwrap_or("unknown")
            .to_owned();
        let capture_output = target.windows_output_name().unwrap_or("unknown").to_owned();
        let encoder_interop = "d3d11-nvenc-direct".to_owned();
        let output_dimensions = plan
            .output_dimensions(target.dimensions())
            .context("optimized Windows capture has no output dimensions")?;
        let source_dimensions = target
            .dimensions()
            .context("optimized Windows capture has no source dimensions")?;
        let frame_pool_capacity = initial_diagnostics
            .frame_pool_capacity
            .context("optimized capture became ready without a frame-pool capacity")?;
        let output_pool_capacity = initial_diagnostics
            .output_pool_capacity
            .context("optimized capture became ready without an output-pool capacity")?;
        let encoder_depth =
            u32::try_from(NATIVE_ENCODER_SLOT_COUNT).expect("native encoder slot count fits u32");
        let maximum_texture_bytes = maximum_texture_bytes(
            source_dimensions,
            output_dimensions,
            frame_pool_capacity,
            output_pool_capacity,
            0,
            encoder_depth,
        );
        info!(
            frame_pool_capacity,
            output_pool_capacity,
            filter_buffered_frame_limit = 0,
            encoder_depth,
            maximum_texture_bytes,
            "optimized capture resource bounds"
        );
        let gpu_stages = vec![
            "windows_graphics_capture_bgra_d3d11".to_owned(),
            "scale_d3d11_video_processor_nv12".to_owned(),
            encoder_interop.clone(),
        ];
        let capture = CaptureMetadata {
            schema_version: 1,
            backend: "native_windows_graphics_capture_d3d11".to_owned(),
            diagnostics_abi: CAPTURE_DIAGNOSTIC_ABI,
            support_label: "optimized-unvalidated".to_owned(),
            capture_adapter_luid: capture_adapter_luid.clone(),
            encoder_adapter_luid: capture_adapter_luid.clone(),
            capture_adapter_name: capture_adapter_name.clone(),
            capture_output: capture_output.clone(),
            encoder_backend: plan.encoder().label().to_owned(),
            encoder_interop: encoder_interop.clone(),
            media_runtime_id: ffmpeg.runtime_id().to_owned(),
            source_format: "d3d11_bgra".to_owned(),
            converted_format: "d3d11_nv12".to_owned(),
            host_readback: false,
            gpu_stages,
            frame_pool_capacity,
            capture_output_pool_capacity: output_pool_capacity,
            filter_buffered_frame_limit: 0,
            encoder_depth,
            progress_stall_timeout_seconds: u32::try_from(CAPTURE_PROGRESS_STALL_TIMEOUT.as_secs())
                .unwrap_or(u32::MAX),
            maximum_texture_bytes,
            source_frames_surfaced: initial_diagnostics.source_frames_surfaced,
            source_frames_superseded: initial_diagnostics.source_frames_superseded,
            encoded_frames: initial_diagnostics.encoded_frames,
            muxed_bytes: initial_diagnostics.muxed_bytes,
            cfr_duplicates: initial_diagnostics.cfr_duplicates,
            cfr_discards: initial_diagnostics.cfr_discards,
            pool_recreations: initial_diagnostics.pool_recreations,
            first_qpc_100ns: initial_diagnostics.first_qpc.unwrap_or_default(),
            latest_qpc_100ns: initial_diagnostics.latest_qpc.unwrap_or_default(),
            terminal_progress: false,
        };
        Ok((
            capture.backend.clone(),
            Some(capture_adapter_luid),
            Some(capture_adapter_name),
            Some(capture_output),
            Some(encoder_interop),
            capture.support_label.clone(),
            Some(capture),
        ))
    })();
    #[cfg(target_os = "windows")]
    let capture_details = match capture_details_result {
        Ok(details) => details,
        Err(error) => {
            let cleanup = session
                .stop_with_failure(format!(
                    "recording startup invariant failed before publication: {error:#}"
                ))
                .await
                .err();
            let error = match cleanup {
                Some(cleanup_error) => {
                    error.context(format!("video cleanup also failed: {cleanup_error:#}"))
                }
                None => error,
            };
            return Err(RecordingStartupFailure::terminal(error));
        }
    };
    #[cfg(not(target_os = "windows"))]
    let capture_details = (
        "avfoundation".to_owned(),
        None,
        None,
        None,
        None,
        "existing-platform-path".to_owned(),
        None,
    );

    let poller = match PollerSession::start(
        &directory,
        media_id.clone(),
        session.video_started_at(),
    )
    .await
    {
        Ok(poller) => poller,
        Err(error) => {
            let cleanup = session
                .stop_with_failure(format!("Live Client poller startup failed: {error:#}"))
                .await
                .err();
            let error = error.context("failed to start the Live Client poller");
            let error = match cleanup {
                Some(cleanup_error) => {
                    error.context(format!("video cleanup also failed: {cleanup_error:#}"))
                }
                None => error,
            };
            return Err(RecordingStartupFailure::retryable(error));
        }
    };
    if *cancellation.borrow() {
        let (_, video_cleanup) = tokio::join!(
            poller.stop(),
            session.stop_with_failure("recording startup was cancelled".to_owned())
        );
        let error = video_cleanup.err().map_or_else(
            || anyhow::anyhow!("recording startup was cancelled"),
            |cleanup_error| {
                anyhow::anyhow!(
                    "recording startup was cancelled; video cleanup also failed: {cleanup_error:#}"
                )
            },
        );
        return Err(RecordingStartupFailure::cancelled(error));
    }

    #[cfg(target_os = "windows")]
    info!(
        pid = process.pid,
        directory = %directory.display(),
        capture_adapter_luid = format_args!(
            "{:016x}",
            target.windows_adapter_luid().unwrap_or_default()
        ),
        encoder_interop = "d3d11-nvenc-direct",
        backend = "native_windows_graphics_capture_d3d11",
        "recording started"
    );
    #[cfg(not(target_os = "windows"))]
    info!(
        pid = process.pid,
        directory = %directory.display(),
        "recording started"
    );
    #[cfg(target_os = "windows")]
    log_capture_progress(
        &initial_diagnostics,
        session.video_started_at().elapsed(),
        false,
    );
    let now = Instant::now();
    Ok(ActiveRecording {
        media_id,
        ffprobe_path: ffmpeg.ffprobe_path().to_path_buf(),
        session,
        target,
        target_state_cache,
        diagnostics,
        progress_watchdog: CaptureProgressWatchdog::new(&initial_diagnostics, now),
        progress_report_due: now + Duration::from_secs(10),
        poller,
        details: RecordingDetails {
            encoder_used: plan.encoder().label().to_owned(),
            codec: plan.codec().label().to_owned(),
            profile: plan.profile().label().to_owned(),
            resolution: recording_resolution,
            fps: plan.fps(),
            capture_backend: capture_details.0,
            capture_adapter_luid: capture_details.1,
            capture_adapter_name: capture_details.2,
            capture_output: capture_details.3,
            encoder_interop: capture_details.4,
            media_runtime_id: ffmpeg.runtime_id().to_owned(),
            capture_support_label: capture_details.5,
            source_frames_surfaced: 0,
            source_frames_superseded: 0,
            cfr_duplicates: 0,
            cfr_discards: 0,
            pool_recreations: 0,
            capture: capture_details.6,
        },
    })
}

#[cfg(target_os = "windows")]
fn native_recording_plan(recording: &RecordingConfig) -> Result<RecordingPlan> {
    if recording.codec == CodecPreference::Hevc {
        bail!("the native recorder currently supports H.264 only");
    }
    if !matches!(
        recording.profile,
        RecordingProfile::Auto | RecordingProfile::High
    ) {
        bail!("the native recorder currently requires the auto or high 1080p60 profile");
    }
    RecordingPlan::new(EncoderKind::Nvenc, VideoCodec::H264, RecordingProfile::High)
        .context("could not select the fixed native 1080p60 H.264 plan")
}

#[cfg(not(target_os = "windows"))]
async fn startup_cancelled(cancellation: &mut watch::Receiver<bool>) {
    loop {
        if *cancellation.borrow() || cancellation.changed().await.is_err() {
            return;
        }
    }
}

async fn stop_recording(recording: ActiveRecording, events: &EventSink) {
    stop_recording_inner(recording, events, None).await;
}

async fn stop_recording_with_failure(
    recording: ActiveRecording,
    events: &EventSink,
    failure_reason: String,
) {
    stop_recording_inner(recording, events, Some(failure_reason)).await;
}

async fn stop_recording_inner(
    recording: ActiveRecording,
    events: &EventSink,
    failure_reason: Option<String>,
) {
    let ActiveRecording {
        media_id,
        ffprobe_path,
        session,
        target: _,
        target_state_cache: _,
        diagnostics,
        progress_watchdog: _,
        progress_report_due: _,
        poller,
        mut details,
    } = recording;
    let recorded_at = session.recorded_at();
    let duration = session.video_started_at().elapsed();
    let video_stop = async {
        match failure_reason {
            Some(reason) => session.stop_with_failure(reason).await,
            None => session.stop().await,
        }
    };
    let (summary, video_result) = tokio::join!(poller.stop(), video_stop);

    let final_diagnostics = diagnostics.borrow().clone();
    log_capture_progress(
        &final_diagnostics,
        duration,
        final_diagnostics.capture_terminal && final_diagnostics.progress_end,
    );
    details.source_frames_surfaced = final_diagnostics.source_frames_surfaced;
    details.source_frames_superseded = final_diagnostics.source_frames_superseded;
    details.cfr_duplicates = final_diagnostics.cfr_duplicates;
    details.cfr_discards = final_diagnostics.cfr_discards;
    details.pool_recreations = final_diagnostics.pool_recreations;
    if let Some(capture) = details.capture.as_mut() {
        capture.source_frames_surfaced = final_diagnostics.source_frames_surfaced;
        capture.source_frames_superseded = final_diagnostics.source_frames_superseded;
        capture.encoded_frames = final_diagnostics.encoded_frames;
        capture.muxed_bytes = final_diagnostics.muxed_bytes;
        capture.cfr_duplicates = final_diagnostics.cfr_duplicates;
        capture.cfr_discards = final_diagnostics.cfr_discards;
        capture.pool_recreations = final_diagnostics.pool_recreations;
        capture.first_qpc_100ns = final_diagnostics.first_qpc.unwrap_or_default();
        capture.latest_qpc_100ns = final_diagnostics.latest_qpc.unwrap_or_default();
        capture.terminal_progress =
            final_diagnostics.capture_terminal && final_diagnostics.progress_end;
    }

    match video_result {
        Ok(candidate) => {
            let published = async {
                let recorded_at_label = OffsetDateTime::from(recorded_at)
                    .format(&Rfc3339)
                    .context("failed to format recording start time")?;
                let first_qpc_100ns = final_diagnostics
                    .first_qpc
                    .context("capture completed without a first QPC anchor")?;
                let expectations = FinalizationExpectationsV2 {
                    media_id: media_id.clone(),
                    expected_video_codec: details.codec.clone(),
                    expected_audio_codec: Some("aac".to_owned()),
                    producer: ProducerEvidenceV2 {
                        backend: details.capture_backend.clone(),
                        expected_frame_rate: candidate.expected_frame_rate(),
                        expected_frame_count: candidate.expected_frame_count(),
                        media_runtime_id: details.media_runtime_id.clone(),
                    },
                    capture: Some(CaptureAnchorsV2 {
                        first_qpc_100ns,
                        recorded_at: recorded_at_label.clone(),
                    }),
                    require_zero_video_start: true,
                };
                let validated = validate_candidate(&ffprobe_path, candidate, expectations).await?;
                let metadata = RecordingMetadata::new(
                    recorded_at_label,
                    validated.timeline().clone(),
                    summary,
                    details,
                );
                publish_validated_candidate(validated, &media_id, &metadata).await
            }
            .await;
            match published {
                Ok(directory) => {
                    info!(directory = %directory.display(), "validated recording published")
                }
                Err(error) => {
                    error!(error = %error, "recording finalization/publication failed");
                    events(ServiceEvent::Error {
                        message: format!(
                            "Recording media could not prove the canonical replay-time contract; no canonical video or metadata was published. Any recoverable output remains under an explicit .partial.mp4 name: {error:#}"
                        ),
                    });
                }
            }
        }
        Err(stop_error) => {
            error!(error = %stop_error, "recording stopped with an error");
            events(ServiceEvent::Error {
                message: format!(
                    "Recording stopped unexpectedly; no canonical video was published. Any recoverable output remains under an explicit .partial.mp4 name: {stop_error:#}"
                ),
            });
        }
    }
}

fn log_capture_progress(evidence: &RecordingEvidence, elapsed: Duration, terminal: bool) {
    info!(
        "queueback_capture_progress elapsed_ms={} source_frames_surfaced={} source_frames_superseded={} encoded_frames={} muxed_bytes={} latest_qpc_100ns={} cfr_duplicates={} cfr_discards={} pool_recreations={} terminal={}",
        elapsed.as_millis(),
        evidence.source_frames_surfaced,
        evidence.source_frames_superseded,
        evidence.encoded_frames,
        evidence.muxed_bytes,
        evidence.latest_qpc.unwrap_or_default(),
        evidence.cfr_duplicates,
        evidence.cfr_discards,
        evidence.pool_recreations,
        terminal,
    );
}

fn maximum_texture_bytes(
    source: (u32, u32),
    output: (u32, u32),
    frame_pool_capacity: u32,
    capture_output_pool_capacity: u32,
    filter_buffered_frame_limit: u32,
    encoder_depth: u32,
) -> u64 {
    let source_pixels = u64::from(source.0).saturating_mul(u64::from(source.1));
    let output_pixels = u64::from(output.0).saturating_mul(u64::from(output.1));
    let bgra_textures =
        u64::from(frame_pool_capacity).saturating_add(u64::from(capture_output_pool_capacity));
    let nv12_textures =
        u64::from(filter_buffered_frame_limit).saturating_add(u64::from(encoder_depth));
    source_pixels
        .saturating_mul(4)
        .saturating_mul(bgra_textures)
        .saturating_add(
            output_pixels
                .saturating_mul(3)
                .saturating_div(2)
                .saturating_mul(nv12_textures),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn native_plan_is_fixed_to_high_h264() {
        let mut recording = RecordingConfig::default();
        let plan = native_recording_plan(&recording).unwrap();
        assert_eq!(plan.encoder(), EncoderKind::Nvenc);
        assert_eq!(plan.codec(), VideoCodec::H264);
        assert_eq!(plan.profile(), RecordingProfile::High);

        recording.codec = CodecPreference::Hevc;
        assert!(native_recording_plan(&recording).is_err());
        recording.codec = CodecPreference::H264;
        recording.profile = RecordingProfile::Low;
        assert!(native_recording_plan(&recording).is_err());
    }

    #[test]
    fn a_process_without_a_window_remains_eligible_on_the_next_tick() {
        let process = LeagueProcess { pid: 42 };
        let retry = RecordingStartupRetryState::default();
        let now = TokioInstant::now();
        assert_eq!(
            startup_candidate(Some(process), false, false, &retry, now),
            Some(process)
        );
        assert_eq!(
            startup_candidate(Some(process), false, false, &retry, now),
            Some(process)
        );
        assert_eq!(
            startup_candidate(Some(process), true, false, &retry, now),
            None
        );
        assert_eq!(
            startup_candidate(Some(process), false, true, &retry, now),
            None
        );
    }

    #[tokio::test(start_paused = true)]
    async fn retryable_startup_failures_follow_the_exact_bounded_schedule() {
        let process = LeagueProcess { pid: 42 };
        let mut retry = RecordingStartupRetryState::default();

        for (index, expected_delay) in [2_u64, 5, 10, 30, 30].into_iter().enumerate() {
            let now = TokioInstant::now();
            let action = retry.record_failure(process, RecordingStartupDisposition::Retryable, now);
            let retry_at = action.retry_at.expect("retryable failure has a deadline");
            assert_eq!(
                retry_at.duration_since(now),
                Duration::from_secs(expected_delay)
            );
            assert_eq!(action.notify_user, index == 0);
            assert!(!retry.permits(process, retry_at - Duration::from_nanos(1)));
            assert!(retry.permits(process, retry_at));
            assert_eq!(
                startup_candidate(Some(process), false, false, &retry, retry_at),
                Some(process)
            );
            tokio::time::advance(Duration::from_secs(expected_delay)).await;
            assert_eq!(TokioInstant::now(), retry_at);
        }
    }

    #[test]
    fn terminal_startup_failure_blocks_only_the_same_process() {
        let process = LeagueProcess { pid: 42 };
        let replacement = LeagueProcess { pid: 43 };
        let now = TokioInstant::now();
        let mut retry = RecordingStartupRetryState::default();

        let action = retry.record_failure(process, RecordingStartupDisposition::Terminal, now);

        assert!(action.notify_user);
        assert_eq!(action.retry_at, None);
        assert!(!retry.permits(process, now + Duration::from_secs(300)));
        let repeated = retry.record_failure(
            process,
            RecordingStartupDisposition::Terminal,
            now + Duration::from_secs(1),
        );
        assert!(!repeated.notify_user);
        assert!(retry.permits(replacement, now));
        retry.reset();
        assert!(retry.permits(process, now));
    }

    #[test]
    fn cancellation_resets_pending_backoff_without_alerting() {
        let process = LeagueProcess { pid: 42 };
        let now = TokioInstant::now();
        let mut retry = RecordingStartupRetryState::default();
        retry.record_failure(process, RecordingStartupDisposition::Retryable, now);

        let action = retry.record_failure(
            process,
            RecordingStartupDisposition::Cancelled,
            now + Duration::from_secs(1),
        );

        assert_eq!(action.retry_at, None);
        assert!(!action.notify_user);
        assert!(retry.permits(process, now + Duration::from_secs(1)));
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn native_startup_timeout_waits_for_offloaded_cleanup_instead_of_aborting() {
        let (cancellation, mut receiver) = watch::channel(false);
        let cleanup_completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_cleanup_completed = Arc::clone(&cleanup_completed);
        let task = tokio::spawn(async move {
            receiver
                .changed()
                .await
                .expect("fixture cancellation sender remains alive");
            assert!(*receiver.borrow());
            tokio::time::sleep(Duration::from_millis(20)).await;
            task_cleanup_completed.store(true, std::sync::atomic::Ordering::Release);
            Err(RecordingStartupFailure::cancelled(anyhow::anyhow!(
                "fixture native startup cancelled"
            )))
        });
        let starting = StartingRecording {
            process: LeagueProcess { pid: 42 },
            cancellation,
            task,
        };
        let events: EventSink = Arc::new(|_| {});

        cancel_starting_with_timeout(Some(starting), &events, Duration::from_millis(1)).await;

        assert!(cleanup_completed.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn active_failure_does_not_create_an_implicit_second_segment() {
        let process = LeagueProcess { pid: 42 };
        let now = TokioInstant::now();
        let mut retry = RecordingStartupRetryState::default();

        retry.block_active_process(Some(process));

        assert!(!retry.permits(process, now + Duration::from_secs(300)));
        retry.reset();
        assert!(retry.permits(process, now));
    }

    #[test]
    fn retry_attempt_directories_are_unique_and_preserved() {
        let directory = tempfile::tempdir().unwrap();

        let first = create_game_directory(directory.path(), 123).unwrap();
        let second = create_game_directory(directory.path(), 123).unwrap();

        assert_ne!(first, second);
        assert!(first.is_dir());
        assert!(second.is_dir());
    }

    #[test]
    fn texture_budget_includes_every_declared_pool_and_depth() {
        let bytes = maximum_texture_bytes((1920, 1080), (1920, 1080), 2, 8, 32, 16);
        let bgra = 1920_u64 * 1080 * 4 * 10;
        let nv12 = 1920_u64 * 1080 * 3 / 2 * 48;
        assert_eq!(bytes, bgra + nv12);
    }

    #[test]
    fn capture_progress_watchdog_requires_source_encode_and_mux_progress() {
        let start = Instant::now();
        let mut evidence = RecordingEvidence {
            latest_qpc: Some(100),
            encoded_frames: 10,
            muxed_bytes: 1_000,
            ..RecordingEvidence::default()
        };
        let mut watchdog = CaptureProgressWatchdog::new(&evidence, start);

        evidence.latest_qpc = Some(200);
        evidence.encoded_frames = 20;
        evidence.muxed_bytes = 2_000;
        assert_eq!(
            watchdog.observe(
                &evidence,
                start + Duration::from_secs(14),
                CaptureTargetVisibility::Visible
            ),
            CaptureProgressObservation::Healthy
        );
        assert_eq!(
            watchdog.observe(
                &evidence,
                start + Duration::from_secs(29),
                CaptureTargetVisibility::Visible
            ),
            CaptureProgressObservation::Stalled(
                "WGC source timestamp did not advance for 15 seconds".to_owned()
            )
        );
    }

    #[test]
    fn capture_progress_watchdog_surfaces_latched_protocol_failure() {
        let start = Instant::now();
        let mut evidence = RecordingEvidence::default();
        let mut watchdog = CaptureProgressWatchdog::new(&evidence, start);
        evidence.protocol_error = Some("counter regression".to_owned());
        assert_eq!(
            watchdog.observe(&evidence, start, CaptureTargetVisibility::Visible),
            CaptureProgressObservation::Stalled(
                "capture diagnostics failed: counter regression".to_owned()
            )
        );
    }

    #[test]
    fn capture_progress_watchdog_pauses_and_resets_for_window_visibility() {
        let start = Instant::now();
        let evidence = RecordingEvidence {
            latest_qpc: Some(100),
            encoded_frames: 10,
            muxed_bytes: 1_000,
            ..RecordingEvidence::default()
        };
        let mut watchdog = CaptureProgressWatchdog::new(&evidence, start);

        assert_eq!(
            watchdog.observe(
                &evidence,
                start + Duration::from_secs(20),
                CaptureTargetVisibility::PausedByWindowVisibility
            ),
            CaptureProgressObservation::PausedByWindowVisibility
        );
        assert_eq!(
            watchdog.observe(
                &evidence,
                start + Duration::from_secs(80),
                CaptureTargetVisibility::PausedByWindowVisibility
            ),
            CaptureProgressObservation::Healthy
        );
        assert_eq!(
            watchdog.observe(
                &evidence,
                start + Duration::from_secs(81),
                CaptureTargetVisibility::Visible
            ),
            CaptureProgressObservation::ResumedAfterWindowVisibility
        );
        assert_eq!(
            watchdog.observe(
                &evidence,
                start + Duration::from_secs(95),
                CaptureTargetVisibility::Visible
            ),
            CaptureProgressObservation::Healthy
        );
        assert_eq!(
            watchdog.observe(
                &evidence,
                start + Duration::from_secs(96),
                CaptureTargetVisibility::Visible
            ),
            CaptureProgressObservation::Stalled(
                "WGC source timestamp did not advance for 15 seconds".to_owned()
            )
        );
    }
}
