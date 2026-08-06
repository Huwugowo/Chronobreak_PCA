use std::cmp::Ordering;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Deserialize)]
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

pub fn list_games(games_directory: &Path) -> Result<Vec<GameSummary>> {
    if !games_directory.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(games_directory).with_context(|| {
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
        games.push(read_game_summary(&path, timestamp)?);
    }

    games.sort_by(compare_games);
    Ok(games)
}

pub fn playback_probe(games_directory: &Path, origin: &str) -> Result<Option<PlaybackProbe>> {
    let Some(game) = list_games(games_directory)?
        .into_iter()
        .filter(|game| game.video_size_bytes > 0)
        .max_by_key(|game| game.video_size_bytes)
    else {
        return Ok(None);
    };

    let markers = read_game_log(&games_directory.join(&game.timestamp).join(GAME_LOG_JSON))?
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
        .collect();

    Ok(Some(PlaybackProbe {
        video_url: format!("{origin}/games/{}/video.mp4", game.timestamp),
        game,
        markers,
    }))
}

fn read_game_summary(directory: &Path, timestamp: String) -> Result<GameSummary> {
    let video_size_bytes = fs::metadata(directory.join(VIDEO_MP4))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let metadata_path = directory.join(METADATA_JSON);
    if !metadata_path.exists() {
        return Ok(incomplete_summary(timestamp, video_size_bytes));
    }

    let metadata: MetadataDocument = read_json(&metadata_path)?;
    let game_log = read_game_log(&directory.join(GAME_LOG_JSON))?;
    let (kills, deaths, assists) = metadata
        .local_player_summoner_name
        .as_deref()
        .map(|player| derive_kda(player, &game_log.events))
        .unwrap_or_default();

    Ok(GameSummary {
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
        win_method: metadata.win_method,
        saved: metadata.saved,
        incomplete: false,
        matchv5_fetched: metadata.matchv5_fetched,
        video_size_bytes,
    })
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
    }
}

fn derive_kda(player: &str, events: &[GameEvent]) -> (u32, u32, u32) {
    // Live Client event names omit the Riot tag that metadata includes.
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
        _ => right.timestamp.cmp(&left.timestamp),
    }
}

fn valid_timestamp(timestamp: &str) -> bool {
    !timestamp.is_empty() && timestamp.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn lists_current_bundles_and_derives_kda() {
        let root = tempdir().unwrap();
        let game = root.path().join("1786000000");
        fs::create_dir(&game).unwrap();
        fs::write(game.join(VIDEO_MP4), b"video").unwrap();
        fs::write(
            game.join(METADATA_JSON),
            serde_json::to_vec(&json!({
                "recorded_at": "2026-08-06T10:00:00Z",
                "duration_ms": 120000,
                "game_mode": "CLASSIC",
                "local_player_summoner_name": "Player#EUW",
                "local_player_champion": "Syndra",
                "win": null,
                "win_method": "unknown",
                "matchv5_fetched": false,
                "saved": false
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

        let games = list_games(root.path()).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].champion, "Syndra");
        assert_eq!(
            (games[0].kills, games[0].deaths, games[0].assists),
            (1, 1, 1)
        );
        assert_eq!(games[0].video_size_bytes, 5);
    }

    #[test]
    fn ignores_non_timestamp_directories_and_keeps_incomplete_video() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("notes")).unwrap();
        let incomplete = root.path().join("1786000001");
        fs::create_dir(&incomplete).unwrap();
        fs::write(incomplete.join(VIDEO_MP4), b"partial").unwrap();

        let games = list_games(root.path()).unwrap();
        assert_eq!(games.len(), 1);
        assert!(games[0].incomplete);
        assert_eq!(games[0].video_size_bytes, 7);
    }
}
