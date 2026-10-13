//! Purpose-built EOG ingestion. Unknown keys are skipped, never archived.
use chronobreak_league_data::{
    Coverage, Families, MAX_COUNTER, MAX_PLAYERS, MAX_TEAMS, Outcome, Perk, Player, ResultFile,
    Stats, Team, bounded,
};
use chronobreak_replay_time::MediaId;
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone)]
pub(super) struct Binding {
    pub media_id: MediaId,
    pub game_id: String,
    pub puuid: String,
    pub riot_id: String,
    pub map_id: u32,
    pub mode: String,
    pub queue_id: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Rejection {
    NotReady,
    Association,
}

// Reading only an explicitly allowlisted field permits malformed scalars to be
// discarded without making otherwise useful facts fail as a unit.
fn lenient<'de, D: Deserializer<'de>, T: serde::de::DeserializeOwned>(
    d: D,
) -> Result<Option<T>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Field<T> {
        Valid(T),
        Invalid(IgnoredAny),
    }
    Ok(match Field::<T>::deserialize(d)? {
        Field::Valid(value) => Some(value),
        Field::Invalid(_) => None,
    })
}
fn number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    Ok(lenient::<D, u64>(d)?.filter(|v| *v <= MAX_COUNTER))
}
fn text<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(lenient::<D, String>(d)?.filter(|v| bounded(v)))
}
fn game_id<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    Ok(lenient::<D, u64>(d)?.filter(|v| *v > 0))
}
fn items<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<Option<u64>>>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_array().filter(|v| v.len() <= 8).map(|v| {
        v.iter()
            .map(|v| v.as_u64().filter(|v| *v <= MAX_COUNTER))
            .collect()
    }))
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Row<T> {
    Valid(T),
    Invalid(IgnoredAny),
}
fn players<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<RawPlayer>>, D::Error> {
    Ok(lenient::<D, Vec<Row<RawPlayer>>>(d)?.map(|rows| {
        rows.into_iter()
            .filter_map(|r| match r {
                Row::Valid(p) => Some(p),
                Row::Invalid(_) => None,
            })
            .collect()
    }))
}

#[derive(Default)]
struct RawStats {
    stats: Stats,
    win: Option<u64>,
    perks: [Option<u64>; 6],
    vars: [[Option<u64>; 3]; 6],
    primary: Option<u64>,
    secondary: Option<u64>,
    augments: [Option<u64>; 4],
    subteam: Option<u64>,
    placement: Option<u64>,
}
impl<'de> Deserialize<'de> for RawStats {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Read;
        impl<'de> Visitor<'de> for Read {
            type Value = RawStats;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("allowlisted numeric stats")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut out = RawStats::default();
                let mut seen = BTreeMap::<String, Option<u64>>::new();
                while let Some(key) = map.next_key::<String>()? {
                    let canonical = Stats::alias(&key).map(str::to_owned).or_else(|| {
                        if key == "WIN"
                            || key == "PERK_PRIMARY_STYLE"
                            || key == "PERK_SUB_STYLE"
                            || key == "PLAYER_SUBTEAM"
                            || key == "SUBTEAM_PLACEMENT"
                            || (0..6).any(|i| {
                                key == format!("PERK{i}")
                                    || (1..=3).any(|j| key == format!("PERK{i}_VAR{j}"))
                            })
                            || (1..=4).any(|i| key == format!("PLAYER_AUGMENT{i}"))
                        {
                            Some(key.clone())
                        } else {
                            None
                        }
                    });
                    if let Some(canonical) = canonical {
                        let v = map
                            .next_value::<serde_json::Value>()?
                            .as_u64()
                            .filter(|v| *v <= MAX_COUNTER);
                        seen.entry(canonical)
                            .and_modify(|old| {
                                if *old != v {
                                    *old = None;
                                }
                            })
                            .or_insert(v);
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                for (key, v) in seen {
                    out.stats.set(&key, v);
                    match key.as_str() {
                        "WIN" => out.win = v.filter(|v| *v <= 1),
                        "PERK_PRIMARY_STYLE" => out.primary = v,
                        "PERK_SUB_STYLE" => out.secondary = v,
                        "PLAYER_SUBTEAM" => out.subteam = v,
                        "SUBTEAM_PLACEMENT" => out.placement = v,
                        _ => {}
                    }
                    for i in 0..6 {
                        if key == format!("PERK{i}") {
                            out.perks[i] = v;
                        }
                        for j in 0..3 {
                            if key == format!("PERK{i}_VAR{}", j + 1) {
                                out.vars[i][j] = v;
                            }
                        }
                    }
                    for i in 0..4 {
                        if key == format!("PLAYER_AUGMENT{}", i + 1) {
                            out.augments[i] = v;
                        }
                    }
                }
                Ok(out)
            }
        }
        d.deserialize_map(Read)
    }
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RawPlayer {
    #[serde(deserialize_with = "game_id")]
    game_id: Option<u64>,
    #[serde(deserialize_with = "text")]
    puuid: Option<String>,
    #[serde(deserialize_with = "text")]
    riot_id_game_name: Option<String>,
    #[serde(deserialize_with = "text")]
    riot_id_tag_line: Option<String>,
    #[serde(deserialize_with = "number")]
    team_id: Option<u64>,
    #[serde(deserialize_with = "number")]
    champion_id: Option<u64>,
    #[serde(deserialize_with = "text")]
    champion_name: Option<String>,
    #[serde(deserialize_with = "lenient")]
    is_local_player: Option<bool>,
    #[serde(deserialize_with = "number")]
    spell1_id: Option<u64>,
    #[serde(deserialize_with = "number")]
    spell2_id: Option<u64>,
    #[serde(deserialize_with = "text")]
    selected_position: Option<String>,
    #[serde(deserialize_with = "text")]
    detected_team_position: Option<String>,
    #[serde(deserialize_with = "items")]
    items: Option<Vec<Option<u64>>>,
    #[serde(deserialize_with = "lenient")]
    stats: Option<RawStats>,
}
impl RawPlayer {
    fn riot(&self) -> Option<String> {
        Some(format!(
            "{}#{}",
            self.riot_id_game_name.as_ref()?,
            self.riot_id_tag_line.as_ref()?
        ))
        .filter(|v| super::full_identity(v))
    }
    fn win(&self) -> Option<u64> {
        self.stats.as_ref()?.win
    }
    fn facts(&self, local: bool) -> Player {
        let empty = RawStats::default();
        let s = self.stats.as_ref().unwrap_or(&empty);
        Player {
            team_id: self.team_id,
            champion_id: self.champion_id,
            champion_name: self.champion_name.clone(),
            riot_id: self.riot(),
            is_local: local,
            spell1_id: self.spell1_id,
            spell2_id: self.spell2_id,
            position: self.selected_position.clone(),
            detected_position: self.detected_team_position.clone(),
            items: self
                .items
                .as_ref()
                .filter(|v| v.len() <= 8)
                .map(|v| v.iter().map(|v| v.filter(|v| *v <= MAX_COUNTER)).collect())
                .unwrap_or_default(),
            perks: (0..6)
                .map(|i| Perk {
                    id: s.perks[i],
                    counters: s.vars[i],
                })
                .collect(),
            primary_style: s.primary,
            secondary_style: s.secondary,
            augments: s.augments,
            subteam_id: s.subteam,
            subteam_placement: s.placement,
            stats: s.stats.clone(),
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RawTeam {
    #[serde(deserialize_with = "number")]
    team_id: Option<u64>,
    #[serde(deserialize_with = "lenient")]
    is_winning_team: Option<bool>,
    #[serde(deserialize_with = "players")]
    players: Option<Vec<RawPlayer>>,
    #[serde(deserialize_with = "lenient")]
    stats: Option<RawStats>,
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Eog {
    #[serde(deserialize_with = "game_id")]
    game_id: Option<u64>,
    #[serde(deserialize_with = "lenient")]
    local_player: Option<RawPlayer>,
    #[serde(deserialize_with = "lenient")]
    teams: Option<Vec<RawTeam>>,
    #[serde(deserialize_with = "number")]
    game_length: Option<u64>,
    #[serde(deserialize_with = "number")]
    end_of_game_timestamp: Option<u64>,
    #[serde(deserialize_with = "text")]
    game_mode: Option<String>,
    #[serde(deserialize_with = "text")]
    game_version: Option<String>,
    #[serde(deserialize_with = "text")]
    game_type: Option<String>,
    #[serde(deserialize_with = "text")]
    queue_type: Option<String>,
    #[serde(deserialize_with = "number")]
    queue_id: Option<u64>,
    #[serde(deserialize_with = "number")]
    map_id: Option<u64>,
    #[serde(deserialize_with = "lenient")]
    invalid: Option<bool>,
    #[serde(deserialize_with = "lenient")]
    game_ended_in_early_surrender: Option<bool>,
    #[serde(deserialize_with = "lenient")]
    team_early_surrendered: Option<bool>,
    #[serde(deserialize_with = "lenient")]
    caused_early_surrender: Option<bool>,
    #[serde(deserialize_with = "lenient")]
    early_surrender_accomplice: Option<bool>,
}
fn coverage(values: &[Option<u64>]) -> Coverage {
    if values.iter().all(Option::is_some) {
        Coverage::Present
    } else if values.iter().all(Option::is_none) {
        Coverage::Missing
    } else {
        Coverage::Partial
    }
}
impl Eog {
    pub(super) fn primary_win(&self) -> Option<u64> {
        self.local_player.as_ref().and_then(RawPlayer::win)
    }
    pub(super) fn confirm(&self, b: &Binding) -> Result<ResultFile, Rejection> {
        let id = self.game_id.ok_or(Rejection::NotReady)?;
        if id.to_string() != b.game_id {
            return Err(Rejection::NotReady);
        }
        let teams = self.teams.as_deref().unwrap_or_default();
        if teams.len() > MAX_TEAMS
            || teams
                .iter()
                .map(|t| t.players.as_ref().map_or(0, Vec::len))
                .sum::<usize>()
                > MAX_PLAYERS
        {
            return Err(Rejection::Association);
        }
        let local = self.local_player.as_ref();
        let identity_agrees = |p: &RawPlayer| {
            p.game_id.is_none_or(|v| v == id)
                && p.puuid.as_ref().is_none_or(|v| v == &b.puuid)
                && p.riot().is_none_or(|v| v == b.riot_id)
        };
        if local.is_some_and(|p| !identity_agrees(p)) {
            return Err(Rejection::Association);
        }
        let mut flagged = None;
        for (i, t) in teams.iter().enumerate() {
            for p in t.players.as_deref().unwrap_or_default() {
                if p.game_id.is_some_and(|v| v != id) {
                    return Err(Rejection::Association);
                }
                if p.is_local_player == Some(false)
                    && (p.puuid.as_ref().is_some_and(|v| v == &b.puuid)
                        || p.riot().is_some_and(|v| v == b.riot_id))
                {
                    return Err(Rejection::Association);
                }
                if p.is_local_player == Some(true) {
                    if flagged.is_some()
                        || !identity_agrees(p)
                        || local.is_some_and(|l| {
                            l.is_local_player == Some(false)
                                || l.team_id.zip(p.team_id).is_some_and(|(a, b)| a != b)
                                || l.champion_id
                                    .zip(p.champion_id)
                                    .is_some_and(|(a, b)| a != b)
                        })
                    {
                        return Err(Rejection::Association);
                    }
                    flagged = Some((i, p));
                }
            }
        }
        let primary = local.and_then(RawPlayer::win);
        let roster = flagged.and_then(|(_, p)| p.win());
        let local_team = local
            .and_then(|l| l.team_id)
            .or_else(|| flagged.and_then(|(i, _)| teams[i].team_id));
        let agrees = primary.is_some_and(|p| {
            roster.is_none_or(|v| v == p)
                && teams
                    .iter()
                    .enumerate()
                    .filter(|(i, t)| {
                        local_team.is_some_and(|id| t.team_id == Some(id))
                            || flagged.is_some_and(|(flag, _)| flag == *i)
                    })
                    .all(|(_, t)| t.is_winning_team.is_none_or(|win| u64::from(win) == p))
        });
        // Real primary-local LOSS encoding is not yet captured. WIN:0 stays unknown.
        let outcome = if agrees && primary == Some(1) {
            Outcome::Win
        } else {
            Outcome::Unknown
        };
        let facts = local.map(|p| p.facts(true));
        let empty = Player::default();
        let p = facts.as_ref().unwrap_or(&empty);
        let s = &p.stats;
        let loadout = std::iter::once(p.champion_id)
            .chain([
                p.spell1_id,
                p.spell2_id,
                p.position.as_ref().map(|_| 1),
                p.detected_position.as_ref().map(|_| 1),
            ])
            .chain(if p.items.is_empty() {
                vec![None]
            } else {
                p.items.clone()
            })
            .collect::<Vec<_>>();
        let runes = std::iter::once(p.primary_style)
            .chain([p.secondary_style])
            .chain(
                p.perks
                    .iter()
                    .flat_map(|p| std::iter::once(p.id).chain(p.counters)),
            )
            .collect::<Vec<_>>();
        let families = Families {
            combat: coverage(&[
                s.kills,
                s.deaths,
                s.assists,
                s.level,
                s.minions,
                s.jungle_minions,
                s.ally_jungle_minions,
                s.enemy_jungle_minions,
                s.time_dead,
            ]),
            economy: coverage(&[s.gold_earned, s.gold_spent]),
            damage: coverage(&[
                s.damage,
                s.champion_damage,
                s.damage_taken,
                s.objective_damage,
                s.turret_damage,
                s.physical_damage,
                s.magic_damage,
                s.true_damage,
                s.physical_champion_damage,
                s.magic_champion_damage,
                s.true_champion_damage,
                s.physical_damage_taken,
                s.magic_damage_taken,
                s.true_damage_taken,
                s.critical_strike,
            ]),
            support: coverage(&[
                s.healing,
                s.teammate_healing,
                s.shielding,
                s.mitigated,
                s.crowd_control,
                s.total_crowd_control,
                s.units_healed,
            ]),
            vision: coverage(&[
                s.vision,
                s.wards_placed,
                s.wards_killed,
                s.control_wards_bought,
                s.detector_wards_placed,
            ]),
            objectives: coverage(&[
                s.multi_kill,
                s.double_kills,
                s.triple_kills,
                s.quadra_kills,
                s.penta_kills,
                s.turrets,
                s.inhibitors,
                s.barons,
                s.dragons,
                s.killing_spree,
                s.objectives_stolen,
                s.objectives_stolen_assists,
                s.first_blood,
                s.first_blood_assist,
                s.first_tower,
                s.first_tower_assist,
                s.spell1_casts,
                s.spell2_casts,
                s.spell3_casts,
                s.spell4_casts,
                s.summoner1_casts,
                s.summoner2_casts,
            ]),
            loadout: coverage(&loadout),
            runes: coverage(&runes),
        };
        Ok(ResultFile {
            schema_version: 1,
            source: "lcu_eog".into(),
            confirmed: true,
            media_id: b.media_id.clone(),
            game_id: b.game_id.clone(),
            outcome,
            ended_early: [
                self.invalid,
                self.game_ended_in_early_surrender,
                self.team_early_surrendered,
                self.caused_early_surrender,
                self.early_surrender_accomplice,
            ]
            .contains(&Some(true)),
            coverage: families,
            duration_seconds: self.game_length,
            end_timestamp_ms: self.end_of_game_timestamp,
            queue_id: self.queue_id,
            map_id: self.map_id,
            game_version: self.game_version.clone(),
            game_mode: self.game_mode.clone(),
            game_type: self.game_type.clone(),
            queue_type: self.queue_type.clone(),
            label_mismatch: self.queue_id.is_some_and(|v| v != u64::from(b.queue_id))
                || self.map_id.is_some_and(|v| v != u64::from(b.map_id))
                || self.game_mode.as_ref().is_some_and(|v| v != &b.mode),
            local_player: facts,
            teams: teams
                .iter()
                .map(|t| Team {
                    team_id: t.team_id,
                    is_winning: t.is_winning_team,
                    stats: t
                        .stats
                        .as_ref()
                        .map(|v| v.stats.clone())
                        .unwrap_or_default(),
                    players: t
                        .players
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .map(|p| p.facts(p.is_local_player == Some(true)))
                        .collect(),
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        Binding {
            media_id: MediaId::new_v4(),
            game_id: "18446744073709551614".into(),
            puuid: "synthetic".into(),
            riot_id: "Synthetic#TEST".into(),
            map_id: 11,
            mode: "CLASSIC".into(),
            queue_id: 400,
        }
    }
    fn parse(v: serde_json::Value) -> Eog {
        serde_json::from_value(v).unwrap()
    }
    #[test]
    fn duplicate_stat_keys_conflict_and_bad_roster_rows_do_not_hide_good_contradictions() {
        let b = binding();
        let eog:Eog=serde_json::from_str(r#"{"gameId":18446744073709551614,"localPlayer":{"stats":{"WIN":1,"kills":3,"kills":4}},"teams":[{"players":[null,{"gameId":7}]}]}"#).unwrap();
        assert_eq!(
            eog.local_player
                .as_ref()
                .unwrap()
                .stats
                .as_ref()
                .unwrap()
                .stats
                .kills,
            None
        );
        assert_eq!(eog.confirm(&b).unwrap_err(), Rejection::Association);
    }
    #[test]
    fn false_local_flags_and_all_matching_team_outcomes_are_checked() {
        let b = binding();
        let game = b.game_id.parse::<u64>().unwrap();
        let eog = parse(
            serde_json::json!({"gameId":game,"localPlayer":{"stats":{"WIN":1}},"teams":[{"players":[{"puuid":"synthetic","isLocalPlayer":false}]}]}),
        );
        assert_eq!(eog.confirm(&b).unwrap_err(), Rejection::Association);
        let eog = parse(
            serde_json::json!({"gameId":game,"localPlayer":{"teamId":100,"stats":{"WIN":1}},"teams":[{"teamId":100,"isWinningTeam":true},{"teamId":100,"isWinningTeam":false}]}),
        );
        assert_eq!(eog.confirm(&b).unwrap().outcome, Outcome::Unknown);
    }
    #[test]
    fn array_caps_invalid_items_and_nonbinary_outcomes_remain_truthful() {
        let b = binding();
        let game = b.game_id.parse::<u64>().unwrap();
        for win in [
            serde_json::json!(2),
            serde_json::json!(-1),
            serde_json::json!(true),
            serde_json::json!("1"),
            serde_json::json!({}),
        ] {
            assert_eq!(parse(serde_json::json!({"gameId":game,"localPlayer":{"stats":{"WIN":win}},"gameEndedInEarlySurrender":true})).confirm(&b).unwrap().outcome,Outcome::Unknown);
        }
        let eog = parse(
            serde_json::json!({"gameId":game,"localPlayer":{"items":[1,"bad",3],"stats":{"WIN":1}},"gameEndedInEarlySurrender":true}),
        );
        let r = eog.confirm(&b).unwrap();
        assert!(r.ended_early);
        assert_eq!(r.local_player.unwrap().items, vec![Some(1), None, Some(3)]);
        for (teams, players) in [(17, 0), (1, 65)] {
            let v = serde_json::json!({"gameId":game,"teams":vec![serde_json::json!({"players":vec![serde_json::json!({});players]});teams]});
            assert_eq!(parse(v).confirm(&b).unwrap_err(), Rejection::Association);
        }
    }
    #[test]
    fn exact_id_absent_corroborators_and_unknown_are_valid() {
        let b = binding();
        let r = parse(serde_json::json!({"gameId":18446744073709551614_u64}))
            .confirm(&b)
            .unwrap();
        assert!(r.valid_for(&b.media_id, &b.game_id));
        assert_eq!(r.outcome, Outcome::Unknown);
        for id in [
            serde_json::json!(0),
            serde_json::json!("18446744073709551614"),
            serde_json::json!(14),
        ] {
            assert_eq!(
                parse(serde_json::json!({"gameId":id}))
                    .confirm(&b)
                    .unwrap_err(),
                Rejection::NotReady
            );
        }
    }
    #[test]
    fn association_contradiction_rejects_but_outcome_disagreement_keeps_stats() {
        let b = binding();
        assert_eq!(parse(serde_json::json!({"gameId":18446744073709551614_u64,"localPlayer":{"puuid":"other"}})).confirm(&b).unwrap_err(),Rejection::Association);
        let mut v = serde_json::json!({"gameId":18446744073709551614_u64,"localPlayer":{"teamId":100,"stats":{"WIN":1,"kills":4}},"teams":[{"teamId":100,"isWinningTeam":false}]});
        let r = parse(v.clone()).confirm(&b).unwrap();
        assert_eq!(r.outcome, Outcome::Unknown);
        assert_eq!(r.local_player.unwrap().stats.kills, Some(4));
        v["teams"][0]["isWinningTeam"] = true.into();
        assert_eq!(parse(v).confirm(&b).unwrap().outcome, Outcome::Win);
    }
    #[test]
    fn aliases_invalid_fields_loadout_and_mode_are_bounded() {
        let b = binding();
        let r=parse(serde_json::json!({"gameId":18446744073709551614_u64,"mapId":99,"localPlayer":{"items":[1,2,3],"stats":{"WIN":0,"kills":3,"CHAMPIONS_KILLED":4,"deaths":-1,"assists":{"error":4},"goldEarned":3,"GOLD_EARNED":3,"PERK0":8005,"PERK0_VAR1":3,"PLAYER_AUGMENT1":9,"unlistedSecret":"discard"}}})).confirm(&b).unwrap();
        assert!(r.label_mismatch);
        assert_eq!(r.outcome, Outcome::Unknown);
        let p = r.local_player.unwrap();
        assert_eq!(p.stats.kills, None);
        assert_eq!(p.stats.deaths, None);
        assert_eq!(p.stats.gold_earned, Some(3));
        assert_eq!(p.perks[0].id, Some(8005));
        assert_eq!(p.augments[0], Some(9));
    }
    #[test]
    fn duplicate_local_and_participant_id_mismatch_reject() {
        let b = binding();
        for players in [
            serde_json::json!([{"isLocalPlayer":true},{"isLocalPlayer":true}]),
            serde_json::json!([{"gameId":7}]),
        ] {
            assert_eq!(parse(serde_json::json!({"gameId":18446744073709551614_u64,"teams":[{"players":players}]})).confirm(&b).unwrap_err(),Rejection::Association);
        }
    }
}
