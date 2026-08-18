use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::config::{CodecPreference, Config, RecordingConfig, RecordingProfile};
use crate::encoder::capabilities::{
    DirectInterop, FILTER_BUFFERED_FRAME_LIMIT, NVENC_SURFACE_LIMIT,
};
use crate::encoder::{
    AudioSource, CAPTURE_DIAGNOSTIC_ABI, EncoderKind, Ffmpeg, RecordingCandidateStart,
    RecordingEvidence, RecordingPlan, RecordingSession, VideoCodec,
};
#[cfg(target_os = "windows")]
use crate::native::NativeRecordingSession;
use crate::platform::{
    CaptureTarget, CaptureTargetStateCache, CaptureTargetVisibility, capture_target_for_process,
    fallback_capture_target, query_capture_target_state,
};
use crate::poller::{CaptureMetadata, PollerSession, RecordingDetails, RecordingMetadata};
use crate::storage::{METADATA_JSON, create_game_directory, unix_timestamp_now, write_json_atomic};
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
    task: JoinHandle<Result<ActiveRecording>>,
}

struct RecordingStartup {
    ffmpeg: Ffmpeg,
    recording_config: RecordingConfig,
    hevc_playback_supported: bool,
    audio: AudioSource,
    output_path: PathBuf,
    process: LeagueProcess,
    target: CaptureTarget,
    #[cfg(target_os = "windows")]
    windows_backend: WindowsRecorderBackend,
}

const STARTUP_CANCELLATION_TIMEOUT: Duration = Duration::from_secs(6);
const CAPTURE_PROGRESS_STALL_TIMEOUT: Duration = Duration::from_secs(15);
const CONTROL_TELEMETRY_INTERVAL: Duration = Duration::from_secs(60);
#[cfg(target_os = "windows")]
const WINDOWS_RECORDING_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(target_os = "windows")]
const WINDOWS_RECORDER_BACKEND_ENV: &str = "QUEUEBACK_WINDOWS_RECORDER_BACKEND";

enum VideoRecordingSession {
    Ffmpeg(Box<RecordingSession>),
    #[cfg(target_os = "windows")]
    Native(NativeRecordingSession),
}

impl VideoRecordingSession {
    fn directory(&self) -> &std::path::Path {
        match self {
            Self::Ffmpeg(session) => session.directory(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.directory(),
        }
    }

    fn video_started_at(&self) -> Instant {
        match self {
            Self::Ffmpeg(session) => session.video_started_at(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.video_started_at(),
        }
    }

    fn recorded_at(&self) -> std::time::SystemTime {
        match self {
            Self::Ffmpeg(session) => session.recorded_at(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.recorded_at(),
        }
    }

    fn evidence_receiver(&self) -> watch::Receiver<RecordingEvidence> {
        match self {
            Self::Ffmpeg(session) => session.evidence_receiver(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.evidence_receiver(),
        }
    }

    fn has_exited(&mut self) -> Result<bool> {
        match self {
            Self::Ffmpeg(session) => session.has_exited(),
            #[cfg(target_os = "windows")]
            Self::Native(session) => Ok(session.has_exited()),
        }
    }

    async fn stop(self) -> Result<PathBuf> {
        match self {
            Self::Ffmpeg(session) => (*session).stop().await,
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.stop().await,
        }
    }

    async fn stop_with_failure(self, reason: String) -> Result<PathBuf> {
        match self {
            Self::Ffmpeg(session) => (*session).stop_with_failure(reason).await,
            #[cfg(target_os = "windows")]
            Self::Native(session) => session.stop_with_failure(reason).await,
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsRecorderBackend {
    Ffmpeg,
    Native,
}

#[cfg(target_os = "windows")]
impl WindowsRecorderBackend {
    fn from_environment() -> Result<Self> {
        Self::parse(env::var(WINDOWS_RECORDER_BACKEND_ENV).ok().as_deref())
    }

    fn parse(value: Option<&str>) -> Result<Self> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("ffmpeg") | Some("ffmpeg-wgc") => Ok(Self::Ffmpeg),
            Some("native") => Ok(Self::Native),
            Some(value) => bail!(
                "unsupported {WINDOWS_RECORDER_BACKEND_ENV} value {value:?}; expected ffmpeg or native"
            ),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg-wgc",
            Self::Native => "native-wgc-d3d11-nvenc",
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
struct WindowsCapturePath {
    interop: DirectInterop,
    encoder_depth: u32,
    filter_buffered_frame_limit: u32,
    backend: &'static str,
    support_label: &'static str,
}

#[derive(Debug, Default)]
struct RecorderControlTelemetry {
    startup_attempts: u64,
    startup_ready: u64,
    startup_errors: u64,
    startup_task_failures: u64,
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

    let ffmpeg = Ffmpeg::resolve().await?;
    let audio = ffmpeg.detect_audio_source().await;
    #[cfg(target_os = "windows")]
    let windows_backend = WindowsRecorderBackend::from_environment()?;
    #[cfg(target_os = "windows")]
    info!(
        backend = windows_backend.label(),
        selector = WINDOWS_RECORDER_BACKEND_ENV,
        "selected developer Windows recorder backend"
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
    let mut failed_process: Option<LeagueProcess> = None;
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
                        failed_process = None;
                        info!(pid = process.pid, "League process appeared; waiting for its capture window");
                    }
                    ProcessTransition::Replaced(process) => {
                        warn!(pid = process.pid, "League process was replaced between watcher polls");
                        cancel_starting(starting.take(), &events).await;
                        if let Some(recording) = active.take() {
                            stop_recording(recording, &events).await;
                        }
                        failed_process = None;
                    }
                    ProcessTransition::Disappeared => {
                        cancel_starting(starting.take(), &events).await;
                        if let Some(recording) = active.take() {
                            stop_recording(recording, &events).await;
                            events(ServiceEvent::Idle);
                        }
                        failed_process = None;
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
                            failed_process = Some(process);
                            error!(error = %start_error, "recording could not start");
                            events(ServiceEvent::Error {
                                message: format!("Recording failed to start: {start_error:#}"),
                            });
                        }
                        Err(join_error) => {
                            control_telemetry.startup_task_failures =
                                control_telemetry.startup_task_failures.saturating_add(1);
                            failed_process = Some(process);
                            error!(error = %join_error, "recording startup task failed");
                            events(ServiceEvent::Error {
                                message: format!("Recording failed to start: {join_error}"),
                            });
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
                            failed_process = current_process;
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
                            failed_process = current_process;
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
                            failed_process = current_process;
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
                    failed_process,
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
                                #[cfg(target_os = "windows")]
                                windows_backend,
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
    failed: Option<LeagueProcess>,
) -> Option<LeagueProcess> {
    current.filter(|process| !has_active && !has_starting && failed != Some(*process))
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
    let Some(mut starting) = starting else {
        return;
    };
    let _ = starting.cancellation.send(true);
    match tokio::time::timeout(STARTUP_CANCELLATION_TIMEOUT, &mut starting.task).await {
        Ok(Ok(Ok(recording))) => {
            stop_recording(recording, events).await;
        }
        Ok(Ok(Err(_))) | Ok(Err(_)) => {}
        Err(_) => {
            warn!(
                pid = starting.process.pid,
                "recording startup cancellation exceeded its deadline; aborting the task"
            );
            starting.task.abort();
            let _ = starting.task.await;
        }
    }
}

pub async fn diagnose(config: &Config) -> Result<DiagnosticReport> {
    let ffmpeg = Ffmpeg::resolve().await?;
    let source_dimensions = fallback_capture_target()
        .ok()
        .and_then(|target| target.dimensions());
    let plan = ffmpeg
        .select_recording_plan(
            &config.recording,
            config.app.hevc_playback_supported,
            source_dimensions,
        )
        .await?;
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
    mut cancellation: watch::Receiver<bool>,
) -> Result<ActiveRecording> {
    let RecordingStartup {
        ffmpeg,
        recording_config,
        hevc_playback_supported,
        audio,
        output_path,
        process,
        target,
        #[cfg(target_os = "windows")]
        windows_backend,
    } = startup;
    let mut target_state_cache = CaptureTargetStateCache::default();
    let initial_target_state = query_capture_target_state(&target, &mut target_state_cache)
        .context("capture target changed before recording startup")?;
    if initial_target_state.visibility == CaptureTargetVisibility::PausedByWindowVisibility {
        bail!("capture target became hidden or minimized before recording startup");
    }
    #[cfg(target_os = "windows")]
    let candidates = if windows_backend == WindowsRecorderBackend::Ffmpeg {
        Some(tokio::select! {
            result = ffmpeg.windows_capture_candidates(
                &target,
                &recording_config,
                hevc_playback_supported,
            ) => result?,
            _ = startup_cancelled(&mut cancellation) => {
                bail!("recording startup was cancelled");
            }
        })
    } else {
        None
    };
    #[cfg(not(target_os = "windows"))]
    let plan = tokio::select! {
        result = ffmpeg.select_recording_plan(
            &recording_config,
            hevc_playback_supported,
            target.dimensions(),
        ) => result?,
        _ = startup_cancelled(&mut cancellation) => {
            bail!("recording startup was cancelled");
        }
    };

    if *cancellation.borrow() {
        bail!("recording startup was cancelled");
    }
    let directory = create_game_directory(&output_path, unix_timestamp_now()?)?;

    #[cfg(target_os = "windows")]
    let (session, plan, capture_path) = match windows_backend {
        WindowsRecorderBackend::Ffmpeg => {
            let deadline = std::time::Instant::now() + WINDOWS_RECORDING_STARTUP_TIMEOUT;
            let mut failures = Vec::new();
            let mut selected = None;
            for (index, (plan, candidate)) in candidates
                .context("FFmpeg/WGC candidates were not planned")?
                .into_iter()
                .enumerate()
            {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    failures.push("the total 15-second candidate deadline expired".to_owned());
                    break;
                }
                match ffmpeg
                    .start_recording_candidate(
                        directory.clone(),
                        &target,
                        plan,
                        &audio,
                        RecordingCandidateStart::new(index, remaining, cancellation.clone()),
                    )
                    .await
                {
                    Ok(session) => {
                        selected = Some((
                            VideoRecordingSession::Ffmpeg(Box::new(session)),
                            plan,
                            WindowsCapturePath {
                                interop: candidate.interop,
                                encoder_depth: candidate.encoder_depth,
                                filter_buffered_frame_limit: FILTER_BUFFERED_FRAME_LIMIT,
                                backend: "windows_graphics_capture_d3d11",
                                support_label: "optimized-unvalidated",
                            },
                        ));
                        break;
                    }
                    Err(error) => {
                        if *cancellation.borrow() {
                            bail!("recording startup was cancelled");
                        }
                        warn!(
                            encoder = plan.encoder().codec_name(plan.codec()),
                            interop = candidate.interop.label(),
                            error = %error,
                            "same-adapter recording candidate failed"
                        );
                        failures.push(format!(
                            "{} via {}: {error:#}",
                            plan.encoder().codec_name(plan.codec()),
                            candidate.interop.label()
                        ));
                    }
                }
            }
            selected.with_context(|| {
                format!(
                    "exact-window GPU capture failed; no display/GDI fallback was attempted ({})",
                    failures.join("; ")
                )
            })?
        }
        WindowsRecorderBackend::Native => {
            let plan = native_recording_plan(&recording_config)?;
            let session = NativeRecordingSession::start(
                directory.clone(),
                target.clone(),
                ffmpeg.path().to_path_buf(),
                audio.clone(),
                cancellation.clone(),
                WINDOWS_RECORDING_STARTUP_TIMEOUT,
            )
            .await?;
            (
                VideoRecordingSession::Native(session),
                plan,
                WindowsCapturePath {
                    interop: DirectInterop::D3d11Nvenc,
                    encoder_depth: NVENC_SURFACE_LIMIT,
                    filter_buffered_frame_limit: 0,
                    backend: "native_windows_graphics_capture_d3d11",
                    support_label: "native-provisional",
                },
            )
        }
    };

    #[cfg(not(target_os = "windows"))]
    let (session, plan) = {
        let session = ffmpeg
            .start_recording(directory.clone(), &target, plan, &audio)
            .await?;
        (VideoRecordingSession::Ffmpeg(Box::new(session)), plan)
    };

    if *cancellation.borrow() {
        let _ = session
            .stop_with_failure("recording startup was cancelled".to_owned())
            .await;
        bail!("recording startup was cancelled");
    }

    let recording_resolution = plan
        .output_dimensions(target.dimensions())
        .map(|(width, height)| format!("{width}x{height}"))
        .unwrap_or_else(|| "source".to_owned());
    let poller = match PollerSession::start(&directory, session.video_started_at()).await {
        Ok(poller) => poller,
        Err(error) => {
            let _ = session
                .stop_with_failure(format!("Live Client poller startup failed: {error:#}"))
                .await;
            return Err(error).context("failed to start the Live Client poller");
        }
    };
    if *cancellation.borrow() {
        let _ = tokio::join!(
            poller.stop(),
            session.stop_with_failure("recording startup was cancelled".to_owned())
        );
        bail!("recording startup was cancelled");
    }
    let diagnostics = session.evidence_receiver();
    let initial_diagnostics = diagnostics.borrow().clone();

    #[cfg(target_os = "windows")]
    let capture_details = {
        let capture_adapter_luid_value = target
            .windows_adapter_luid()
            .context("optimized Windows target has no adapter LUID")?;
        let capture_adapter_luid = format!("{capture_adapter_luid_value:016x}");
        let capture_adapter_name = target
            .windows_adapter_name()
            .unwrap_or("unknown")
            .to_owned();
        let capture_output = target.windows_output_name().unwrap_or("unknown").to_owned();
        let encoder_interop = capture_path.interop.label().to_owned();
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
        let maximum_texture_bytes = maximum_texture_bytes(
            source_dimensions,
            output_dimensions,
            frame_pool_capacity,
            output_pool_capacity,
            capture_path.filter_buffered_frame_limit,
            capture_path.encoder_depth,
        );
        info!(
            frame_pool_capacity,
            output_pool_capacity,
            filter_buffered_frame_limit = capture_path.filter_buffered_frame_limit,
            encoder_depth = capture_path.encoder_depth,
            maximum_texture_bytes,
            "optimized capture resource bounds"
        );
        let mut gpu_stages = vec![
            "windows_graphics_capture_bgra_d3d11".to_owned(),
            "scale_d3d11_video_processor_nv12".to_owned(),
        ];
        if plan.encoder().label() == "qsv" {
            gpu_stages.push("d3d11_to_qsv_direct_hwmap".to_owned());
        }
        gpu_stages.push(encoder_interop.clone());
        let capture = CaptureMetadata {
            schema_version: 1,
            backend: capture_path.backend.to_owned(),
            diagnostics_abi: CAPTURE_DIAGNOSTIC_ABI,
            support_label: capture_path.support_label.to_owned(),
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
            filter_buffered_frame_limit: capture_path.filter_buffered_frame_limit,
            encoder_depth: capture_path.encoder_depth,
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
        (
            capture.backend.clone(),
            Some(capture_adapter_luid),
            Some(capture_adapter_name),
            Some(capture_output),
            Some(encoder_interop),
            capture.support_label.clone(),
            Some(capture),
        )
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

    #[cfg(target_os = "windows")]
    info!(
        pid = process.pid,
        directory = %directory.display(),
        capture_adapter_luid = format_args!(
            "{:016x}",
            target.windows_adapter_luid().unwrap_or_default()
        ),
        encoder_interop = capture_path.interop.label(),
        backend = capture_path.backend,
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
        bail!("the provisional native recorder currently supports H.264 only");
    }
    if !matches!(
        recording.profile,
        RecordingProfile::Auto | RecordingProfile::High
    ) {
        bail!(
            "the provisional native recorder currently requires the auto or high 1080p60 profile"
        );
    }
    RecordingPlan::new(EncoderKind::Nvenc, VideoCodec::H264, RecordingProfile::High)
        .context("could not select the fixed native 1080p60 H.264 plan")
}

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
        Ok(directory) => {
            info!(directory = %directory.display(), "video closed");
            let metadata_result = RecordingMetadata::new(recorded_at, duration, summary, details);
            match metadata_result {
                Ok(metadata) => {
                    if let Err(metadata_error) =
                        write_json_atomic(&directory.join(METADATA_JSON), &metadata).await
                    {
                        error!(error = %metadata_error, "could not write recording metadata");
                        events(ServiceEvent::Error {
                            message: format!(
                                "Recording metadata could not be saved: {metadata_error:#}"
                            ),
                        });
                    }
                }
                Err(metadata_error) => {
                    error!(error = %metadata_error, "could not build recording metadata");
                    events(ServiceEvent::Error {
                        message: format!(
                            "Recording metadata could not be created: {metadata_error:#}"
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
    fn windows_recorder_backend_is_explicit_and_defaults_to_ffmpeg() {
        assert_eq!(
            WindowsRecorderBackend::parse(None).unwrap(),
            WindowsRecorderBackend::Ffmpeg
        );
        assert_eq!(
            WindowsRecorderBackend::parse(Some("ffmpeg-wgc")).unwrap(),
            WindowsRecorderBackend::Ffmpeg
        );
        assert_eq!(
            WindowsRecorderBackend::parse(Some("native")).unwrap(),
            WindowsRecorderBackend::Native
        );
        assert!(WindowsRecorderBackend::parse(Some("automatic")).is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn provisional_native_plan_is_fixed_to_high_h264() {
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
        assert_eq!(
            startup_candidate(Some(process), false, false, None),
            Some(process)
        );
        assert_eq!(
            startup_candidate(Some(process), false, false, None),
            Some(process)
        );
        assert_eq!(
            startup_candidate(Some(process), false, false, Some(process)),
            None
        );
        assert_eq!(startup_candidate(Some(process), true, false, None), None);
        assert_eq!(startup_candidate(Some(process), false, true, None), None);
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
