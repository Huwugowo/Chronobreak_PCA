//! Bounded, allowlisted optional post-game facts shared by producer and consumer.
use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Serialize};

pub const RESULT_FILE: &str = "league_result.json";
pub const MAX_RESULT_BYTES: usize = 256 * 1024;
pub const MAX_PLAYERS: usize = 64;
pub const MAX_TEAMS: usize = 16;
pub const MAX_COUNTER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Win,
    Loss,
    Unknown,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Present,
    Partial,
    Missing,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Families {
    pub combat: Coverage,
    pub economy: Coverage,
    pub damage: Coverage,
    pub support: Coverage,
    pub vision: Coverage,
    pub objectives: Coverage,
    pub loadout: Coverage,
    pub runes: Coverage,
}
impl Families {
    pub fn partial(&self) -> bool {
        [
            self.combat,
            self.economy,
            self.damage,
            self.support,
            self.vision,
            self.objectives,
            self.loadout,
            self.runes,
        ]
        .iter()
        .any(|v| *v != Coverage::Present)
    }
}

// The macro defines a concrete numeric contract, not a raw-stat archive.
macro_rules! stats {
    ($($field:ident => [$($alias:literal),+]),+ $(,)?) => {
        #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(deny_unknown_fields)]
        pub struct Stats { $(pub $field: Option<u64>,)+ }
        impl Stats {
            pub fn valid(&self) -> bool { true $(&& self.$field.is_none_or(|v| v <= MAX_COUNTER))+ }
            pub fn alias(key: &str) -> Option<&'static str> {
                match key { $($($alias)|+ => Some(stringify!($field)),)+ _ => None }
            }
            pub fn set(&mut self, key: &str, value: Option<u64>) {
                match key { $(stringify!($field) => self.$field = value,)+ _ => {} }
            }
        }
    }
}
stats! {
    kills => ["CHAMPIONS_KILLED", "kills"], deaths => ["NUM_DEATHS", "deaths"], assists => ["ASSISTS", "assists"],
    level => ["LEVEL", "champLevel"], minions => ["MINIONS_KILLED", "totalMinionsKilled"],
    jungle_minions => ["NEUTRAL_MINIONS_KILLED", "neutralMinionsKilled"],
    ally_jungle_minions => ["NEUTRAL_MINIONS_KILLED_YOUR_JUNGLE", "totalAllyJungleMinionsKilled"],
    enemy_jungle_minions => ["NEUTRAL_MINIONS_KILLED_ENEMY_JUNGLE", "totalEnemyJungleMinionsKilled"],
    gold_earned => ["GOLD_EARNED", "goldEarned"], gold_spent => ["goldSpent"],
    damage => ["TOTAL_DAMAGE_DEALT", "totalDamageDealt"], champion_damage => ["TOTAL_DAMAGE_DEALT_TO_CHAMPIONS", "totalDamageDealtToChampions"],
    objective_damage => ["TOTAL_DAMAGE_DEALT_TO_OBJECTIVES", "damageDealtToObjectives"], turret_damage => ["TOTAL_DAMAGE_DEALT_TO_TURRETS", "damageDealtToTurrets"],
    physical_damage => ["PHYSICAL_DAMAGE_DEALT_PLAYER", "physicalDamageDealt"], physical_champion_damage => ["PHYSICAL_DAMAGE_DEALT_TO_CHAMPIONS", "physicalDamageDealtToChampions"],
    magic_damage => ["MAGIC_DAMAGE_DEALT_PLAYER", "magicDamageDealt"], magic_champion_damage => ["MAGIC_DAMAGE_DEALT_TO_CHAMPIONS", "magicDamageDealtToChampions"],
    true_damage => ["TRUE_DAMAGE_DEALT_PLAYER", "trueDamageDealt"], true_champion_damage => ["TRUE_DAMAGE_DEALT_TO_CHAMPIONS", "trueDamageDealtToChampions"],
    damage_taken => ["TOTAL_DAMAGE_TAKEN", "totalDamageTaken"], physical_damage_taken => ["PHYSICAL_DAMAGE_TAKEN", "physicalDamageTaken"],
    magic_damage_taken => ["MAGIC_DAMAGE_TAKEN", "magicDamageTaken"], true_damage_taken => ["TRUE_DAMAGE_TAKEN", "trueDamageTaken"],
    mitigated => ["TOTAL_DAMAGE_SELF_MITIGATED", "damageSelfMitigated"], shielding => ["TOTAL_DAMAGE_SHIELDED_ON_TEAMMATES", "totalDamageShieldedOnTeammates"],
    healing => ["TOTAL_HEAL", "totalHeal"], teammate_healing => ["TOTAL_HEAL_ON_TEAMMATES", "totalHealsOnTeammates"],
    crowd_control => ["TIME_CCING_OTHERS", "timeCCingOthers"], total_crowd_control => ["totalTimeCCDealt"],
    time_dead => ["totalTimeSpentDead"], units_healed => ["totalUnitsHealed"],
    vision => ["VISION_SCORE", "visionScore"], wards_killed => ["WARD_KILLED", "wardsKilled"], wards_placed => ["WARD_PLACED", "wardsPlaced"],
    control_wards_bought => ["VISION_WARDS_BOUGHT_IN_GAME", "visionWardsBoughtInGame"], detector_wards_placed => ["detectorWardsPlaced"],
    critical_strike => ["LARGEST_CRITICAL_STRIKE", "largestCriticalStrike"], killing_spree => ["LARGEST_KILLING_SPREE", "largestKillingSpree"],
    multi_kill => ["LARGEST_MULTI_KILL", "largestMultiKill"], double_kills => ["doubleKills"], triple_kills => ["tripleKills"],
    quadra_kills => ["quadraKills"], penta_kills => ["pentaKills"], turrets => ["TURRETS_KILLED", "turretKills"],
    inhibitors => ["BARRACKS_KILLED", "inhibitorKills"], barons => ["baronKills"], dragons => ["dragonKills"],
    objectives_stolen => ["objectivesStolen"], objectives_stolen_assists => ["objectivesStolenAssists"],
    first_blood => ["firstBloodKill"], first_blood_assist => ["firstBloodAssist"], first_tower => ["firstTowerKill"], first_tower_assist => ["firstTowerAssist"],
    spell1_casts => ["spell1Casts"], spell2_casts => ["spell2Casts"], spell3_casts => ["spell3Casts"], spell4_casts => ["spell4Casts"],
    summoner1_casts => ["summoner1Casts"], summoner2_casts => ["summoner2Casts"]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Perk {
    pub id: Option<u64>,
    pub counters: [Option<u64>; 3],
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Player {
    pub team_id: Option<u64>,
    pub champion_id: Option<u64>,
    pub champion_name: Option<String>,
    pub riot_id: Option<String>,
    pub is_local: bool,
    pub spell1_id: Option<u64>,
    pub spell2_id: Option<u64>,
    pub position: Option<String>,
    pub detected_position: Option<String>,
    pub items: Vec<Option<u64>>,
    pub perks: Vec<Perk>,
    pub primary_style: Option<u64>,
    pub secondary_style: Option<u64>,
    pub augments: [Option<u64>; 4],
    pub subteam_id: Option<u64>,
    pub subteam_placement: Option<u64>,
    pub stats: Stats,
}
impl Player {
    fn valid(&self) -> bool {
        self.items.len() <= 8
            && self.perks.len() <= 6
            && self.stats.valid()
            && [
                &self.champion_name,
                &self.riot_id,
                &self.position,
                &self.detected_position,
            ]
            .iter()
            .all(|s| s.as_ref().is_none_or(|v| bounded(v)))
            && [
                self.team_id,
                self.champion_id,
                self.spell1_id,
                self.spell2_id,
                self.primary_style,
                self.secondary_style,
                self.subteam_id,
                self.subteam_placement,
            ]
            .into_iter()
            .chain(self.items.iter().copied())
            .chain(self.augments)
            .chain(
                self.perks
                    .iter()
                    .flat_map(|p| std::iter::once(p.id).chain(p.counters)),
            )
            .all(|v| v.is_none_or(|v| v <= MAX_COUNTER))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Team {
    pub team_id: Option<u64>,
    pub is_winning: Option<bool>,
    pub stats: Stats,
    pub players: Vec<Player>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResultFile {
    pub schema_version: u32,
    pub source: String,
    pub confirmed: bool,
    pub media_id: MediaId,
    pub game_id: String,
    pub outcome: Outcome,
    pub ended_early: bool,
    pub coverage: Families,
    pub duration_seconds: Option<u64>,
    pub end_timestamp_ms: Option<u64>,
    pub queue_id: Option<u64>,
    pub map_id: Option<u64>,
    pub game_version: Option<String>,
    pub game_mode: Option<String>,
    pub game_type: Option<String>,
    pub queue_type: Option<String>,
    pub label_mismatch: bool,
    pub local_player: Option<Player>,
    pub teams: Vec<Team>,
}
impl ResultFile {
    pub fn valid_for(&self, media: &MediaId, game: &str) -> bool {
        self.schema_version == 1
            && self.source == "lcu_eog"
            && self.confirmed
            && &self.media_id == media
            && self.game_id == game
            && canonical_game_id(game)
            && self.teams.len() <= MAX_TEAMS
            && self.teams.iter().map(|t| t.players.len()).sum::<usize>() <= MAX_PLAYERS
            && self.local_player.as_ref().is_none_or(Player::valid)
            && self.teams.iter().all(|t| {
                t.stats.valid()
                    && t.team_id.is_none_or(|v| v <= MAX_COUNTER)
                    && t.players.iter().all(Player::valid)
            })
            && [
                self.duration_seconds,
                self.end_timestamp_ms,
                self.queue_id,
                self.map_id,
            ]
            .iter()
            .all(|v| v.is_none_or(|v| v <= MAX_COUNTER))
            && [
                &self.game_mode,
                &self.game_type,
                &self.queue_type,
                &self.game_version,
            ]
            .iter()
            .all(|s| s.as_ref().is_none_or(|v| bounded(v)))
    }
    pub fn summary(&self) -> Summary {
        Summary {
            outcome: self.outcome,
            ended_early: self.ended_early,
            partial: self.coverage.partial(),
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Summary {
    pub outcome: Outcome,
    pub ended_early: bool,
    pub partial: bool,
}
pub fn bounded(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
pub fn canonical_game_id(v: &str) -> bool {
    v.parse::<u64>().is_ok_and(|n| n > 0 && n.to_string() == v)
}
