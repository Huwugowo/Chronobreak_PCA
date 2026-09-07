use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
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
const GAME_LOG_JSON: &str = "game_log.json";
const VIDEO_MP4: &str = "video.mp4";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GameSummary {
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
    pub saved: bool,
    pub incomplete: bool,
    pub video_size_bytes: u64,
    pub video_available: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ViewerEvent {
    pub event_type: String,
    pub game_tick: GameTick,
    pub mapped_replay_time: MappedReplayTime,
    pub killer: Option<String>,
    pub victim: Option<String>,
    pub assisters: Vec<String>,
    pub dragon_type: Option<String>,
    pub kill_streak: Option<u32>,
    pub acer: Option<String>,
    pub acing_team: Option<String>,
    pub turret: Option<String>,
    pub inhibitor: Option<String>,
    pub result: Option<String>,
    pub relation: EventRelation,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventRelation {
    Ally,
    Enemy,
    Neutral,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReplayParticipant {
    pub summoner_name: String,
    pub champion: String,
    pub relation: EventRelation,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlayerTimelinePoint {
    pub game_tick: GameTick,
    pub mapped_replay_time: MappedReplayTime,
    pub cs: u32,
    pub level: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KdaTimelinePoint {
    pub mapped_replay_time: MappedReplayTime,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlaybackProbe {
    pub game: GameSummary,
    pub video_url: String,
    pub media_timeline: MediaTimelineV2,
    pub local_player_name: Option<String>,
    pub participants: Vec<ReplayParticipant>,
    pub player_timeline: Vec<PlayerTimelinePoint>,
    pub kda_timeline: Vec<KdaTimelinePoint>,
    pub events: Vec<ViewerEvent>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GameLogDocument {
    schema_version: u32,
    media_id: MediaId,
    calibration: Option<GameCalibrationV2>,
    snapshots: Vec<GameSnapshot>,
    events: Vec<GameEvent>,
    #[serde(rename = "snapshot_derived_changes")]
    _snapshot_derived_changes: Vec<SnapshotChangeDocument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GameSnapshot {
    game_tick: GameTick,
    players: Vec<SnapshotPlayer>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotPlayer {
    summoner_name: String,
    team: String,
    champion: String,
    #[serde(rename = "gold")]
    _gold: Option<i64>,
    #[serde(rename = "hp")]
    _hp: Option<i64>,
    #[serde(rename = "hp_max")]
    _hp_max: Option<i64>,
    cs: u32,
    level: u32,
    items: Vec<SnapshotItem>,
    summoner_spells: Option<Vec<String>>,
    keystone_id: Option<u32>,
    #[serde(rename = "rune_ids")]
    _rune_ids: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotItem {
    item_id: u32,
    slot: u32,
    #[serde(rename = "count")]
    _count: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotChangeDocument {
    #[serde(rename = "game_tick")]
    _game_tick: GameTick,
    #[serde(rename = "player")]
    _player: String,
    #[serde(rename = "change_type")]
    _change_type: SnapshotChangeTypeDocument,
    #[serde(rename = "item_id")]
    _item_id: Option<u32>,
    #[serde(rename = "new_level")]
    _new_level: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
enum SnapshotChangeTypeDocument {
    ItemPurchased,
    ItemSold,
    LevelUp,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GameEvent {
    #[serde(rename = "type")]
    event_type: String,
    game_tick: GameTick,
    killer: Option<String>,
    victim: Option<String>,
    assisters: Option<Vec<String>>,
    dragon_type: Option<String>,
    #[serde(rename = "stolen")]
    _stolen: Option<bool>,
    kill_streak: Option<u32>,
    acer: Option<String>,
    acing_team: Option<String>,
    turret: Option<String>,
    inhibitor: Option<String>,
    result: Option<String>,
}

pub fn list_games(output_directory: &Path) -> Result<Vec<GameSummary>> {
    let games_directory = output_directory.join("games");
    if !games_directory.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&games_directory).with_context(|| {
        format!(
            "failed to read games directory {}",
            games_directory.display()
        )
    })?;

    let mut games = Vec::new();
    for entry in entries {
        let entry = entry.context("failed to read a games directory entry")?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let timestamp = entry.file_name().to_string_lossy().into_owned();
        if !valid_game_id(&timestamp) {
            continue;
        }
        games.push(read_game_summary(&path, timestamp));
    }

    games.sort_by(compare_games);
    Ok(games)
}

pub fn playback_probe(
    output_directory: &Path,
    origin: &str,
    timestamp: &str,
) -> Result<PlaybackProbe> {
    if !valid_game_id(timestamp) {
        bail!("invalid game identifier");
    }
    let game_directory = output_directory.join("games").join(timestamp);
    let (metadata, log) = read_recording_bundle(&game_directory)?;
    let game = game_summary_from_bundle(&game_directory, timestamp.to_owned(), &metadata, &log);
    if !game.video_available {
        bail!("recording video is unavailable");
    }

    let local_player_name = metadata.local_player_summoner_name.clone();
    let mut team_by_player = HashMap::new();
    let mut roster = Vec::new();
    let mut roster_seen = HashSet::new();
    for player in log
        .snapshots
        .iter()
        .flat_map(|snapshot| snapshot.players.iter())
    {
        if !player.summoner_name.is_empty() {
            let normalized_name = normalized_player_name(&player.summoner_name);
            if roster_seen.insert(normalized_name.clone()) {
                roster.push((
                    player.summoner_name.clone(),
                    player.champion.clone(),
                    player.team.clone(),
                ));
            }
            if player.team.is_empty() {
                continue;
            }
            team_by_player
                .entry(normalized_name)
                .or_insert_with(|| player.team.clone());
        }
    }
    let local_player_team = metadata.local_player_team.clone().or_else(|| {
        local_player_name
            .as_deref()
            .and_then(|player| team_by_player.get(&normalized_player_name(player)).cloned())
    });
    let mut participants = roster
        .into_iter()
        .map(|(summoner_name, champion, team)| ReplayParticipant {
            summoner_name,
            champion: if champion.is_empty() {
                "Unknown".to_owned()
            } else {
                champion
            },
            relation: match local_player_team.as_deref() {
                Some(local_team) if team.eq_ignore_ascii_case(local_team) => EventRelation::Ally,
                Some(_) if !team.is_empty() => EventRelation::Enemy,
                _ => EventRelation::Neutral,
            },
        })
        .collect::<Vec<_>>();
    participants.sort_by_key(|participant| match participant.relation {
        EventRelation::Ally => 0,
        EventRelation::Enemy => 1,
        EventRelation::Neutral => 2,
    });
    let mut player_timeline = Vec::new();
    if let Some(local_player) = local_player_name.as_deref() {
        for snapshot in &log.snapshots {
            let Some(player) = snapshot
                .players
                .iter()
                .find(|player| same_player(&player.summoner_name, local_player))
            else {
                continue;
            };
            player_timeline.push(PlayerTimelinePoint {
                game_tick: snapshot.game_tick,
                mapped_replay_time: map_game_tick(
                    &log,
                    snapshot.game_tick,
                    &metadata.media_timeline,
                )?,
                cs: player.cs,
                level: player.level,
            });
        }
    }
    player_timeline.sort_by_key(|point| point.game_tick);
    player_timeline
        .dedup_by(|current, previous| current.cs == previous.cs && current.level == previous.level);

    let mut events = Vec::with_capacity(log.events.len());
    for event in &log.events {
        let relation = event_relation(event, local_player_team.as_deref(), &team_by_player);
        events.push(viewer_event(
            event.clone(),
            relation,
            map_game_tick(&log, event.game_tick, &metadata.media_timeline)?,
        ));
    }
    events.sort_by_key(|event| event.game_tick);
    let kda_timeline = local_player_name
        .as_deref()
        .map(|player| build_kda_timeline(player, &events))
        .unwrap_or_default();
    Ok(PlaybackProbe {
        video_url: format!("{origin}/games/{timestamp}/video.mp4"),
        game,
        media_timeline: metadata.media_timeline,
        local_player_name,
        participants,
        player_timeline,
        kda_timeline,
        events,
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
    let summary = read_game_summary(&game_directory, timestamp.to_owned());
    if summary.saved {
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
    let Ok((metadata, game_log)) = read_recording_bundle(directory) else {
        return incomplete_summary(timestamp, video_size_bytes);
    };
    game_summary_from_bundle(directory, timestamp, &metadata, &game_log)
}

fn game_summary_from_bundle(
    directory: &Path,
    timestamp: String,
    metadata: &MetadataDocument,
    game_log: &GameLogDocument,
) -> GameSummary {
    let video_size_bytes = fs::metadata(directory.join(VIDEO_MP4))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let local_player_name = metadata.local_player_summoner_name.as_deref();
    let (kills, deaths, assists) = local_player_name
        .map(|player| derive_kda(player, &game_log.events))
        .unwrap_or_default();
    let (summoner_spells, keystone_id, items) = local_player_name
        .map(|player| derive_loadout(player, &game_log.snapshots))
        .unwrap_or_default();

    GameSummary {
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
        kills,
        deaths,
        assists,
        summoner_spells,
        keystone_id,
        items,
        saved: metadata.saved,
        incomplete: false,
        video_size_bytes,
        video_available: video_size_bytes > 0,
    }
}

fn incomplete_summary(timestamp: String, video_size_bytes: u64) -> GameSummary {
    GameSummary {
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
        saved: false,
        incomplete: true,
        video_size_bytes,
        video_available: video_size_bytes > 0,
    }
}

fn derive_loadout(
    player: &str,
    snapshots: &[GameSnapshot],
) -> (Vec<String>, Option<u32>, Vec<GameItemSummary>) {
    let stable = snapshots
        .iter()
        .filter_map(|snapshot| {
            snapshot
                .players
                .iter()
                .find(|candidate| same_player(&candidate.summoner_name, player))
        })
        .find(|candidate| candidate.summoner_spells.is_some() || candidate.keystone_id.is_some());
    let summoner_spells = stable
        .and_then(|candidate| candidate.summoner_spells.clone())
        .unwrap_or_default();
    let keystone_id = stable.and_then(|candidate| candidate.keystone_id);

    let mut items = snapshots
        .iter()
        .rev()
        .find_map(|snapshot| {
            snapshot
                .players
                .iter()
                .find(|candidate| same_player(&candidate.summoner_name, player))
                .map(|candidate| {
                    candidate
                        .items
                        .iter()
                        .filter(|item| item.item_id > 0)
                        .map(|item| GameItemSummary {
                            item_id: item.item_id,
                            slot: item.slot,
                        })
                        .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default();
    items.sort_by_key(|item| item.slot);
    items.dedup_by_key(|item| item.slot);

    (summoner_spells, keystone_id, items)
}

fn derive_kda(player: &str, events: &[GameEvent]) -> (u32, u32, u32) {
    let mut kills = 0;
    let mut deaths = 0;
    let mut assists = 0;
    for event in events
        .iter()
        .filter(|event| event.event_type == "ChampionKill")
    {
        kills += u32::from(
            event
                .killer
                .as_deref()
                .is_some_and(|name| same_player(name, player)),
        );
        deaths += u32::from(
            event
                .victim
                .as_deref()
                .is_some_and(|name| same_player(name, player)),
        );
        assists += u32::from(
            event
                .assisters
                .iter()
                .flatten()
                .any(|assister| same_player(assister, player)),
        );
    }
    (kills, deaths, assists)
}

fn same_player(left: &str, right: &str) -> bool {
    let left = left.split_once('#').map_or(left, |(name, _)| name);
    let right = right.split_once('#').map_or(right, |(name, _)| name);
    left.eq_ignore_ascii_case(right)
}

fn normalized_player_name(player: &str) -> String {
    player
        .split_once('#')
        .map_or(player, |(name, _)| name)
        .to_lowercase()
}

fn event_relation(
    event: &GameEvent,
    local_team: Option<&str>,
    team_by_player: &HashMap<String, String>,
) -> EventRelation {
    let Some(local_team) = local_team else {
        return EventRelation::Neutral;
    };
    let actor_team = event.acing_team.as_deref().or_else(|| {
        event
            .killer
            .as_deref()
            .or(event.acer.as_deref())
            .and_then(|player| team_by_player.get(&normalized_player_name(player)))
            .map(String::as_str)
    });
    match actor_team {
        Some(team) if team.eq_ignore_ascii_case(local_team) => EventRelation::Ally,
        Some(_) => EventRelation::Enemy,
        None => EventRelation::Neutral,
    }
}

fn viewer_event(
    event: GameEvent,
    relation: EventRelation,
    mapped_replay_time: MappedReplayTime,
) -> ViewerEvent {
    ViewerEvent {
        event_type: event.event_type,
        game_tick: event.game_tick,
        mapped_replay_time,
        killer: event.killer,
        victim: event.victim,
        assisters: event.assisters.unwrap_or_default(),
        dragon_type: event.dragon_type,
        kill_streak: event.kill_streak,
        acer: event.acer,
        acing_team: event.acing_team,
        turret: event.turret,
        inhibitor: event.inhibitor,
        result: event.result,
        relation,
    }
}

fn build_kda_timeline(player: &str, events: &[ViewerEvent]) -> Vec<KdaTimelinePoint> {
    let mut points = Vec::new();
    let mut kills = 0;
    let mut deaths = 0;
    let mut assists = 0;
    for event in events
        .iter()
        .filter(|event| event.event_type == "ChampionKill")
    {
        let is_kill = event
            .killer
            .as_deref()
            .is_some_and(|name| same_player(name, player));
        let is_death = event
            .victim
            .as_deref()
            .is_some_and(|name| same_player(name, player));
        let is_assist = event.assisters.iter().any(|name| same_player(name, player));
        if !(is_kill || is_death || is_assist) {
            continue;
        }
        kills += u32::from(is_kill);
        deaths += u32::from(is_death);
        assists += u32::from(is_assist);
        points.push(KdaTimelinePoint {
            mapped_replay_time: event.mapped_replay_time.clone(),
            kills,
            deaths,
            assists,
        });
    }
    points
}

fn read_recording_bundle(directory: &Path) -> Result<(MetadataDocument, GameLogDocument)> {
    let metadata = read_metadata(&directory.join(METADATA_JSON))?;
    let game_log = read_game_log(&directory.join(GAME_LOG_JSON))?;
    if metadata.media_id != game_log.media_id
        || metadata.media_id != metadata.media_timeline.media_id
    {
        bail!("rebuild development library: schema-v2 media_id values do not match")
    }
    Ok((metadata, game_log))
}
pub(crate) fn recording_export_authority(directory: &Path) -> Result<ExportRecordingAuthority> {
    let (metadata, _) = read_recording_bundle(directory)?;
    Ok(ExportRecordingAuthority {
        media_timeline: metadata.media_timeline,
        encoder_used: metadata.encoder_used,
    })
}

fn read_metadata(path: &Path) -> Result<MetadataDocument> {
    let metadata: MetadataDocument = read_json(path)?;
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

fn read_game_log(path: &Path) -> Result<GameLogDocument> {
    let game_log: GameLogDocument = read_json(path)?;
    if game_log.schema_version != REPLAY_TIME_SCHEMA_VERSION {
        bail!("rebuild development library: game_log.json is not schema version 2")
    }
    if let Some(calibration) = &game_log.calibration {
        calibration
            .validate()
            .context("rebuild development library: game_log.json calibration is invalid")?;
    }
    Ok(game_log)
}

fn map_game_tick(
    game_log: &GameLogDocument,
    game_tick: GameTick,
    media_timeline: &MediaTimelineV2,
) -> Result<MappedReplayTime> {
    match &game_log.calibration {
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
mod tests {
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

    fn write_game(root: &Path, timestamp: &str, recorded_at: &str, saved: bool) -> PathBuf {
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
        fs::write(
            game.join(GAME_LOG_JSON),
            serde_json::to_vec(&json!({
                "schema_version": 2,
                "media_id": MEDIA_ID,
                "calibration": available_calibration(),
                "snapshots": [
                    {"game_tick":"0","players":[{"summoner_name":"Player#EUW","team":"ORDER","champion":"Syndra","gold":null,"hp":null,"hp_max":null,"cs":1,"level":1,"items":[{"item_id":1056,"slot":0,"count":1}],"summoner_spells":["SummonerFlash","SummonerTeleport"],"keystone_id":8214}]},
                    {"game_tick":"3000000","players":[{"summoner_name":"Player#EUW","team":"ORDER","champion":"Syndra","gold":null,"hp":null,"hp_max":null,"cs":8,"level":2,"items":[{"item_id":3020,"slot":1,"count":1},{"item_id":6657,"slot":0,"count":1},{"item_id":3340,"slot":6,"count":1}]}]}
                ],
                "events":[
                    {"type":"ChampionKill","game_tick":"0","killer":"Player","victim":"Enemy","assisters":[]},
                    {"type":"ChampionKill","game_tick":"1000000","killer":"Enemy","victim":"Player","assisters":[]},
                    {"type":"ChampionKill","game_tick":"2000000","killer":"Ally","victim":"Enemy","assisters":["Player"]}
                ],
                "snapshot_derived_changes": []
            }))
            .unwrap(),
        )
        .unwrap();
        game
    }

    #[test]
    fn builds_schema_v2_payload_with_exact_mapping_results() {
        let root = tempdir().unwrap();
        write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);

        let probe = playback_probe(root.path(), "http://127.0.0.1:9000", "1786000000").unwrap();

        assert_eq!(probe.media_timeline.media_id.as_str(), MEDIA_ID);
        assert_eq!(probe.game.duration_ms, 10_000);
        assert_eq!(probe.local_player_name.as_deref(), Some("Player#EUW"));
        assert!(matches!(
            probe.events[0].mapped_replay_time,
            MappedReplayTime::BeforeMedia { .. }
        ));
        assert!(matches!(
            probe.events[1].mapped_replay_time,
            MappedReplayTime::InsideMedia { .. }
        ));
        assert!(matches!(
            probe.events[2].mapped_replay_time,
            MappedReplayTime::InsideMedia { .. }
        ));
        assert_eq!(probe.kda_timeline.len(), 3);
        assert_eq!(
            (probe.game.kills, probe.game.deaths, probe.game.assists),
            (1, 1, 1)
        );
    }

    #[test]
    fn rejects_schema_v1_unknown_missing_and_mismatched_bundles() {
        let root = tempdir().unwrap();
        let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
        let metadata_path = game.join(METADATA_JSON);
        let mut metadata: serde_json::Value = read_json(&metadata_path).unwrap();
        metadata["schema_version"] = json!(1);
        fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
        assert!(
            playback_probe(root.path(), "http://127.0.0.1:9000", "1786000000")
                .unwrap_err()
                .to_string()
                .contains("rebuild development library")
        );

        metadata["schema_version"] = json!(2);
        metadata["unexpected"] = json!(true);
        fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
        assert!(playback_probe(root.path(), "http://127.0.0.1:9000", "1786000000").is_err());

        metadata.as_object_mut().unwrap().remove("unexpected");
        metadata.as_object_mut().unwrap().remove("media_timeline");
        fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
        assert!(playback_probe(root.path(), "http://127.0.0.1:9000", "1786000000").is_err());

        write_game(root.path(), "1786000001", "2026-08-05T10:00:00Z", false);
        let log_path = root
            .path()
            .join("games")
            .join("1786000001")
            .join(GAME_LOG_JSON);
        let mut log: serde_json::Value = read_json(&log_path).unwrap();
        log["schema_version"] = json!(1);
        fs::write(&log_path, serde_json::to_vec(&log).unwrap()).unwrap();
        assert!(
            playback_probe(root.path(), "http://127.0.0.1:9000", "1786000001")
                .unwrap_err()
                .to_string()
                .contains("game_log.json is not schema version 2")
        );

        log["schema_version"] = json!(2);
        log["media_id"] = json!("123e4567-e89b-42d3-a456-426614174001");
        fs::write(log_path, serde_json::to_vec(&log).unwrap()).unwrap();
        assert!(
            playback_probe(root.path(), "http://127.0.0.1:9000", "1786000001")
                .unwrap_err()
                .to_string()
                .contains("media_id")
        );
    }

    #[test]
    fn reports_after_media_and_unavailable_without_saturation() {
        let root = tempdir().unwrap();
        let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
        let log_path = game.join(GAME_LOG_JSON);
        let mut log: serde_json::Value = read_json(&log_path).unwrap();
        log["events"].as_array_mut().unwrap()[2]["game_tick"] = json!("30000000");
        fs::write(&log_path, serde_json::to_vec(&log).unwrap()).unwrap();
        let probe = playback_probe(root.path(), "http://127.0.0.1:9000", "1786000000").unwrap();
        assert!(matches!(
            probe.events[2].mapped_replay_time,
            MappedReplayTime::AfterMedia { .. }
        ));

        log["calibration"] = serde_json::Value::Null;
        fs::write(log_path, serde_json::to_vec(&log).unwrap()).unwrap();
        let probe = playback_probe(root.path(), "http://127.0.0.1:9000", "1786000000").unwrap();
        assert!(matches!(
            probe.events[0].mapped_replay_time,
            MappedReplayTime::Unavailable { .. }
        ));
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
