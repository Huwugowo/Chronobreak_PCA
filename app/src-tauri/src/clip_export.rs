use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use chronobreak_replay_time::{
    ClipRange, FFPROBE_SUMMARY_SHOW_ENTRIES, FinalizationExpectationsV2, FrameBoundary, MediaId,
    MediaTimelineV2, ProducerEvidenceV2, REPLAY_TICKS_PER_SECOND, Rational, RoundingMode,
    checked_scale, frame_boundary_to_replay_tick, parse_ffprobe_media_timeline,
};
use queueback_media_runtime::{BoundedProcessErrorKind, run_bounded_process};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

use crate::{library, music};

const MINIMUM_CLIP_SECONDS: u64 = 5;
const FFMPEG_TIMESTAMP_DECIMAL_PLACES: u32 = 12;
const DISCORD_LIMIT_BYTES: u64 = 10_000_000;
const DISCORD_TARGET_BYTES: u64 = 9_400_000;
const OUTPUT_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const OUTPUT_PROBE_LIMIT: usize = 1024 * 1024;
const MAXIMUM_AUDIO_ALIGNMENT_TICKS: i64 = 2_400_000;

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
#[serde(deny_unknown_fields)]
pub struct ClipExportRequest {
    pub game_timestamp: String,
    pub media_id: MediaId,
    pub start_frame: FrameBoundary,
    pub end_frame_exclusive: FrameBoundary,
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
    pub validation_elapsed_ms: u64,
    pub validated_frame_count: String,
    pub validated_video_replay_end: String,
    pub validated_audio_replay_start: String,
    pub validated_audio_replay_end: String,
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
    Validating,
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

#[derive(Debug, Clone)]
struct ClipTiming {
    start_seconds: String,
    duration_seconds: String,
    duration_replay_ticks: u64,
    source_frame_count: u64,
    source_frame_rate: Rational,
    media_runtime_id: String,
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
    output_frame_rate: Rational,
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
    ffprobe: &'a Path,
    source_video: &'a Path,
    music_path: Option<&'a Path>,
    request: &'a ClipExportRequest,
    timing: &'a ClipTiming,
    encoders: &'a [H264Encoder],
}

struct EncodeOutcome {
    encoder_used: String,
    attempts: Vec<ClipExportAttempt>,
}

#[derive(Debug, Clone)]
struct OutputValidation {
    frame_count: FrameBoundary,
    video_replay_end: u64,
    audio_replay_start: i64,
    audio_replay_end: i64,
}

struct OutputDiagnostics {
    encoder_used: String,
    encode_elapsed_ms: u64,
    validation_elapsed_ms: u64,
    validation: OutputValidation,
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
    ffprobe: &Path,
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
    let authority = library::recording_export_authority(&game_directory)
        .context("recording bundle does not satisfy the schema-v2 export contract")?;
    let source_probe_elapsed_ms = elapsed_ms(source_probe_started);
    let timing = validate_for_metadata(&request, &authority.media_timeline)?;

    let music_path = resolve_music(music_directory, &request.music)?;
    let clips_directory = output_directory.join("clips");
    fs::create_dir_all(&clips_directory)
        .with_context(|| format!("failed to create {}", clips_directory.display()))?;
    let encoders = encoder_candidates(&authority.encoder_used);
    let encoding = EncodingContext {
        ffmpeg,
        ffprobe,
        source_video: &source_video,
        music_path: music_path.as_deref(),
        request: &request,
        timing: &timing,
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
        let diagnostics = match encode_output(&encoding, preset, &paths, &mut reporter).await {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                cleanup_export_paths(&paths, false);
                for (_, staged_paths, _, _) in &staged {
                    cleanup_export_paths(staged_paths, false);
                }
                return Err(error);
            }
        };
        let file_size_bytes =
            match fs::metadata(&paths.partial_video).context("failed to inspect staged clip") {
                Ok(metadata) => metadata.len(),
                Err(error) => {
                    cleanup_export_paths(&paths, false);
                    for (_, staged_paths, _, _) in &staged {
                        cleanup_export_paths(staged_paths, false);
                    }
                    return Err(error);
                }
            };
        staged.push((preset, paths, diagnostics, file_size_bytes));
    }

    let finalize_started = Instant::now();
    let staged_paths = staged
        .iter()
        .map(|(_, paths, _, _)| paths)
        .collect::<Vec<_>>();
    publish_export_paths(&staged_paths)?;

    let mut outputs = Vec::with_capacity(staged.len());
    let mut total_file_size_bytes = 0_u64;
    for (preset, paths, diagnostics, file_size_bytes) in staged {
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
            validation_elapsed_ms: diagnostics.validation_elapsed_ms,
            validated_frame_count: diagnostics.validation.frame_count.to_string(),
            validated_video_replay_end: diagnostics.validation.video_replay_end.to_string(),
            validated_audio_replay_start: diagnostics.validation.audio_replay_start.to_string(),
            validated_audio_replay_end: diagnostics.validation.audio_replay_end.to_string(),
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
    paths: &ExportPaths,
    progress: &mut BatchProgress<'_>,
) -> Result<OutputDiagnostics> {
    let encode_started = Instant::now();
    let mut profile = export_profile(encoding.request, encoding.timing, preset, None)?;
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
            profile = export_profile(encoding.request, encoding.timing, preset, Some(adjusted))?;
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

    progress.send(ExportStage::Validating, 96);
    let validation_started = Instant::now();
    let validation = validate_exported_media(
        encoding.ffprobe,
        &paths.partial_video,
        encoding.request,
        encoding.timing,
        &profile,
    )
    .await?;
    let validation_elapsed_ms = elapsed_ms(validation_started);

    progress.send(ExportStage::Thumbnail, 97);
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
        validation_elapsed_ms,
        validation,
        thumbnail_elapsed_ms: elapsed_ms(thumbnail_started),
        retry_count,
        attempts: outcome.attempts,
    })
}

fn validate_request(request: &ClipExportRequest) -> Result<()> {
    if !library::valid_game_id(&request.game_timestamp) {
        bail!("invalid game identifier")
    }
    if request.start_frame >= request.end_frame_exclusive {
        bail!("clip frame range must be nonempty and ordered")
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

fn validate_for_metadata(
    request: &ClipExportRequest,
    media_timeline: &MediaTimelineV2,
) -> Result<ClipTiming> {
    media_timeline
        .validate()
        .context("recording metadata media_timeline is invalid")?;

    let range = ClipRange::new(
        request.media_id.clone(),
        request.start_frame,
        request.end_frame_exclusive,
        media_timeline.video.frame_count,
    )
    .context("clip frame range is outside recording coverage")?;
    range
        .validate_for(media_timeline)
        .context("clip request does not match the loaded recording")?;

    let frame_rate = media_timeline.video.frame_rate;
    let start_tick =
        frame_boundary_to_replay_tick(range.start_frame, frame_rate, RoundingMode::Exact)
            .context("failed to derive the exact clip start")?;
    let end_tick =
        frame_boundary_to_replay_tick(range.end_frame_exclusive, frame_rate, RoundingMode::Exact)
            .context("failed to derive the exact clip end")?;
    let duration_replay_ticks = end_tick
        .get()
        .checked_sub(start_tick.get())
        .context("clip frame range produced a negative duration")?;
    let minimum_ticks = REPLAY_TICKS_PER_SECOND
        .checked_mul(MINIMUM_CLIP_SECONDS)
        .context("failed to compute the minimum clip duration")?;
    if duration_replay_ticks < minimum_ticks {
        bail!("clips must be at least five seconds long")
    }

    Ok(ClipTiming {
        start_seconds: replay_ticks_as_ffmpeg_seconds(start_tick.get())?,
        duration_seconds: replay_ticks_as_ffmpeg_seconds(duration_replay_ticks)?,
        duration_replay_ticks,
        source_frame_count: range.frame_count(),
        source_frame_rate: frame_rate,
        media_runtime_id: media_timeline.producer.media_runtime_id.clone(),
    })
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

fn publish_export_paths(all_paths: &[&ExportPaths]) -> Result<()> {
    for paths in all_paths {
        let finalized = fs::rename(&paths.partial_thumbnail, &paths.thumbnail)
            .context("failed to finalize clip thumbnail")
            .and_then(|()| {
                fs::rename(&paths.partial_video, &paths.video)
                    .context("failed to finalize exported clip")
            });
        if let Err(error) = finalized {
            for cleanup in all_paths {
                cleanup_export_paths(cleanup, true);
            }
            return Err(error);
        }
    }
    Ok(())
}

fn export_profile(
    request: &ClipExportRequest,
    timing: &ClipTiming,
    preset: ClipExportPreset,
    bitrate_override: Option<u64>,
) -> Result<ExportProfile> {
    let source_frame_rate = timing.source_frame_rate;
    let trimmed_input = format!(
        "[0:v]trim=start_frame=0:end_frame={},setpts=PTS-STARTPTS",
        timing.source_frame_count
    );
    match preset {
        ClipExportPreset::Horizontal => Ok(ExportProfile {
            video_filter: format!(
                "{trimmed_input},scale=w='trunc(min(1920,iw)/2)*2':h=-2:flags=lanczos,setsar=1,format=yuv420p[vout]"
            ),
            video_bitrate_kbps: 24_000,
            max_video_bitrate_kbps: 36_000,
            audio_bitrate_kbps: 192,
            output_frame_rate: source_frame_rate,
            high_quality: true,
        }),
        ClipExportPreset::Vertical => Ok(ExportProfile {
            video_filter: vertical_filter(
                &trimmed_input,
                request.vertical_focus,
                request.vertical_position,
            ),
            video_bitrate_kbps: 24_000,
            max_video_bitrate_kbps: 36_000,
            audio_bitrate_kbps: 192,
            output_frame_rate: source_frame_rate,
            high_quality: true,
        }),
        ClipExportPreset::Discord => {
            let seconds = timing.duration_replay_ticks as f64 / REPLAY_TICKS_PER_SECOND as f64;
            let audio_bitrate_kbps = 128_u64;
            let available_total_kbps =
                ((DISCORD_TARGET_BYTES as f64 * 8.0 * 0.90) / seconds / 1_000.0).floor() as u64;
            if available_total_kbps <= audio_bitrate_kbps + 250 {
                bail!("clip is too long to remain publishable below 10 MB")
            }
            let video_bitrate_kbps = bitrate_override
                .unwrap_or(available_total_kbps - audio_bitrate_kbps)
                .clamp(250, 5_000);
            let thirty_fps = Rational::new(30, 1).context("failed to build 30 FPS export rate")?;
            let (width, height, output_frame_rate) = if video_bitrate_kbps >= 2_500 {
                (1_280, 720, source_frame_rate)
            } else if video_bitrate_kbps >= 1_200 {
                (
                    1_280,
                    720,
                    capped_frame_rate(source_frame_rate, thirty_fps)?,
                )
            } else if video_bitrate_kbps >= 650 {
                (960, 540, capped_frame_rate(source_frame_rate, thirty_fps)?)
            } else {
                (854, 480, capped_frame_rate(source_frame_rate, thirty_fps)?)
            };
            let fps_filter = if output_frame_rate != source_frame_rate {
                format!(
                    ",fps=fps={}:start_time=0",
                    ffmpeg_frame_rate(output_frame_rate)?
                )
            } else {
                String::new()
            };
            Ok(ExportProfile {
                video_filter: format!(
                    "{trimmed_input},scale={width}:{height}:force_original_aspect_ratio=decrease:flags=lanczos,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2{fps_filter},setsar=1,format=yuv420p[vout]"
                ),
                video_bitrate_kbps,
                max_video_bitrate_kbps: video_bitrate_kbps,
                audio_bitrate_kbps,
                output_frame_rate,
                high_quality: false,
            })
        }
    }
}

fn vertical_filter(input: &str, focus: f64, position: f64) -> String {
    format!(
        "{input}[trimmed];\
         [trimmed]split=2[bg][fg];\
         [bg]scale=270:480:force_original_aspect_ratio=increase:flags=bilinear,crop=270:480,boxblur=12:2,scale=1080:1920:flags=bilinear,eq=brightness=-0.16:saturation=0.82[back];\
         [fg]crop=w='trunc((iw-{focus:.4}*(iw-ih))/2)*2':h=ih:x='(iw-ow)*{position:.4}':y=0,scale=1080:-2:flags=lanczos,setsar=1[front];\
         [back][front]overlay=x=(W-w)/2:y=(H-h)/2,format=yuv420p[vout]"
    )
}

fn capped_frame_rate(source: Rational, maximum: Rational) -> Result<Rational> {
    let source_numerator = u64::try_from(source.numerator())
        .context("source frame-rate numerator must be positive")?;
    let maximum_numerator = u64::try_from(maximum.numerator())
        .context("maximum frame-rate numerator must be positive")?;
    let left = u128::from(source_numerator)
        .checked_mul(u128::from(maximum.denominator()))
        .context("source frame-rate comparison overflowed")?;
    let right = u128::from(maximum_numerator)
        .checked_mul(u128::from(source.denominator()))
        .context("maximum frame-rate comparison overflowed")?;
    Ok(if left > right { maximum } else { source })
}

fn ffmpeg_frame_rate(rate: Rational) -> Result<String> {
    let numerator =
        u64::try_from(rate.numerator()).context("output frame-rate numerator must be positive")?;
    Ok(format!("{numerator}/{}", rate.denominator()))
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
            encoding.timing,
            profile,
            *encoder,
            output,
        )?;
        let attempt_started = Instant::now();
        match run_ffmpeg(
            encoding.ffmpeg,
            arguments,
            encoding.timing.duration_replay_ticks,
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
    timing: &ClipTiming,
    profile: &ExportProfile,
    encoder: H264Encoder,
    output: &Path,
) -> Result<Vec<OsString>> {
    let duration = &timing.duration_seconds;
    let mut arguments = strings(&[
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-ss",
        &timing.start_seconds,
        "-i",
    ]);
    arguments.push(source_video.as_os_str().to_owned());
    if let Some(path) = music_path {
        arguments.extend(strings(&["-stream_loop", "-1", "-i"]));
        arguments.push(path.as_os_str().to_owned());
    }

    let audio_filter = if music_path.is_some() {
        format!(
            "[0:a]atrim=start=0:end={duration},asetpts=PTS-STARTPTS,volume={:.3}[game];\
             [1:a]atrim=duration={duration},asetpts=PTS-STARTPTS,volume={:.3}[music];\
             [game][music]amix=inputs=2:duration=first:dropout_transition=0[aout]",
            request.game_audio_volume, request.music_volume
        )
    } else {
        format!(
            "[0:a]atrim=start=0:end={duration},asetpts=PTS-STARTPTS,volume={:.3}[aout]",
            request.game_audio_volume
        )
    };
    arguments.extend(strings(&[
        "-t",
        duration,
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
        &gop_frame_count(profile.output_frame_rate)?.to_string(),
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
    Ok(arguments)
}

fn gop_frame_count(rate: Rational) -> Result<u64> {
    let numerator =
        u64::try_from(rate.numerator()).context("output frame-rate numerator must be positive")?;
    let two_seconds = numerator
        .checked_mul(2)
        .context("output GOP frame count overflowed")?;
    Ok(two_seconds.div_ceil(rate.denominator()))
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
    duration_replay_ticks: u64,
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
        let progress_ticks = u128::from(microseconds)
            .checked_mul(u128::from(REPLAY_TICKS_PER_SECOND / 1_000_000))
            .context("ffmpeg progress timestamp overflowed")?;
        let scaled_percent = progress_ticks
            .checked_mul(94)
            .context("ffmpeg progress percentage overflowed")?
            / u128::from(duration_replay_ticks);
        let percent = u8::try_from(scaled_percent.min(94))
            .context("ffmpeg progress percentage is out of range")?;
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

async fn validate_exported_media(
    ffprobe: &Path,
    output_path: &Path,
    request: &ClipExportRequest,
    timing: &ClipTiming,
    profile: &ExportProfile,
) -> Result<OutputValidation> {
    let expected_frame_count =
        expected_output_frame_count(timing.duration_replay_ticks, profile.output_frame_rate)?;
    let expectations = FinalizationExpectationsV2 {
        media_id: request.media_id.clone(),
        expected_video_codec: "h264".to_owned(),
        expected_audio_codec: Some("aac".to_owned()),
        producer: ProducerEvidenceV2 {
            backend: "clip-export-full-reencode".to_owned(),
            expected_frame_rate: profile.output_frame_rate,
            expected_frame_count,
            media_runtime_id: timing.media_runtime_id.clone(),
        },
        capture: None,
        require_zero_video_start: true,
    };

    let mut command = Command::new(ffprobe);
    command
        .args(strings(&[
            "-hide_banner",
            "-loglevel",
            "error",
            "-show_entries",
            FFPROBE_SUMMARY_SHOW_ENTRIES,
            "-of",
            "json",
        ]))
        .arg(output_path);
    hide_console(&mut command);
    let output = run_bounded_process(command, OUTPUT_PROBE_TIMEOUT, OUTPUT_PROBE_LIMIT)
        .await
        .map_err(|error| match error.kind() {
            BoundedProcessErrorKind::Timeout => {
                anyhow::Error::msg("export validation ffprobe timed out")
            }
            BoundedProcessErrorKind::OutputLimit => {
                anyhow::Error::msg("export validation ffprobe exceeded its one MiB output limit")
            }
            BoundedProcessErrorKind::Spawn | BoundedProcessErrorKind::Execution => {
                anyhow::Error::new(error).context("failed to run export validation ffprobe")
            }
        })?;
    if !output.status.success() {
        bail!(
            "export validation ffprobe failed: {}",
            concise_error(&String::from_utf8_lossy(&output.stderr))
        )
    }
    let stdout = String::from_utf8(output.stdout)
        .context("export validation ffprobe output is not UTF-8")?;
    let timeline = parse_ffprobe_media_timeline(&stdout, expectations).map_err(|error| {
        anyhow::Error::msg(format!(
            "exported media does not satisfy its exact frame contract: {error}"
        ))
    })?;
    if timeline.container.start_seconds
        != Rational::new(0, 1).context("failed to construct zero container origin")?
    {
        bail!("exported media container does not start at zero")
    }

    let requested_end = timing.duration_replay_ticks;
    let output_end = timeline.video.replay_end.get();
    let one_output_frame = frame_boundary_to_replay_tick(
        FrameBoundary::new(1).context("failed to construct one output frame")?,
        profile.output_frame_rate,
        RoundingMode::Exact,
    )
    .context("output frame rate is not exact in replay ticks")?
    .get();
    if output_end < requested_end || output_end - requested_end > one_output_frame {
        bail!("exported video coverage is outside one output frame of the requested range")
    }

    let audio_start = timeline
        .audio
        .replay_start
        .context("exported media has no audio start mapping")?
        .get();
    let audio_end = timeline
        .audio
        .replay_end
        .context("exported media has no audio end mapping")?
        .get();
    let output_end_signed =
        i64::try_from(output_end).context("exported video coverage exceeds the signed range")?;
    if audio_start.unsigned_abs() > MAXIMUM_AUDIO_ALIGNMENT_TICKS as u64
        || audio_end.abs_diff(output_end_signed) > MAXIMUM_AUDIO_ALIGNMENT_TICKS as u64
    {
        bail!("exported audio coverage differs from video by more than 50 milliseconds")
    }

    Ok(OutputValidation {
        frame_count: timeline.video.frame_count,
        video_replay_end: output_end,
        audio_replay_start: audio_start,
        audio_replay_end: audio_end,
    })
}

fn expected_output_frame_count(
    duration_replay_ticks: u64,
    frame_rate: Rational,
) -> Result<FrameBoundary> {
    let denominator = i128::from(REPLAY_TICKS_PER_SECOND)
        .checked_mul(i128::from(frame_rate.denominator()))
        .context("output frame-count denominator overflowed")?;
    let frame_count = checked_scale(
        i128::from(duration_replay_ticks),
        i128::from(frame_rate.numerator()),
        denominator,
        RoundingMode::Ceil,
    )
    .map_err(anyhow::Error::new)
    .context("failed to derive output frame count")?;
    FrameBoundary::new(
        u64::try_from(frame_count).context("output frame count is outside the supported range")?,
    )
    .map_err(anyhow::Error::new)
    .context("output frame count exceeds the replay contract")
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

fn replay_ticks_as_ffmpeg_seconds(replay_ticks: u64) -> Result<String> {
    let whole_seconds = replay_ticks / REPLAY_TICKS_PER_SECOND;
    let remainder = replay_ticks % REPLAY_TICKS_PER_SECOND;
    let decimal_scale = 10_u128
        .checked_pow(FFMPEG_TIMESTAMP_DECIMAL_PLACES)
        .context("failed to build FFmpeg timestamp decimal scale")?;
    let fraction = u128::from(remainder)
        .checked_mul(decimal_scale)
        .context("FFmpeg timestamp conversion overflowed")?
        / u128::from(REPLAY_TICKS_PER_SECOND);
    Ok(format!(
        "{whole_seconds}.{fraction:0width$}",
        width = usize::try_from(FFMPEG_TIMESTAMP_DECIMAL_PLACES)
            .context("FFmpeg timestamp precision is out of range")?
    ))
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
    use chronobreak_replay_time::{
        AudioTimelineV2, ContainerTimelineV2, MediaPts, ProducerEvidenceV2, ReplayTick,
        SignedReplayTick, VideoTimelineV2,
    };
    use serde_json::json;

    use super::*;

    const MEDIA_ID: &str = "123e4567-e89b-42d3-a456-426614174000";

    fn request(preset: ClipExportPreset, duration_frames: u64) -> ClipExportRequest {
        ClipExportRequest {
            game_timestamp: "1786000000".to_owned(),
            media_id: MediaId::parse(MEDIA_ID).unwrap(),
            start_frame: FrameBoundary::new(60).unwrap(),
            end_frame_exclusive: FrameBoundary::new(60 + duration_frames).unwrap(),
            presets: vec![preset],
            vertical_focus: 0.72,
            vertical_position: 0.5,
            music: ClipMusicSource::None,
            game_audio_volume: 1.0,
            music_volume: 1.0,
        }
    }

    fn timeline() -> MediaTimelineV2 {
        MediaTimelineV2 {
            schema_version: 2,
            replay_ticks_per_second: REPLAY_TICKS_PER_SECOND,
            media_id: MediaId::parse(MEDIA_ID).unwrap(),
            video: VideoTimelineV2 {
                codec: "h264".to_owned(),
                profile: Some("High".to_owned()),
                time_base: Rational::positive(1, 15_360, "video time base").unwrap(),
                first_pts: MediaPts::new(0),
                frame_rate: Rational::positive(60, 1, "frame rate").unwrap(),
                frame_count: FrameBoundary::new(1_200).unwrap(),
                one_past_last_pts: MediaPts::new(307_200),
                replay_end: ReplayTick::new(960_000_000).unwrap(),
                exact_cfr: true,
            },
            audio: AudioTimelineV2 {
                present: true,
                codec: Some("aac".to_owned()),
                sample_rate: Some(48_000),
                time_base: Some(Rational::positive(1, 48_000, "audio time base").unwrap()),
                first_pts: Some(MediaPts::new(0)),
                replay_start: Some(SignedReplayTick::new(0).unwrap()),
                replay_end: Some(SignedReplayTick::new(960_000_000).unwrap()),
            },
            container: ContainerTimelineV2 {
                start_seconds: Rational::new(0, 1).unwrap(),
                duration_seconds: Rational::positive(20, 1, "duration").unwrap(),
            },
            producer: ProducerEvidenceV2 {
                backend: "native".to_owned(),
                expected_frame_rate: Rational::positive(60, 1, "expected frame rate").unwrap(),
                expected_frame_count: FrameBoundary::new(1_200).unwrap(),
                media_runtime_id: "test-runtime".to_owned(),
            },
            capture: None,
        }
    }

    fn metadata() -> MediaTimelineV2 {
        timeline()
    }

    #[test]
    fn rejects_short_or_out_of_range_requests() {
        assert!(
            validate_for_metadata(&request(ClipExportPreset::Horizontal, 299), &metadata())
                .is_err()
        );
        let mut invalid = request(ClipExportPreset::Vertical, 600);
        invalid.vertical_focus = 1.1;
        assert!(validate_request(&invalid).is_err());

        let mut duplicate = request(ClipExportPreset::Horizontal, 600);
        duplicate.presets.push(ClipExportPreset::Horizontal);
        assert!(validate_request(&duplicate).is_err());

        let mut out_of_range = request(ClipExportPreset::Horizontal, 600);
        out_of_range.end_frame_exclusive = FrameBoundary::new(1_201).unwrap();
        assert!(validate_for_metadata(&out_of_range, &metadata()).is_err());
    }

    #[test]
    fn strict_request_rejects_legacy_millisecond_fields() {
        let mut legacy = json!({
            "game_timestamp": "1786000000",
            "media_id": MEDIA_ID,
            "start_frame": "60",
            "end_frame_exclusive": "360",
            "presets": ["horizontal"],
            "vertical_focus": 0.72,
            "vertical_position": 0.5,
            "music": { "kind": "none" },
            "game_audio_volume": 1.0,
            "music_volume": 1.0
        });
        legacy["clip_start_ms"] = json!(1_000);
        assert!(serde_json::from_value::<ClipExportRequest>(legacy).is_err());
    }

    #[test]
    fn metadata_binding_derives_exact_frame_interval() {
        let timing =
            validate_for_metadata(&request(ClipExportPreset::Horizontal, 300), &metadata())
                .unwrap();

        assert_eq!(timing.start_seconds, "1.000000000000");
        assert_eq!(timing.duration_seconds, "5.000000000000");
        assert_eq!(timing.duration_replay_ticks, 240_000_000);
        assert_eq!(timing.source_frame_count, 300);

        let mut stale = request(ClipExportPreset::Horizontal, 300);
        stale.media_id = MediaId::parse("aaaaaaaa-2222-4333-8444-555555555555").unwrap();
        assert!(validate_for_metadata(&stale, &metadata()).is_err());
    }

    #[test]
    fn publication_failure_removes_every_staged_and_final_output() {
        let temporary = tempfile::tempdir().unwrap();
        let export_paths = |name: &str| ExportPaths {
            filename: format!("{name}.mp4"),
            video: temporary.path().join(format!("{name}.mp4")),
            thumbnail: temporary.path().join(format!("{name}.jpg")),
            partial_video: temporary.path().join(format!("{name}.part.mp4")),
            partial_thumbnail: temporary.path().join(format!("{name}.part.jpg")),
        };
        let first = export_paths("first");
        let second = export_paths("second");

        for path in [
            &first.partial_video,
            &first.partial_thumbnail,
            &second.partial_thumbnail,
        ] {
            fs::write(path, b"staged").unwrap();
        }

        let error = publish_export_paths(&[&first, &second]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("failed to finalize exported clip"),
            "{error:#}"
        );
        for paths in [&first, &second] {
            for path in [
                &paths.partial_video,
                &paths.partial_thumbnail,
                &paths.video,
                &paths.thumbnail,
            ] {
                assert!(
                    !path.exists(),
                    "failed publication left an output at {}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn ffmpeg_decimal_preserves_ntsc_frame_precision() {
        assert_eq!(
            replay_ticks_as_ffmpeg_seconds(1_601_600).unwrap(),
            "0.033366666666"
        );
    }
    #[test]
    fn capped_output_frame_count_uses_the_half_open_coverage_ceiling() {
        assert_eq!(
            expected_output_frame_count(
                301 * (REPLAY_TICKS_PER_SECOND / 60),
                Rational::positive(30, 1, "output rate").unwrap(),
            )
            .unwrap()
            .get(),
            151
        );
    }

    #[test]
    fn ffmpeg_arguments_use_exact_seek_and_decoded_frame_trim() {
        let request = request(ClipExportPreset::Horizontal, 300);
        let timing = validate_for_metadata(&request, &metadata()).unwrap();
        let profile =
            export_profile(&request, &timing, ClipExportPreset::Horizontal, None).unwrap();
        let arguments = ffmpeg_arguments(
            Path::new("source.mp4"),
            None,
            &request,
            &timing,
            &profile,
            H264Encoder::Software,
            Path::new("output.mp4"),
        )
        .unwrap();
        let arguments = arguments
            .iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>();

        let seek = arguments.iter().position(|value| value == "-ss").unwrap();
        assert_eq!(arguments[seek + 1], "1.000000000000");
        let duration = arguments.iter().position(|value| value == "-t").unwrap();
        assert_eq!(arguments[duration + 1], "5.000000000000");
        let filter = arguments
            .iter()
            .position(|value| value == "-filter_complex")
            .unwrap();
        assert!(arguments[filter + 1].contains("trim=start_frame=0:end_frame=300"));
        assert!(arguments[filter + 1].contains("setpts=PTS-STARTPTS"));
        assert!(arguments[filter + 1].contains("atrim=start=0:end=5.000000000000"));
    }

    #[test]
    fn accepts_collision_suffixed_game_ids_for_export() {
        let mut collision = request(ClipExportPreset::Horizontal, 600);
        collision.game_timestamp = "1786000000-1".to_owned();
        assert!(validate_request(&collision).is_ok());

        collision.game_timestamp = "../1786000000-1".to_owned();
        assert!(validate_request(&collision).is_err());
    }

    #[test]
    fn vertical_filter_combines_blurred_context_and_adjustable_crop() {
        let filter = vertical_filter("[0:v]trim=start_frame=0:end_frame=300", 0.72, 0.35);
        assert!(filter.contains("trim=start_frame=0:end_frame=300"));
        assert!(filter.contains("boxblur"));
        assert!(filter.contains("iw-0.7200*(iw-ih)"));
        assert!(filter.contains("(iw-ow)*0.3500"));
        assert!(filter.contains("overlay"));
    }

    #[test]
    fn discord_profile_spends_less_than_the_file_budget() {
        let profile = export_profile(
            &request(ClipExportPreset::Discord, 1_800),
            &ClipTiming {
                start_seconds: "1.000000000000".to_owned(),
                duration_seconds: "30.000000000000".to_owned(),
                duration_replay_ticks: 1_440_000_000,
                source_frame_count: 1_800,
                source_frame_rate: Rational::positive(60, 1, "frame rate").unwrap(),
                media_runtime_id: timeline().producer.media_runtime_id,
            },
            ClipExportPreset::Discord,
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
            &request(ClipExportPreset::Horizontal, 900),
            &ClipTiming {
                start_seconds: "1.000000000000".to_owned(),
                duration_seconds: "15.000000000000".to_owned(),
                duration_replay_ticks: 720_000_000,
                source_frame_count: 900,
                source_frame_rate: Rational::positive(60, 1, "frame rate").unwrap(),
                media_runtime_id: timeline().producer.media_runtime_id,
            },
            ClipExportPreset::Horizontal,
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
