//! Bounded source decoding, separate from transport, persistence and association.
use serde_json::{Map, Value};

use super::*;

/// Decodes only the allowed fields. A failed response never fabricates a value.
pub fn live(endpoint: Endpoint, bytes: &[u8]) -> Result<Payload, GapReason> {
    let limit = if endpoint == Endpoint::EndOfGame {
        crate::MAX_RESULT_BYTES
    } else {
        MAX_BODY_BYTES
    };
    if bytes.len() > limit {
        return Err(GapReason::BodyLimit);
    }
    let mut value: StrictValue =
        serde_json::from_slice(bytes).map_err(|_| GapReason::InvalidJson)?;
    decode_value(endpoint, &mut value.0)
}

/// Used by the bounded LCU transport as well as fixture/live acquisition.
pub fn end_of_game<'de, D: serde::Deserializer<'de>>(d: D) -> Result<FinalFacts, D::Error> {
    use serde::de::Error;
    let mut value = StrictValue::deserialize(d)?;
    object(&mut value.0)
        .map(final_facts)
        .map_err(|_| D::Error::custom("EOG object"))
}

fn decode_value(endpoint: Endpoint, value: &mut Value) -> Result<Payload, GapReason> {
    match endpoint {
        Endpoint::GameStats => object(value).map(game_stats).map(Payload::GameStats),
        Endpoint::ActivePlayer => object(value).map(active_player).map(Payload::ActivePlayer),
        Endpoint::PlayerList => {
            let rows = value.as_array_mut().ok_or(GapReason::InvalidShape)?;
            if rows.len() > MAX_ROSTER {
                return Err(GapReason::Capacity);
            }
            Ok(Payload::PlayerList(
                rows.iter_mut().map(roster_player).collect(),
            ))
        }
        Endpoint::Events => {
            let root = object(value)?;
            let rows = root
                .get_mut("Events")
                .and_then(Value::as_array_mut)
                .ok_or(GapReason::InvalidShape)?;
            if rows.len() > MAX_EVENTS {
                return Err(GapReason::Capacity);
            }
            Ok(Payload::Events(rows.iter_mut().map(event).collect()))
        }
        Endpoint::EndOfGame => object(value)
            .map(final_facts)
            .map(Box::new)
            .map(Payload::EndOfGame),
        Endpoint::Gameflow | Endpoint::Summoner => Err(GapReason::InvalidShape),
    }
}

// Reject duplicate JSON keys instead of letting a later value silently replace
// an identity or counter. Distinct aliases remain field-level conflicts below.
struct StrictValue(Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{Error, MapAccess, SeqAccess, Visitor};
        struct Read;
        impl<'de> Visitor<'de> for Read {
            type Value = StrictValue;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_bool<E: Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_i64<E: Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_u64<E: Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_f64<E: Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| StrictValue(Value::Number(v)))
                    .ok_or_else(|| E::custom("number"))
            }
            fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_string<E: Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(value) = seq.next_element::<StrictValue>()? {
                    out.push(value.0);
                }
                Ok(StrictValue(Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut out = Map::new();
                while let Some((key, value)) = map.next_entry::<String, StrictValue>()? {
                    if out.insert(key, value.0).is_some() {
                        return Err(A::Error::custom("duplicate key"));
                    }
                }
                Ok(StrictValue(Value::Object(out)))
            }
        }
        d.deserialize_any(Read)
    }
}

fn object(value: &mut Value) -> Result<&mut Map<String, Value>, GapReason> {
    value
        .as_object_mut()
        .filter(|v| !v.contains_key("error"))
        .ok_or(GapReason::InvalidShape)
}

struct Fields<'a> {
    source: &'a mut Map<String, Value>,
    issues: FieldIssues,
}

impl<'a> Fields<'a> {
    fn new(source: &'a mut Map<String, Value>) -> Self {
        Self {
            source,
            issues: FieldIssues::new(),
        }
    }

    fn get<T>(
        &mut self,
        source: &str,
        target: &str,
        parse: impl FnOnce(&mut Value) -> Option<T>,
    ) -> Option<T> {
        let value = self.source.get_mut(source)?;
        let issue = if value.is_null() {
            Some(FieldIssue::Unavailable)
        } else if value.as_object().is_some_and(|o| o.contains_key("error")) {
            Some(FieldIssue::SourceError)
        } else {
            None
        };
        if let Some(issue) = issue {
            self.issues.insert(target.to_owned(), issue);
            return None;
        }
        match parse(value) {
            Some(value) => Some(value),
            None => {
                self.issues.insert(target.to_owned(), FieldIssue::Invalid);
                None
            }
        }
    }

    fn text(&mut self, source: &str, target: &str) -> Option<String> {
        self.get(source, target, text)
    }
    fn uint(&mut self, source: &str, target: &str) -> Option<u64> {
        self.get(source, target, uint)
    }
    fn number(&mut self, source: &str, target: &str) -> Option<f64> {
        self.get(source, target, number)
    }
    fn boolean(&mut self, source: &str, target: &str) -> Option<bool> {
        self.get(source, target, |v| v.as_bool())
    }
    fn nested<T>(
        &mut self,
        source: &str,
        target: &str,
        parse: impl FnOnce(&mut Map<String, Value>) -> T,
    ) -> Option<T> {
        self.get(source, target, |v| v.as_object_mut().map(parse))
    }
    fn time(&mut self, source: &str) -> (Option<serde_json::Number>, Option<GameTick>) {
        let original = self.get(source, "source_time", |v| v.as_number().cloned());
        let tick = original
            .as_ref()
            .and_then(|v| v.as_f64())
            .filter(|v| *v >= 0.0)
            .and_then(|v| GameTick::from_seconds(v).ok());
        if original.is_some() && tick.is_none() {
            self.issues.insert("game_tick".into(), FieldIssue::Invalid);
        }
        (original, tick)
    }
}

fn text(value: &mut Value) -> Option<String> {
    if let Value::String(text) = value
        && text.len() <= MAX_TEXT_BYTES
        && !text.chars().any(char::is_control)
    {
        return Some(std::mem::take(text));
    }
    None
}
fn uint(value: &mut Value) -> Option<u64> {
    value.as_u64().filter(|v| *v <= crate::MAX_COUNTER)
}
fn number(value: &mut Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0 && *v <= crate::MAX_COUNTER as f64)
}

/// Checks a complete identity without normalizing case or stripping the tag.
pub fn full_riot_id(value: &str) -> bool {
    let Some((name, tag)) = value.split_once('#') else {
        return false;
    };
    !name.is_empty()
        && !tag.is_empty()
        && !tag.contains('#')
        && value.len() <= 256
        && !value.chars().any(char::is_control)
        && value.trim() == value
}

fn game_stats(source: &mut Map<String, Value>) -> GameStats {
    let mut f = Fields::new(source);
    let (source_time, game_tick) = f.time("gameTime");
    GameStats {
        source_time,
        game_tick,
        game_mode: f.text("gameMode", "game_mode"),
        map_number: f.uint("mapNumber", "map_number"),
        map_name: f.text("mapName", "map_name"),
        map_terrain: f.text("mapTerrain", "map_terrain"),
        issues: f.issues,
    }
}

fn active_player(source: &mut Map<String, Value>) -> ActiveIdentity {
    let mut f = Fields::new(source);
    let riot_id = f.get("riotId", "riot_id", |v| text(v).filter(|v| full_riot_id(v)));
    ActiveIdentity {
        riot_id,
        issues: f.issues,
    }
}

fn invalid_record(value: &Value) -> FieldIssues {
    BTreeMap::from([(
        "$".to_owned(),
        if value.is_null() {
            FieldIssue::Unavailable
        } else if value.as_object().is_some_and(|v| v.contains_key("error")) {
            FieldIssue::SourceError
        } else {
            FieldIssue::Invalid
        },
    )])
}

fn roster_player(value: &mut Value) -> RosterPlayer {
    let Ok(source) = object(value) else {
        return RosterPlayer {
            issues: invalid_record(value),
            ..Default::default()
        };
    };
    let mut f = Fields::new(source);
    let riot_id = f.get("riotId", "riot_id", |v| text(v).filter(|v| full_riot_id(v)));
    RosterPlayer {
        participant: None,
        riot_id,
        riot_id_game_name: f.text("riotIdGameName", "riot_id_game_name"),
        riot_id_tag_line: f.text("riotIdTagLine", "riot_id_tag_line"),
        summoner_name: f.text("summonerName", "summoner_name"),
        champion_name: f.text("championName", "champion_name"),
        raw_champion_name: f.text("rawChampionName", "raw_champion_name"),
        skin_id: f.uint("skinID", "skin_id"),
        skin_name: f.text("skinName", "skin_name"),
        raw_skin_name: f.text("rawSkinName", "raw_skin_name"),
        team: f.text("team", "team"),
        position: f.text("position", "position"),
        is_bot: f.boolean("isBot", "is_bot"),
        level: f.uint("level", "level"),
        is_dead: f.boolean("isDead", "is_dead"),
        respawn_timer: f.number("respawnTimer", "respawn_timer"),
        scores: f.nested("scores", "scores", scores),
        items: f.get("items", "items", |v| {
            v.as_array_mut()
                .filter(|v| v.len() <= MAX_ITEMS)
                .map(|v| v.iter_mut().map(item).collect())
        }),
        summoner_spells: f.nested("summonerSpells", "summoner_spells", spells),
        runes: f.nested("runes", "runes", runes),
        issues: f.issues,
    }
}

fn scores(source: &mut Map<String, Value>) -> Scores {
    let mut f = Fields::new(source);
    Scores {
        kills: f.uint("kills", "kills"),
        deaths: f.uint("deaths", "deaths"),
        assists: f.uint("assists", "assists"),
        creep_score: f.uint("creepScore", "creep_score"),
        ward_score: f.number("wardScore", "ward_score"),
        issues: f.issues,
    }
}

fn item(value: &mut Value) -> Item {
    let Ok(source) = object(value) else {
        return Item {
            issues: invalid_record(value),
            ..Default::default()
        };
    };
    let mut f = Fields::new(source);
    Item {
        item_id: f.uint("itemID", "item_id"),
        slot: f.uint("slot", "slot"),
        count: f.uint("count", "count"),
        can_use: f.boolean("canUse", "can_use"),
        consumable: f.boolean("consumable", "consumable"),
        price: f.uint("price", "price"),
        display_name: f.text("displayName", "display_name"),
        raw_display_name: f.text("rawDisplayName", "raw_display_name"),
        raw_description: f.text("rawDescription", "raw_description"),
        issues: f.issues,
    }
}

fn label(source: &mut Map<String, Value>) -> Label {
    let mut f = Fields::new(source);
    Label {
        id: f.uint("id", "id"),
        display_name: f.text("displayName", "display_name"),
        raw_display_name: f.text("rawDisplayName", "raw_display_name"),
        raw_description: f.text("rawDescription", "raw_description"),
        issues: f.issues,
    }
}
fn spells(source: &mut Map<String, Value>) -> Spells {
    let mut f = Fields::new(source);
    Spells {
        one: f.nested("summonerSpellOne", "one", label),
        two: f.nested("summonerSpellTwo", "two", label),
        issues: f.issues,
    }
}
fn runes(source: &mut Map<String, Value>) -> Runes {
    let mut f = Fields::new(source);
    Runes {
        keystone: f.nested("keystone", "keystone", label),
        primary: f.nested("primaryRuneTree", "primary", label),
        secondary: f.nested("secondaryRuneTree", "secondary", label),
        issues: f.issues,
    }
}

fn event(value: &mut Value) -> Event {
    let Ok(source) = object(value) else {
        return Event {
            issues: invalid_record(value),
            ..Default::default()
        };
    };
    let mut f = Fields::new(source);
    let (source_time, game_tick) = f.time("EventTime");
    Event {
        source_time,
        game_tick,
        source_id: f.uint("EventID", "source_id"),
        name: f.get("EventName", "name", |v| text(v).filter(|s| !s.is_empty())),
        killer: f.text("KillerName", "killer"),
        victim: f.text("VictimName", "victim"),
        assisters: f.get("Assisters", "assisters", |v| {
            v.as_array_mut()
                .filter(|v| v.len() <= MAX_ROSTER)
                .and_then(|v| v.iter_mut().map(text).collect())
        }),
        recipient: f.text("Recipient", "recipient"),
        kill_streak: f.uint("KillStreak", "kill_streak"),
        acer: f.text("Acer", "acer"),
        acing_team: f.text("AcingTeam", "acing_team"),
        dragon_type: f.text("DragonType", "dragon_type"),
        stolen: f.boolean("Stolen", "stolen"),
        turret: f.text("TurretKilled", "turret"),
        inhibitor: f.text("InhibKilled", "inhibitor"),
        result: f.text("Result", "result"),
        actors: None,
        issues: f.issues,
    }
}

fn final_stats(source: &mut Map<String, Value>) -> FinalStats {
    let mut out = FinalStats::default();
    let mut seen = BTreeMap::<String, (Option<u64>, Option<FieldIssue>)>::new();
    for (key, value) in source {
        let canonical =
            crate::Stats::alias(key).or_else(|| special_stat(key).then_some(key.as_str()));
        let Some(canonical) = canonical else {
            continue;
        };
        out.aliases
            .entry(canonical.to_owned())
            .or_default()
            .push(key.clone());
        let number = uint(value).filter(|v| key != "WIN" || *v <= 1);
        let issue = if number.is_some() {
            None
        } else if value.is_null() {
            Some(FieldIssue::Unavailable)
        } else if value.as_object().is_some_and(|v| v.contains_key("error")) {
            Some(FieldIssue::SourceError)
        } else {
            Some(FieldIssue::Invalid)
        };
        let new = (number, issue);
        seen.entry(canonical.to_owned())
            .and_modify(|old| {
                if *old != new {
                    *old = (None, Some(FieldIssue::Conflict));
                }
            })
            .or_insert(new);
    }
    for (key, (value, issue)) in seen {
        if let Some(issue) = issue {
            out.issues.insert(key.clone(), issue);
        }
        if special_stat(&key) {
            if let Some(value) = value {
                out.extras.insert(key, value);
            }
        } else {
            out.stats.set(&key, value);
        }
    }
    out
}

pub(super) fn special_stat(key: &str) -> bool {
    matches!(
        key,
        "WIN" | "PERK_PRIMARY_STYLE" | "PERK_SUB_STYLE" | "PLAYER_SUBTEAM" | "SUBTEAM_PLACEMENT"
    ) || matches!(
        key.strip_prefix("PERK"),
        Some(
            "0" | "1"
                | "2"
                | "3"
                | "4"
                | "5"
                | "0_VAR1"
                | "0_VAR2"
                | "0_VAR3"
                | "1_VAR1"
                | "1_VAR2"
                | "1_VAR3"
                | "2_VAR1"
                | "2_VAR2"
                | "2_VAR3"
                | "3_VAR1"
                | "3_VAR2"
                | "3_VAR3"
                | "4_VAR1"
                | "4_VAR2"
                | "4_VAR3"
                | "5_VAR1"
                | "5_VAR2"
                | "5_VAR3"
        )
    ) || matches!(
        key.strip_prefix("PLAYER_AUGMENT"),
        Some("1" | "2" | "3" | "4")
    )
}

fn game_id(value: &mut Value) -> Option<String> {
    value.as_u64().filter(|id| *id > 0).map(|id| id.to_string())
}

fn final_player(value: &mut Value) -> FinalPlayer {
    let Ok(source) = object(value) else {
        return FinalPlayer {
            issues: invalid_record(value),
            ..Default::default()
        };
    };
    let mut f = Fields::new(source);
    let mut items_issues = FieldIssues::new();
    let items = f.get("items", "items", |v| {
        v.as_array_mut().filter(|v| v.len() <= 8).map(|items| {
            items
                .iter_mut()
                .enumerate()
                .map(|(index, value)| {
                    let number = uint(value);
                    if number.is_none() {
                        items_issues.insert(
                            format!("items/{index}"),
                            if value.is_null() {
                                FieldIssue::Unavailable
                            } else if value.as_object().is_some_and(|v| v.contains_key("error")) {
                                FieldIssue::SourceError
                            } else {
                                FieldIssue::Invalid
                            },
                        );
                    }
                    number
                })
                .collect()
        })
    });
    f.issues.extend(items_issues);
    FinalPlayer {
        items,
        game_id: f.get("gameId", "game_id", game_id),
        puuid: f.text("puuid", "puuid"),
        riot_id_game_name: f.text("riotIdGameName", "riot_id_game_name"),
        riot_id_tag_line: f.text("riotIdTagLine", "riot_id_tag_line"),
        team_id: f.uint("teamId", "team_id"),
        champion_id: f.uint("championId", "champion_id"),
        champion_name: f.text("championName", "champion_name"),
        is_local_player: f.boolean("isLocalPlayer", "is_local_player"),
        spell1_id: f.uint("spell1Id", "spell1_id"),
        spell2_id: f.uint("spell2Id", "spell2_id"),
        selected_position: f.text("selectedPosition", "selected_position"),
        detected_team_position: f.text("detectedTeamPosition", "detected_team_position"),
        stats: f.nested("stats", "stats", final_stats),
        issues: f.issues,
    }
}
fn final_team(value: &mut Value) -> FinalTeam {
    let Ok(source) = object(value) else {
        return FinalTeam {
            issues: invalid_record(value),
            ..Default::default()
        };
    };
    let mut f = Fields::new(source);
    FinalTeam {
        team_id: f.uint("teamId", "team_id"),
        is_winning_team: f.boolean("isWinningTeam", "is_winning_team"),
        players: f.get("players", "players", |v| {
            v.as_array_mut()
                .filter(|v| v.len() <= MAX_ROSTER)
                .map(|v| v.iter_mut().map(final_player).collect())
        }),
        stats: f.nested("stats", "stats", final_stats),
        issues: f.issues,
    }
}
fn final_facts(source: &mut Map<String, Value>) -> FinalFacts {
    let mut f = Fields::new(source);
    FinalFacts {
        game_id: f.get("gameId", "game_id", game_id),
        local_player: f.get("localPlayer", "local_player", |v| {
            v.is_object().then(|| final_player(v))
        }),
        teams: f.get("teams", "teams", |v| {
            v.as_array_mut()
                .filter(|v| v.len() <= crate::MAX_TEAMS)
                .map(|v| v.iter_mut().map(final_team).collect())
        }),
        game_length: f.uint("gameLength", "game_length"),
        end_of_game_timestamp: f.uint("endOfGameTimestamp", "end_of_game_timestamp"),
        game_mode: f.text("gameMode", "game_mode"),
        game_version: f.text("gameVersion", "game_version"),
        game_type: f.text("gameType", "game_type"),
        queue_type: f.text("queueType", "queue_type"),
        queue_id: f.uint("queueId", "queue_id"),
        map_id: f.uint("mapId", "map_id"),
        invalid: f.boolean("invalid", "invalid"),
        game_ended_in_early_surrender: f
            .boolean("gameEndedInEarlySurrender", "game_ended_in_early_surrender"),
        team_early_surrendered: f.boolean("teamEarlySurrendered", "team_early_surrendered"),
        caused_early_surrender: f.boolean("causedEarlySurrender", "caused_early_surrender"),
        early_surrender_accomplice: f
            .boolean("earlySurrenderAccomplice", "early_surrender_accomplice"),
        issues: f.issues,
    }
}
