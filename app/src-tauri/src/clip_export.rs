use std::collections::HashSet;
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
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
    pub presets: Vec<ClipExportPreset>,
    pub vertical_focus: f64,
    pub vertical_position: f64,
    pub music: ClipMusicSource,
    pub game_audio_volume: f64,
    pub music_volume: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClipExportOutput {
    pub preset: ClipExportPreset,
    pub filename: String,
    pub output_path: String,
    pub thumbnail_path: String,
    pub file_size_bytes: u64,
    pub strategy: String,
    pub encoder_used: String,
    pub encode_elapsed_ms: u64,
    pub thumbnail_elapsed_ms: u64,
    pub retry_count: u32,
    pub attempts: Vec<ClipExportAttempt>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClipExportAttempt {
    pub encoder: String,
    pub elapsed_ms: u64,
    pub successful: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClipExportResult {
    pub outputs: Vec<ClipExportOutput>,
    pub elapsed_ms: u64,
    pub setup_elapsed_ms: u64,
    pub source_probe_elapsed_ms: u64,
    pub source_probe_strategy: String,
    pub finalize_elapsed_ms: u64,
    pub total_file_size_bytes: u64,
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
    pub preset: Option<ClipExportPreset>,
    pub completed_outputs: u8,
    pub total_outputs: u8,
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
    max_video_bitrate_kbps: u64,
    audio_bitrate_kbps: u64,
    output_fps: u32,
    high_quality: bool,
}

struct ExportPaths {
    filename: String,
    video: PathBuf,
    thumbnail: PathBuf,
    partial_video: PathBuf,
    partial_thumbnail: PathBuf,
}

struct EncodingContext<'a> {
    ffmpeg: &'a Path,
    source_video: &'a Path,
    music_path: Option<&'a Path>,
    request: &'a ClipExportRequest,
    encoders: &'a [H264Encoder],
}

struct EncodeOutcome {
    encoder_used: String,
    attempts: Vec<ClipExportAttempt>,
}

struct OutputDiagnostics {
    encoder_used: String,
    encode_elapsed_ms: u64,
    thumbnail_elapsed_ms: u64,
    retry_count: u32,
    attempts: Vec<ClipExportAttempt>,
}

struct BatchProgress<'a> {
    channel: &'a Channel<ClipExportProgress>,
    preset: ClipExportPreset,
    output_index: usize,
    total_outputs: usize,
    last_percent: u8,
    last_stage: Option<ExportStage>,
}

impl BatchProgress<'_> {
    fn send(&mut self, stage: ExportStage, local_percent: u8) {
        let overall = (((self.output_index * 100 + usize::from(local_percent)) * 100)
            / (self.total_outputs * 100))
            .min(100) as u8;
        if overall < self.last_percent
            || (overall == self.last_percent && self.last_stage == Some(stage))
        {
            return;
        }
        self.last_percent = overall;
        self.last_stage = Some(stage);
        self.channel
            .send(ClipExportProgress {
                stage,
                percent: overall,
                preset: Some(self.preset),
                completed_outputs: if local_percent >= 100 {
                    (self.output_index + 1) as u8
                } else {
                    self.output_index as u8
                },
                total_outputs: self.total_outputs as u8,
            })
            .ok();
    }
}

pub async fn export(
    output_directory: &Path,
    music_directory: &Path,
    ffmpeg: &Path,
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
    let source_probe_started = Instant::now();
    let metadata: SourceMetadata = serde_json::from_slice(
        &fs::read(game_directory.join("metadata.json"))
            .context("failed to read recording metadata")?,
    )
    .context("failed to parse recording metadata")?;
    let source_probe_elapsed_ms = elapsed_ms(source_probe_started);
    if metadata.duration_ms > 0 && request.clip_end_ms > metadata.duration_ms.saturating_add(1_000)
    {
        bail!("clip endpoint exceeds the recording duration")
    }

    let music_path = resolve_music(music_directory, &request.music)?;
    let clips_directory = output_directory.join("clips");
    fs::create_dir_all(&clips_directory)
        .with_context(|| format!("failed to create {}", clips_directory.display()))?;
    let source_fps = metadata.recording_fps.clamp(24, 60);
    let encoders = encoder_candidates(&metadata.encoder_used);
    let encoding = EncodingContext {
        ffmpeg,
        source_video: &source_video,
        music_path: music_path.as_deref(),
        request: &request,
        encoders: &encoders,
    };
    let total_outputs = request.presets.len();
    let mut next_clip_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    let mut staged = Vec::with_capacity(total_outputs);
    let setup_elapsed_ms = elapsed_ms(started_at);

    for (output_index, preset) in request.presets.iter().copied().enumerate() {
        let paths = unique_paths(
            &clips_directory,
            &request.game_timestamp,
            &mut next_clip_timestamp,
        )?;
        let mut reporter = BatchProgress {
            channel: &progress,
            preset,
            output_index,
            total_outputs,
            last_percent: ((output_index * 100) / total_outputs) as u8,
            last_stage: None,
        };
        let diagnostics =
            match encode_output(&encoding, preset, source_fps, &paths, &mut reporter).await {
                Ok(diagnostics) => diagnostics,
                Err(error) => {
                    cleanup_export_paths(&paths, false);
                    for (_, staged_paths, _) in &staged {
                        cleanup_export_paths(staged_paths, false);
                    }
                    return Err(error);
                }
            };
        staged.push((preset, paths, diagnostics));
    }

    let finalize_started = Instant::now();
    for (_, paths, _) in &staged {
        let finalized = fs::rename(&paths.partial_thumbnail, &paths.thumbnail)
            .context("failed to finalize clip thumbnail")
            .and_then(|()| {
                fs::rename(&paths.partial_video, &paths.video)
                    .context("failed to finalize exported clip")
            });
        if let Err(error) = finalized {
            for (_, cleanup, _) in &staged {
                cleanup_export_paths(cleanup, true);
            }
            return Err(error);
        }
    }

    let mut outputs = Vec::with_capacity(staged.len());
    let mut total_file_size_bytes = 0_u64;
    for (preset, paths, diagnostics) in staged {
        let file_size_bytes = fs::metadata(&paths.video)
            .context("failed to inspect completed clip")?
            .len();
        total_file_size_bytes = total_file_size_bytes.saturating_add(file_size_bytes);
        outputs.push(ClipExportOutput {
            preset,
            filename: paths.filename,
            output_path: paths.video.to_string_lossy().into_owned(),
            thumbnail_path: paths.thumbnail.to_string_lossy().into_owned(),
            file_size_bytes,
            strategy: "full_reencode".to_owned(),
            encoder_used: diagnostics.encoder_used,
            encode_elapsed_ms: diagnostics.encode_elapsed_ms,
            thumbnail_elapsed_ms: diagnostics.thumbnail_elapsed_ms,
            retry_count: diagnostics.retry_count,
            attempts: diagnostics.attempts,
        });
    }
    progress
        .send(ClipExportProgress {
            stage: ExportStage::Complete,
            percent: 100,
            preset: None,
            completed_outputs: total_outputs as u8,
            total_outputs: total_outputs as u8,
        })
        .ok();
    let finalize_elapsed_ms = elapsed_ms(finalize_started);
    Ok(ClipExportResult {
        outputs,
        elapsed_ms: elapsed_ms(started_at),
        setup_elapsed_ms,
        source_probe_elapsed_ms,
        source_probe_strategy: "recording_metadata".to_owned(),
        finalize_elapsed_ms,
        total_file_size_bytes,
    })
}

async fn encode_output(
    encoding: &EncodingContext<'_>,
    preset: ClipExportPreset,
    source_fps: u32,
    paths: &ExportPaths,
    progress: &mut BatchProgress<'_>,
) -> Result<OutputDiagnostics> {
    let encode_started = Instant::now();
    let mut profile = export_profile(encoding.request, preset, source_fps, None)?;
    let mut outcome =
        encode_with_fallback(encoding, &profile, &paths.partial_video, progress).await?;
    let mut retry_count = 0_u32;

    if preset == ClipExportPreset::Discord {
        let first_size = fs::metadata(&paths.partial_video)
            .context("failed to inspect exported clip")?
            .len();
        if first_size >= DISCORD_LIMIT_BYTES {
            let adjusted = ((profile.video_bitrate_kbps as f64)
                * (DISCORD_TARGET_BYTES as f64 / first_size as f64)
                * 0.94)
                .floor()
                .max(250.0) as u64;
            profile = export_profile(encoding.request, preset, source_fps, Some(adjusted))?;
            let retry =
                encode_with_fallback(encoding, &profile, &paths.partial_video, progress).await?;
            retry_count = retry_count.saturating_add(1);
            outcome.encoder_used = retry.encoder_used;
            outcome.attempts.extend(retry.attempts);
        }
        let final_size = fs::metadata(&paths.partial_video)
            .context("failed to inspect exported clip")?
            .len();
        if final_size >= DISCORD_LIMIT_BYTES {
            bail!("Discord export could not be kept below 10 MB; shorten the clip")
        }
    }

    let encode_elapsed_ms = elapsed_ms(encode_started);

    progress.send(ExportStage::Thumbnail, 96);
    let thumbnail_started = Instant::now();
    generate_thumbnail(
        encoding.ffmpeg,
        &paths.partial_video,
        &paths.partial_thumbnail,
    )
    .await?;
    // Keep 100% reserved for the atomically published batch result.
    progress.send(ExportStage::Thumbnail, 99);
    Ok(OutputDiagnostics {
        encoder_used: outcome.encoder_used,
        encode_elapsed_ms,
        thumbnail_elapsed_ms: elapsed_ms(thumbnail_started),
        retry_count,
        attempts: outcome.attempts,
    })
}

fn validate_request(request: &ClipExportRequest) -> Result<()> {
    if !library::valid_game_id(&request.game_timestamp) {
        bail!("invalid game identifier")
    }
    if request.clip_end_ms <= request.clip_start_ms
        || request.clip_end_ms - request.clip_start_ms < MINIMUM_CLIP_MS
    {
        bail!("clips must be at least five seconds long")
    }
    if request.presets.is_empty() || request.presets.len() > 3 {
        bail!("select between one and three export formats")
    }
    let mut unique_presets = HashSet::new();
    if request
        .presets
        .iter()
        .any(|preset| !unique_presets.insert(*preset))
    {
        bail!("export formats must be unique")
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
        ClipMusicSource::File { path } => music::resolve_imported(path).map(Some),
    }
}

fn unique_paths(
    directory: &Path,
    game_timestamp: &str,
    next_clip_timestamp: &mut u64,
) -> Result<ExportPaths> {
    let mut clip_timestamp = *next_clip_timestamp;
    loop {
        let filename = format!("{game_timestamp}_{clip_timestamp}");
        let video = directory.join(format!("{filename}.mp4"));
        let thumbnail = directory.join(format!("{filename}.jpg"));
        let partial_video = directory.join(format!("{filename}.part.mp4"));
        let partial_thumbnail = directory.join(format!("{filename}.part.jpg"));
        if !video.exists()
            && !thumbnail.exists()
            && !partial_video.exists()
            && !partial_thumbnail.exists()
        {
            *next_clip_timestamp = clip_timestamp.saturating_add(1);
            return Ok(ExportPaths {
                partial_video,
                partial_thumbnail,
                filename,
                video,
                thumbnail,
            });
        }
        clip_timestamp = clip_timestamp.saturating_add(1);
    }
}

fn cleanup_export_paths(paths: &ExportPaths, include_final: bool) {
    fs::remove_file(&paths.partial_video).ok();
    fs::remove_file(&paths.partial_thumbnail).ok();
    if include_final {
        fs::remove_file(&paths.video).ok();
        fs::remove_file(&paths.thumbnail).ok();
    }
}

fn export_profile(
    request: &ClipExportRequest,
    preset: ClipExportPreset,
    source_fps: u32,
    bitrate_override: Option<u64>,
) -> Result<ExportProfile> {
    let duration_ms = request.clip_end_ms - request.clip_start_ms;
    match preset {
        ClipExportPreset::Horizontal => Ok(ExportProfile {
            video_filter: "[0:v]scale=w='trunc(min(1920,iw)/2)*2':h=-2:flags=lanczos,setsar=1,format=yuv420p[vout]".to_owned(),
            video_bitrate_kbps: 24_000,
            max_video_bitrate_kbps: 36_000,
            audio_bitrate_kbps: 192,
            output_fps: source_fps,
            high_quality: true,
        }),
        ClipExportPreset::Vertical => Ok(ExportProfile {
            video_filter: vertical_filter(request.vertical_focus, request.vertical_position),
            video_bitrate_kbps: 24_000,
            max_video_bitrate_kbps: 36_000,
            audio_bitrate_kbps: 192,
            output_fps: source_fps,
            high_quality: true,
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
                max_video_bitrate_kbps: video_bitrate_kbps,
                audio_bitrate_kbps,
                output_fps,
                high_quality: false,
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
    encoding: &EncodingContext<'_>,
    profile: &ExportProfile,
    output: &Path,
    progress: &mut BatchProgress<'_>,
) -> Result<EncodeOutcome> {
    let mut errors = Vec::new();
    let mut attempts = Vec::new();
    for encoder in encoding.encoders {
        fs::remove_file(output).ok();
        let arguments = ffmpeg_arguments(
            encoding.source_video,
            encoding.music_path,
            encoding.request,
            profile,
            *encoder,
            output,
        );
        let attempt_started = Instant::now();
        match run_ffmpeg(
            encoding.ffmpeg,
            arguments,
            encoding.request.clip_end_ms - encoding.request.clip_start_ms,
            progress,
        )
        .await
        {
            Ok(()) => {
                attempts.push(ClipExportAttempt {
                    encoder: encoder_name(*encoder).to_owned(),
                    elapsed_ms: elapsed_ms(attempt_started),
                    successful: true,
                    error: None,
                });
                return Ok(EncodeOutcome {
                    encoder_used: encoder_name(*encoder).to_owned(),
                    attempts,
                });
            }
            Err(error) => {
                attempts.push(ClipExportAttempt {
                    encoder: encoder_name(*encoder).to_owned(),
                    elapsed_ms: elapsed_ms(attempt_started),
                    successful: false,
                    error: Some(error.to_string()),
                });
                errors.push(format!("{}: {error}", encoder_name(*encoder)));
            }
        }
    }
    bail!("all H.264 encoders failed: {}", errors.join(" | "))
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
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
    append_encoder_arguments(encoder, profile.high_quality, &mut arguments);
    arguments.extend(strings(&[
        "-b:v",
        &format!("{}k", profile.video_bitrate_kbps),
        "-maxrate",
        &format!("{}k", profile.max_video_bitrate_kbps),
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

fn append_encoder_arguments(
    encoder: H264Encoder,
    high_quality: bool,
    arguments: &mut Vec<OsString>,
) {
    match encoder {
        H264Encoder::Nvenc if high_quality => arguments.extend(strings(&[
            "-c:v",
            "h264_nvenc",
            "-preset",
            "p6",
            "-tune",
            "hq",
            "-rc",
            "vbr",
            "-multipass",
            "fullres",
            "-spatial-aq",
            "1",
            "-temporal-aq",
            "1",
            "-aq-strength",
            "8",
            "-rc-lookahead",
            "32",
            "-b_ref_mode",
            "middle",
        ])),
        H264Encoder::Nvenc => arguments.extend(strings(&["-c:v", "h264_nvenc", "-preset", "p5"])),
        H264Encoder::Amf if high_quality => {
            arguments.extend(strings(&["-c:v", "h264_amf", "-quality", "quality"]))
        }
        H264Encoder::Amf => {
            arguments.extend(strings(&["-c:v", "h264_amf", "-quality", "balanced"]))
        }
        H264Encoder::Qsv if high_quality => {
            arguments.extend(strings(&["-c:v", "h264_qsv", "-preset", "slow"]))
        }
        H264Encoder::Qsv => arguments.extend(strings(&["-c:v", "h264_qsv", "-preset", "medium"])),
        H264Encoder::VideoToolbox => arguments.extend(strings(&["-c:v", "h264_videotoolbox"])),
        H264Encoder::Software if high_quality => {
            arguments.extend(strings(&["-c:v", "libx264", "-preset", "medium"]))
        }
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
    ffmpeg: &Path,
    arguments: Vec<OsString>,
    duration_ms: u64,
    progress: &mut BatchProgress<'_>,
) -> Result<()> {
    let mut command = Command::new(ffmpeg);
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
            progress.send(ExportStage::Encoding, percent);
        }
    }
    let status = child.wait().await.context("failed to wait for ffmpeg")?;
    let stderr = stderr_task.await.unwrap_or_default();
    if !status.success() {
        bail!("{}", concise_error(&stderr))
    }
    Ok(())
}

async fn generate_thumbnail(ffmpeg: &Path, video: &Path, output: &Path) -> Result<()> {
    let mut command = Command::new(ffmpeg);
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
            presets: vec![preset],
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

        let mut duplicate = request(ClipExportPreset::Horizontal, 10_000);
        duplicate.presets.push(ClipExportPreset::Horizontal);
        assert!(validate_request(&duplicate).is_err());
    }

    #[test]
    fn accepts_collision_suffixed_game_ids_for_export() {
        let mut collision = request(ClipExportPreset::Horizontal, 10_000);
        collision.game_timestamp = "1786000000-1".to_owned();
        assert!(validate_request(&collision).is_ok());

        collision.game_timestamp = "../1786000000-1".to_owned();
        assert!(validate_request(&collision).is_err());
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
        let profile = export_profile(
            &request(ClipExportPreset::Discord, 30_000),
            ClipExportPreset::Discord,
            60,
            None,
        )
        .unwrap();
        let projected_bytes =
            (profile.video_bitrate_kbps + profile.audio_bitrate_kbps) * 1_000 * 30 / 8;
        assert!(projected_bytes < DISCORD_LIMIT_BYTES);
        assert!(profile.video_filter.contains("1280:720"));
    }

    #[test]
    fn publish_profiles_allow_high_motion_bitrate_peaks() {
        let profile = export_profile(
            &request(ClipExportPreset::Horizontal, 15_000),
            ClipExportPreset::Horizontal,
            60,
            None,
        )
        .unwrap();
        assert_eq!(profile.video_bitrate_kbps, 24_000);
        assert_eq!(profile.max_video_bitrate_kbps, 36_000);
        assert!(profile.high_quality);
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
