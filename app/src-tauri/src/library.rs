use std::cmp::Ordering;
use std::collections::HashMap;
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
    pub win: Option<bool>,
    pub win_method: String,
    pub saved: bool,
    pub incomplete: bool,
    pub matchv5_fetched: bool,
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
pub struct EventMarker {
    pub video_time_ms: u64,
    pub event_type: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlaybackProbe {
    pub game: GameSummary,
    pub video_url: String,
    pub markers: Vec<EventMarker>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MetadataDocument {
    recorded_at: String,
    duration_ms: u64,
    game_mode: Option<String>,
    local_player_summoner_name: Option<String>,
    local_player_champion: Option<String>,
    win: Option<bool>,
    win_method: String,
    matchv5_fetched: bool,
    saved: bool,
}

#[derive(Debug, Default, Deserialize)]
struct GameLogDocument {
    #[serde(default)]
    events: Vec<GameEvent>,
}

#[derive(Debug, Deserialize)]
struct GameEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    video_time_ms: i64,
    killer: Option<String>,
    victim: Option<String>,
    #[serde(default)]
    assisters: Vec<String>,
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

    let mut markers = read_game_log(&game_directory.join(GAME_LOG_JSON))?
        .events
        .into_iter()
        .filter_map(|event| {
            u64::try_from(event.video_time_ms)
                .ok()
                .map(|video_time_ms| EventMarker {
                    video_time_ms,
                    event_type: event.event_type,
                })
        })
        .collect::<Vec<_>>();
    markers.sort_by_key(|marker| marker.video_time_ms);

    Ok(PlaybackProbe {
        video_url: format!("{origin}/games/{timestamp}/video.mp4"),
        game,
        markers,
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
        win: metadata.win,
        win_method: if metadata.win_method.is_empty() {
            "unknown".to_owned()
        } else {
            metadata.win_method
        },
        saved: metadata.saved,
        incomplete: false,
        matchv5_fetched: metadata.matchv5_fetched,
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
        win: None,
        win_method: "unknown".to_owned(),
        saved: false,
        incomplete: true,
        matchv5_fetched: false,
        video_size_bytes,
        video_available: video_size_bytes > 0,
    }
}

fn derive_kda(player: &str, events: &[GameEvent]) -> (u32, u32, u32) {
    let player = player.split_once('#').map_or(player, |(name, _)| name);
    let mut kills = 0;
    let mut deaths = 0;
    let mut assists = 0;
    for event in events
        .iter()
        .filter(|event| event.event_type == "ChampionKill")
    {
        kills += u32::from(event.killer.as_deref() == Some(player));
        deaths += u32::from(event.victim.as_deref() == Some(player));
        assists += u32::from(event.assisters.iter().any(|assister| assister == player));
    }
    (kills, deaths, assists)
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
                "game_mode": "CLASSIC",
                "local_player_summoner_name": "Player#EUW",
                "local_player_champion": "Syndra",
                "win": null,
                "win_method": "unknown",
                "matchv5_fetched": false,
                "saved": saved,
                "preserved": "yes"
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            game.join(GAME_LOG_JSON),
            serde_json::to_vec(&json!({
                "events": [
                    {"type":"ChampionKill","video_time_ms":1000,"killer":"Player","victim":"Enemy","assisters":[]},
                    {"type":"ChampionKill","video_time_ms":2000,"killer":"Enemy","victim":"Player","assisters":[]},
                    {"type":"ChampionKill","video_time_ms":3000,"killer":"Ally","victim":"Enemy","assisters":["Player"]}
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
