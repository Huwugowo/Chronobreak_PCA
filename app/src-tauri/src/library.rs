use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
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
    pub saved: bool,
    pub incomplete: bool,
    pub video_size_bytes: u64,
    pub video_available: bool,
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
    pub game_time_ms: u64,
    pub video_time_ms: u64,
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

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct PlayerTimelinePoint {
    pub game_time_ms: u64,
    pub video_time_ms: u64,
    pub cs: u32,
    pub level: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct KdaTimelinePoint {
    pub video_time_ms: u64,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlaybackProbe {
    pub game: GameSummary,
    pub video_url: String,
    pub game_start_video_offset_ms: u64,
    pub local_player_name: Option<String>,
    pub participants: Vec<ReplayParticipant>,
    pub player_timeline: Vec<PlayerTimelinePoint>,
    pub kda_timeline: Vec<KdaTimelinePoint>,
    pub events: Vec<ViewerEvent>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MetadataDocument {
    recorded_at: String,
    duration_ms: u64,
    video_offset_ms: Option<u64>,
    game_mode: Option<String>,
    local_player_summoner_name: Option<String>,
    local_player_champion: Option<String>,
    local_player_team: Option<String>,
    saved: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GameLogDocument {
    game_start_video_offset_ms: i64,
    snapshots: Vec<GameSnapshot>,
    events: Vec<GameEvent>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GameSnapshot {
    game_time_ms: i64,
    players: Vec<SnapshotPlayer>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SnapshotPlayer {
    summoner_name: String,
    team: String,
    champion: String,
    cs: u32,
    level: u32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GameEvent {
    #[serde(rename = "type")]
    event_type: String,
    game_time_ms: i64,
    video_time_ms: i64,
    killer: Option<String>,
    victim: Option<String>,
    assisters: Vec<String>,
    dragon_type: Option<String>,
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
        if !valid_timestamp(&timestamp) {
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
    if !valid_timestamp(timestamp) {
        bail!("invalid game timestamp");
    }
    let game_directory = output_directory.join("games").join(timestamp);
    let game = read_game_summary(&game_directory, timestamp.to_owned());
    if !game.video_available {
        bail!("recording video is unavailable");
    }

    let metadata = read_json::<MetadataDocument>(&game_directory.join(METADATA_JSON)).ok();
    let local_player_name = metadata
        .as_ref()
        .and_then(|document| document.local_player_summoner_name.clone());
    let log = read_game_log(&game_directory.join(GAME_LOG_JSON))?;
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
    let local_player_team = metadata
        .as_ref()
        .and_then(|document| document.local_player_team.clone())
        .or_else(|| {
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
    let game_start_video_offset_ms = u64::try_from(log.game_start_video_offset_ms)
        .ok()
        .filter(|offset| *offset > 0)
        .or_else(|| {
            metadata
                .as_ref()
                .and_then(|document| document.video_offset_ms)
        })
        .unwrap_or(0);

    let mut player_timeline = log
        .snapshots
        .iter()
        .filter_map(|snapshot| {
            let game_time_ms = u64::try_from(snapshot.game_time_ms).ok()?;
            let local_player = local_player_name.as_deref()?;
            let player = snapshot
                .players
                .iter()
                .find(|player| same_player(&player.summoner_name, local_player))?;
            Some(PlayerTimelinePoint {
                game_time_ms,
                video_time_ms: game_start_video_offset_ms.saturating_add(game_time_ms),
                cs: player.cs,
                level: player.level,
            })
        })
        .collect::<Vec<_>>();
    player_timeline.sort_by_key(|point| point.video_time_ms);
    player_timeline
        .dedup_by(|current, previous| current.cs == previous.cs && current.level == previous.level);

    let mut events = log
        .events
        .into_iter()
        .filter_map(|event| {
            let relation = event_relation(&event, local_player_team.as_deref(), &team_by_player);
            viewer_event(event, relation)
        })
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event.video_time_ms);
    let kda_timeline = local_player_name
        .as_deref()
        .map(|player| build_kda_timeline(player, &events))
        .unwrap_or_default();
    Ok(PlaybackProbe {
        video_url: format!("{origin}/games/{timestamp}/video.mp4"),
        game,
        game_start_video_offset_ms,
        local_player_name,
        participants,
        player_timeline,
        kda_timeline,
        events,
    })
}

pub fn list_clips(output_directory: &Path, origin: &str) -> Result<Vec<ClipSummary>> {
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
            duration_ms: probe_duration_ms(&path).unwrap_or(0),
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
    if !valid_timestamp(timestamp) {
        bail!("invalid game timestamp");
    }
    let metadata_path = output_directory
        .join("games")
        .join(timestamp)
        .join(METADATA_JSON);
    if !metadata_path.exists() {
        bail!("incomplete recordings cannot be saved");
    }
    let mut metadata: serde_json::Value = read_json(&metadata_path)?;
    let object = metadata
        .as_object_mut()
        .context("metadata.json must contain an object")?;
    object.insert("saved".to_owned(), serde_json::Value::Bool(saved));
    let mut bytes = serde_json::to_vec_pretty(&metadata).context("failed to serialize metadata")?;
    bytes.push(b'\n');
    config::write_atomic(&metadata_path, &bytes)
}

pub fn delete_game(output_directory: &Path, timestamp: &str) -> Result<()> {
    if !valid_timestamp(timestamp) {
        bail!("invalid game timestamp");
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
    let metadata_path = directory.join(METADATA_JSON);
    let Ok(metadata) = read_json::<MetadataDocument>(&metadata_path) else {
        return incomplete_summary(timestamp, video_size_bytes);
    };

    let game_log = read_game_log(&directory.join(GAME_LOG_JSON)).unwrap_or_default();
    let (kills, deaths, assists) = metadata
        .local_player_summoner_name
        .as_deref()
        .map(|player| derive_kda(player, &game_log.events))
        .unwrap_or_default();

    GameSummary {
        timestamp,
        champion: metadata
            .local_player_champion
            .unwrap_or_else(|| "Unknown".to_owned()),
        game_mode: metadata.game_mode.unwrap_or_else(|| "Unknown".to_owned()),
        duration_ms: metadata.duration_ms,
        recorded_at: metadata.recorded_at,
        kills,
        deaths,
        assists,
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
        saved: false,
        incomplete: true,
        video_size_bytes,
        video_available: video_size_bytes > 0,
    }
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

fn viewer_event(event: GameEvent, relation: EventRelation) -> Option<ViewerEvent> {
    Some(ViewerEvent {
        event_type: event.event_type,
        game_time_ms: u64::try_from(event.game_time_ms).ok()?,
        video_time_ms: u64::try_from(event.video_time_ms).ok()?,
        killer: event.killer,
        victim: event.victim,
        assisters: event.assisters,
        dragon_type: event.dragon_type,
        kill_streak: event.kill_streak,
        acer: event.acer,
        acing_team: event.acing_team,
        turret: event.turret,
        inhibitor: event.inhibitor,
        result: event.result,
        relation,
    })
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
            video_time_ms: event.video_time_ms,
            kills,
            deaths,
            assists,
        });
    }
    points
}

fn read_game_log(path: &Path) -> Result<GameLogDocument> {
    if !path.exists() {
        return Ok(GameLogDocument::default());
    }
    read_json(path)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
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
    (valid_timestamp(game_timestamp) && valid_timestamp(clip_timestamp))
        .then_some((game_timestamp, clip_timestamp))
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

fn probe_duration_ms(path: &Path) -> Option<u64> {
    let mut command = Command::new("ffprobe");
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

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    fn write_game(root: &Path, timestamp: &str, recorded_at: &str, saved: bool) -> PathBuf {
        let game = root.join("games").join(timestamp);
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join(VIDEO_MP4), b"video").unwrap();
        fs::write(
            game.join(METADATA_JSON),
            serde_json::to_vec(&json!({
                "recorded_at": recorded_at,
                "duration_ms": 120000,
                "video_offset_ms": 5000,
                "game_mode": "CLASSIC",
                "local_player_summoner_name": "Player#EUW",
                "local_player_champion": "Syndra",
                "local_player_team": "ORDER",
                "saved": saved,
                "preserved": "yes"
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            game.join(GAME_LOG_JSON),
            serde_json::to_vec(&json!({
                "game_start_video_offset_ms": 5000,
                "snapshots": [
                    {
                        "game_time_ms": 1000,
                        "players": [
                            {"summoner_name":"Player#EUW","team":"ORDER","champion":"Syndra","cs":1,"level":1},
                            {"summoner_name":"Ally#EUW","team":"ORDER","champion":"LeeSin","cs":1,"level":1},
                            {"summoner_name":"Enemy#EUW","team":"CHAOS","champion":"Viktor","cs":2,"level":1}
                        ]
                    },
                    {
                        "game_time_ms": 11000,
                        "players": [
                            {"summoner_name":"Player#EUW","team":"ORDER","champion":"Syndra","cs":8,"level":2}
                        ]
                    }
                ],
                "events": [
                    {"type":"ChampionKill","game_time_ms":1000,"video_time_ms":6000,"killer":"Player","victim":"Enemy","assisters":[]},
                    {"type":"ChampionKill","game_time_ms":2000,"video_time_ms":7000,"killer":"Enemy","victim":"Player","assisters":[]},
                    {"type":"ChampionKill","game_time_ms":3000,"video_time_ms":8000,"killer":"Ally","victim":"Enemy","assisters":["Player"]},
                    {"type":"DragonKill","game_time_ms":4000,"video_time_ms":9000,"killer":"Player","assisters":["Ally"],"dragon_type":"Air"}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        game
    }

    #[test]
    fn lists_current_bundles_by_date_and_derives_kda() {
        let root = tempdir().unwrap();
        write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
        write_game(root.path(), "1786000001", "2026-08-06T10:00:00Z", false);

        let games = list_games(root.path()).unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].timestamp, "1786000001");
        assert_eq!(
            (games[0].kills, games[0].deaths, games[0].assists),
            (1, 1, 1)
        );
        assert_eq!(games[0].video_size_bytes, 5);
    }

    #[test]
    fn builds_sorted_frame_driven_playback_payload() {
        let root = tempdir().unwrap();
        write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);

        let probe = playback_probe(root.path(), "http://127.0.0.1:9000", "1786000000").unwrap();

        assert_eq!(probe.game_start_video_offset_ms, 5_000);
        assert_eq!(probe.local_player_name.as_deref(), Some("Player#EUW"));
        assert_eq!(
            probe.participants,
            vec![
                ReplayParticipant {
                    summoner_name: "Player#EUW".to_owned(),
                    champion: "Syndra".to_owned(),
                    relation: EventRelation::Ally,
                },
                ReplayParticipant {
                    summoner_name: "Ally#EUW".to_owned(),
                    champion: "LeeSin".to_owned(),
                    relation: EventRelation::Ally,
                },
                ReplayParticipant {
                    summoner_name: "Enemy#EUW".to_owned(),
                    champion: "Viktor".to_owned(),
                    relation: EventRelation::Enemy,
                },
            ]
        );
        assert_eq!(
            probe.player_timeline,
            vec![
                PlayerTimelinePoint {
                    game_time_ms: 1_000,
                    video_time_ms: 6_000,
                    cs: 1,
                    level: 1,
                },
                PlayerTimelinePoint {
                    game_time_ms: 11_000,
                    video_time_ms: 16_000,
                    cs: 8,
                    level: 2,
                },
            ]
        );
        assert_eq!(
            probe.kda_timeline,
            vec![
                KdaTimelinePoint {
                    video_time_ms: 6_000,
                    kills: 1,
                    deaths: 0,
                    assists: 0,
                },
                KdaTimelinePoint {
                    video_time_ms: 7_000,
                    kills: 1,
                    deaths: 1,
                    assists: 0,
                },
                KdaTimelinePoint {
                    video_time_ms: 8_000,
                    kills: 1,
                    deaths: 1,
                    assists: 1,
                },
            ]
        );
        assert_eq!(probe.events.len(), 4);
        assert_eq!(probe.events[0].relation, EventRelation::Ally);
        assert_eq!(probe.events[1].relation, EventRelation::Enemy);
        assert_eq!(probe.events[2].relation, EventRelation::Ally);
        assert_eq!(probe.events[3].event_type, "DragonKill");
        assert_eq!(probe.events[3].dragon_type.as_deref(), Some("Air"));
        assert_eq!(
            probe.video_url,
            "http://127.0.0.1:9000/games/1786000000/video.mp4"
        );
    }

    #[test]
    fn keeps_incomplete_video_bundles_at_the_bottom() {
        let root = tempdir().unwrap();
        write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
        let incomplete = root.path().join("games").join("1786000001");
        fs::create_dir_all(&incomplete).unwrap();
        fs::write(incomplete.join(VIDEO_MP4), b"partial").unwrap();

        let games = list_games(root.path()).unwrap();
        assert_eq!(games.len(), 2);
        assert!(!games[0].incomplete);
        assert!(games[1].incomplete);
        assert!(games[1].video_available);
    }

    #[test]
    fn save_updates_metadata_without_losing_fields() {
        let root = tempdir().unwrap();
        let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);

        save_game(root.path(), "1786000000", true).unwrap();

        let metadata: serde_json::Value = read_json(&game.join(METADATA_JSON)).unwrap();
        assert_eq!(metadata["saved"], true);
        assert_eq!(metadata["preserved"], "yes");
    }

    #[test]
    fn delete_requires_saved_games_to_be_unsaved_first() {
        let root = tempdir().unwrap();
        let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", true);
        assert!(delete_game(root.path(), "1786000000").is_err());
        assert!(game.exists());

        save_game(root.path(), "1786000000", false).unwrap();
        delete_game(root.path(), "1786000000").unwrap();
        assert!(!game.exists());
    }

    #[test]
    fn auto_delete_skips_saved_recent_and_incomplete_games() {
        let root = tempdir().unwrap();
        let old = write_game(root.path(), "1786000000", "2025-01-01T00:00:00Z", false);
        let saved = write_game(root.path(), "1786000001", "2025-01-01T00:00:00Z", true);
        let recent = write_game(root.path(), "1786000002", "2026-07-30T00:00:00Z", false);
        let incomplete = root.path().join("games").join("1786000003");
        fs::create_dir_all(&incomplete).unwrap();
        fs::write(incomplete.join(VIDEO_MP4), b"partial").unwrap();
        let now = OffsetDateTime::parse("2026-08-06T00:00:00Z", &Rfc3339).unwrap();

        let result = run_auto_delete_at(root.path(), 30, now).unwrap();

        assert_eq!(result.deleted_count, 1);
        assert!(!old.exists());
        assert!(saved.exists());
        assert!(recent.exists());
        assert!(incomplete.exists());
    }

    #[test]
    fn validates_clip_asset_names() {
        assert!(valid_clip_asset("1786000000_1786000100.mp4"));
        assert!(valid_clip_asset("1786000000_1786000100.jpg"));
        assert!(!valid_clip_asset("../video.mp4"));
        assert!(!valid_clip_asset("1786000000_notes.mp4"));
    }
}
