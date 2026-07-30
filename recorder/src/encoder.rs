use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::config::{RecordingConfig, parse_resolution};
use crate::platform::{CaptureSource, CaptureTarget};
use crate::storage::VIDEO_MP4;

const ENCODER_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const FFMPEG_STARTUP_GRACE: Duration = Duration::from_millis(900);
const FFMPEG_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const FRAGMENTED_MP4_FLAGS: &str = "+frag_keyframe+empty_moov+default_base_moof";

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

    pub fn codec_name(self) -> &'static str {
        match self {
            Self::Nvenc => "h264_nvenc",
            Self::Amf => "h264_amf",
            Self::Qsv => "h264_qsv",
            Self::Videotoolbox => "h264_videotoolbox",
        }
    }

    fn codec_arguments(self) -> &'static [&'static str] {
        match self {
            Self::Nvenc => &["-c:v", "h264_nvenc", "-preset", "p4"],
            Self::Amf => &["-c:v", "h264_amf", "-quality", "quality"],
            Self::Qsv => &["-c:v", "h264_qsv", "-preset", "medium"],
            Self::Videotoolbox => &["-c:v", "h264_videotoolbox"],
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

    pub async fn select_hardware_encoder(&self) -> Result<EncoderKind> {
        let advertised = self.advertised_encoders().await?;
        let order = preferred_encoders();

        for encoder in order.iter().copied() {
            if !advertised.contains(encoder.codec_name()) {
                debug!(
                    encoder = encoder.codec_name(),
                    "ffmpeg does not advertise encoder"
                );
                continue;
            }
            match self.probe_encoder(encoder).await {
                Ok(()) => {
                    info!(
                        encoder = encoder.codec_name(),
                        "selected working hardware encoder"
                    );
                    return Ok(encoder);
                }
                Err(error) => {
                    warn!(
                        encoder = encoder.codec_name(),
                        %error,
                        "hardware encoder probe failed"
                    );
                }
            }
        }

        bail!(
            "no working hardware H.264 encoder was found; software encoding is intentionally unsupported"
        )
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

            let output = Command::new(&self.path)
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
        let output = Command::new(&self.path)
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

    async fn probe_encoder(&self, encoder: EncoderKind) -> Result<()> {
        let mut command = Command::new(&self.path);
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=640x360:r=1",
            "-frames:v",
            "1",
            "-an",
            "-c:v",
            encoder.codec_name(),
            "-f",
            "null",
            "-",
        ]);
        let output = tokio::time::timeout(ENCODER_PROBE_TIMEOUT, command.output())
            .await
            .context("hardware encoder probe timed out")?
            .context("failed to launch hardware encoder probe")?;
        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(())
    }

    pub async fn start_recording(
        &self,
        directory: PathBuf,
        target: &CaptureTarget,
        recording: &RecordingConfig,
        encoder: EncoderKind,
        audio: &AudioSource,
    ) -> Result<RecordingSession> {
        let output = directory.join(VIDEO_MP4);
        let arguments = build_recording_arguments(target, recording, encoder, audio, &output)?;
        info!(
            ffmpeg = %self.path.display(),
            target = %target.description(),
            audio = %audio.description(),
            encoder = encoder.codec_name(),
            "starting recording"
        );
        debug!(arguments = ?arguments, "ffmpeg recording arguments");

        let mut command = Command::new(&self.path);
        command
            .args(&arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start {}", self.path.display()))?;

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
        })
    }
}

pub struct RecordingSession {
    directory: PathBuf,
    child: Child,
    stderr_task: Option<JoinHandle<()>>,
}

impl RecordingSession {
    pub fn directory(&self) -> &Path {
        &self.directory
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
    Command::new(path)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
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
    recording: &RecordingConfig,
    encoder: EncoderKind,
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
            arguments.push(recording.fps.to_string().into());
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
            arguments.push(recording.fps.to_string().into());
            push_args(&mut arguments, &["-i"]);
            arguments.push(input.into());
            push_args(&mut arguments, &["-map", "0:v:0", "-map", "0:a:0"]);
        }
    }

    for argument in encoder.codec_arguments() {
        arguments.push((*argument).into());
    }
    push_args(&mut arguments, &["-b:v"]);
    arguments.push(format!("{}k", recording.bitrate_kbps).into());
    push_args(&mut arguments, &["-maxrate"]);
    arguments.push(format!("{}k", recording.bitrate_kbps * 3 / 2).into());
    push_args(&mut arguments, &["-bufsize"]);
    arguments.push(format!("{}k", recording.bitrate_kbps * 2).into());
    push_args(&mut arguments, &["-g"]);
    arguments.push(
        recording
            .fps
            .checked_mul(2)
            .context("recording FPS is too large")?
            .to_string()
            .into(),
    );

    if let Some((width, height)) = parse_resolution(&recording.resolution)? {
        push_args(&mut arguments, &["-vf"]);
        arguments.push(format!("scale={width}:{height}").into());
    }

    push_args(
        &mut arguments,
        &[
            "-pix_fmt",
            "yuv420p",
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
        let recording = RecordingConfig::default();
        let args = build_recording_arguments(
            &target(),
            &recording,
            EncoderKind::Nvenc,
            &AudioSource::Silent,
            Path::new("video.mp4"),
        )
        .unwrap();
        let args: Vec<String> = args
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["-c:v", "h264_nvenc"]));
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
    fn fixed_resolution_adds_scale_filter() {
        let recording = RecordingConfig {
            resolution: "1920x1080".to_owned(),
            ..RecordingConfig::default()
        };
        let args = build_recording_arguments(
            &target(),
            &recording,
            EncoderKind::Nvenc,
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
                .any(|pair| pair == ["-vf", "scale=1920:1080"])
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
        assert_eq!(
            ffmpeg.select_hardware_encoder().await.unwrap(),
            EncoderKind::Nvenc
        );

        let bundle = directory.path().join("bundle");
        std::fs::create_dir(&bundle).unwrap();
        let session = ffmpeg
            .start_recording(
                bundle.clone(),
                &target(),
                &RecordingConfig::default(),
                EncoderKind::Nvenc,
                &AudioSource::Silent,
            )
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
