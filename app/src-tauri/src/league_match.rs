//! Optional recording context; never a playback or final-statistics authority.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Serialize};

const MAX_BYTES: u64 = 64 * 1024;
const MAX_STRING: usize = 256;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MatchStatus {
    Provisional,
    Confirmed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct LeagueMatch {
    pub media_id: MediaId,
    pub status: MatchStatus,
    pub game_id: String,
    pub queue_id: u32,
    pub local_riot_id: String,
    pub map_id: u32,
    pub game_mode: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    schema_version: u32,
    status: MatchStatus,
    media_id: MediaId,
    game_id: String,
    queue_id: u32,
    local_puuid: String,
    local_riot_id: String,
    lcu_map_id: Option<u32>,
    lcu_queue_map_id: Option<u32>,
    lcu_map_mode: Option<String>,
    lcu_queue_mode: String,
    live_map_id: u32,
    live_mode: String,
    evidence: Evidence,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    lcu_session_source: String,
    local_identity_source: String,
    live_source: String,
    credential_epoch_marker: MediaId,
    first: Round,
    second: Round,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Round {
    live_sequence: u64,
    live_started_us: u64,
    live_finished_us: u64,
    lcu_started_us: u64,
    lcu_finished_us: u64,
}
impl Round {
    fn ordered(&self) -> bool {
        self.live_sequence > 0
            && self.live_started_us <= self.live_finished_us
            && self.lcu_started_us <= self.lcu_finished_us
    }
}
impl Evidence {
    fn valid(&self) -> bool {
        let first = &self.first;
        let second = &self.second;
        let separation = second.lcu_started_us.checked_sub(first.lcu_started_us);
        let oldest = first.live_started_us.min(first.lcu_started_us);
        let latest = second.live_finished_us.max(second.lcu_finished_us);
        // Parsing the opaque nonce validates its canonical UUID without exposing it.
        let _ = &self.credential_epoch_marker;
        self.lcu_session_source == "/lol-gameflow/v1/session"
            && self.local_identity_source == "/lol-summoner/v1/current-summoner"
            && self.live_source == "existing_activeplayer_gamestats_round"
            && first.ordered()
            && second.ordered()
            && second.live_sequence.checked_sub(first.live_sequence) == Some(1)
            && second.live_started_us > first.live_started_us
            && second.live_finished_us >= first.live_finished_us
            && second.lcu_finished_us >= first.lcu_finished_us
            && separation.is_some_and(|us| (2_000_000..=30_000_000).contains(&us))
            && latest
                .checked_sub(oldest)
                .is_some_and(|age| age <= 15_000_000)
    }
}

fn bounded(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_STRING && !value.chars().any(char::is_control)
}
fn full_riot_id(value: &str) -> bool {
    bounded(value)
        && value
            .split_once('#')
            .is_some_and(|(name, tag)| !name.is_empty() && !tag.is_empty() && !tag.contains('#'))
}

impl Candidate {
    fn project(self, media_id: &MediaId) -> Option<LeagueMatch> {
        let game = self.game_id.parse::<u64>().ok()?;
        let maps = [self.lcu_map_id, self.lcu_queue_map_id];
        // Version 1 is the provisional producer contract. A confirmed status alone
        // cannot supply 002's not-yet-implemented exact post-game confirmation proof.
        if self.schema_version != 1
            || self.status != MatchStatus::Provisional
            || self.media_id != *media_id
            || game == 0
            || game.to_string() != self.game_id
            || !bounded(&self.local_puuid)
            || !full_riot_id(&self.local_riot_id)
            || self.live_map_id == 0
            || !maps.iter().any(Option::is_some)
            || maps.into_iter().flatten().any(|id| id != self.live_map_id)
            || self
                .lcu_map_mode
                .as_ref()
                .is_some_and(|mode| mode.len() > MAX_STRING || mode.chars().any(char::is_control))
            || !bounded(&self.live_mode)
            || self.lcu_queue_mode != self.live_mode
            || !self.evidence.valid()
        {
            return None;
        }
        Some(LeagueMatch {
            media_id: self.media_id,
            status: self.status,
            game_id: self.game_id,
            queue_id: self.queue_id,
            local_riot_id: self.local_riot_id,
            map_id: self.live_map_id,
            game_mode: self.live_mode,
        })
    }
}

pub(crate) fn read(directory: &Path, media_id: &MediaId) -> Option<LeagueMatch> {
    let path = directory.join("league_match.json");
    if !fs::symlink_metadata(&path).ok()?.is_file() {
        return None;
    }
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    serde_json::from_slice::<Candidate>(&bytes)
        .ok()?
        .project(media_id)
}

#[cfg(test)]
pub(crate) fn fixture(media_id: &MediaId) -> serde_json::Value {
    serde_json::json!({
        "schema_version":1, "status":"provisional", "media_id":media_id,
        "game_id":"9007199254740993", "queue_id":0,
        "local_puuid":"synthetic-local-puuid", "local_riot_id":"Player#EUW",
        "lcu_map_id":11, "lcu_queue_map_id":11, "lcu_map_mode":"CLASSIC",
        "lcu_queue_mode":"SWIFTPLAY", "live_map_id":11, "live_mode":"SWIFTPLAY",
        "evidence":{
            "lcu_session_source":"/lol-gameflow/v1/session",
            "local_identity_source":"/lol-summoner/v1/current-summoner",
            "live_source":"existing_activeplayer_gamestats_round",
            "credential_epoch_marker":"11111111-2222-4333-8444-555555555555",
            "first":{"live_sequence":1,"live_started_us":0,"live_finished_us":50,
                "lcu_started_us":50,"lcu_finished_us":100},
            "second":{"live_sequence":2,"live_started_us":10_000_000,"live_finished_us":10_000_050,
                "lcu_started_us":10_000_050,"lcu_finished_us":10_000_100}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_requires_exact_version_identity_context_and_provenance() {
        let media = MediaId::new_v4();
        let good = fixture(&media);
        let projection = serde_json::from_value::<Candidate>(good.clone())
            .unwrap()
            .project(&media)
            .unwrap();
        assert_eq!(projection.game_id, "9007199254740993");
        assert_eq!(projection.queue_id, 0);
        assert_eq!(projection.status, MatchStatus::Provisional);
        for (pointer, invalid) in [
            ("/schema_version", serde_json::json!(2)),
            ("/status", serde_json::json!("confirmed")),
            ("/media_id", serde_json::json!(MediaId::new_v4())),
            ("/game_id", serde_json::json!("09007199254740993")),
            ("/game_id", serde_json::json!("0")),
            ("/game_id", serde_json::json!("18446744073709551616")),
            ("/queue_id", serde_json::json!(null)),
            ("/local_riot_id", serde_json::json!("Player")),
            ("/local_puuid", serde_json::json!("")),
            ("/lcu_queue_mode", serde_json::json!("CLASSIC")),
            ("/lcu_map_id", serde_json::json!(12)),
            ("/live_mode", serde_json::json!("x".repeat(257))),
            ("/evidence/second/live_sequence", serde_json::json!(3)),
            (
                "/evidence/second/lcu_finished_us",
                serde_json::json!(20_000_000),
            ),
            ("/evidence/lcu_session_source", serde_json::json!("other")),
        ] {
            let mut value = good.clone();
            *value.pointer_mut(pointer).unwrap() = invalid;
            assert!(
                serde_json::from_value::<Candidate>(value)
                    .ok()
                    .and_then(|c| c.project(&media))
                    .is_none(),
                "{pointer}"
            );
        }
    }

    #[test]
    fn missing_corrupt_large_and_non_file_context_is_unavailable() {
        let root = tempfile::tempdir().unwrap();
        let media = MediaId::new_v4();
        let path = root.path().join("league_match.json");
        assert!(read(root.path(), &media).is_none());
        for bytes in [b"invalid".to_vec(), vec![b' '; MAX_BYTES as usize + 1]] {
            fs::write(&path, bytes).unwrap();
            assert!(read(root.path(), &media).is_none());
        }
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(read(root.path(), &media).is_none());
    }
}
