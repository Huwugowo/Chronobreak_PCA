//! Allowlisted League observations. Values carry one shared source/timing envelope;
//! absent fields are missing, while exceptional field states are stored sparsely.
//! These are source observations, never media or participant association authority.

use std::collections::BTreeMap;

use chronobreak_replay_time::{GameCalibrationV2, GameTick, MediaId};
use serde::{Deserialize, Serialize};

pub mod checkpoint;
pub mod decode;
pub mod events;
mod final_facts;
pub mod identity;
pub mod journal;
pub mod manifest;
mod queue_charge;
pub mod roster;

/// Hash the existing JSON representation without allocating an encoded copy.
fn fingerprint(value: &impl Serialize) -> Result<[u8; 32], serde_json::Error> {
    use sha2::{Digest, Sha256};
    struct HashWriter(Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, value)?;
    Ok(writer.0.finalize().into())
}

/// Safety caps, not claims about the roster sizes or modes League supports.
pub const MAX_ROSTER: usize = 64;
pub const MAX_ITEMS: usize = 32;
pub const MAX_EVENTS: usize = 65_536;
pub const MAX_TEXT_BYTES: usize = 1_024;
pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// An ID local to a recording and acquisition run. It is not a League game ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObservationId(pub u64);

/// References are scoped to this epoch; restarting cannot prove source continuity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEpoch {
    pub media_id: MediaId,
    pub run_id: MediaId,
    pub epoch: u64,
}

/// Assigned only after an unambiguous exact source-identity match within an epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParticipantId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Endpoint {
    GameStats,
    ActivePlayer,
    PlayerList,
    Events,
    Gameflow,
    Summoner,
    EndOfGame,
}

/// Monotonic microseconds within `SourceEpoch.run_id`; never replay coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestTiming {
    pub start_us: u64,
    pub end_us: u64,
    pub received_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub id: ObservationId,
    pub source: SourceEpoch,
    pub endpoint: Endpoint,
    pub timing: RequestTiming,
    /// References the independently acquired GS observation, if one is available.
    pub game_stats: Option<ObservationId>,
    /// References the calibration that actually existed at ingestion, never a guess.
    pub calibration: Option<ObservationId>,
}

/// Exceptions only; missing keys have implicit `Absent` semantics. A null source
/// value is explicitly unavailable. Not-applicable always names its evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldIssue {
    Unavailable,
    SourceError,
    Invalid,
    Conflict,
    NotApplicable { evidence: ObservationId },
}

pub type FieldIssues = BTreeMap<String, FieldIssue>;

macro_rules! payload {
    ($(#[$attr:meta])* $name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            $(#[serde(default, skip_serializing_if = "Option::is_none")]
            pub $field: Option<$ty>,)*
            #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
            pub issues: FieldIssues,
        }
        impl queue_charge::Charge for $name {
            fn charge(&self) -> usize {
                let mut bytes = std::mem::size_of::<Self>().saturating_add(128);
                $(if let Some(value) = &self.$field {
                    bytes = bytes.saturating_add(stringify!($field).len() + 4)
                        .saturating_add(queue_charge::Charge::charge(value));
                })*
                bytes.saturating_add(queue_charge::Charge::charge(&self.issues))
            }
        }
    };
}

payload!(GameStats {
    source_time: serde_json::Number,
    game_tick: GameTick,
    game_mode: String,
    map_number: u64,
    map_name: String,
    map_terrain: String,
});

payload!(ActiveIdentity { riot_id: String });

payload!(RosterPlayer {
    participant: ParticipantId,
    riot_id: String,
    riot_id_game_name: String,
    riot_id_tag_line: String,
    summoner_name: String,
    champion_name: String,
    raw_champion_name: String,
    skin_id: u64,
    skin_name: String,
    raw_skin_name: String,
    team: String,
    position: String,
    is_bot: bool,
    level: u64,
    is_dead: bool,
    respawn_timer: f64,
    scores: Scores,
    items: Vec<Item>,
    summoner_spells: Spells,
    runes: Runes,
});

payload!(Scores {
    kills: u64,
    deaths: u64,
    assists: u64,
    creep_score: u64,
    ward_score: f64
});
payload!(Item {
    item_id: u64,
    slot: u64,
    count: u64,
    can_use: bool,
    consumable: bool,
    price: u64,
    display_name: String,
    raw_display_name: String,
    raw_description: String,
});
payload!(Label {
    id: u64,
    display_name: String,
    raw_display_name: String,
    raw_description: String
});
payload!(Spells {
    one: Label,
    two: Label
});
payload!(Runes {
    keystone: Label,
    primary: Label,
    secondary: Label
});

// Acquisition facts only: this does not confirm a game ID or enable LOSS.
payload!(#[derive(Eq)] FinalPlayer {
    game_id: String, puuid: String, riot_id_game_name: String, riot_id_tag_line: String,
    team_id: u64, champion_id: u64, champion_name: String, is_local_player: bool,
    spell1_id: u64, spell2_id: u64, selected_position: String, detected_team_position: String,
    items: Vec<Option<u64>>, stats: FinalStats,
});
payload!(#[derive(Eq)] FinalTeam { team_id: u64, is_winning_team: bool, players: Vec<FinalPlayer>, stats: FinalStats });
payload!(#[derive(Eq)] FinalFacts {
    game_id: String, local_player: FinalPlayer, teams: Vec<FinalTeam>,
    game_length: u64, end_of_game_timestamp: u64, game_mode: String, game_version: String,
    game_type: String, queue_type: String, queue_id: u64, map_id: u64,
    invalid: bool, game_ended_in_early_surrender: bool, team_early_surrendered: bool,
    caused_early_surrender: bool, early_surrender_accomplice: bool,
});

/// Canonical counters reuse the existing stat alias contract. Rune/mode/WIN keys
/// are explicitly allowlisted separately; aliases retain source provenance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalStats {
    pub stats: crate::Stats,
    pub extras: BTreeMap<String, u64>,
    pub aliases: BTreeMap<String, Vec<String>>,
    pub issues: FieldIssues,
}

// Unknown kinds retain the envelope and these allowlisted fields. No raw payload
// archive or invented actor reference is produced for unknown names.
payload!(Event {
    source_id: u64,
    source_time: serde_json::Number,
    game_tick: GameTick,
    name: String,
    killer: String,
    victim: String,
    assisters: Vec<String>,
    recipient: String,
    kill_streak: u64,
    acer: String,
    acing_team: String,
    dragon_type: String,
    stolen: bool,
    turret: String,
    inhibitor: String,
    result: String,
    actors: events::EventActors,
});

impl Event {
    pub fn known_kind(&self) -> bool {
        matches!(
            self.name.as_deref(),
            Some(
                "GameStart"
                    | "MinionsSpawning"
                    | "GameEnd"
                    | "ChampionKill"
                    | "FirstBlood"
                    | "Multikill"
                    | "Ace"
                    | "DragonKill"
                    | "BaronKill"
                    | "HeraldKill"
                    | "TurretKilled"
                    | "FirstBrick"
                    | "InhibKilled"
                    | "InhibRespawningSoon"
                    | "InhibRespawned"
            )
        )
    }

    pub fn valid_envelope(&self) -> bool {
        self.source_id.is_some() && self.game_tick.is_some() && self.name.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Payload {
    GameStats(GameStats),
    ActivePlayer(ActiveIdentity),
    PlayerList(Vec<RosterPlayer>),
    Events(Vec<Event>),
    EndOfGame(Box<FinalFacts>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    Timeout,
    Transport,
    Http,
    InvalidJson,
    InvalidShape,
    BodyLimit,
    Capacity,
    Backpressure,
    Cancelled,
    SourceReset,
    CrashTail,
    DiskFailure,
}

/// Every attempt, including equal successful observations, gets its own evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation {
    Success {
        evidence: Evidence,
        payload: Payload,
    },
    Gap {
        evidence: Evidence,
        reason: GapReason,
    },
}

/// Bounded gap aggregation is reserved for attempts the sink could not retain.
/// It does not stand in for successful observation timing or imply full coverage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case", deny_unknown_fields)]
pub enum Record {
    Observation {
        observation: Box<Observation>,
    },
    EventObservation {
        evidence: Evidence,
        revisions: Vec<events::EventRevision>,
    },
    RosterObservation {
        evidence: Evidence,
        roster: roster::RosterObservation,
    },
    Calibration {
        evidence: Evidence,
        calibration: GameCalibrationV2,
    },
    CalibrationInvalidated {
        evidence: Evidence,
    },
    GapInterval {
        first: Evidence,
        last: Evidence,
        attempts: u64,
        reason: GapReason,
    },
    Boundary {
        evidence: Evidence,
        active: bool,
    },
    SourceReset {
        evidence: Evidence,
        previous_epoch: u64,
    },
    LocalParticipant {
        evidence: Evidence,
        participant: ParticipantId,
    },
    IdentityRound {
        round: Box<identity::IdentityRound>,
    },
}
