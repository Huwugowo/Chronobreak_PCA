//! Recording-owned QB-DATA-003 provisional context and read-only feasibility tools.

mod association;
pub(crate) mod collector;
pub mod probe;
pub(crate) mod sidecar;
mod transport;

use std::time::Instant;

use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Serialize};

const MAX_STRING: usize = 256;

// Neither Debug nor Serialize: personal observations stay in memory.
#[derive(Clone)]
pub(crate) struct LiveObservation {
    pub media_id: MediaId,
    pub sequence: u64,
    pub started: Instant,
    pub finished: Instant,
    pub identity: Option<LiveIdentity>,
}

#[derive(Clone)]
pub(crate) struct LiveIdentity {
    pub active: String,
    pub map_id: Option<u32>,
    pub game_mode: Option<String>,
}

pub(crate) fn bounded(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_STRING && !value.chars().any(char::is_control)
}

pub(crate) fn full_identity(value: &str) -> bool {
    bounded(value)
        && value
            .split_once('#')
            .is_some_and(|(name, tag)| !name.is_empty() && !tag.is_empty() && !tag.contains('#'))
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Summoner {
    puuid: Option<String>,
    game_name: Option<String>,
    tag_line: Option<String>,
}

impl Summoner {
    fn full_identity(&self) -> Option<String> {
        let name = self.game_name.as_deref()?;
        let tag = self.tag_line.as_deref()?;
        if name.contains('#') || tag.contains('#') {
            return None;
        }
        let id = format!("{name}#{tag}");
        full_identity(&id).then_some(id)
    }
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Gameflow {
    phase: Option<String>,
    #[serde(default)]
    game_data: GameData,
    #[serde(default)]
    map: MapContext,
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MapContext {
    id: Option<u32>,
    game_mode: Option<String>,
}

// Roster/champion keys are intentionally unknown fields. Even malformed shapes
// for those unrelated keys cannot influence the provisional association.
#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GameData {
    game_id: Option<u64>,
    #[serde(default)]
    queue: Queue,
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Queue {
    id: Option<u32>,
    map_id: Option<u32>,
    game_mode: Option<String>,
}

struct LcuRound {
    summoner: Summoner,
    gameflow: Gameflow,
}

impl LcuRound {
    fn validate_strings(&self) -> Result<(), &'static str> {
        if [
            &self.summoner.puuid,
            &self.summoner.game_name,
            &self.summoner.tag_line,
            &self.gameflow.phase,
            &self.gameflow.map.game_mode,
            &self.gameflow.game_data.queue.game_mode,
        ]
        .into_iter()
        .flatten()
        .any(|s| s.len() > MAX_STRING || s.chars().any(char::is_control))
        {
            return Err("lcu_string_size");
        }
        Ok(())
    }
}

/// Sanitized measurements only; this is not an authoritative association.
#[derive(Default, Serialize)]
struct Comparison {
    in_progress: bool,
    game_id_present: bool,
    queue_id_present: bool,
    local_puuid_present: bool,
    full_local_identity_agrees: bool,
    lcu_map_available: bool,
    live_map_available: bool,
    map_ids_agree: Option<bool>,
    lcu_queue_mode_available: bool,
    live_mode_available: bool,
    queue_mode_agrees: Option<bool>,
}

impl Comparison {
    fn coherent(&self) -> bool {
        self.in_progress
            && self.game_id_present
            && self.queue_id_present
            && self.local_puuid_present
            && self.full_local_identity_agrees
            && self.map_ids_agree == Some(true)
            && self.queue_mode_agrees == Some(true)
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Evidence {
    game_id: u64,
    queue_id: u32,
    puuid: String,
    riot_id: String,
    lcu_map: Option<u32>,
    lcu_queue_map: Option<u32>,
    lcu_map_mode: Option<String>,
    lcu_queue_mode: String,
    live_map: u32,
    live_mode: String,
}

fn compare(round: &LcuRound, live: Option<&LiveIdentity>) -> Comparison {
    let game = &round.gameflow.game_data;
    let maps: Vec<_> = [round.gameflow.map.id, game.queue.map_id]
        .into_iter()
        .flatten()
        .collect();
    let mut result = Comparison {
        in_progress: round.gameflow.phase.as_deref() == Some("InProgress"),
        game_id_present: game.game_id.is_some_and(|id| id > 0),
        queue_id_present: game.queue.id.is_some(),
        local_puuid_present: round.summoner.puuid.as_deref().is_some_and(bounded),
        lcu_map_available: !maps.is_empty() && maps.iter().all(|id| *id > 0),
        lcu_queue_mode_available: game.queue.game_mode.as_deref().is_some_and(bounded),
        ..Comparison::default()
    };
    let Some(live) = live else {
        return result;
    };
    result.full_local_identity_agrees = round
        .summoner
        .full_identity()
        .is_some_and(|id| id == live.active && full_identity(&live.active));
    result.live_map_available = live.map_id.is_some_and(|id| id > 0);
    result.map_ids_agree = live
        .map_id
        .filter(|_| !maps.is_empty())
        .map(|id| id > 0 && maps.iter().all(|lcu| *lcu == id));
    result.live_mode_available = live.game_mode.as_deref().is_some_and(bounded);
    // LCU map.gameMode is a broad category (CLASSIC); only queue.gameMode
    // has demonstrated comparability with Live's queue-specific SWIFTPLAY.
    result.queue_mode_agrees = game
        .queue
        .game_mode
        .as_deref()
        .filter(|mode| bounded(mode))
        .zip(live.game_mode.as_deref().filter(|mode| bounded(mode)))
        .map(|(lcu, live)| lcu == live);
    result
}

fn evidence(round: &LcuRound, live: Option<&LiveIdentity>) -> Option<Evidence> {
    if !compare(round, live).coherent() || round.validate_strings().is_err() {
        return None;
    }
    let live = live?;
    let game = &round.gameflow.game_data;
    Some(Evidence {
        game_id: game.game_id?,
        queue_id: game.queue.id?,
        puuid: round.summoner.puuid.clone()?,
        riot_id: round.summoner.full_identity()?,
        lcu_map: round.gameflow.map.id,
        lcu_queue_map: game.queue.map_id,
        lcu_map_mode: round.gameflow.map.game_mode.clone(),
        lcu_queue_mode: game.queue.game_mode.clone()?,
        live_map: live.map_id?,
        live_mode: live.game_mode.clone()?,
    })
}

#[cfg(test)]
mod tests;
