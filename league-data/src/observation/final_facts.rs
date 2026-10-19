//! Validation at the optional sidecar/index trust boundary.
use super::{FieldIssues, FinalFacts, FinalPlayer, FinalStats, MAX_ROSTER, MAX_TEXT_BYTES, decode};
use crate::{MAX_COUNTER, MAX_PLAYERS, MAX_TEAMS, Stats, canonical_game_id};
use serde_json::Value;

impl FinalFacts {
    pub fn valid_for(&self, game: &str) -> bool {
        self.game_id.as_deref() == Some(game)
            && canonical_game_id(game)
            && self.local_player.as_ref().is_none_or(|p| player(p, game))
            && self.teams.as_ref().is_none_or(|teams| {
                teams.len() <= MAX_TEAMS
                    && teams
                        .iter()
                        .map(|t| t.players.as_ref().map_or(0, Vec::len))
                        .sum::<usize>()
                        <= MAX_PLAYERS
                    && teams.iter().all(|t| {
                        t.stats.as_ref().is_none_or(stats)
                            && t.players
                                .as_ref()
                                .is_none_or(|rows| rows.iter().all(|p| player(p, game)))
                            && payload_issues(t, &t.issues, TEAM_FIELDS, true, false)
                    })
            })
            && payload_issues(self, &self.issues, FACT_FIELDS, false, false)
            && serde_json::to_value(self).is_ok_and(|value| values(&value))
    }
}

fn player(p: &FinalPlayer, game: &str) -> bool {
    p.game_id.as_ref().is_none_or(|id| id == game)
        && p.items.as_ref().is_none_or(|v| v.len() <= 8)
        && p.stats.as_ref().is_none_or(stats)
        && payload_issues(p, &p.issues, PLAYER_FIELDS, true, true)
}

const FACT_FIELDS: &[&str] = &[
    "game_id",
    "local_player",
    "teams",
    "game_length",
    "end_of_game_timestamp",
    "game_mode",
    "game_version",
    "game_type",
    "queue_type",
    "queue_id",
    "map_id",
    "invalid",
    "game_ended_in_early_surrender",
    "team_early_surrendered",
    "caused_early_surrender",
    "early_surrender_accomplice",
];
const PLAYER_FIELDS: &[&str] = &[
    "game_id",
    "puuid",
    "riot_id_game_name",
    "riot_id_tag_line",
    "team_id",
    "champion_id",
    "champion_name",
    "is_local_player",
    "spell1_id",
    "spell2_id",
    "selected_position",
    "detected_team_position",
    "items",
    "stats",
];
const TEAM_FIELDS: &[&str] = &["team_id", "is_winning_team", "players", "stats"];

fn payload_issues(
    value: &impl serde::Serialize,
    issues: &FieldIssues,
    allowed: &[&str],
    record_issue: bool,
    item_issues: bool,
) -> bool {
    let Ok(Value::Object(fields)) = serde_json::to_value(value) else {
        return false;
    };
    issues.iter().all(|(key, issue)| {
        // EOG has no observation-ID namespace for an applicability reference.
        if !matches!(
            issue,
            super::FieldIssue::Unavailable
                | super::FieldIssue::SourceError
                | super::FieldIssue::Invalid
        ) {
            return false;
        }
        if key == "$" {
            return record_issue && fields.len() == 1 && issues.len() == 1;
        }
        if let Some(index) = key.strip_prefix("items/") {
            return item_issues
                && index.parse::<usize>().ok().is_some_and(|n| {
                    index == n.to_string()
                        && fields
                            .get("items")
                            .and_then(Value::as_array)
                            .and_then(|items| items.get(n))
                            .is_some_and(Value::is_null)
                });
        }
        allowed.contains(&key.as_str()) && !fields.contains_key(key)
    })
}

fn stats(s: &FinalStats) -> bool {
    let canonical = |key: &str| Stats::KEYS.contains(&key) || decode::special_stat(key);
    s.stats.valid()
        && s.extras.iter().all(|(key, value)| {
            decode::special_stat(key) && *value <= MAX_COUNTER && (key != "WIN" || *value <= 1)
        })
        && s.aliases.iter().all(|(key, aliases)| {
            canonical(key)
                && !aliases.is_empty()
                && aliases.len() <= 2
                && aliases.iter().enumerate().all(|(i, alias)| {
                    !aliases[..i].contains(alias)
                        && (Stats::alias(alias) == Some(key)
                            || (decode::special_stat(key) && alias == key))
                })
        })
        && s.issues.iter().all(|(key, issue)| {
            canonical(key)
                && s.stats.get(key).is_none()
                && !s.extras.contains_key(key)
                && !matches!(issue, super::FieldIssue::NotApplicable { .. })
        })
}

fn values(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(_) => true,
        Value::Number(n) => n.as_u64().is_some_and(|n| n <= MAX_COUNTER),
        Value::String(s) => s.len() <= MAX_TEXT_BYTES && !s.chars().any(char::is_control),
        Value::Array(rows) => rows.len() <= MAX_ROSTER && rows.iter().all(values),
        Value::Object(fields) => {
            fields.len() <= 256
                && fields.iter().all(|(key, value)| {
                    !key.is_empty()
                        && key.len() <= 128
                        && !key.chars().any(char::is_control)
                        && values(value)
                })
        }
    }
}
