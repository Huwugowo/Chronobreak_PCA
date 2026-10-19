use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use chronobreak_replay_time::{
    GameCalibrationV2, GameTick, MappedReplayTime, MappingUnavailableReason, MediaId,
    MediaTimelineV2, REPLAY_TICKS_PER_SECOND, REPLAY_TIME_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::config;

const METADATA_JSON: &str = "metadata.json";
const VIDEO_MP4: &str = "video.mp4";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameSummary {
    pub media_id: Option<MediaId>,
    pub timestamp: String,
    pub champion: String,
    pub game_mode: String,
    pub duration_ms: u64,
    pub recorded_at: String,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub summoner_spells: Vec<String>,
    pub keystone_id: Option<u32>,
    pub items: Vec<GameItemSummary>,
    pub participants: Vec<ReplayParticipant>,
    pub saved: bool,
    pub incomplete: bool,
    pub video_size_bytes: u64,
    pub video_available: bool,
    pub league_result: Option<chronobreak_league_data::Summary>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameItemSummary {
    pub item_id: u32,
    pub slot: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClipSummary {
    pub filename: String,
    pub game_timestamp: String,
    pub clip_timestamp: String,
    pub duration_ms: u64,
    pub file_size_bytes: u64,
    pub thumbnail_path: Option<String>,
    pub thumbnail_url: Option<String>,
    pub video_url: String,
    pub source_champion: Option<String>,
    pub source_date: Option<String>,
}

/// Lightweight clip display data. Duration is optional and never probed by a core scan.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct LibraryClip {
    pub filename: String,
    pub game_timestamp: String,
    pub clip_timestamp: String,
    pub duration_ms: Option<u64>,
    pub file_size_bytes: u64,
    pub thumbnail_path: Option<String>,
    pub thumbnail_url: Option<String>,
    pub video_url: String,
    pub source_champion: Option<String>,
    pub source_date: Option<String>,
}

/// Disposable results derived from one completed filesystem scan.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct LibrarySnapshot {
    pub token: String,
    pub games: Vec<GameSummary>,
    pub clips: Vec<LibraryClip>,
    pub usage: StorageUsage,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct StorageUsage {
    pub games_bytes: u64,
    pub clips_bytes: u64,
    pub game_count: u64,
    pub clip_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct AutoDeleteResult {
    pub deleted_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventRelation {
    Ally,
    Enemy,
    Neutral,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayParticipant {
    pub summoner_name: String,
    pub champion: String,
    pub relation: EventRelation,
}

/// Selected recording media authority, independent of semantic projections.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ReplayDescriptor {
    pub snapshot_token: String,
    pub game_timestamp: String,
    pub video_url: String,
    pub media_timeline: MediaTimelineV2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataDocument {
    schema_version: u32,
    media_id: MediaId,
    media_timeline: MediaTimelineV2,
    recorded_at: String,
    game_mode: Option<String>,
    local_player_summoner_name: Option<String>,
    local_player_champion: Option<String>,
    local_player_team: Option<String>,
    encoder_used: String,
    #[serde(rename = "recording_codec")]
    _recording_codec: String,
    #[serde(rename = "recording_profile")]
    _recording_profile: String,
    #[serde(rename = "recording_resolution")]
    _recording_resolution: String,
    #[serde(rename = "capture_backend")]
    _capture_backend: String,
    #[serde(rename = "capture_adapter_luid")]
    _capture_adapter_luid: Option<String>,
    #[serde(rename = "capture_adapter_name")]
    _capture_adapter_name: Option<String>,
    #[serde(rename = "capture_output")]
    _capture_output: Option<String>,
    #[serde(rename = "encoder_interop")]
    _encoder_interop: Option<String>,
    #[serde(rename = "media_runtime_id")]
    _media_runtime_id: String,
    #[serde(rename = "capture_support_label")]
    _capture_support_label: String,
    #[serde(rename = "source_frames_surfaced")]
    _source_frames_surfaced: u64,
    #[serde(rename = "source_frames_superseded")]
    _source_frames_superseded: u64,
    #[serde(rename = "cfr_duplicates")]
    _cfr_duplicates: u64,
    #[serde(rename = "cfr_discards")]
    _cfr_discards: u64,
    #[serde(rename = "pool_recreations")]
    _pool_recreations: u64,
    #[serde(rename = "capture")]
    _capture: Option<CaptureMetadataDocument>,
    saved: bool,
}
#[derive(Debug, Serialize)]
pub(crate) struct LeagueFacts {
    pub media_id: MediaId,
    pub local_player_name: Option<String>,
    pub league_match: Option<crate::league_match::LeagueMatch>,
    pub league_result: Option<chronobreak_league_data::ResultFile>,
}
pub(crate) fn league_facts(root: &Path, bundle: &str, media: &MediaId) -> Result<LeagueFacts> {
    let directory = root.join("games").join(bundle);
    let metadata = read_metadata(&directory.join(METADATA_JSON))?;
    anyhow::ensure!(
        &metadata.media_id == media,
        "recording facts identity changed"
    );
    let candidate = crate::league_match::read(&directory, media);
    let result = crate::league_result::read(&directory, media, candidate.as_ref());
    Ok(LeagueFacts {
        media_id: media.clone(),
        local_player_name: metadata.local_player_summoner_name,
        league_match: candidate,
        league_result: result,
    })
}

pub(crate) struct ExportRecordingAuthority {
    pub media_timeline: MediaTimelineV2,
    pub encoder_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureMetadataDocument {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    #[serde(rename = "backend")]
    _backend: String,
    #[serde(rename = "diagnostics_abi")]
    _diagnostics_abi: u32,
    #[serde(rename = "support_label")]
    _support_label: String,
    #[serde(rename = "capture_adapter_luid")]
    _capture_adapter_luid: String,
    #[serde(rename = "encoder_adapter_luid")]
    _encoder_adapter_luid: String,
    #[serde(rename = "capture_adapter_name")]
    _capture_adapter_name: String,
    #[serde(rename = "capture_output")]
    _capture_output: String,
    #[serde(rename = "encoder_backend")]
    _encoder_backend: String,
    #[serde(rename = "encoder_interop")]
    _encoder_interop: String,
    #[serde(rename = "media_runtime_id")]
    _media_runtime_id: String,
    #[serde(rename = "source_format")]
    _source_format: String,
    #[serde(rename = "converted_format")]
    _converted_format: String,
    #[serde(rename = "host_readback")]
    _host_readback: bool,
    #[serde(rename = "gpu_stages")]
    _gpu_stages: Vec<String>,
    #[serde(rename = "frame_pool_capacity")]
    _frame_pool_capacity: u32,
    #[serde(rename = "capture_output_pool_capacity")]
    _capture_output_pool_capacity: u32,
    #[serde(rename = "filter_buffered_frame_limit")]
    _filter_buffered_frame_limit: u32,
    #[serde(rename = "encoder_depth")]
    _encoder_depth: u32,
    #[serde(rename = "progress_stall_timeout_seconds")]
    _progress_stall_timeout_seconds: u32,
    #[serde(rename = "maximum_texture_bytes")]
    _maximum_texture_bytes: u64,
    #[serde(rename = "source_frames_surfaced")]
    _source_frames_surfaced: u64,
    #[serde(rename = "source_frames_superseded")]
    _source_frames_superseded: u64,
    #[serde(rename = "encoded_frames")]
    _encoded_frames: u64,
    #[serde(rename = "muxed_bytes")]
    _muxed_bytes: u64,
    #[serde(rename = "cfr_duplicates")]
    _cfr_duplicates: u64,
    #[serde(rename = "cfr_discards")]
    _cfr_discards: u64,
    #[serde(rename = "pool_recreations")]
    _pool_recreations: u64,
    #[serde(rename = "first_qpc_100ns")]
    _first_qpc_100ns: i64,
    #[serde(rename = "latest_qpc_100ns")]
    _latest_qpc_100ns: i64,
    #[serde(rename = "terminal_progress")]
    _terminal_progress: bool,
}

/// The caller owns blocking admission and supplies a cheap cancellation check.
/// No media runtime or external probe is used on this path.
pub(crate) fn build_library_snapshot(
    output_directory: &Path,
    origin: &str,
    token: String,
    check_current: impl Fn() -> Result<()>,
) -> Result<LibrarySnapshot> {
    build_library_snapshot_with(
        output_directory,
        origin,
        token,
        check_current,
        read_game_summary,
    )
}

fn build_library_snapshot_with(
    output_directory: &Path,
    origin: &str,
    token: String,
    check_current: impl Fn() -> Result<()>,
    read_summary: impl FnMut(&Path, String) -> GameSummary,
) -> Result<LibrarySnapshot> {
    let games = scan_games(output_directory, &check_current, read_summary)?;
    let sources = games
        .iter()
        .map(|game| (game.timestamp.as_str(), game))
        .collect::<HashMap<_, _>>();
    let mut usage = StorageUsage {
        games_bytes: games.iter().map(|game| game.video_size_bytes).sum(),
        game_count: games.len() as u64,
        clips_bytes: 0,
        clip_count: 0,
    };
    let mut clips = Vec::new();
    check_current()?;
    let clips_directory = output_directory.join("clips");
    if let Some(entries) = read_library_directory(&clips_directory)? {
        for entry in entries {
            check_current()?;
            let entry = entry.context("failed to read a clips directory entry")?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("mp4") {
                continue;
            }
            let file_size_bytes = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            // All existing MP4 entries count toward storage, even without a displayable ID.
            usage.clips_bytes += file_size_bytes;
            usage.clip_count += 1;
            let Some(filename) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Some((game_timestamp, clip_timestamp)) = parse_clip_filename(filename) else {
                continue;
            };
            let thumbnail = clips_directory.join(format!("{filename}.jpg"));
            let has_thumbnail = thumbnail.exists();
            let source = sources.get(game_timestamp);
            clips.push(LibraryClip {
                filename: filename.to_owned(),
                game_timestamp: game_timestamp.to_owned(),
                clip_timestamp: clip_timestamp.to_owned(),
                duration_ms: None,
                file_size_bytes,
                thumbnail_path: has_thumbnail.then(|| thumbnail.to_string_lossy().into_owned()),
                thumbnail_url: has_thumbnail.then(|| format!("{origin}/clips/{filename}.jpg")),
                video_url: format!("{origin}/clips/{filename}.mp4"),
                source_champion: source.map(|game| game.champion.clone()),
                source_date: source.map(|game| game.recorded_at.clone()),
            });
        }
    }
    check_current()?;
    clips.sort_by(|left, right| right.clip_timestamp.cmp(&left.clip_timestamp));
    Ok(LibrarySnapshot {
        token,
        games,
        clips,
        usage,
    })
}

fn read_library_directory(path: &Path) -> Result<Option<fs::ReadDir>> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(Some(entries)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error)
            .with_context(|| format!("failed to read library directory {}", path.display())),
    }
}

pub fn list_games(output_directory: &Path) -> Result<Vec<GameSummary>> {
    scan_games(output_directory, || Ok(()), read_game_summary)
}

fn scan_games(
    output_directory: &Path,
    check_current: impl Fn() -> Result<()>,
    mut read_summary: impl FnMut(&Path, String) -> GameSummary,
) -> Result<Vec<GameSummary>> {
    check_current()?;
    let games_directory = output_directory.join("games");
    let Some(entries) = read_library_directory(&games_directory)? else {
        return Ok(Vec::new());
    };

    let mut games = Vec::new();
    for entry in entries {
        check_current()?;
        let entry = entry.context("failed to read a games directory entry")?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let timestamp = entry.file_name().to_string_lossy().into_owned();
        if !valid_game_id(&timestamp) {
            continue;
        }
        games.push(read_summary(&path, timestamp));
    }

    games.sort_by(compare_games);
    Ok(games)
}

pub(crate) fn replay_descriptor(
    output_directory: &Path,
    origin: &str,
    timestamp: &str,
    snapshot_token: String,
) -> Result<ReplayDescriptor> {
    if !valid_game_id(timestamp) {
        bail!("invalid game identifier");
    }
    let directory = output_directory.join("games").join(timestamp);
    let metadata = read_metadata(&directory.join(METADATA_JSON))?;
    let video =
        fs::metadata(directory.join(VIDEO_MP4)).context("recording video is unavailable")?;
    if !video.is_file() || video.len() == 0 {
        bail!("recording video is unavailable");
    }
    Ok(ReplayDescriptor {
        snapshot_token,
        game_timestamp: timestamp.to_owned(),
        video_url: format!("{origin}/games/{timestamp}/video.mp4"),
        media_timeline: metadata.media_timeline,
    })
}

pub fn list_clips(
    output_directory: &Path,
    origin: &str,
    ffprobe: Option<&Path>,
) -> Result<Vec<ClipSummary>> {
    let clips_directory = output_directory.join("clips");
    if !clips_directory.exists() {
        return Ok(Vec::new());
    }

    let games = list_games(output_directory)?
        .into_iter()
        .map(|game| (game.timestamp.clone(), game))
        .collect::<HashMap<_, _>>();
    let mut clips = Vec::new();
    for entry in fs::read_dir(&clips_directory).with_context(|| {
        format!(
            "failed to read clips directory {}",
            clips_directory.display()
        )
    })? {
        let entry = entry.context("failed to read a clips directory entry")?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("mp4") {
            continue;
        }
        let Some(filename) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some((game_timestamp, clip_timestamp)) = parse_clip_filename(filename) else {
            continue;
        };
        let thumbnail = clips_directory.join(format!("{filename}.jpg"));
        let source = games.get(game_timestamp);
        clips.push(ClipSummary {
            filename: filename.to_owned(),
            game_timestamp: game_timestamp.to_owned(),
            clip_timestamp: clip_timestamp.to_owned(),
            duration_ms: ffprobe
                .and_then(|tool| probe_duration_ms(tool, &path))
                .unwrap_or(0),
            file_size_bytes: entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
            thumbnail_path: thumbnail
                .exists()
                .then(|| thumbnail.to_string_lossy().into_owned()),
            thumbnail_url: thumbnail
                .exists()
                .then(|| format!("{origin}/clips/{filename}.jpg")),
            video_url: format!("{origin}/clips/{filename}.mp4"),
            source_champion: source.map(|game| game.champion.clone()),
            source_date: source.map(|game| game.recorded_at.clone()),
        });
    }
    clips.sort_by(|left, right| right.clip_timestamp.cmp(&left.clip_timestamp));
    Ok(clips)
}

pub fn storage_usage(output_directory: &Path) -> Result<StorageUsage> {
    let games = list_games(output_directory)?;
    let games_bytes = games.iter().map(|game| game.video_size_bytes).sum();
    let clips_directory = output_directory.join("clips");
    let mut clips_bytes = 0;
    let mut clip_count = 0;
    if clips_directory.exists() {
        for entry in fs::read_dir(&clips_directory).with_context(|| {
            format!(
                "failed to read clips directory {}",
                clips_directory.display()
            )
        })? {
            let entry = entry.context("failed to read a clips directory entry")?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("mp4") {
                clips_bytes += entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                clip_count += 1;
            }
        }
    }
    Ok(StorageUsage {
        games_bytes,
        clips_bytes,
        game_count: games.len() as u64,
        clip_count,
    })
}

pub fn save_game(output_directory: &Path, timestamp: &str, saved: bool) -> Result<()> {
    if !valid_game_id(timestamp) {
        bail!("invalid game identifier");
    }
    let metadata_path = output_directory
        .join("games")
        .join(timestamp)
        .join(METADATA_JSON);
    if !metadata_path.exists() {
        bail!("incomplete recordings cannot be saved");
    }
    let mut metadata = read_metadata(&metadata_path)?;
    metadata.saved = saved;
    let mut bytes = serde_json::to_vec_pretty(&metadata).context("failed to serialize metadata")?;
    bytes.push(b'\n');
    config::write_atomic(&metadata_path, &bytes)
}

pub fn delete_game(output_directory: &Path, timestamp: &str) -> Result<()> {
    if !valid_game_id(timestamp) {
        bail!("invalid game identifier");
    }
    let game_directory = output_directory.join("games").join(timestamp);
    if !game_directory.is_dir() {
        bail!("recording does not exist");
    }
    // Saved intent is media metadata, independent of optional telemetry. An
    // unreadable metadata file cannot safely authorize destructive deletion.
    if read_metadata(&game_directory.join(METADATA_JSON))?.saved {
        bail!("saved recordings must be unsaved before deletion");
    }
    fs::remove_dir_all(&game_directory)
        .with_context(|| format!("failed to delete recording {}", game_directory.display()))
}

pub fn delete_clip(output_directory: &Path, filename: &str) -> Result<()> {
    if parse_clip_filename(filename).is_none() {
        bail!("invalid clip filename");
    }
    let clips_directory = output_directory.join("clips");
    let video = clips_directory.join(format!("{filename}.mp4"));
    if !video.is_file() {
        bail!("clip does not exist");
    }
    fs::remove_file(&video)
        .with_context(|| format!("failed to delete clip {}", video.display()))?;
    let thumbnail = clips_directory.join(format!("{filename}.jpg"));
    if thumbnail.exists() {
        fs::remove_file(&thumbnail)
            .with_context(|| format!("failed to delete thumbnail {}", thumbnail.display()))?;
    }
    Ok(())
}

pub fn run_auto_delete(output_directory: &Path, auto_delete_days: u32) -> Result<AutoDeleteResult> {
    run_auto_delete_at(
        output_directory,
        auto_delete_days,
        OffsetDateTime::now_utc(),
    )
}

fn run_auto_delete_at(
    output_directory: &Path,
    auto_delete_days: u32,
    now: OffsetDateTime,
) -> Result<AutoDeleteResult> {
    if auto_delete_days == 0 {
        return Ok(AutoDeleteResult { deleted_count: 0 });
    }
    let threshold = time::Duration::days(i64::from(auto_delete_days));
    let mut deleted_count = 0;
    for game in list_games(output_directory)? {
        if game.saved || game.incomplete || game.recorded_at.is_empty() {
            continue;
        }
        let Ok(recorded_at) = OffsetDateTime::parse(&game.recorded_at, &Rfc3339) else {
            continue;
        };
        if now - recorded_at <= threshold {
            continue;
        }
        let directory = output_directory.join("games").join(&game.timestamp);
        fs::remove_dir_all(&directory)
            .with_context(|| format!("failed to auto-delete {}", directory.display()))?;
        deleted_count += 1;
    }
    Ok(AutoDeleteResult { deleted_count })
}

fn read_game_summary(directory: &Path, timestamp: String) -> GameSummary {
    let video_size_bytes = fs::metadata(directory.join(VIDEO_MP4))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let Ok(metadata) = read_metadata(&directory.join(METADATA_JSON)) else {
        return incomplete_summary(timestamp, video_size_bytes);
    };
    let mut game = game_summary_from_bundle(video_size_bytes, timestamp, &metadata);
    if game.video_available {
        let candidate = crate::league_match::read(directory, &metadata.media_id);
        game.league_result =
            crate::league_result::read(directory, &metadata.media_id, candidate.as_ref())
                .map(|r| r.summary());
    }
    game
}

fn game_summary_from_bundle(
    video_size_bytes: u64,
    timestamp: String,
    metadata: &MetadataDocument,
) -> GameSummary {
    GameSummary {
        media_id: Some(metadata.media_id.clone()),
        timestamp,
        champion: metadata
            .local_player_champion
            .clone()
            .unwrap_or_else(|| "Unknown".to_owned()),
        game_mode: metadata
            .game_mode
            .clone()
            .unwrap_or_else(|| "Unknown".to_owned()),
        duration_ms: display_duration_ms(&metadata.media_timeline),
        recorded_at: metadata.recorded_at.clone(),
        // History and replay semantics live in the indexed source data. These
        // compatibility fields deliberately carry no inferred event history.
        kills: 0,
        deaths: 0,
        assists: 0,
        summoner_spells: Vec::new(),
        keystone_id: None,
        items: Vec::new(),
        participants: Vec::new(),
        saved: metadata.saved,
        incomplete: false,
        video_size_bytes,
        video_available: video_size_bytes > 0,
        league_result: None,
    }
}

fn incomplete_summary(timestamp: String, video_size_bytes: u64) -> GameSummary {
    GameSummary {
        media_id: None,
        recorded_at: String::new(),
        timestamp,
        champion: "Unknown".to_owned(),
        game_mode: "Unknown".to_owned(),
        duration_ms: 0,
        kills: 0,
        deaths: 0,
        assists: 0,
        summoner_spells: Vec::new(),
        keystone_id: None,
        items: Vec::new(),
        participants: Vec::new(),
        saved: false,
        incomplete: true,
        video_size_bytes,
        video_available: video_size_bytes > 0,
        league_result: None,
    }
}

pub(crate) fn recording_export_authority(directory: &Path) -> Result<ExportRecordingAuthority> {
    let metadata = read_metadata(&directory.join(METADATA_JSON))?;
    Ok(ExportRecordingAuthority {
        media_timeline: metadata.media_timeline,
        encoder_used: metadata.encoder_used,
    })
}

fn read_metadata(path: &Path) -> Result<MetadataDocument> {
    use std::io::Read;
    const MAX_METADATA_BYTES: u64 = 64 * 1024;
    let file = fs::File::open(path).context("recording metadata is unavailable")?;
    let info = file
        .metadata()
        .context("could not inspect recording metadata")?;
    if !info.is_file() || info.len() > MAX_METADATA_BYTES {
        bail!("recording metadata exceeds its bounded format")
    }
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .context("could not read recording metadata")?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        bail!("recording metadata exceeds its bounded format")
    }
    let metadata: MetadataDocument =
        serde_json::from_slice(&bytes).context("recording metadata is malformed")?;
    if metadata.schema_version != REPLAY_TIME_SCHEMA_VERSION {
        bail!("rebuild development library: metadata.json is not schema version 2")
    }
    metadata
        .media_timeline
        .validate()
        .context("rebuild development library: metadata.json media_timeline is invalid")?;
    if metadata.media_id != metadata.media_timeline.media_id {
        bail!("rebuild development library: metadata.json media_id does not match media_timeline")
    }
    if metadata._recording_codec != metadata.media_timeline.video.codec
        || metadata._capture_backend != metadata.media_timeline.producer.backend
        || metadata._media_runtime_id != metadata.media_timeline.producer.media_runtime_id
    {
        bail!(
            "rebuild development library: metadata.json recorder facts do not match media_timeline"
        )
    }
    if let Some(capture) = &metadata._capture
        && (capture._schema_version != 1
            || capture._backend != metadata._capture_backend
            || capture._media_runtime_id != metadata._media_runtime_id)
    {
        bail!(
            "rebuild development library: metadata.json capture facts do not match recorder facts"
        )
    }
    Ok(metadata)
}

pub(crate) fn map_game_tick(
    calibration: Option<&GameCalibrationV2>,
    game_tick: GameTick,
    media_timeline: &MediaTimelineV2,
) -> Result<MappedReplayTime> {
    match calibration {
        Some(calibration) => calibration
            .map_game_tick(game_tick, media_timeline.video.replay_end)
            .context("rebuild development library: game-log affine mapping is invalid"),
        None => Ok(MappedReplayTime::Unavailable {
            reason: MappingUnavailableReason::CalibrationUnavailable,
        }),
    }
}

fn display_duration_ms(media_timeline: &MediaTimelineV2) -> u64 {
    media_timeline.video.replay_end.get() / (REPLAY_TICKS_PER_SECOND / 1_000)
}

#[cfg(test)]
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| {
        format!(
            "rebuild development library: failed to read {}",
            path.display()
        )
    })?;
    serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "rebuild development library: failed to parse {}",
            path.display()
        )
    })
}

fn compare_games(left: &GameSummary, right: &GameSummary) -> Ordering {
    match (left.incomplete, right.incomplete) {
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        _ => right
            .recorded_at
            .cmp(&left.recorded_at)
            .then_with(|| right.timestamp.cmp(&left.timestamp)),
    }
}

fn parse_clip_filename(filename: &str) -> Option<(&str, &str)> {
    let (game_timestamp, clip_timestamp) = filename.split_once('_')?;
    (valid_game_id(game_timestamp) && valid_timestamp(clip_timestamp))
        .then_some((game_timestamp, clip_timestamp))
}

pub(crate) fn valid_game_id(game_id: &str) -> bool {
    let Some((timestamp, suffix)) = game_id.split_once('-') else {
        return valid_timestamp(game_id);
    };
    if !valid_timestamp(timestamp) || !valid_timestamp(suffix) || suffix.contains('-') {
        return false;
    }
    suffix
        .parse::<u32>()
        .is_ok_and(|value| (1..1000).contains(&value) && value.to_string() == suffix)
}

pub(crate) fn valid_timestamp(timestamp: &str) -> bool {
    !timestamp.is_empty() && timestamp.bytes().all(|byte| byte.is_ascii_digit())
}

pub(crate) fn valid_clip_asset(filename: &str) -> bool {
    let Some((stem, extension)) = filename.rsplit_once('.') else {
        return false;
    };
    matches!(extension, "mp4" | "jpg") && parse_clip_filename(stem).is_some()
}

fn probe_duration_ms(ffprobe: &Path, path: &Path) -> Option<u64> {
    let mut command = Command::new(ffprobe);
    command.args([
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
    ]);
    command.arg(path);
    hide_console(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let seconds = String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()?;
    (seconds.is_finite() && seconds >= 0.0).then_some((seconds * 1_000.0).round() as u64)
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_command: &mut Command) {}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use chronobreak_replay_time::{
        AudioTimelineV2, CalibrationStatus, ContainerTimelineV2, GameCalibrationV2, MediaPts,
        ProducerEvidenceV2, Rational, ReplayTick, SignedReplayTick, VideoTimelineV2,
    };
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    const MEDIA_ID: &str = "123e4567-e89b-42d3-a456-426614174000";

    fn timeline() -> MediaTimelineV2 {
        MediaTimelineV2 {
            schema_version: REPLAY_TIME_SCHEMA_VERSION,
            replay_ticks_per_second: REPLAY_TICKS_PER_SECOND,
            media_id: MediaId::parse(MEDIA_ID).unwrap(),
            video: VideoTimelineV2 {
                codec: "h264".to_owned(),
                profile: Some("High".to_owned()),
                time_base: Rational::positive(1, 15_360, "video time base").unwrap(),
                first_pts: MediaPts::new(0),
                frame_rate: Rational::positive(60, 1, "frame rate").unwrap(),
                frame_count: chronobreak_replay_time::FrameBoundary::new(600).unwrap(),
                one_past_last_pts: MediaPts::new(153_600),
                replay_end: ReplayTick::new(480_000_000).unwrap(),
                exact_cfr: true,
            },
            audio: AudioTimelineV2 {
                present: true,
                codec: Some("aac".to_owned()),
                sample_rate: Some(48_000),
                time_base: Some(Rational::positive(1, 48_000, "audio time base").unwrap()),
                first_pts: Some(MediaPts::new(0)),
                replay_start: Some(SignedReplayTick::new(0).unwrap()),
                replay_end: Some(SignedReplayTick::new(480_000_000).unwrap()),
            },
            container: ContainerTimelineV2 {
                start_seconds: Rational::new(0, 1).unwrap(),
                duration_seconds: Rational::positive(10, 1, "duration").unwrap(),
            },
            producer: ProducerEvidenceV2 {
                backend: "native".to_owned(),
                expected_frame_rate: Rational::positive(60, 1, "expected frame rate").unwrap(),
                expected_frame_count: chronobreak_replay_time::FrameBoundary::new(600).unwrap(),
                media_runtime_id: "test-runtime".to_owned(),
            },
            capture: None,
        }
    }

    fn available_calibration() -> serde_json::Value {
        serde_json::to_value(GameCalibrationV2 {
            status: CalibrationStatus::Available,
            sample_count: 5,
            first_game_tick: Some(GameTick::new(0).unwrap()),
            last_game_tick: Some(GameTick::new(20_000_000).unwrap()),
            replay_tick_at_game_zero: Some(SignedReplayTick::new(-48_000_000).unwrap()),
            maximum_rtt_game_ticks: 1_000,
            maximum_residual_game_ticks: 1,
            uncertainty_game_ticks: 1,
        })
        .unwrap()
    }

    // The disposable index tests need committed source observations.  This is
    // deliberately a source fixture: no library-facing snapshot or replay
    // projection is written alongside the recording media.
    fn write_index_source_fixture(directory: &Path) {
        use chronobreak_league_data::observation as obs;
        let source = obs::SourceEpoch {
            media_id: MediaId::parse(MEDIA_ID).unwrap(),
            run_id: MediaId::parse("123e4567-e89b-42d3-a456-426614174001").unwrap(),
            epoch: 0,
        };
        let header = obs::journal::Header::new(&source);
        let file = fs::File::create(directory.join("league_observations.jsonl")).unwrap();
        let mut writer = obs::journal::Writer::new(file, header.clone()).unwrap();
        let mut state = obs::checkpoint::State::new(header, writer.position().clone());
        let evidence = |id, endpoint, game_stats| obs::Evidence {
            id: obs::ObservationId(id),
            source: source.clone(),
            endpoint,
            timing: obs::RequestTiming {
                start_us: id * 1_000,
                end_us: id * 1_000 + 10,
                received_at: "2026-08-05T10:00:00Z".into(),
            },
            game_stats,
            calibration: Some(obs::ObservationId(1)),
        };
        let calibration: GameCalibrationV2 =
            serde_json::from_value(available_calibration()).unwrap();
        let player = obs::RosterPlayer {
            participant: Some(obs::ParticipantId(1)),
            riot_id: Some("Player#EUW".into()),
            team: Some("ORDER".into()),
            champion_name: Some("Syndra".into()),
            level: Some(2),
            scores: Some(obs::Scores {
                creep_score: Some(8),
                ..Default::default()
            }),
            ..Default::default()
        };
        let roster = |id, game_stats| obs::Record::RosterObservation {
            evidence: evidence(
                id,
                obs::Endpoint::PlayerList,
                Some(obs::ObservationId(game_stats)),
            ),
            roster: obs::roster::RosterObservation {
                full: true,
                references: vec![obs::roster::PayloadId(1)],
                payloads: vec![obs::roster::SharedPlayer {
                    id: obs::roster::PayloadId(1),
                    player: player.clone(),
                }],
            },
        };
        let tracker = obs::events::EventTracker::new(source.clone());
        let resolved_local = || {
            vec![obs::events::ActorReference {
                participant: Some(obs::ParticipantId(1)),
                status: obs::events::ActorStatus::Resolved,
            }]
        };
        let unresolved = || {
            vec![obs::events::ActorReference {
                participant: None,
                status: obs::events::ActorStatus::Unresolved,
            }]
        };
        let events = tracker
            .prepare(
                &source,
                vec![
                    obs::Event {
                        source_id: Some(0),
                        source_time: Some(0.into()),
                        game_tick: Some(GameTick::new(0).unwrap()),
                        name: Some("ChampionKill".into()),
                        killer: Some("Player#EUW".into()),
                        victim: Some("Enemy#EUW".into()),
                        actors: Some(
                            [
                                (obs::events::ActorRole::Killer, resolved_local()),
                                (obs::events::ActorRole::Victim, unresolved()),
                            ]
                            .into(),
                        ),
                        ..Default::default()
                    },
                    obs::Event {
                        source_id: Some(1),
                        source_time: Some(1.into()),
                        game_tick: Some(GameTick::new(1_000_000).unwrap()),
                        name: Some("ChampionKill".into()),
                        killer: Some("Enemy#EUW".into()),
                        victim: Some("Player#EUW".into()),
                        actors: Some(
                            [
                                (obs::events::ActorRole::Killer, unresolved()),
                                (obs::events::ActorRole::Victim, resolved_local()),
                            ]
                            .into(),
                        ),
                        ..Default::default()
                    },
                    obs::Event {
                        source_id: Some(2),
                        source_time: Some(2.into()),
                        game_tick: Some(GameTick::new(2_000_000).unwrap()),
                        name: Some("DragonKill".into()),
                        stolen: Some(true),
                        recipient: Some("Player#EUW".into()),
                        actors: Some(
                            [(obs::events::ActorRole::Recipient, resolved_local())].into(),
                        ),
                        ..Default::default()
                    },
                ],
            )
            .unwrap();
        let rows = vec![
            obs::Record::Calibration {
                evidence: evidence(1, obs::Endpoint::GameStats, None),
                calibration,
            },
            obs::Record::Observation {
                observation: Box::new(obs::Observation::Success {
                    evidence: evidence(2, obs::Endpoint::GameStats, None),
                    payload: obs::Payload::GameStats(obs::GameStats {
                        game_tick: Some(GameTick::new(0).unwrap()),
                        game_mode: Some("CLASSIC".into()),
                        ..Default::default()
                    }),
                }),
            },
            obs::Record::LocalParticipant {
                evidence: evidence(3, obs::Endpoint::PlayerList, Some(obs::ObservationId(2))),
                participant: obs::ParticipantId(1),
            },
            roster(3, 2),
            obs::Record::Observation {
                observation: Box::new(obs::Observation::Success {
                    evidence: evidence(4, obs::Endpoint::GameStats, None),
                    payload: obs::Payload::GameStats(obs::GameStats {
                        game_tick: Some(GameTick::new(3_000_000).unwrap()),
                        game_mode: Some("CLASSIC".into()),
                        ..Default::default()
                    }),
                }),
            },
            roster(5, 4),
            obs::Record::EventObservation {
                evidence: evidence(6, obs::Endpoint::Events, Some(obs::ObservationId(4))),
                revisions: events.revisions().to_vec(),
            },
        ];
        let position = writer.commit(&rows).unwrap();
        state.apply(&rows, &position).unwrap();
        fs::write(
            directory.join("league_checkpoint.json"),
            state.snapshot().encode().unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join("game_log.json"),
            obs::manifest::Manifest::from_checkpoint(state.snapshot())
                .encode()
                .unwrap(),
        )
        .unwrap();
    }

    pub(crate) fn write_game(
        root: &Path,
        timestamp: &str,
        recorded_at: &str,
        saved: bool,
    ) -> PathBuf {
        let game = root.join("games").join(timestamp);
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join(VIDEO_MP4), b"video").unwrap();
        let media_timeline = serde_json::to_value(timeline()).unwrap();
        fs::write(
            game.join(METADATA_JSON),
            serde_json::to_vec(&json!({
                "schema_version": 2,
                "media_id": MEDIA_ID,
                "media_timeline": media_timeline,
                "recorded_at": recorded_at,
                "game_mode": "CLASSIC",
                "local_player_summoner_name": "Player#EUW",
                "local_player_champion": "Syndra",
                "local_player_team": "ORDER",
                "encoder_used": "nvenc",
                "recording_codec": "h264",
                "recording_profile": "high",
                "recording_resolution": "1920x1080",
                "capture_backend": "native",
                "capture_adapter_luid": null,
                "capture_adapter_name": null,
                "capture_output": null,
                "encoder_interop": null,
                "media_runtime_id": "test-runtime",
                "capture_support_label": "fixture",
                "source_frames_surfaced": 0,
                "source_frames_superseded": 0,
                "cfr_duplicates": 0,
                "cfr_discards": 0,
                "pool_recreations": 0,
                "capture": null,
                "saved": saved
            }))
            .unwrap(),
        )
        .unwrap();
        write_index_source_fixture(&game);
        game
    }

    include!("replay_descriptor_tests.rs");
    include!("league_match_tests.rs");

    fn core_snapshot(root: &Path) -> Result<LibrarySnapshot> {
        build_library_snapshot(
            root,
            "http://127.0.0.1:9000/cap/test",
            "test-scan".into(),
            || Ok(()),
        )
    }

    #[test]
    fn snapshot_derives_all_views_from_one_summary_read_per_game_without_probes() {
        let root = tempdir().unwrap();
        write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
        write_game(root.path(), "1786000001-1", "2026-08-06T10:00:00Z", true);
        let clips = root.path().join("clips");
        fs::create_dir(&clips).unwrap();
        // Deliberately non-media fixture bytes: core scan must not depend on probing.
        fs::write(clips.join("1786000000_100.mp4"), b"clip-one").unwrap();
        fs::write(clips.join("1786000000_100.jpg"), b"thumbnail").unwrap();
        fs::write(clips.join("999_200.mp4"), b"source-less").unwrap();
        fs::write(clips.join("unrecognized.mp4"), b"storage-only").unwrap();
        fs::write(clips.join("1786000000_300.MP4"), b"ignored-extension").unwrap();
        fs::create_dir(clips.join("1786000000_400.mp4")).unwrap();
        fs::create_dir(root.path().join("games/not-a-game")).unwrap();
        let mut reads = HashMap::new();
        let snapshot = build_library_snapshot_with(
            root.path(),
            "http://127.0.0.1:9000/cap/test",
            "one-scan".into(),
            || Ok(()),
            |path, timestamp| {
                *reads.entry(timestamp.clone()).or_insert(0) += 1;
                let game = read_game_summary(path, timestamp);
                // Change the backing files after their read. Clip enrichment and
                // storage must use the collected summary, not re-read these files.
                fs::write(path.join(METADATA_JSON), b"changed after read").unwrap();
                fs::write(path.join(VIDEO_MP4), b"changed media length").unwrap();
                game
            },
        )
        .unwrap();
        assert_eq!(reads.len(), 2);
        assert!(reads.values().all(|count| *count == 1));
        assert_eq!(
            snapshot
                .games
                .iter()
                .map(|g| g.timestamp.as_str())
                .collect::<Vec<_>>(),
            ["1786000001-1", "1786000000"]
        );
        assert!(snapshot.games.iter().all(|game| !game.incomplete));
        assert_eq!(
            snapshot.usage,
            StorageUsage {
                games_bytes: 10,
                clips_bytes: 31,
                game_count: 2,
                clip_count: 3
            }
        );
        assert_eq!(
            snapshot
                .clips
                .iter()
                .map(|clip| clip.filename.as_str())
                .collect::<Vec<_>>(),
            ["999_200", "1786000000_100"]
        );
        let orphan = &snapshot.clips[0];
        assert_eq!(orphan.source_champion, None);
        assert_eq!(orphan.source_date, None);
        assert_eq!(orphan.thumbnail_url, None);
        let sourced = &snapshot.clips[1];
        assert_eq!(sourced.source_champion.as_deref(), Some("Syndra"));
        assert_eq!(
            sourced.source_date.as_deref(),
            Some(snapshot.games[1].recorded_at.as_str())
        );
        assert_eq!(sourced.file_size_bytes, 8);
        assert!(sourced.thumbnail_path.is_some());
        assert_eq!(
            sourced.thumbnail_url.as_deref(),
            Some("http://127.0.0.1:9000/cap/test/clips/1786000000_100.jpg")
        );
        assert!(snapshot.clips.iter().all(|clip| clip.duration_ms.is_none()));
        assert!(serde_json::to_value(&snapshot).unwrap()["clips"][0]["duration_ms"].is_null());
        // A later explicit scan is rebuilt from disk, not the previous result.
        let rebuilt = core_snapshot(root.path()).unwrap();
        assert!(rebuilt.games.iter().all(|game| game.incomplete));
        assert_eq!(rebuilt.usage.games_bytes, 40);
    }

    #[test]
    fn snapshot_requires_strict_media_without_reading_optional_source_data() {
        let root = tempdir().unwrap();
        write_game(root.path(), "100", "2026-08-05T10:00:00Z", false);
        write_game(root.path(), "101", "2026-08-05T10:00:00Z", false);
        let invalid = write_game(root.path(), "102", "2026-08-05T10:00:00Z", false);
        fs::write(invalid.join(METADATA_JSON), b"invalid").unwrap();
        let v1 = write_game(root.path(), "103", "2026-08-05T10:00:00Z", false);
        let mut metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(v1.join(METADATA_JSON)).unwrap()).unwrap();
        metadata["schema_version"] = json!(1);
        fs::write(
            v1.join(METADATA_JSON),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        let no_video = write_game(root.path(), "104", "2026-08-05T10:00:00Z", false);
        fs::remove_file(no_video.join(VIDEO_MP4)).unwrap();
        fs::create_dir(root.path().join("games/105")).unwrap();
        let snapshot = core_snapshot(root.path()).unwrap();
        assert_eq!(snapshot.usage.game_count, 6);
        assert_eq!(snapshot.usage.games_bytes, 20);
        assert_eq!(snapshot.games.iter().filter(|g| g.incomplete).count(), 3);
        assert!(
            snapshot
                .games
                .iter()
                .find(|g| g.timestamp == "101")
                .is_some_and(|g| !g.incomplete && g.video_available && g.participants.is_empty())
        );
        assert!(
            snapshot
                .games
                .iter()
                .filter(|g| g.incomplete)
                .all(|g| g.participants.is_empty())
        );
        assert!(
            snapshot
                .games
                .iter()
                .find(|g| g.timestamp == "100")
                .is_some_and(|g| !g.incomplete && g.video_available)
        );
        assert!(
            snapshot
                .games
                .iter()
                .find(|g| g.timestamp == "104")
                .is_some_and(|g| !g.video_available)
        );
        for timestamp in ["102", "103", "104", "105"] {
            assert!(
                replay_descriptor(root.path(), "test-origin", timestamp, "token".into()).is_err()
            );
        }
    }

    #[test]
    fn snapshot_missing_directories_are_empty_but_enumeration_failures_are_errors() {
        let root = tempdir().unwrap();
        let empty = core_snapshot(root.path()).unwrap();
        assert_eq!(
            empty.usage,
            StorageUsage {
                games_bytes: 0,
                clips_bytes: 0,
                game_count: 0,
                clip_count: 0
            }
        );
        for name in ["games", "clips"] {
            let blocked = root.path().join(name);
            fs::write(&blocked, b"not a directory").unwrap();
            assert!(core_snapshot(root.path()).is_err());
            fs::remove_file(blocked).unwrap();
        }
    }

    #[test]
    fn snapshot_cancellation_stops_between_game_entries() {
        use std::cell::Cell;
        let root = tempdir().unwrap();
        for timestamp in ["100", "101", "102"] {
            write_game(root.path(), timestamp, "2026-08-05T10:00:00Z", false);
        }
        let reads = Cell::new(0);
        let result = build_library_snapshot_with(
            root.path(),
            "test-origin",
            "cancelled".into(),
            || {
                anyhow::ensure!(reads.get() == 0, "superseded");
                Ok(())
            },
            |path, timestamp| {
                reads.set(reads.get() + 1);
                read_game_summary(path, timestamp)
            },
        );
        assert!(result.is_err());
        assert_eq!(reads.get(), 1);
    }

    #[test]
    fn save_requires_and_preserves_strict_schema_v2_metadata() {
        let root = tempdir().unwrap();
        let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
        save_game(root.path(), "1786000000", true).unwrap();
        let metadata: MetadataDocument = read_metadata(&game.join(METADATA_JSON)).unwrap();
        assert!(metadata.saved);
        assert_eq!(metadata.media_id.as_str(), MEDIA_ID);
    }

    #[test]
    fn keeps_incomplete_bundles_at_the_bottom() {
        let root = tempdir().unwrap();
        write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
        let incomplete = root.path().join("games").join("1786000001");
        fs::create_dir_all(&incomplete).unwrap();
        fs::write(incomplete.join(VIDEO_MP4), b"partial").unwrap();
        let games = list_games(root.path()).unwrap();
        assert_eq!(games.len(), 2);
        assert!(!games[0].incomplete);
        assert!(games[1].incomplete);
    }

    #[test]
    fn validates_clip_asset_names() {
        assert!(valid_clip_asset("1786000000_1786000100.mp4"));
        assert!(valid_clip_asset("1786000000_1786000100.jpg"));
        assert!(valid_clip_asset("1786000000-1_1786000100.mp4"));
        assert!(!valid_clip_asset("../video.mp4"));
        assert!(!valid_clip_asset("1786000000_notes.mp4"));
    }

    #[test]
    fn accepts_only_canonical_recorder_collision_suffixes_as_game_ids() {
        assert!(valid_game_id("1786000000"));
        assert!(valid_game_id("1786000000-1"));
        assert!(valid_game_id("1786000000-999"));
        assert!(!valid_game_id("1786000000-0"));
        assert!(!valid_game_id("1786000000-01"));
        assert!(!valid_game_id("1786000000-1000"));
        assert!(!valid_game_id("1786000000-1-2"));
        assert!(!valid_game_id("../1786000000-1"));
    }
}
