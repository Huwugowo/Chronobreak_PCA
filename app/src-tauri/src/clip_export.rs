use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

use crate::{library, music};

const MINIMUM_CLIP_MS: u64 = 5_000;
const DISCORD_LIMIT_BYTES: u64 = 10_000_000;
const DISCORD_TARGET_BYTES: u64 = 9_400_000;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClipExportPreset {
    Discord,
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ClipMusicSource {
    None,
    Builtin { filename: String },
    File { path: String },
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ClipExportRequest {
    pub game_timestamp: String,
    pub clip_start_ms: u64,
    pub clip_end_ms: u64,
    pub preset: ClipExportPreset,
    pub vertical_focus: f64,
    pub vertical_position: f64,
    pub music: ClipMusicSource,
    pub game_audio_volume: f64,
    pub music_volume: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClipExportResult {
    pub filename: String,
    pub output_path: String,
    pub thumbnail_path: String,
    pub elapsed_ms: u64,
    pub file_size_bytes: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExportStage {
    Encoding,
    Thumbnail,
    Complete,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct ClipExportProgress {
    pub stage: ExportStage,
    pub percent: u8,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SourceMetadata {
    duration_ms: u64,
    encoder_used: String,
    recording_fps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum H264Encoder {
    Nvenc,
    Amf,
    Qsv,
    VideoToolbox,
    Software,
}

#[derive(Debug, Clone)]
struct ExportProfile {
    video_filter: String,
    video_bitrate_kbps: u64,
    audio_bitrate_kbps: u64,
    output_fps: u32,
}

struct ExportPaths {
    filename: String,
    video: PathBuf,
    thumbnail: PathBuf,
    partial_video: PathBuf,
    partial_thumbnail: PathBuf,
}

pub async fn export(
    output_directory: &Path,
    music_directory: &Path,
    request: ClipExportRequest,
    progress: Channel<ClipExportProgress>,
) -> Result<ClipExportResult> {
    validate_request(&request)?;
    let started_at = Instant::now();
    let game_directory = output_directory.join("games").join(&request.game_timestamp);
    let source_video = game_directory.join("video.mp4");
    if !source_video.is_file() {
        bail!("recording video is unavailable")
    }
    let metadata: SourceMetadata = serde_json::from_slice(
        &fs::read(game_directory.join("metadata.json"))
            .context("failed to read recording metadata")?,
    )
    .context("failed to parse recording metadata")?;
    if metadata.duration_ms > 0 && request.clip_end_ms > metadata.duration_ms.saturating_add(1_000)
    {
        bail!("clip endpoint exceeds the recording duration")
    }

    let music_path = resolve_music(music_directory, &request.music)?;
    let clips_directory = output_directory.join("clips");
    fs::create_dir_all(&clips_directory)
        .with_context(|| format!("failed to create {}", clips_directory.display()))?;
    let paths = unique_paths(&clips_directory, &request.game_timestamp)?;
    let source_fps = metadata.recording_fps.clamp(24, 60);
    let mut profile = export_profile(&request, source_fps, None)?;
    let encoders = encoder_candidates(&metadata.encoder_used);
    let result = async {
        encode_with_fallback(
            &source_video,
            music_path.as_deref(),
            &request,
            &profile,
            &encoders,
            &paths.partial_video,
            &progress,
        )
        .await?;

        if request.preset == ClipExportPreset::Discord {
            let first_size = fs::metadata(&paths.partial_video)
                .context("failed to inspect exported clip")?
                .len();
            if first_size >= DISCORD_LIMIT_BYTES {
                let adjusted = ((profile.video_bitrate_kbps as f64)
                    * (DISCORD_TARGET_BYTES as f64 / first_size as f64)
                    * 0.94)
                    .floor()
                    .max(250.0) as u64;
                profile = export_profile(&request, source_fps, Some(adjusted))?;
                encode_with_fallback(
                    &source_video,
                    music_path.as_deref(),
                    &request,
                    &profile,
                    &encoders,
                    &paths.partial_video,
                    &progress,
                )
                .await?;
            }
            let final_size = fs::metadata(&paths.partial_video)
                .context("failed to inspect exported clip")?
                .len();
            if final_size >= DISCORD_LIMIT_BYTES {
                bail!("Discord export could not be kept below 10 MB; shorten the clip")
            }
        }

        progress
            .send(ClipExportProgress {
                stage: ExportStage::Thumbnail,
                percent: 96,
            })
            .ok();
        generate_thumbnail(&paths.partial_video, &paths.partial_thumbnail).await?;
        fs::rename(&paths.partial_thumbnail, &paths.thumbnail)
            .context("failed to finalize clip thumbnail")?;
        if let Err(error) = fs::rename(&paths.partial_video, &paths.video) {
            fs::remove_file(&paths.thumbnail).ok();
            return Err(error).context("failed to finalize exported clip");
        }
        let file_size_bytes = fs::metadata(&paths.video)
            .context("failed to inspect completed clip")?
            .len();
        progress
            .send(ClipExportProgress {
                stage: ExportStage::Complete,
                percent: 100,
            })
            .ok();
        Ok(ClipExportResult {
            filename: paths.filename.clone(),
            output_path: paths.video.to_string_lossy().into_owned(),
            thumbnail_path: paths.thumbnail.to_string_lossy().into_owned(),
            elapsed_ms: u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            file_size_bytes,
        })
    }
    .await;

    if result.is_err() {
        fs::remove_file(&paths.partial_video).ok();
        fs::remove_file(&paths.partial_thumbnail).ok();
    }
    result
}

fn validate_request(request: &ClipExportRequest) -> Result<()> {
    if !library::valid_timestamp(&request.game_timestamp) {
        bail!("invalid game timestamp")
    }
    if request.clip_end_ms <= request.clip_start_ms
        || request.clip_end_ms - request.clip_start_ms < MINIMUM_CLIP_MS
    {
        bail!("clips must be at least five seconds long")
    }
    for (label, value) in [
        ("vertical focus", request.vertical_focus),
        ("vertical position", request.vertical_position),
        ("game audio volume", request.game_audio_volume),
        ("music volume", request.music_volume),
    ] {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            bail!("{label} must be between zero and one")
        }
    }
    Ok(())
}

fn resolve_music(directory: &Path, source: &ClipMusicSource) -> Result<Option<PathBuf>> {
    match source {
        ClipMusicSource::None => Ok(None),
        ClipMusicSource::Builtin { filename } => music::resolve(directory, filename).map(Some),
        ClipMusicSource::File { path } => {
            let path = PathBuf::from(path);
            if !path.is_absolute() || !path.is_file() {
                bail!("imported music file is unavailable")
            }
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !matches!(extension.as_str(), "mp3" | "wav") {
                bail!("imported music must be an MP3 or WAV file")
            }
            Ok(Some(path))
        }
    }
}

fn unique_paths(directory: &Path, game_timestamp: &str) -> Result<ExportPaths> {
    let mut clip_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    loop {
        let filename = format!("{game_timestamp}_{clip_timestamp}");
        let video = directory.join(format!("{filename}.mp4"));
        let thumbnail = directory.join(format!("{filename}.jpg"));
        if !video.exists() && !thumbnail.exists() {
            return Ok(ExportPaths {
                partial_video: directory.join(format!("{filename}.part.mp4")),
                partial_thumbnail: directory.join(format!("{filename}.part.jpg")),
                filename,
                video,
                thumbnail,
            });
        }
        clip_timestamp = clip_timestamp.saturating_add(1);
    }
}

fn export_profile(
    request: &ClipExportRequest,
    source_fps: u32,
    bitrate_override: Option<u64>,
) -> Result<ExportProfile> {
    let duration_ms = request.clip_end_ms - request.clip_start_ms;
    match request.preset {
        ClipExportPreset::Horizontal => Ok(ExportProfile {
            video_filter: "[0:v]scale=w='trunc(min(1920,iw)/2)*2':h=-2:flags=lanczos,setsar=1,format=yuv420p[vout]".to_owned(),
            video_bitrate_kbps: 12_000,
            audio_bitrate_kbps: 192,
            output_fps: source_fps,
        }),
        ClipExportPreset::Vertical => Ok(ExportProfile {
            video_filter: vertical_filter(request.vertical_focus, request.vertical_position),
            video_bitrate_kbps: 12_000,
            audio_bitrate_kbps: 192,
            output_fps: source_fps,
        }),
        ClipExportPreset::Discord => {
            let seconds = duration_ms as f64 / 1_000.0;
            let audio_bitrate_kbps = 128_u64;
            let available_total_kbps =
                ((DISCORD_TARGET_BYTES as f64 * 8.0 * 0.90) / seconds / 1_000.0).floor()
                    as u64;
            if available_total_kbps <= audio_bitrate_kbps + 250 {
                bail!("clip is too long to remain publishable below 10 MB")
            }
            let video_bitrate_kbps = bitrate_override
                .unwrap_or(available_total_kbps - audio_bitrate_kbps)
                .clamp(250, 5_000);
            let (width, height, output_fps) = if video_bitrate_kbps >= 2_500 {
                (1_280, 720, source_fps)
            } else if video_bitrate_kbps >= 1_200 {
                (1_280, 720, 30)
            } else if video_bitrate_kbps >= 650 {
                (960, 540, 30)
            } else {
                (854, 480, 30)
            };
            let fps_filter = if output_fps < source_fps {
                format!(",fps={output_fps}")
            } else {
                String::new()
            };
            Ok(ExportProfile {
                video_filter: format!(
                    "[0:v]scale={width}:{height}:force_original_aspect_ratio=decrease:flags=lanczos,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2{fps_filter},setsar=1,format=yuv420p[vout]"
                ),
                video_bitrate_kbps,
                audio_bitrate_kbps,
                output_fps,
            })
        }
    }
}

fn vertical_filter(focus: f64, position: f64) -> String {
    format!(
        "[0:v]split=2[bg][fg];\
         [bg]scale=270:480:force_original_aspect_ratio=increase:flags=bilinear,crop=270:480,boxblur=12:2,scale=1080:1920:flags=bilinear,eq=brightness=-0.16:saturation=0.82[back];\
         [fg]crop=w='trunc((iw-{focus:.4}*(iw-ih))/2)*2':h=ih:x='(iw-ow)*{position:.4}':y=0,scale=1080:-2:flags=lanczos,setsar=1[front];\
         [back][front]overlay=x=(W-w)/2:y=(H-h)/2,format=yuv420p[vout]"
    )
}

fn encoder_candidates(recording_encoder: &str) -> Vec<H264Encoder> {
    let preferred = match recording_encoder {
        "nvenc" => Some(H264Encoder::Nvenc),
        "amf" => Some(H264Encoder::Amf),
        "qsv" => Some(H264Encoder::Qsv),
        "videotoolbox" => Some(H264Encoder::VideoToolbox),
        _ => None,
    };
    preferred
        .into_iter()
        .chain(std::iter::once(H264Encoder::Software))
        .collect()
}

async fn encode_with_fallback(
    source_video: &Path,
    music_path: Option<&Path>,
    request: &ClipExportRequest,
    profile: &ExportProfile,
    encoders: &[H264Encoder],
    output: &Path,
    progress: &Channel<ClipExportProgress>,
) -> Result<()> {
    let mut errors = Vec::new();
    for encoder in encoders {
        fs::remove_file(output).ok();
        let arguments =
            ffmpeg_arguments(source_video, music_path, request, profile, *encoder, output);
        match run_ffmpeg(
            arguments,
            request.clip_end_ms - request.clip_start_ms,
            progress,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("{}: {error}", encoder_name(*encoder))),
        }
    }
    bail!("all H.264 encoders failed: {}", errors.join(" | "))
}

fn ffmpeg_arguments(
    source_video: &Path,
    music_path: Option<&Path>,
    request: &ClipExportRequest,
    profile: &ExportProfile,
    encoder: H264Encoder,
    output: &Path,
) -> Vec<OsString> {
    let duration_ms = request.clip_end_ms - request.clip_start_ms;
    let duration = seconds(duration_ms);
    let mut arguments = strings(&[
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-ss",
        &seconds(request.clip_start_ms),
        "-i",
    ]);
    arguments.push(source_video.as_os_str().to_owned());
    if let Some(path) = music_path {
        arguments.extend(strings(&["-stream_loop", "-1", "-i"]));
        arguments.push(path.as_os_str().to_owned());
    }

    let audio_filter = if music_path.is_some() {
        format!(
            "[0:a]atrim=duration={duration},asetpts=PTS-STARTPTS,volume={:.3}[game];\
             [1:a]atrim=duration={duration},asetpts=PTS-STARTPTS,volume={:.3}[music];\
             [game][music]amix=inputs=2:duration=first:dropout_transition=0[aout]",
            request.game_audio_volume, request.music_volume
        )
    } else {
        format!(
            "[0:a]atrim=duration={duration},asetpts=PTS-STARTPTS,volume={:.3}[aout]",
            request.game_audio_volume
        )
    };
    arguments.extend(strings(&[
        "-t",
        &duration,
        "-filter_complex",
        &format!("{};{}", profile.video_filter, audio_filter),
        "-map",
        "[vout]",
        "-map",
        "[aout]",
    ]));
    append_encoder_arguments(encoder, &mut arguments);
    arguments.extend(strings(&[
        "-b:v",
        &format!("{}k", profile.video_bitrate_kbps),
        "-maxrate",
        &format!("{}k", profile.video_bitrate_kbps),
        "-bufsize",
        &format!("{}k", profile.video_bitrate_kbps * 2),
        "-g",
        &(profile.output_fps * 2).to_string(),
        "-pix_fmt",
        "yuv420p",
        "-profile:v",
        "high",
        "-tag:v",
        "avc1",
        "-c:a",
        "aac",
        "-b:a",
        &format!("{}k", profile.audio_bitrate_kbps),
        "-ar",
        "48000",
        "-ac",
        "2",
        "-movflags",
        "+faststart",
        "-progress",
        "pipe:1",
        "-nostats",
    ]));
    arguments.push(output.as_os_str().to_owned());
    arguments
}

fn append_encoder_arguments(encoder: H264Encoder, arguments: &mut Vec<OsString>) {
    match encoder {
        H264Encoder::Nvenc => arguments.extend(strings(&["-c:v", "h264_nvenc", "-preset", "p5"])),
        H264Encoder::Amf => {
            arguments.extend(strings(&["-c:v", "h264_amf", "-quality", "balanced"]))
        }
        H264Encoder::Qsv => arguments.extend(strings(&["-c:v", "h264_qsv", "-preset", "medium"])),
        H264Encoder::VideoToolbox => arguments.extend(strings(&["-c:v", "h264_videotoolbox"])),
        H264Encoder::Software => {
            arguments.extend(strings(&["-c:v", "libx264", "-preset", "veryfast"]))
        }
    }
}

fn encoder_name(encoder: H264Encoder) -> &'static str {
    match encoder {
        H264Encoder::Nvenc => "NVENC",
        H264Encoder::Amf => "AMF",
        H264Encoder::Qsv => "QSV",
        H264Encoder::VideoToolbox => "VideoToolbox",
        H264Encoder::Software => "libx264",
    }
}

async fn run_ffmpeg(
    arguments: Vec<OsString>,
    duration_ms: u64,
    progress: &Channel<ClipExportProgress>,
) -> Result<()> {
    let mut command = Command::new("ffmpeg");
    command
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    let mut child = command.spawn().context("failed to start ffmpeg")?;
    let stdout = child
        .stdout
        .take()
        .context("failed to read ffmpeg progress")?;
    let mut stderr = child
        .stderr
        .take()
        .context("failed to read ffmpeg errors")?;
    let stderr_task = tokio::spawn(async move {
        let mut message = String::new();
        stderr.read_to_string(&mut message).await.ok();
        message
    });
    let mut lines = BufReader::new(stdout).lines();
    let mut last_percent = 0_u8;
    while let Some(line) = lines
        .next_line()
        .await
        .context("failed to read ffmpeg progress")?
    {
        let Some(value) = line.strip_prefix("out_time_us=") else {
            continue;
        };
        let Ok(microseconds) = value.parse::<u64>() else {
            continue;
        };
        let percent = ((microseconds as f64 / 1_000.0 / duration_ms as f64) * 94.0)
            .floor()
            .clamp(0.0, 94.0) as u8;
        if percent > last_percent {
            last_percent = percent;
            progress
                .send(ClipExportProgress {
                    stage: ExportStage::Encoding,
                    percent,
                })
                .ok();
        }
    }
    let status = child.wait().await.context("failed to wait for ffmpeg")?;
    let stderr = stderr_task.await.unwrap_or_default();
    if !status.success() {
        bail!("{}", concise_error(&stderr))
    }
    Ok(())
}

async fn generate_thumbnail(video: &Path, output: &Path) -> Result<()> {
    let mut command = Command::new("ffmpeg");
    command
        .args(strings(&[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-ss",
            "0.1",
            "-i",
        ]))
        .arg(video)
        .args(strings(&["-frames:v", "1", "-q:v", "3"]))
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    let output_result = command
        .output()
        .await
        .context("failed to start ffmpeg thumbnail generation")?;
    if !output_result.status.success() {
        bail!(
            "thumbnail generation failed: {}",
            concise_error(&String::from_utf8_lossy(&output_result.stderr))
        )
    }
    Ok(())
}

fn concise_error(message: &str) -> String {
    message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("ffmpeg exited without an error message")
        .trim()
        .chars()
        .take(320)
        .collect()
}

fn seconds(milliseconds: u64) -> String {
    format!("{:.3}", milliseconds as f64 / 1_000.0)
}

fn strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(preset: ClipExportPreset, duration_ms: u64) -> ClipExportRequest {
        ClipExportRequest {
            game_timestamp: "1786000000".to_owned(),
            clip_start_ms: 10_000,
            clip_end_ms: 10_000 + duration_ms,
            preset,
            vertical_focus: 0.72,
            vertical_position: 0.5,
            music: ClipMusicSource::None,
            game_audio_volume: 1.0,
            music_volume: 1.0,
        }
    }

    #[test]
    fn rejects_short_or_out_of_range_requests() {
        assert!(validate_request(&request(ClipExportPreset::Horizontal, 4_999)).is_err());
        let mut invalid = request(ClipExportPreset::Vertical, 10_000);
        invalid.vertical_focus = 1.1;
        assert!(validate_request(&invalid).is_err());
    }

    #[test]
    fn vertical_filter_combines_blurred_context_and_adjustable_crop() {
        let filter = vertical_filter(0.72, 0.35);
        assert!(filter.contains("boxblur"));
        assert!(filter.contains("iw-0.7200*(iw-ih)"));
        assert!(filter.contains("(iw-ow)*0.3500"));
        assert!(filter.contains("overlay"));
    }

    #[test]
    fn discord_profile_spends_less_than_the_file_budget() {
        let profile =
            export_profile(&request(ClipExportPreset::Discord, 30_000), 60, None).unwrap();
        let projected_bytes =
            (profile.video_bitrate_kbps + profile.audio_bitrate_kbps) * 1_000 * 30 / 8;
        assert!(projected_bytes < DISCORD_LIMIT_BYTES);
        assert!(profile.video_filter.contains("1280:720"));
    }

    #[test]
    fn prefers_the_hardware_family_used_for_recording_then_falls_back() {
        assert_eq!(
            encoder_candidates("nvenc"),
            vec![H264Encoder::Nvenc, H264Encoder::Software]
        );
        assert_eq!(encoder_candidates("unknown"), vec![H264Encoder::Software]);
    }
}
