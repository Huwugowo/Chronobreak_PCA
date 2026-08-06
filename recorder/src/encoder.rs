use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::config::{CodecPreference, RecordingConfig, RecordingProfile};
use crate::platform::{CaptureSource, CaptureTarget};
use crate::storage::VIDEO_MP4;

const ENCODER_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const PROFILE_BENCHMARK_DURATION: Duration = Duration::from_secs(3);
const FFMPEG_STARTUP_GRACE: Duration = Duration::from_millis(900);
const FFMPEG_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const FRAGMENTED_MP4_FLAGS: &str = "+frag_keyframe+empty_moov+default_base_moof";
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
                push_args(arguments, &["-preset", preset]);
            }
            Self::Amf => {
                let quality = match speed {
                    EncoderSpeed::Speed => "speed",
                    EncoderSpeed::Balanced => "balanced",
                    EncoderSpeed::Quality => "quality",
                };
                push_args(arguments, &["-quality", quality]);
            }
            Self::Qsv => {
                let preset = match speed {
                    EncoderSpeed::Speed => "veryfast",
                    EncoderSpeed::Balanced => "medium",
                    EncoderSpeed::Quality => "slow",
                };
                push_args(arguments, &["-preset", preset]);
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

#[derive(Debug)]
pub struct Ffmpeg {
    path: PathBuf,
}

impl Ffmpeg {
    pub async fn resolve() -> Result<Self> {
        let candidates = ffmpeg_candidates()?;
        for path in candidates {
            if command_works(&path).await {
                return Ok(Self { path });
            }
        }
        bail!("ffmpeg was not found; set LEAGUE_REPLAY_FFMPEG or install ffmpeg for Phase 1")
    }

    pub fn path(&self) -> &Path {
        &self.path
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

            let output = ffmpeg_command(&self.path)
                .args([
                    "-hide_banner",
                    "-list_devices",
                    "true",
                    "-f",
                    "dshow",
                    "-i",
                    "dummy",
                ])
                .output()
                .await;

            if let Ok(output) = output {
                let listing = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                if let Some(device) = choose_audio_device(&listing) {
                    return AudioSource::DirectShow(device);
                }
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
        let output = ffmpeg_command(&self.path)
            .args(["-hide_banner", "-encoders"])
            .output()
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
        let output = tokio::time::timeout(ENCODER_PROBE_TIMEOUT, command.output())
            .await
            .context("hardware recording plan probe timed out")?
            .context("failed to launch hardware recording plan probe")?;
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
        let output = tokio::time::timeout(
            ENCODER_PROBE_TIMEOUT,
            ffmpeg_command(&self.path).args(arguments).output(),
        )
        .await
        .context("recording profile benchmark timed out")?
        .context("failed to launch recording profile benchmark")?;
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
        let output = directory.join(VIDEO_MP4);
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
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start {}", self.path.display()))?;
        let video_started_at = Instant::now();
        let recorded_at = SystemTime::now();

        let stderr = child.stderr.take();
        let stderr_task = tokio::spawn(async move {
            if let Some(stderr) = stderr {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        debug!(target: "ffmpeg", "{line}");
                    }
                }
            }
        });

        tokio::time::sleep(FFMPEG_STARTUP_GRACE).await;
        if let Some(status) = child.try_wait().context("failed to inspect ffmpeg")? {
            let _ = stderr_task.await;
            bail!("ffmpeg exited during startup with status {status}");
        }

        Ok(RecordingSession {
            directory,
            child,
            stderr_task: Some(stderr_task),
            video_started_at,
            recorded_at,
        })
    }
}

pub struct RecordingSession {
    directory: PathBuf,
    child: Child,
    stderr_task: Option<JoinHandle<()>>,
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

    pub fn has_exited(&mut self) -> Result<bool> {
        Ok(self
            .child
            .try_wait()
            .context("failed to inspect ffmpeg process")?
            .is_some())
    }

    pub async fn stop(mut self) -> Result<PathBuf> {
        let mut exit_status = self.child.try_wait().context("failed to inspect ffmpeg")?;
        if exit_status.is_none() {
            if let Some(mut stdin) = self.child.stdin.take() {
                if let Err(error) = stdin.write_all(b"q\n").await {
                    warn!(%error, "failed to send graceful stop to ffmpeg");
                }
                let _ = stdin.flush().await;
            }

            match tokio::time::timeout(FFMPEG_STOP_TIMEOUT, self.child.wait()).await {
                Ok(Ok(status)) => {
                    info!(%status, "ffmpeg stopped");
                    exit_status = Some(status);
                }
                Ok(Err(error)) => {
                    warn!(%error, "failed while waiting for ffmpeg");
                }
                Err(_) => {
                    warn!("ffmpeg did not stop within 10 seconds; forcing termination");
                    let _ = self.child.kill().await;
                    exit_status = self.child.wait().await.ok();
                }
            }
        }

        if let Some(task) = self.stderr_task.take() {
            let _ = task.await;
        }

        let output = self.directory.join(VIDEO_MP4);
        let output_metadata = tokio::fs::metadata(&output)
            .await
            .with_context(|| format!("recording is missing {}", output.display()))?;
        if output_metadata.len() == 0 {
            bail!("recording {} is empty", output.display());
        }

        if let Some(status) = exit_status
            && !status.success()
        {
            bail!("ffmpeg exited with {status}; the partial MP4 was preserved");
        }
        Ok(self.directory)
    }
}

fn ffmpeg_candidates() -> Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("LEAGUE_REPLAY_FFMPEG") {
        candidates.push(PathBuf::from(path));
    }
    let executable = env::current_exe().context("could not resolve recorder executable path")?;
    if let Some(directory) = executable.parent() {
        #[cfg(target_os = "windows")]
        candidates.push(
            directory
                .join("resources")
                .join("ffmpeg")
                .join("ffmpeg.exe"),
        );
        #[cfg(not(target_os = "windows"))]
        candidates.push(directory.join("resources").join("ffmpeg").join("ffmpeg"));
    }
    candidates.push(PathBuf::from(if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }));
    Ok(candidates)
}

async fn command_works(path: &Path) -> bool {
    ffmpeg_command(path)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
}

fn ffmpeg_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    command
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
    push_args(&mut arguments, &["-hide_banner", "-loglevel", "warning"]);

    match &target.source {
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

            match audio {
                AudioSource::DirectShow(device) => {
                    push_args(
                        &mut arguments,
                        &["-thread_queue_size", "1024", "-f", "dshow", "-i"],
                    );
                    arguments.push(format!("audio={device}").into());
                }
                AudioSource::Silent | AudioSource::AvFoundationCombined => {
                    push_args(
                        &mut arguments,
                        &[
                            "-f",
                            "lavfi",
                            "-i",
                            "anullsrc=channel_layout=stereo:sample_rate=48000",
                        ],
                    );
                }
            }
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
            "-movflags",
            FRAGMENTED_MP4_FLAGS,
            "-y",
        ],
    );
    arguments.push(output.as_os_str().to_owned());
    Ok(arguments)
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

fn push_args(target: &mut Vec<OsString>, values: &[&str]) {
    target.extend(values.iter().map(OsString::from));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> CaptureTarget {
        CaptureTarget {
            source: CaptureSource::DesktopRegion {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
                window_title: Some("League".to_owned()),
            },
        }
    }

    fn plan(profile: RecordingProfile, codec: VideoCodec) -> RecordingPlan {
        RecordingPlan {
            encoder: EncoderKind::Nvenc,
            codec,
            profile,
        }
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
    fn builds_nvenc_region_capture_with_silent_audio() {
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
        assert!(args.windows(2).any(|pair| pair == ["-offset_x", "-1920"]));
        assert!(
            args.iter()
                .any(|arg| arg == "anullsrc=channel_layout=stereo:sample_rate=48000")
        );
        assert!(args.windows(2).any(|pair| pair == ["-g", "120"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-movflags", FRAGMENTED_MP4_FLAGS])
        );
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
                .any(|pair| pair == ["-vf", "scale=1280:720"])
        );
        assert!(args.windows(2).any(|pair| pair == ["-g", "60"]));
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
:wait
set "line="
set /p line=
if /I "!line!"=="q" exit /b 0
goto wait
"#,
        )
        .unwrap();

        let ffmpeg = Ffmpeg { path: fake_ffmpeg };
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
        assert!(bundle.join(VIDEO_MP4).exists());

        let completed_directory = session.stop().await.unwrap();
        assert_eq!(completed_directory, bundle);
        assert!(bundle.join(VIDEO_MP4).exists());
        let completed_files: Vec<_> = std::fs::read_dir(&bundle)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(completed_files, [std::ffi::OsString::from(VIDEO_MP4)]);
    }
}
