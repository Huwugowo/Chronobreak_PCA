use std::env;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::config::{CodecPreference, RecordingConfig, RecordingProfile};
use crate::platform::{
    CaptureSource, CaptureTarget, instant_from_qpc_100ns, validate_capture_target,
};
use crate::storage::{VIDEO_MP4, VIDEO_PARTIAL_MP4};

pub mod capabilities;
mod progress;

use capabilities::{
    CaptureCandidate, EncoderCapability, FILTER_BUFFERED_FRAME_LIMIT, NVENC_SURFACE_LIMIT,
    plan_candidates,
};
pub use progress::{CAPTURE_DIAGNOSTIC_ABI, RecordingEvidence};
use progress::{StreamKind, drain_stream};

const ENCODER_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const PROFILE_BENCHMARK_DURATION: Duration = Duration::from_secs(3);
const FFMPEG_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const FFMPEG_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const FFMPEG_PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const PROBE_OUTPUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const FRAGMENTED_MP4_FLAGS: &str = "+frag_keyframe+empty_moov+default_base_moof";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EncoderKind {
    Nvenc,
    Amf,
    Qsv,
    Videotoolbox,
}

impl EncoderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Nvenc => "nvenc",
            Self::Amf => "amf",
            Self::Qsv => "qsv",
            Self::Videotoolbox => "videotoolbox",
        }
    }

    pub fn codec_name(self, codec: VideoCodec) -> &'static str {
        match (self, codec) {
            (Self::Nvenc, VideoCodec::H264) => "h264_nvenc",
            (Self::Nvenc, VideoCodec::Hevc) => "hevc_nvenc",
            (Self::Amf, VideoCodec::H264) => "h264_amf",
            (Self::Amf, VideoCodec::Hevc) => "hevc_amf",
            (Self::Qsv, VideoCodec::H264) => "h264_qsv",
            (Self::Qsv, VideoCodec::Hevc) => "hevc_qsv",
            (Self::Videotoolbox, VideoCodec::H264) => "h264_videotoolbox",
            (Self::Videotoolbox, VideoCodec::Hevc) => "hevc_videotoolbox",
        }
    }

    fn append_codec_arguments(
        self,
        codec: VideoCodec,
        speed: EncoderSpeed,
        arguments: &mut Vec<OsString>,
    ) {
        push_args(arguments, &["-c:v", self.codec_name(codec)]);
        match self {
            Self::Nvenc => {
                let preset = match speed {
                    EncoderSpeed::Speed => "p3",
                    EncoderSpeed::Balanced => "p4",
                    EncoderSpeed::Quality => "p5",
                };
                push_args(arguments, &["-preset", preset, "-bf", "0", "-surfaces"]);
                arguments.push(NVENC_SURFACE_LIMIT.to_string().into());
            }
            Self::Amf => {
                let quality = match speed {
                    EncoderSpeed::Speed => "speed",
                    EncoderSpeed::Balanced => "balanced",
                    EncoderSpeed::Quality => "quality",
                };
                push_args(arguments, &["-quality", quality, "-async_depth", "4"]);
            }
            Self::Qsv => {
                let preset = match speed {
                    EncoderSpeed::Speed => "veryfast",
                    EncoderSpeed::Balanced => "medium",
                    EncoderSpeed::Quality => "slow",
                };
                push_args(arguments, &["-preset", preset, "-async_depth", "4"]);
            }
            // VideoToolbox selects the hardware real-time path automatically. Avoid optional
            // ffmpeg switches here so the same arguments work across supported macOS releases.
            Self::Videotoolbox => {}
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    Hevc,
}

impl VideoCodec {
    pub fn label(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::Hevc => "hevc",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncoderSpeed {
    Speed,
    Balanced,
    Quality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProfileSpec {
    max_dimensions: (u32, u32),
    fps: u32,
    h264_bitrate_kbps: u32,
    hevc_bitrate_kbps: u32,
    speed: EncoderSpeed,
}

impl RecordingProfile {
    fn spec(self) -> Option<ProfileSpec> {
        match self {
            Self::Auto => None,
            Self::VeryLow => Some(ProfileSpec {
                max_dimensions: (1280, 720),
                fps: 30,
                h264_bitrate_kbps: 3_000,
                hevc_bitrate_kbps: 2_000,
                speed: EncoderSpeed::Speed,
            }),
            Self::Low => Some(ProfileSpec {
                max_dimensions: (1280, 720),
                fps: 60,
                h264_bitrate_kbps: 5_000,
                hevc_bitrate_kbps: 3_500,
                speed: EncoderSpeed::Speed,
            }),
            Self::Medium => Some(ProfileSpec {
                max_dimensions: (1920, 1080),
                fps: 30,
                h264_bitrate_kbps: 7_000,
                hevc_bitrate_kbps: 5_000,
                speed: EncoderSpeed::Balanced,
            }),
            Self::High => Some(ProfileSpec {
                max_dimensions: (1920, 1080),
                fps: 60,
                h264_bitrate_kbps: 12_000,
                hevc_bitrate_kbps: 8_000,
                speed: EncoderSpeed::Balanced,
            }),
            Self::VeryHigh => Some(ProfileSpec {
                max_dimensions: (2560, 1440),
                fps: 60,
                h264_bitrate_kbps: 18_000,
                hevc_bitrate_kbps: 12_000,
                speed: EncoderSpeed::Quality,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingPlan {
    pub encoder: EncoderKind,
    pub codec: VideoCodec,
    pub profile: RecordingProfile,
}

impl RecordingPlan {
    pub fn fps(self) -> u32 {
        self.profile
            .spec()
            .expect("a selected recording plan has a concrete profile")
            .fps
    }

    pub fn output_dimensions(self, source: Option<(u32, u32)>) -> Option<(u32, u32)> {
        let maximum = self
            .profile
            .spec()
            .expect("a selected recording plan has a concrete profile")
            .max_dimensions;
        source.map(|dimensions| fit_within(dimensions, maximum))
    }

    fn benchmark_dimensions(self, source: Option<(u32, u32)>) -> (u32, u32) {
        self.output_dimensions(source)
            .unwrap_or_else(|| self.profile.spec().unwrap().max_dimensions)
    }

    fn bitrate_kbps(self, dimensions: (u32, u32)) -> u32 {
        let spec = self.profile.spec().unwrap();
        let base = match self.codec {
            VideoCodec::H264 => spec.h264_bitrate_kbps,
            VideoCodec::Hevc => spec.hevc_bitrate_kbps,
        };
        if self.profile == RecordingProfile::VeryHigh
            && u64::from(dimensions.0) * u64::from(dimensions.1) > 1920 * 1080
        {
            base * 4 / 3
        } else {
            base
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioSource {
    DirectShow(String),
    AvFoundationCombined,
    Silent,
}

impl AudioSource {
    pub fn description(&self) -> String {
        match self {
            Self::DirectShow(device) => format!("DirectShow device {device:?}"),
            Self::AvFoundationCombined => "AVFoundation default audio".to_owned(),
            Self::Silent => "silent stereo fallback".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Ffmpeg {
    path: PathBuf,
    runtime_id: String,
}

pub(crate) struct RecordingCandidateStart {
    candidate_index: usize,
    startup_timeout: Duration,
    cancellation: watch::Receiver<bool>,
}

impl RecordingCandidateStart {
    pub(crate) fn new(
        candidate_index: usize,
        startup_timeout: Duration,
        cancellation: watch::Receiver<bool>,
    ) -> Self {
        Self {
            candidate_index,
            startup_timeout,
            cancellation,
        }
    }
}

struct RecordingOutputStart {
    output_name: String,
    startup_timeout: Duration,
    cancellation: Option<watch::Receiver<bool>>,
}

impl Ffmpeg {
    pub async fn resolve() -> Result<Self> {
        let executable =
            env::current_exe().context("could not resolve recorder executable path")?;
        let packaged_root = queueback_media_runtime::production_root_for_executable(&executable)?;
        let tools = queueback_media_runtime::resolve(&packaged_root).await?;
        Ok(Self {
            path: tools.ffmpeg().to_path_buf(),
            runtime_id: tools.runtime_id().to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    pub async fn select_recording_plan(
        &self,
        recording: &RecordingConfig,
        hevc_playback_supported: bool,
        source_dimensions: Option<(u32, u32)>,
    ) -> Result<RecordingPlan> {
        let advertised = self.advertised_encoders().await?;
        let order = preferred_encoders();
        let codecs = codec_candidates(recording.codec, hevc_playback_supported);

        if recording.codec == CodecPreference::Auto && !hevc_playback_supported {
            debug!(
                "HEVC playback has not been validated by the app; codec auto is restricted to H.264"
            );
        }

        for encoder in order.iter().copied() {
            for codec in codecs.iter().copied() {
                let codec_name = encoder.codec_name(codec);
                if !advertised.contains(codec_name) {
                    debug!(encoder = codec_name, "ffmpeg does not advertise encoder");
                    continue;
                }

                let selection = match recording.profile {
                    RecordingProfile::Auto => {
                        self.recommend_profile(encoder, codec, source_dimensions)
                            .await
                    }
                    profile => {
                        let plan = RecordingPlan {
                            encoder,
                            codec,
                            profile,
                        };
                        self.probe_plan(plan, source_dimensions)
                            .await
                            .map(|()| profile)
                    }
                };

                match selection {
                    Ok(profile) => {
                        let plan = RecordingPlan {
                            encoder,
                            codec,
                            profile,
                        };
                        info!(
                            encoder = codec_name,
                            codec = codec.label(),
                            profile = profile.label(),
                            "selected working hardware recording plan"
                        );
                        return Ok(plan);
                    }
                    Err(error) => {
                        warn!(
                            encoder = codec_name,
                            codec = codec.label(),
                            %error,
                            "hardware recording plan probe failed"
                        );
                    }
                }
            }
        }

        let requested = match recording.codec {
            CodecPreference::Auto => "a compatible H.264/HEVC",
            CodecPreference::H264 => "an H.264",
            CodecPreference::Hevc => "an HEVC",
        };
        bail!(
            "no working hardware {requested} encoder was found; software encoding is intentionally unsupported"
        )
    }

    pub async fn windows_capture_candidates(
        &self,
        target: &CaptureTarget,
        recording: &RecordingConfig,
        hevc_playback_supported: bool,
    ) -> Result<Vec<(RecordingPlan, CaptureCandidate)>> {
        let adapter_luid = target
            .windows_adapter_luid()
            .context("the optimized Windows target has no DXGI adapter LUID")?;
        let advertised = self.advertised_encoders().await?;
        let codecs = codec_candidates(recording.codec, hevc_playback_supported);
        let mut capabilities = Vec::new();

        for encoder in [EncoderKind::Nvenc, EncoderKind::Amf, EncoderKind::Qsv] {
            for codec in codecs.iter().copied() {
                if advertised.contains(encoder.codec_name(codec)) {
                    capabilities.push(EncoderCapability::compiled_candidate(
                        encoder,
                        codec,
                        adapter_luid,
                    ));
                }
            }
        }

        let candidates = plan_candidates(adapter_luid, &codecs, &capabilities)
            .context("optimized Windows capture is unsupported")?;
        let profile = match recording.profile {
            RecordingProfile::Auto => RecordingProfile::High,
            profile => profile,
        };
        Ok(candidates
            .into_iter()
            .map(|candidate| {
                (
                    RecordingPlan {
                        encoder: candidate.encoder,
                        codec: candidate.codec,
                        profile,
                    },
                    candidate,
                )
            })
            .collect())
    }

    async fn recommend_profile(
        &self,
        encoder: EncoderKind,
        codec: VideoCodec,
        source_dimensions: Option<(u32, u32)>,
    ) -> Result<RecordingProfile> {
        let headroom_limit = PROFILE_BENCHMARK_DURATION.mul_f64(2.0 / 3.0);
        let mut lowest_working = None;

        for profile in [
            RecordingProfile::High,
            RecordingProfile::Medium,
            RecordingProfile::Low,
            RecordingProfile::VeryLow,
        ] {
            let plan = RecordingPlan {
                encoder,
                codec,
                profile,
            };
            match self.benchmark_plan(plan, source_dimensions).await {
                Ok(elapsed) => {
                    lowest_working = Some(profile);
                    debug!(
                        encoder = encoder.codec_name(codec),
                        profile = profile.label(),
                        elapsed_ms = elapsed.as_millis(),
                        "recording profile benchmark completed"
                    );
                    if elapsed <= headroom_limit {
                        return Ok(profile);
                    }
                }
                Err(error) => {
                    warn!(
                        encoder = encoder.codec_name(codec),
                        profile = profile.label(),
                        %error,
                        "recording profile benchmark failed"
                    );
                }
            }
        }

        let profile = lowest_working.context("no profile completed the hardware encode probe")?;
        warn!(
            encoder = encoder.codec_name(codec),
            profile = profile.label(),
            "no profile reached the preferred encode headroom; using the lowest working profile"
        );
        Ok(profile)
    }

    pub async fn detect_audio_source(&self) -> AudioSource {
        #[cfg(target_os = "macos")]
        {
            AudioSource::AvFoundationCombined
        }

        #[cfg(target_os = "windows")]
        {
            if let Ok(explicit) = env::var("LEAGUE_REPLAY_AUDIO_DEVICE") {
                if explicit.eq_ignore_ascii_case("none") || explicit.eq_ignore_ascii_case("silent")
                {
                    return AudioSource::Silent;
                }
                return AudioSource::DirectShow(explicit);
            }

            let mut command = ffmpeg_command(&self.path);
            command.args([
                "-hide_banner",
                "-list_devices",
                "true",
                "-f",
                "dshow",
                "-i",
                "dummy",
            ]);
            let output = run_bounded_probe(
                command,
                ENCODER_PROBE_TIMEOUT,
                "Windows audio-device discovery",
            )
            .await;

            match output {
                Ok(output) => {
                    let listing = format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    if let Some(device) = choose_audio_device(&listing) {
                        return AudioSource::DirectShow(device);
                    }
                }
                Err(error) => warn!(%error, "Windows audio-device discovery failed"),
            }

            warn!("no Windows loopback audio device found; recording a silent audio track");
            AudioSource::Silent
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            AudioSource::Silent
        }
    }

    async fn advertised_encoders(&self) -> Result<String> {
        let mut command = ffmpeg_command(&self.path);
        command.args(["-hide_banner", "-encoders"]);
        let output = run_bounded_probe(command, ENCODER_PROBE_TIMEOUT, "FFmpeg encoder discovery")
            .await
            .with_context(|| format!("failed to query encoders from {}", self.path.display()))?;
        let listing = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(listing)
    }

    async fn probe_plan(
        &self,
        plan: RecordingPlan,
        source_dimensions: Option<(u32, u32)>,
    ) -> Result<()> {
        let output_dimensions = plan.benchmark_dimensions(source_dimensions);
        let input_dimensions = source_dimensions.unwrap_or(output_dimensions);
        let mut arguments = vec![
            OsString::from("-hide_banner"),
            OsString::from("-loglevel"),
            OsString::from("error"),
            OsString::from("-f"),
            OsString::from("lavfi"),
            OsString::from("-i"),
            OsString::from(format!(
                "color=c=black:s={}x{}:r={}",
                input_dimensions.0,
                input_dimensions.1,
                plan.fps()
            )),
            OsString::from("-frames:v"),
            OsString::from("1"),
            OsString::from("-an"),
        ];
        append_encoding_arguments(&mut arguments, plan, output_dimensions)?;
        append_fixed_scale_filter(&mut arguments, input_dimensions, output_dimensions);
        push_args(&mut arguments, &["-pix_fmt", "yuv420p", "-f", "null", "-"]);

        let mut command = ffmpeg_command(&self.path);
        command.args(arguments);
        let output = run_bounded_probe(
            command,
            ENCODER_PROBE_TIMEOUT,
            "hardware recording plan probe",
        )
        .await?;
        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(())
    }

    async fn benchmark_plan(
        &self,
        plan: RecordingPlan,
        source_dimensions: Option<(u32, u32)>,
    ) -> Result<Duration> {
        let output_dimensions = plan.benchmark_dimensions(source_dimensions);
        let input_dimensions = source_dimensions.unwrap_or(output_dimensions);
        let mut arguments = vec![
            OsString::from("-hide_banner"),
            OsString::from("-loglevel"),
            OsString::from("error"),
            OsString::from("-f"),
            OsString::from("lavfi"),
            OsString::from("-i"),
            OsString::from(format!(
                "testsrc2=size={}x{}:rate={}:duration={}",
                input_dimensions.0,
                input_dimensions.1,
                plan.fps(),
                PROFILE_BENCHMARK_DURATION.as_secs()
            )),
            OsString::from("-an"),
        ];
        append_encoding_arguments(&mut arguments, plan, output_dimensions)?;
        append_fixed_scale_filter(&mut arguments, input_dimensions, output_dimensions);
        push_args(&mut arguments, &["-pix_fmt", "yuv420p", "-f", "null", "-"]);

        let started = Instant::now();
        let mut command = ffmpeg_command(&self.path);
        command.args(arguments);
        let output = run_bounded_probe(
            command,
            ENCODER_PROBE_TIMEOUT,
            "recording profile benchmark",
        )
        .await?;
        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(started.elapsed())
    }

    pub async fn start_recording(
        &self,
        directory: PathBuf,
        target: &CaptureTarget,
        plan: RecordingPlan,
        audio: &AudioSource,
    ) -> Result<RecordingSession> {
        self.start_recording_output(
            directory,
            target,
            plan,
            audio,
            RecordingOutputStart {
                output_name: VIDEO_PARTIAL_MP4.to_owned(),
                startup_timeout: FFMPEG_STARTUP_TIMEOUT,
                cancellation: None,
            },
        )
        .await
    }

    pub(crate) async fn start_recording_candidate(
        &self,
        directory: PathBuf,
        target: &CaptureTarget,
        plan: RecordingPlan,
        audio: &AudioSource,
        start: RecordingCandidateStart,
    ) -> Result<RecordingSession> {
        self.start_recording_output(
            directory,
            target,
            plan,
            audio,
            RecordingOutputStart {
                output_name: format!("video-candidate-{}.partial.mp4", start.candidate_index),
                startup_timeout: start.startup_timeout,
                cancellation: Some(start.cancellation),
            },
        )
        .await
    }

    async fn start_recording_output(
        &self,
        directory: PathBuf,
        target: &CaptureTarget,
        plan: RecordingPlan,
        audio: &AudioSource,
        start: RecordingOutputStart,
    ) -> Result<RecordingSession> {
        #[cfg(test)]
        if self.runtime_id != "test-runtime" {
            validate_capture_target(target)?;
        }
        #[cfg(not(test))]
        validate_capture_target(target)?;
        let output = directory.join(&start.output_name);
        let arguments = build_recording_arguments(target, plan, audio, &output)?;
        info!(
            ffmpeg = %self.path.display(),
            target = %target.description(),
            audio = %audio.description(),
            encoder = plan.encoder.codec_name(plan.codec),
            codec = plan.codec.label(),
            profile = plan.profile.label(),
            "starting recording"
        );
        debug!(arguments = ?arguments, "ffmpeg recording arguments");

        let mut command = ffmpeg_command(&self.path);
        command
            .args(&arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start {}", self.path.display()))?;
        let (evidence_sender, mut evidence_receiver) = watch::channel(RecordingEvidence::default());
        let stdout = child
            .stdout
            .take()
            .context("FFmpeg progress pipe was not created")?;
        let stdout_sender = evidence_sender.clone();
        let stdout_task = tokio::spawn(async move {
            drain_stream(stdout, StreamKind::Progress, stdout_sender).await;
        });
        let stderr = child
            .stderr
            .take()
            .context("FFmpeg diagnostics pipe was not created")?;
        let stderr_task = tokio::spawn(async move {
            drain_stream(stderr, StreamKind::Stderr, evidence_sender).await;
        });

        let startup = tokio::time::timeout(
            start.startup_timeout,
            wait_for_startup(&mut child, &mut evidence_receiver, start.cancellation),
        )
        .await;
        let evidence = match startup {
            Ok(Ok(evidence)) => evidence,
            Ok(Err(error)) => {
                terminate_failed_startup(&mut child, stdout_task, stderr_task).await;
                return Err(error);
            }
            Err(_) => {
                terminate_failed_startup(&mut child, stdout_task, stderr_task).await;
                bail!(
                    "FFmpeg did not report a real WGC frame and advancing encoded output within {} seconds",
                    start.startup_timeout.as_secs_f64()
                );
            }
        };

        let first_qpc = evidence
            .first_qpc
            .context("capture became ready without a first-frame QPC timestamp")?;
        #[cfg(test)]
        let video_started_at = if self.runtime_id == "test-runtime" {
            Instant::now()
        } else {
            instant_from_qpc_100ns(first_qpc)?
        };
        #[cfg(not(test))]
        let video_started_at = instant_from_qpc_100ns(first_qpc)?;
        let now = Instant::now();
        let recorded_at = SystemTime::now()
            .checked_sub(now.saturating_duration_since(video_started_at))
            .context("WGC first-frame wall-clock anchor underflowed")?;

        info!(
            first_qpc,
            source_frames = evidence.source_frames_received(),
            encoded_frames = evidence.encoded_frames,
            muxed_bytes = evidence.muxed_bytes,
            "recording graph produced its first frame"
        );

        Ok(RecordingSession {
            directory,
            output,
            child,
            stdout_task: Some(stdout_task),
            stderr_task: Some(stderr_task),
            evidence: evidence_receiver,
            video_started_at,
            recorded_at,
        })
    }
}

async fn wait_for_startup(
    child: &mut Child,
    evidence: &mut watch::Receiver<RecordingEvidence>,
    mut cancellation: Option<watch::Receiver<bool>>,
) -> Result<RecordingEvidence> {
    loop {
        if cancellation
            .as_ref()
            .is_some_and(|receiver| *receiver.borrow())
        {
            bail!("recording startup was cancelled");
        }
        let snapshot = evidence.borrow().clone();
        if let Some(error) = snapshot.protocol_error.as_deref() {
            bail!("FFmpeg capture observability failed: {error}");
        }
        if snapshot.startup_ready() {
            return Ok(snapshot);
        }
        if let Some(status) = child
            .try_wait()
            .context("failed to inspect FFmpeg during startup")?
        {
            bail!("FFmpeg exited during startup with status {status}");
        }

        tokio::select! {
            changed = evidence.changed() => {
                if changed.is_err() {
                    bail!("FFmpeg capture/progress pipes closed before startup became ready");
                }
            }
            _ = wait_for_cancellation(&mut cancellation) => {
                bail!("recording startup was cancelled");
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
}

async fn wait_for_cancellation(cancellation: &mut Option<watch::Receiver<bool>>) {
    let Some(receiver) = cancellation else {
        std::future::pending::<()>().await;
        return;
    };
    loop {
        if *receiver.borrow() || receiver.changed().await.is_err() {
            return;
        }
    }
}

async fn terminate_failed_startup(
    child: &mut Child,
    mut stdout_task: JoinHandle<()>,
    mut stderr_task: JoinHandle<()>,
) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(FFMPEG_PIPE_DRAIN_TIMEOUT, child.wait()).await;
    tokio::join!(
        join_or_abort(&mut stdout_task),
        join_or_abort(&mut stderr_task)
    );
}

async fn join_or_abort(task: &mut JoinHandle<()>) {
    if tokio::time::timeout(FFMPEG_PIPE_DRAIN_TIMEOUT, &mut *task)
        .await
        .is_err()
    {
        task.abort();
        let _ = task.await;
    }
}

pub struct RecordingSession {
    directory: PathBuf,
    output: PathBuf,
    child: Child,
    stdout_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    evidence: watch::Receiver<RecordingEvidence>,
    video_started_at: Instant,
    recorded_at: SystemTime,
}

impl RecordingSession {
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn video_started_at(&self) -> Instant {
        self.video_started_at
    }

    pub fn recorded_at(&self) -> SystemTime {
        self.recorded_at
    }

    /// Returns the bounded, latest-value capture evidence stream for diagnostics
    /// and dedicated verification fixtures.
    pub fn evidence_receiver(&self) -> watch::Receiver<RecordingEvidence> {
        self.evidence.clone()
    }

    pub fn has_exited(&mut self) -> Result<bool> {
        Ok(self
            .child
            .try_wait()
            .context("failed to inspect ffmpeg process")?
            .is_some())
    }

    pub async fn stop(self) -> Result<PathBuf> {
        self.stop_inner(None).await
    }

    pub(crate) async fn stop_with_failure(self, failure_reason: String) -> Result<PathBuf> {
        self.stop_inner(Some(failure_reason)).await
    }

    async fn stop_inner(mut self, failure_reason: Option<String>) -> Result<PathBuf> {
        let mut exit_status = self.child.try_wait().context("failed to inspect ffmpeg")?;
        let exited_before_stop = exit_status.is_some();
        let mut stop_error = failure_reason;
        if exit_status.is_none() {
            if let Some(mut stdin) = self.child.stdin.take() {
                let delivery = tokio::time::timeout(FFMPEG_PIPE_DRAIN_TIMEOUT, async {
                    stdin.write_all(b"q\n").await?;
                    stdin.flush().await
                })
                .await;
                match delivery {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        warn!(%error, "failed to send graceful stop to ffmpeg");
                        stop_error.get_or_insert_with(|| {
                            format!("could not deliver FFmpeg stop command: {error}")
                        });
                    }
                    Err(_) => {
                        warn!("timed out sending graceful stop to ffmpeg");
                        stop_error
                            .get_or_insert_with(|| "FFmpeg stop command timed out".to_owned());
                    }
                }
            } else {
                stop_error.get_or_insert_with(|| "FFmpeg stop pipe was unavailable".to_owned());
            }

            match tokio::time::timeout(FFMPEG_STOP_TIMEOUT, self.child.wait()).await {
                Ok(Ok(status)) => {
                    info!(%status, "ffmpeg stopped");
                    exit_status = Some(status);
                }
                Ok(Err(error)) => {
                    warn!(%error, "failed while waiting for ffmpeg");
                    stop_error
                        .get_or_insert_with(|| format!("failed while waiting for FFmpeg: {error}"));
                    let _ = self.child.start_kill();
                    exit_status =
                        tokio::time::timeout(FFMPEG_PIPE_DRAIN_TIMEOUT, self.child.wait())
                            .await
                            .ok()
                            .and_then(|result| result.ok());
                }
                Err(_) => {
                    warn!("ffmpeg did not stop within 10 seconds; forcing termination");
                    stop_error
                        .get_or_insert_with(|| "FFmpeg did not stop within 10 seconds".to_owned());
                    let _ = self.child.start_kill();
                    exit_status =
                        tokio::time::timeout(FFMPEG_PIPE_DRAIN_TIMEOUT, self.child.wait())
                            .await
                            .ok()
                            .and_then(|result| result.ok());
                }
            }
        }

        let mut stdout_task = self.stdout_task.take();
        let mut stderr_task = self.stderr_task.take();
        tokio::join!(
            async {
                if let Some(task) = stdout_task.as_mut() {
                    join_or_abort(task).await;
                }
            },
            async {
                if let Some(task) = stderr_task.as_mut() {
                    join_or_abort(task).await;
                }
            }
        );

        let evidence = self.evidence.borrow().clone();
        if let Some(error) = evidence.protocol_error {
            stop_error.get_or_insert(format!("FFmpeg capture observability failed: {error}"));
        }
        if !evidence.progress_end || !evidence.capture_terminal {
            stop_error.get_or_insert_with(|| {
                "FFmpeg did not report both terminal capture and mux progress".to_owned()
            });
        }

        let output = self.output.clone();
        let output_metadata = tokio::fs::metadata(&output)
            .await
            .with_context(|| format!("recording is missing {}", output.display()))?;
        if output_metadata.len() == 0 {
            bail!(
                "recording {} is empty; partial output was not published",
                output.display()
            );
        }

        if let Some(status) = exit_status
            && !status.success()
        {
            bail!(
                "ffmpeg exited with {status}; partial MP4 preserved at {}",
                output.display()
            );
        }
        if exited_before_stop {
            bail!(
                "FFmpeg exited before QueueBack requested stop; partial MP4 preserved at {}",
                output.display()
            );
        }
        if let Some(error) = stop_error {
            bail!(
                "{error}; partial fragmented MP4 preserved at {}",
                output.display()
            );
        }

        let canonical_output = self.directory.join(VIDEO_MP4);
        if tokio::fs::try_exists(&canonical_output).await? {
            bail!(
                "refusing to overwrite an existing recording at {}; validated partial remains at {}",
                canonical_output.display(),
                output.display()
            );
        }
        tokio::fs::rename(&output, &canonical_output)
            .await
            .with_context(|| {
                format!(
                    "failed to publish validated output {} as {}",
                    output.display(),
                    canonical_output.display()
                )
            })?;
        Ok(self.directory)
    }
}

fn ffmpeg_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

async fn run_bounded_probe(
    mut command: Command,
    deadline: Duration,
    operation: &str,
) -> Result<Output> {
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to launch {operation}"))?;
    let stdout = child
        .stdout
        .take()
        .with_context(|| format!("{operation} stdout pipe was not created"))?;
    let stderr = child
        .stderr
        .take()
        .with_context(|| format!("{operation} stderr pipe was not created"))?;
    let mut stdout_task = tokio::spawn(drain_probe_pipe(stdout));
    let mut stderr_task = tokio::spawn(drain_probe_pipe(stderr));

    let status = match tokio::time::timeout(deadline, child.wait()).await {
        Ok(status) => status.with_context(|| format!("failed to wait for {operation}"))?,
        Err(_) => {
            let kill_error = child
                .start_kill()
                .err()
                .map(|error| format!("kill failed: {error}"));
            let reap_error =
                match tokio::time::timeout(FFMPEG_PIPE_DRAIN_TIMEOUT, child.wait()).await {
                    Ok(Ok(_)) => None,
                    Ok(Err(error)) => Some(format!("reap failed: {error}")),
                    Err(_) => Some(format!(
                        "reap exceeded {} seconds",
                        FFMPEG_PIPE_DRAIN_TIMEOUT.as_secs_f64()
                    )),
                };
            stop_probe_reader(&mut stdout_task).await;
            stop_probe_reader(&mut stderr_task).await;
            let cleanup = [kill_error, reap_error]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("; ");
            if cleanup.is_empty() {
                bail!(
                    "{operation} timed out after {} seconds; child was killed and reaped",
                    deadline.as_secs_f64()
                );
            }
            bail!(
                "{operation} timed out after {} seconds ({cleanup})",
                deadline.as_secs_f64()
            );
        }
    };

    let (stdout, stdout_overflow) =
        finish_probe_reader(&mut stdout_task, operation, "stdout").await?;
    let (stderr, stderr_overflow) =
        finish_probe_reader(&mut stderr_task, operation, "stderr").await?;
    if stdout_overflow || stderr_overflow {
        bail!(
            "{operation} exceeded the {} byte diagnostic-output limit",
            PROBE_OUTPUT_LIMIT_BYTES
        );
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

async fn drain_probe_pipe<R>(mut reader: R) -> io::Result<(Vec<u8>, bool)>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut output = Vec::new();
    let mut overflow = false;
    let mut chunk = [0_u8; 4096];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            return Ok((output, overflow));
        }
        let remaining = PROBE_OUTPUT_LIMIT_BYTES.saturating_sub(output.len());
        let retained = remaining.min(read);
        output.extend_from_slice(&chunk[..retained]);
        overflow |= retained < read;
    }
}

async fn finish_probe_reader(
    task: &mut JoinHandle<io::Result<(Vec<u8>, bool)>>,
    operation: &str,
    stream: &str,
) -> Result<(Vec<u8>, bool)> {
    match tokio::time::timeout(FFMPEG_PIPE_DRAIN_TIMEOUT, &mut *task).await {
        Ok(Ok(result)) => result.with_context(|| format!("failed to read {operation} {stream}")),
        Ok(Err(error)) => {
            Err(anyhow::Error::new(error).context(format!("{operation} {stream} reader panicked")))
        }
        Err(_) => {
            task.abort();
            let _ = task.await;
            bail!(
                "{operation} {stream} did not close within {} seconds",
                FFMPEG_PIPE_DRAIN_TIMEOUT.as_secs_f64()
            )
        }
    }
}

async fn stop_probe_reader(task: &mut JoinHandle<io::Result<(Vec<u8>, bool)>>) {
    if tokio::time::timeout(FFMPEG_PIPE_DRAIN_TIMEOUT, &mut *task)
        .await
        .is_err()
    {
        task.abort();
        let _ = task.await;
    }
}

fn preferred_encoders() -> &'static [EncoderKind] {
    #[cfg(target_os = "macos")]
    {
        &[EncoderKind::Videotoolbox]
    }
    #[cfg(not(target_os = "macos"))]
    {
        &[EncoderKind::Nvenc, EncoderKind::Amf, EncoderKind::Qsv]
    }
}

fn codec_candidates(preference: CodecPreference, hevc_playback_supported: bool) -> Vec<VideoCodec> {
    match preference {
        CodecPreference::H264 => vec![VideoCodec::H264],
        CodecPreference::Hevc => vec![VideoCodec::Hevc],
        CodecPreference::Auto if hevc_playback_supported => {
            vec![VideoCodec::Hevc, VideoCodec::H264]
        }
        CodecPreference::Auto => vec![VideoCodec::H264],
    }
}

#[cfg(any(target_os = "windows", test))]
fn choose_audio_device(listing: &str) -> Option<String> {
    let names = quoted_device_names(listing);
    names
        .iter()
        .find(|name| name.to_ascii_lowercase().contains("stereo mix"))
        .cloned()
        .or_else(|| {
            names
                .iter()
                .find(|name| {
                    let lower = name.to_ascii_lowercase();
                    lower.contains("wasapi") && lower.contains("loopback")
                })
                .cloned()
        })
}

#[cfg(any(target_os = "windows", test))]
fn quoted_device_names(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| {
            let start = line.find('"')?;
            let remainder = &line[start + 1..];
            let end = remainder.find('"')?;
            let name = &remainder[..end];
            (!name.starts_with('@')).then(|| name.to_owned())
        })
        .collect()
}

fn build_recording_arguments(
    target: &CaptureTarget,
    plan: RecordingPlan,
    audio: &AudioSource,
    output: &Path,
) -> Result<Vec<OsString>> {
    let mut arguments = Vec::<OsString>::new();
    push_args(
        &mut arguments,
        &[
            "-hide_banner",
            "-loglevel",
            "info",
            "-nostats",
            "-stats_period",
            "0.25",
            "-progress",
            "pipe:1",
            "-filter_buffered_frames",
        ],
    );
    arguments.push(FILTER_BUFFERED_FRAME_LIMIT.to_string().into());

    match &target.source {
        CaptureSource::WindowsGraphicsCapture {
            hwnd,
            adapter_index,
            ..
        } => {
            push_args(&mut arguments, &["-init_hw_device"]);
            arguments.push(format!("d3d11va=queueback:{adapter_index}").into());
            if plan.encoder == EncoderKind::Qsv {
                push_args(
                    &mut arguments,
                    &["-init_hw_device", "qsv=queueback_qsv@queueback"],
                );
            }
            push_args(
                &mut arguments,
                &["-filter_hw_device", "queueback", "-f", "lavfi", "-i"],
            );
            arguments.push(
                format!(
                    "gfxcapture=hwnd={hwnd}:capture_cursor=false:capture_border=false:display_border=false:max_framerate={}:output_fmt=bgra",
                    plan.fps()
                )
                .into(),
            );

            append_windows_audio_arguments(&mut arguments, audio);
            push_args(&mut arguments, &["-map", "0:v:0", "-map", "1:a:0"]);
        }
        CaptureSource::DesktopRegion {
            x,
            y,
            width,
            height,
            ..
        } => {
            push_args(
                &mut arguments,
                &[
                    "-thread_queue_size",
                    "1024",
                    "-f",
                    "gdigrab",
                    "-draw_mouse",
                    "0",
                    "-framerate",
                ],
            );
            arguments.push(plan.fps().to_string().into());
            push_args(&mut arguments, &["-offset_x"]);
            arguments.push(x.to_string().into());
            push_args(&mut arguments, &["-offset_y"]);
            arguments.push(y.to_string().into());
            push_args(&mut arguments, &["-video_size"]);
            arguments.push(format!("{width}x{height}").into());
            push_args(&mut arguments, &["-i", "desktop"]);

            append_windows_audio_arguments(&mut arguments, audio);
            push_args(&mut arguments, &["-map", "0:v:0", "-map", "1:a:0"]);
        }
        CaptureSource::AvFoundation { input } => {
            push_args(
                &mut arguments,
                &[
                    "-thread_queue_size",
                    "1024",
                    "-f",
                    "avfoundation",
                    "-capture_cursor",
                    "0",
                    "-framerate",
                ],
            );
            arguments.push(plan.fps().to_string().into());
            push_args(&mut arguments, &["-i"]);
            arguments.push(input.into());
            push_args(&mut arguments, &["-map", "0:v:0", "-map", "0:a:0"]);
        }
    }

    let source_dimensions = target.dimensions();
    let output_dimensions = plan.benchmark_dimensions(source_dimensions);
    append_encoding_arguments(&mut arguments, plan, output_dimensions)?;

    if matches!(&target.source, CaptureSource::WindowsGraphicsCapture { .. }) {
        push_args(&mut arguments, &["-vf"]);
        let mut filter = format!(
            "scale_d3d11=width={}:height={}:format=nv12",
            output_dimensions.0, output_dimensions.1
        );
        if plan.encoder == EncoderKind::Qsv {
            filter.push_str(",hwmap=derive_device=qsv:mode=read+direct");
        }
        arguments.push(filter.into());
        push_args(&mut arguments, &["-fps_mode", "cfr", "-r"]);
        arguments.push(plan.fps().to_string().into());
    } else {
        if let Some(source) = source_dimensions {
            append_fixed_scale_filter(&mut arguments, source, output_dimensions);
        } else {
            let maximum = plan.profile.spec().unwrap().max_dimensions;
            push_args(&mut arguments, &["-vf"]);
            arguments.push(
                format!(
                    "scale=w='min(iw,{})':h='min(ih,{})':force_original_aspect_ratio=decrease:force_divisible_by=2",
                    maximum.0, maximum.1
                )
                .into(),
            );
        }
        push_args(&mut arguments, &["-pix_fmt", "yuv420p"]);
    }
    if plan.codec == VideoCodec::Hevc {
        // `hvc1` is the interoperable MP4 sample entry expected by Apple media stacks.
        push_args(&mut arguments, &["-tag:v", "hvc1"]);
    }
    push_args(
        &mut arguments,
        &[
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-flush_packets",
            "1",
            "-movflags",
            FRAGMENTED_MP4_FLAGS,
            "-y",
        ],
    );
    arguments.push(output.as_os_str().to_owned());
    Ok(arguments)
}

pub(crate) fn append_windows_audio_arguments(arguments: &mut Vec<OsString>, audio: &AudioSource) {
    match audio {
        AudioSource::DirectShow(device) => {
            push_args(
                arguments,
                &["-thread_queue_size", "128", "-f", "dshow", "-i"],
            );
            arguments.push(format!("audio={device}").into());
        }
        AudioSource::Silent | AudioSource::AvFoundationCombined => {
            push_args(
                arguments,
                &[
                    "-re",
                    "-f",
                    "lavfi",
                    "-i",
                    "anullsrc=channel_layout=stereo:sample_rate=48000",
                ],
            );
        }
    }
}

fn append_fixed_scale_filter(
    arguments: &mut Vec<OsString>,
    source: (u32, u32),
    output: (u32, u32),
) {
    if source != output {
        push_args(arguments, &["-vf"]);
        arguments.push(format!("scale={}:{}", output.0, output.1).into());
    }
}

fn append_encoding_arguments(
    arguments: &mut Vec<OsString>,
    plan: RecordingPlan,
    dimensions: (u32, u32),
) -> Result<()> {
    let spec = plan
        .profile
        .spec()
        .context("recording profile is unresolved")?;
    plan.encoder
        .append_codec_arguments(plan.codec, spec.speed, arguments);

    let bitrate_kbps = plan.bitrate_kbps(dimensions);
    push_args(arguments, &["-b:v"]);
    arguments.push(format!("{bitrate_kbps}k").into());
    push_args(arguments, &["-maxrate"]);
    arguments.push(format!("{}k", bitrate_kbps * 3 / 2).into());
    push_args(arguments, &["-bufsize"]);
    arguments.push(format!("{}k", bitrate_kbps * 2).into());
    push_args(arguments, &["-g"]);
    arguments.push(
        plan.fps()
            .checked_mul(2)
            .context("recording FPS is too large")?
            .to_string()
            .into(),
    );
    Ok(())
}

fn fit_within(source: (u32, u32), maximum: (u32, u32)) -> (u32, u32) {
    let (source_width, source_height) = source;
    let (max_width, max_height) = maximum;
    if source_width <= max_width && source_height <= max_height {
        return (even_dimension(source_width), even_dimension(source_height));
    }

    let (width, height) = if u64::from(source_width) * u64::from(max_height)
        > u64::from(max_width) * u64::from(source_height)
    {
        (
            max_width,
            (u64::from(source_height) * u64::from(max_width) / u64::from(source_width)) as u32,
        )
    } else {
        (
            (u64::from(source_width) * u64::from(max_height) / u64::from(source_height)) as u32,
            max_height,
        )
    };
    (even_dimension(width), even_dimension(height))
}

fn even_dimension(value: u32) -> u32 {
    value.saturating_sub(value % 2).max(2)
}

pub(crate) fn push_args(target: &mut Vec<OsString>, values: &[&str]) {
    target.extend(values.iter().map(OsString::from));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> CaptureTarget {
        CaptureTarget {
            source: CaptureSource::WindowsGraphicsCapture {
                pid: 42,
                generation: 1,
                hwnd: 0x1234,
                width: 1920,
                height: 1080,
                dpi: 96,
                window_title: Some("League".to_owned()),
                adapter_index: 0,
                adapter_luid: 0x1122_3344_5566_7788,
                adapter_name: "Fixture GPU".to_owned(),
                output_name: "DISPLAY1".to_owned(),
            },
        }
    }

    fn plan(profile: RecordingProfile, codec: VideoCodec) -> RecordingPlan {
        plan_with_encoder(EncoderKind::Nvenc, profile, codec)
    }

    fn plan_with_encoder(
        encoder: EncoderKind,
        profile: RecordingProfile,
        codec: VideoCodec,
    ) -> RecordingPlan {
        RecordingPlan {
            encoder,
            codec,
            profile,
        }
    }

    #[cfg(target_os = "windows")]
    fn write_recording_fixture(path: &Path, stop_exit_code: u32) {
        let script = r#"@echo off
setlocal EnableDelayedExpansion
set "last="
for %%A in (%*) do set "last=%%~A"
> "!last!" echo fake-fragmented-mp4
>&2 echo queueback_capture abi=1 event=ready frame_pool_capacity=2 output_pool_capacity=8
>&2 echo queueback_capture abi=1 event=first_frame source_frames_surfaced=1 source_frames_superseded=0 first_qpc=100 latest_qpc=100
echo frame=2
echo total_size=20
echo out_time_us=16667
echo progress=continue
:wait
set "line="
set /p line=
if /I "!line!"=="q" (
  echo frame=3
  echo total_size=20
  echo out_time_us=33333
  echo progress=end
  >&2 echo queueback_capture abi=1 event=terminal source_frames_surfaced=2 source_frames_superseded=0 pool_recreations=0 first_qpc=100 latest_qpc=200
  exit /b __STOP_EXIT_CODE__
)
goto wait
"#
        .replace("__STOP_EXIT_CODE__", &stop_exit_code.to_string());
        std::fs::write(path, script).unwrap();
    }

    #[test]
    fn prefers_stereo_mix_over_wasapi() {
        let listing = r#"
            [dshow] "WASAPI loopback"
            [dshow] "Stereo Mix (Realtek)"
        "#;
        assert_eq!(
            choose_audio_device(listing).as_deref(),
            Some("Stereo Mix (Realtek)")
        );
    }

    #[test]
    fn ignores_alternative_device_ids() {
        let listing = r#"
            [dshow] "Stereo Mix"
            [dshow] "@device_cm_{something}"
        "#;
        assert_eq!(quoted_device_names(listing), vec!["Stereo Mix"]);
    }

    #[test]
    fn builds_gpu_resident_nvenc_window_capture_with_silent_audio() {
        let args = build_recording_arguments(
            &target(),
            plan(RecordingProfile::High, VideoCodec::H264),
            &AudioSource::Silent,
            Path::new("video.mp4"),
        )
        .unwrap();
        let args: Vec<String> = args
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["-c:v", "h264_nvenc"]));
        assert!(args.windows(2).any(|pair| pair == ["-b:v", "12000k"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-init_hw_device", "d3d11va=queueback:0"])
        );
        assert!(args.iter().any(|argument| {
            argument.starts_with("gfxcapture=hwnd=4660:")
                && argument.contains("capture_cursor=false")
                && argument.contains("display_border=false")
        }));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-vf", "scale_d3d11=width=1920:height=1080:format=nv12"])
        );
        assert!(args.windows(2).any(|pair| pair == ["-surfaces", "4"]));
        assert!(args.windows(2).any(|pair| pair == ["-bf", "0"]));
        assert!(!args.iter().any(|argument| argument == "gdigrab"));
        assert!(!args.iter().any(|argument| argument == "yuv420p"));
        assert!(!args.iter().any(|argument| argument == "1024"));
        assert!(
            args.iter()
                .any(|arg| arg == "anullsrc=channel_layout=stereo:sample_rate=48000")
        );
        assert!(args.windows(2).any(|pair| pair == ["-re", "-f"]));
        assert!(args.windows(2).any(|pair| pair == ["-g", "120"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-movflags", FRAGMENTED_MP4_FLAGS])
        );
        assert!(args.windows(2).any(|pair| pair == ["-flush_packets", "1"]));
        assert_eq!(args.last().map(String::as_str), Some("video.mp4"));
    }

    #[test]
    fn lower_profile_adds_aspect_preserving_scale_filter() {
        let args = build_recording_arguments(
            &target(),
            plan(RecordingProfile::VeryLow, VideoCodec::H264),
            &AudioSource::Silent,
            Path::new("video.mp4"),
        )
        .unwrap();
        let args: Vec<String> = args
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-vf", "scale_d3d11=width=1280:height=720:format=nv12"])
        );
        assert!(args.windows(2).any(|pair| pair == ["-g", "60"]));
    }

    #[test]
    fn gpu_native_amf_and_qsv_keep_the_vendor_boundary_after_d3d11() {
        let amf = build_recording_arguments(
            &target(),
            plan_with_encoder(EncoderKind::Amf, RecordingProfile::High, VideoCodec::H264),
            &AudioSource::Silent,
            Path::new("video.mp4"),
        )
        .unwrap();
        let amf = amf
            .iter()
            .map(|argument| argument.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(amf.windows(2).any(|pair| pair == ["-c:v", "h264_amf"]));
        assert!(amf.windows(2).any(|pair| pair == ["-async_depth", "4"]));
        assert!(!amf.iter().any(|argument| argument.contains("hwdownload")));

        let qsv = build_recording_arguments(
            &target(),
            plan_with_encoder(EncoderKind::Qsv, RecordingProfile::High, VideoCodec::H264),
            &AudioSource::Silent,
            Path::new("video.mp4"),
        )
        .unwrap();
        let qsv = qsv
            .iter()
            .map(|argument| argument.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(
            qsv.windows(2)
                .any(|pair| pair == ["-init_hw_device", "qsv=queueback_qsv@queueback"])
        );
        assert!(qsv.windows(2).any(|pair| pair == ["-c:v", "h264_qsv"]));
        assert!(
            qsv.iter()
                .any(|argument| argument.contains("hwmap=derive_device=qsv:mode=read+direct"))
        );
        assert!(!qsv.iter().any(|argument| argument.contains("hwdownload")));
    }

    #[test]
    fn hevc_uses_smaller_target_and_interoperable_mp4_tag() {
        let args = build_recording_arguments(
            &target(),
            plan(RecordingProfile::High, VideoCodec::Hevc),
            &AudioSource::Silent,
            Path::new("video.mp4"),
        )
        .unwrap();
        let args: Vec<String> = args
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["-c:v", "hevc_nvenc"]));
        assert!(args.windows(2).any(|pair| pair == ["-b:v", "8000k"]));
        assert!(args.windows(2).any(|pair| pair == ["-tag:v", "hvc1"]));
    }

    #[test]
    fn profile_scaling_never_upscales_and_preserves_aspect_ratio() {
        assert_eq!(fit_within((1024, 768), (1920, 1080)), (1024, 768));
        assert_eq!(fit_within((2560, 1080), (1920, 1080)), (1920, 810));
        assert_eq!(fit_within((3840, 2160), (2560, 1440)), (2560, 1440));
    }

    #[test]
    fn auto_codec_requires_a_positive_playback_probe_for_hevc() {
        assert_eq!(
            codec_candidates(CodecPreference::Auto, false),
            vec![VideoCodec::H264]
        );
        assert_eq!(
            codec_candidates(CodecPreference::Auto, true),
            vec![VideoCodec::Hevc, VideoCodec::H264]
        );
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn bounded_probe_kills_and_reaps_a_timed_out_child() {
        use std::os::windows::fs::OpenOptionsExt;

        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("hang-probe.ps1");
        let ready = directory.path().join("ready.txt");
        let lock = directory.path().join("child.lock");
        std::fs::write(
            &script,
            r#"param([string]$ReadyFile, [string]$LockFile)
$stream = [System.IO.File]::Open(
    $LockFile,
    [System.IO.FileMode]::OpenOrCreate,
    [System.IO.FileAccess]::ReadWrite,
    [System.IO.FileShare]::None
)
try {
    [System.IO.File]::WriteAllText($ReadyFile, [string]$PID)
    while ($true) { Start-Sleep -Milliseconds 100 }
} finally {
    $stream.Dispose()
}
"#,
        )
        .unwrap();

        let mut command = ffmpeg_command(Path::new("powershell.exe"));
        command
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .arg(&ready)
            .arg(&lock);
        let started = Instant::now();
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            run_bounded_probe(command, Duration::from_secs(1), "hanging test probe"),
        )
        .await
        .expect("the helper itself must remain bounded")
        .unwrap_err();

        assert!(error.to_string().contains("timed out"));
        assert!(ready.exists(), "the child never reached its wait loop");
        assert!(started.elapsed() < Duration::from_secs(5));
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&lock)
            .expect("the timed-out child still owns its exclusive lock");
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn recording_session_stops_with_one_mp4() {
        let directory = tempfile::tempdir().unwrap();
        let fake_ffmpeg = directory.path().join("fake-ffmpeg.cmd");
        std::fs::write(
            &fake_ffmpeg,
            r#"@echo off
setlocal EnableDelayedExpansion
set "last="
for %%A in (%*) do (
  if /I "%%~A"=="-version" exit /b 0
  if /I "%%~A"=="-encoders" (
    echo V....D h264_nvenc fake encoder
    exit /b 0
  )
  set "last=%%~A"
)
if "!last!"=="-" exit /b 0
> "!last!" echo fake-fragmented-mp4
>&2 echo queueback_capture abi=1 event=ready frame_pool_capacity=2 output_pool_capacity=8
>&2 echo queueback_capture abi=1 event=first_frame source_frames_surfaced=1 source_frames_superseded=0 first_qpc=100 latest_qpc=100
echo frame=2
echo total_size=20
echo out_time_us=16667
echo progress=continue
:wait
set "line="
set /p line=
if /I "!line!"=="q" (
  echo frame=3
  echo total_size=20
  echo out_time_us=33333
  echo progress=end
  >&2 echo queueback_capture abi=1 event=terminal source_frames_surfaced=2 source_frames_superseded=0 pool_recreations=0 first_qpc=100 latest_qpc=200
  exit /b 0
)
goto wait
"#,
        )
        .unwrap();

        let ffmpeg = Ffmpeg {
            path: fake_ffmpeg,
            runtime_id: "test-runtime".to_owned(),
        };
        let selected = ffmpeg
            .select_recording_plan(&RecordingConfig::default(), false, Some((1920, 1080)))
            .await
            .unwrap();
        assert_eq!(selected.encoder, EncoderKind::Nvenc);
        assert_eq!(selected.codec, VideoCodec::H264);
        assert_eq!(selected.profile, RecordingProfile::High);

        let bundle = directory.path().join("bundle");
        std::fs::create_dir(&bundle).unwrap();
        let session = ffmpeg
            .start_recording(bundle.clone(), &target(), selected, &AudioSource::Silent)
            .await
            .unwrap();
        assert!(!bundle.join(VIDEO_MP4).exists());
        assert!(bundle.join(VIDEO_PARTIAL_MP4).exists());

        let completed_directory = session.stop().await.unwrap();
        assert_eq!(completed_directory, bundle);
        assert!(bundle.join(VIDEO_MP4).exists());
        assert!(!bundle.join(VIDEO_PARTIAL_MP4).exists());
        let completed_files: Vec<_> = std::fs::read_dir(&bundle)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(completed_files, [std::ffi::OsString::from(VIDEO_MP4)]);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn failed_ffmpeg_stop_never_publishes_the_partial_recording() {
        let directory = tempfile::tempdir().unwrap();
        let fake_ffmpeg = directory.path().join("failing-stop-ffmpeg.cmd");
        write_recording_fixture(&fake_ffmpeg, 7);
        let bundle = directory.path().join("bundle");
        std::fs::create_dir(&bundle).unwrap();
        let ffmpeg = Ffmpeg {
            path: fake_ffmpeg,
            runtime_id: "test-runtime".to_owned(),
        };
        let session = ffmpeg
            .start_recording(
                bundle.clone(),
                &target(),
                plan(RecordingProfile::High, VideoCodec::H264),
                &AudioSource::Silent,
            )
            .await
            .unwrap();

        let error = session.stop().await.unwrap_err();
        assert!(error.to_string().contains("partial MP4 preserved"));
        assert!(!bundle.join(VIDEO_MP4).exists());
        assert!(bundle.join(VIDEO_PARTIAL_MP4).exists());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn external_failure_never_publishes_an_otherwise_valid_recording() {
        let directory = tempfile::tempdir().unwrap();
        let fake_ffmpeg = directory.path().join("external-failure-ffmpeg.cmd");
        write_recording_fixture(&fake_ffmpeg, 0);
        let bundle = directory.path().join("bundle");
        std::fs::create_dir(&bundle).unwrap();
        let ffmpeg = Ffmpeg {
            path: fake_ffmpeg,
            runtime_id: "test-runtime".to_owned(),
        };
        let session = ffmpeg
            .start_recording(
                bundle.clone(),
                &target(),
                plan(RecordingProfile::High, VideoCodec::H264),
                &AudioSource::Silent,
            )
            .await
            .unwrap();

        let error = session
            .stop_with_failure("fixture watchdog failure".to_owned())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("fixture watchdog failure"));
        assert!(!bundle.join(VIDEO_MP4).exists());
        assert!(bundle.join(VIDEO_PARTIAL_MP4).exists());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn startup_cancellation_terminates_the_child_before_the_readiness_deadline() {
        let directory = tempfile::tempdir().unwrap();
        let fake_ffmpeg = directory.path().join("never-ready-ffmpeg.cmd");
        std::fs::write(
            &fake_ffmpeg,
            r#"@echo off
setlocal EnableDelayedExpansion
set "last="
for %%A in (%*) do set "last=%%~A"
> "!last!" echo partial-fragment
:wait
set "line="
set /p line=
goto wait
"#,
        )
        .unwrap();
        let bundle = directory.path().join("bundle");
        std::fs::create_dir(&bundle).unwrap();
        let ffmpeg = Ffmpeg {
            path: fake_ffmpeg,
            runtime_id: "test-runtime".to_owned(),
        };
        let (cancel, receiver) = watch::channel(false);
        let target = target();
        let started = Instant::now();
        let task = tokio::spawn(async move {
            ffmpeg
                .start_recording_candidate(
                    bundle,
                    &target,
                    plan(RecordingProfile::High, VideoCodec::H264),
                    &AudioSource::Silent,
                    RecordingCandidateStart::new(0, Duration::from_secs(60), receiver),
                )
                .await
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(4), task)
            .await
            .expect("cancellation must remain bounded")
            .unwrap();
        let error = match result {
            Ok(_) => panic!("a cancelled startup unexpectedly became ready"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("cancelled"));
        assert!(started.elapsed() < Duration::from_secs(4));
    }
}
