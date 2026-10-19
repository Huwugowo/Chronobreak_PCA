//! Test-only historical decoder cost baseline; never used by recording or reads.
#![allow(dead_code)]
use serde::Deserialize;
use serde_json::{Map, Value};
#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct RawActivePlayer {
    #[serde(rename = "currentGold")]
    current_gold: Option<f64>,
    #[serde(rename = "summonerName")]
    summoner_name: Option<String>,
    #[serde(rename = "riotId")]
    riot_id: Option<String>,
    #[serde(rename = "championStats")]
    champion_stats: Option<RawChampionStats>,
    #[serde(rename = "fullRunes", default)]
    full_runes: RawFullRunes,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawChampionStats {
    #[serde(rename = "currentHealth")]
    current_health: Option<f64>,
    #[serde(rename = "maxHealth")]
    max_health: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawFullRunes {
    #[serde(rename = "generalRunes", default)]
    general_runes: Vec<RawRune>,
    #[serde(rename = "statRunes", default)]
    stat_runes: Vec<RawRune>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct RawPlayer {
    #[serde(rename = "summonerName", default)]
    summoner_name: String,
    #[serde(rename = "riotId")]
    riot_id: Option<String>,
    #[serde(rename = "championName", default)]
    champion_name: String,
    #[serde(default)]
    team: String,
    #[serde(default)]
    level: u32,
    #[serde(default, deserialize_with = "deserialize_items")]
    items: Vec<RawItem>,
    #[serde(default)]
    scores: RawScores,
    #[serde(default)]
    runes: RawRunes,
    #[serde(rename = "summonerSpells", default)]
    summoner_spells: RawSummonerSpells,
}

#[derive(Debug, Clone, Deserialize)]
struct RawItem {
    #[serde(rename = "itemID", default)]
    item_id: u32,
    #[serde(default)]
    slot: u32,
    #[serde(default)]
    count: u32,
}

fn deserialize_items<'de, D>(deserializer: D) -> std::result::Result<Vec<RawItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Array(items) => {
            serde_json::from_value(Value::Array(items)).map_err(serde::de::Error::custom)
        }
        Value::Object(error) if error.contains_key("error") => Ok(Vec::new()),
        other => Err(serde::de::Error::custom(format!(
            "expected an item array or API error object, got {other}"
        ))),
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawScores {
    #[serde(rename = "creepScore", default)]
    creep_score: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawRunes {
    keystone: Option<RawRune>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRune {
    id: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawSummonerSpells {
    #[serde(rename = "summonerSpellOne")]
    spell_one: Option<RawSpell>,
    #[serde(rename = "summonerSpellTwo")]
    spell_two: Option<RawSpell>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSpell {
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(rename = "rawDisplayName", default)]
    raw_display_name: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct RawGameData {
    #[serde(rename = "gameTime")]
    game_time: Option<f64>,
    #[serde(rename = "gameMode")]
    game_mode: Option<String>,
    #[serde(rename = "mapNumber", default, deserialize_with = "optional_probe_u32")]
    map_number: Option<u32>,
}

fn optional_probe_u32<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<u32>, D::Error> {
    Ok(Value::deserialize(deserializer)?
        .as_u64()
        .and_then(|n| u32::try_from(n).ok()))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct RawEventData {
    #[serde(rename = "Events", default)]
    events: Vec<RawEvent>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawEvent {
    #[serde(rename = "EventID")]
    event_id: i64,
    #[serde(rename = "EventName")]
    event_name: String,
    #[serde(rename = "EventTime")]
    event_time: f64,
    #[serde(flatten)]
    fields: Map<String, Value>,
}
