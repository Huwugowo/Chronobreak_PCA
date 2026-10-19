//! Purpose-built EOG ingestion. Unknown keys are skipped, never archived.
use chronobreak_league_data::observation::{FieldIssue, FinalFacts, FinalPlayer, decode};
use chronobreak_league_data::{
    Coverage, Families, MAX_PLAYERS, MAX_TEAMS, Outcome, Perk, Player, ResultFile, Team, bounded,
};
use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Deserializer};
use std::ops::Deref;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Production and fixtures use the same allowlisted field/status decoder.
#[derive(Default)]
pub(super) struct Eog {
    facts: FinalFacts,
    pub evidence: Option<chronobreak_league_data::ResultEvidence>,
}
impl<'de> Deserialize<'de> for Eog {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self {
            facts: decode::end_of_game(d)?,
            evidence: None,
        })
    }
}

fn riot(player: &FinalPlayer) -> Option<String> {
    Some(format!(
        "{}#{}",
        player.riot_id_game_name.as_ref()?,
        player.riot_id_tag_line.as_ref()?
    ))
    .filter(|v| super::full_identity(v))
}
fn win(player: &FinalPlayer) -> Option<u64> {
    player.stats.as_ref()?.extras.get("WIN").copied()
}
fn player_facts(player: &FinalPlayer, local: bool) -> Player {
    let empty = chronobreak_league_data::observation::FinalStats::default();
    let s = player.stats.as_ref().unwrap_or(&empty);
    let extra = |key: &str| s.extras.get(key).copied();
    let label = |v: &Option<String>| v.as_ref().filter(|s| bounded(s)).cloned();
    Player {
        team_id: player.team_id,
        champion_id: player.champion_id,
        champion_name: label(&player.champion_name),
        riot_id: riot(player),
        is_local: local,
        spell1_id: player.spell1_id,
        spell2_id: player.spell2_id,
        position: label(&player.selected_position),
        detected_position: label(&player.detected_team_position),
        items: player.items.clone().unwrap_or_default(),
        perks: (0..6)
            .map(|i| Perk {
                id: extra(&format!("PERK{i}")),
                counters: std::array::from_fn(|j| extra(&format!("PERK{i}_VAR{}", j + 1))),
            })
            .collect(),
        primary_style: extra("PERK_PRIMARY_STYLE"),
        secondary_style: extra("PERK_SUB_STYLE"),
        augments: std::array::from_fn(|i| extra(&format!("PLAYER_AUGMENT{}", i + 1))),
        subteam_id: extra("PLAYER_SUBTEAM"),
        subteam_placement: extra("SUBTEAM_PLACEMENT"),
        stats: s.stats.clone(),
    }
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
impl Deref for Eog {
    type Target = FinalFacts;
    fn deref(&self) -> &Self::Target {
        &self.facts
    }
}
impl Eog {
    pub(super) fn primary_win(&self) -> Option<u64> {
        self.local_player.as_ref().and_then(win)
    }
    pub(super) fn confirm(&self, b: &Binding) -> Result<ResultFile, Rejection> {
        let id = self.game_id.as_deref().ok_or(Rejection::NotReady)?;
        if id != b.game_id {
            return Err(Rejection::NotReady);
        }
        let teams = self.teams.as_deref().unwrap_or_default();
        if self.issues.get("teams") == Some(&FieldIssue::Invalid)
            || teams
                .iter()
                .any(|t| t.issues.get("players") == Some(&FieldIssue::Invalid))
            || teams.len() > MAX_TEAMS
            || teams
                .iter()
                .map(|t| t.players.as_ref().map_or(0, Vec::len))
                .sum::<usize>()
                > MAX_PLAYERS
        {
            return Err(Rejection::Association);
        }
        let local = self.local_player.as_ref();
        let identity_agrees = |p: &FinalPlayer| {
            p.game_id.as_deref().is_none_or(|v| v == id)
                && p.puuid.as_ref().is_none_or(|v| v == &b.puuid)
                && riot(p).is_none_or(|v| v == b.riot_id)
        };
        if local.is_some_and(|p| !identity_agrees(p)) {
            return Err(Rejection::Association);
        }
        let mut flagged = None;
        for (i, t) in teams.iter().enumerate() {
            for p in t.players.as_deref().unwrap_or_default() {
                if p.game_id.as_deref().is_some_and(|v| v != id) {
                    return Err(Rejection::Association);
                }
                if p.is_local_player == Some(false)
                    && (p.puuid.as_ref().is_some_and(|v| v == &b.puuid)
                        || riot(p).is_some_and(|v| v == b.riot_id))
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
        let primary = local.and_then(win);
        let roster = flagged.and_then(|(_, p)| win(p));
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
        let facts = local.map(|p| player_facts(p, true));
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
            schema_version: chronobreak_league_data::RESULT_SCHEMA_VERSION,
            source_facts: self.facts.clone(),
            evidence: self.evidence.clone(),
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
            game_version: self.game_version.as_ref().filter(|v| bounded(v)).cloned(),
            game_mode: self.game_mode.as_ref().filter(|v| bounded(v)).cloned(),
            game_type: self.game_type.as_ref().filter(|v| bounded(v)).cloned(),
            queue_type: self.queue_type.as_ref().filter(|v| bounded(v)).cloned(),
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
                        .map(|p| player_facts(p, p.is_local_player == Some(true)))
                        .collect(),
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chronobreak_league_data::observation::{Endpoint, Payload};

    #[test]
    fn shared_facts_and_receipt_survive_production_confirmation_and_disk_round_trip() {
        let b = binding();
        let body = serde_json::to_vec(&serde_json::json!({
            "gameId": b.game_id.parse::<u64>().unwrap(),
            "localPlayer": {
                "gameId": b.game_id.parse::<u64>().unwrap(), "puuid": b.puuid,
                "riotIdGameName": "Synthetic", "riotIdTagLine": "TEST", "isLocalPlayer": true,
                "items": [0, null, {"error": "unavailable"}, "invalid"],
                "stats": {"WIN": 0, "CHAMPIONS_KILLED": 3, "kills": 4,
                    "GOLD_EARNED": 0, "goldEarned": 0, "deaths": null,
                    "assists": {"error": "unavailable"}, "PERK0": 8005, "PLAYER_SUBTEAM": 7,
                    "unlistedSecret": "discard"}
            },
            "teams": [{"teamId": 300, "players": [null, {"championId": 103}]}],
            "invalid": false, "teamEarlySurrendered": true, "causedEarlySurrender": null
        }))
        .unwrap();
        let Payload::EndOfGame(expected) = decode::live(Endpoint::EndOfGame, &body).unwrap() else {
            panic!("EOG");
        };
        let mut response: Eog = serde_json::from_slice(&body).unwrap();
        response.evidence = Some(chronobreak_league_data::ResultEvidence {
            run_id: MediaId::new_v4(),
            timing: chronobreak_league_data::observation::RequestTiming {
                start_us: 10,
                end_us: 20,
                received_at: "2026-10-16T00:00:00Z".into(),
            },
        });
        let result = response.confirm(&b).unwrap();
        assert_eq!(&result.source_facts, expected.as_ref());
        assert!(result.valid_for(&b.media_id, &b.game_id));
        assert_eq!(result.outcome, Outcome::Unknown);
        let p = result.source_facts.local_player.as_ref().unwrap();
        let s = p.stats.as_ref().unwrap();
        assert_eq!(s.issues["kills"], FieldIssue::Conflict);
        assert_eq!(s.issues["deaths"], FieldIssue::Unavailable);
        assert_eq!(s.issues["assists"], FieldIssue::SourceError);
        assert_eq!(s.extras["WIN"], 0);
        assert_eq!(s.stats.gold_earned, Some(0));
        assert_eq!(s.aliases["gold_earned"], ["GOLD_EARNED", "goldEarned"]);
        assert_eq!(p.issues["items/1"], FieldIssue::Unavailable);
        assert_eq!(p.issues["items/2"], FieldIssue::SourceError);
        assert_eq!(p.issues["items/3"], FieldIssue::Invalid);
        assert_eq!(
            result.source_facts.teams.as_ref().unwrap()[0]
                .players
                .as_ref()
                .unwrap()[0]
                .issues["$"],
            FieldIssue::Unavailable
        );
        let encoded = serde_json::to_vec(&result).unwrap();
        assert!(!String::from_utf8_lossy(&encoded).contains("unlistedSecret"));
        let decoded: ResultFile = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, result);
        assert!(decoded.valid_for(&b.media_id, &b.game_id));
        let mut serialized = serde_json::to_value(&decoded).unwrap();
        for (pointer, value) in [
            ("/source_facts/local_player/game_id", serde_json::json!("7")),
            (
                "/source_facts/teams/0/players/1/game_id",
                serde_json::json!("7"),
            ),
            (
                "/source_facts/local_player/issues/puuid",
                serde_json::json!({"state":"invalid"}),
            ),
            (
                "/source_facts/issues/fabricated",
                serde_json::json!({"state":"invalid"}),
            ),
            (
                "/source_facts/local_player/issues/items~10",
                serde_json::json!({"state":"invalid"}),
            ),
            (
                "/source_facts/local_player/issues/items~18",
                serde_json::json!({"state":"invalid"}),
            ),
            (
                "/source_facts/teams/0/issues",
                serde_json::json!({"team_id":{"state":"invalid"}}),
            ),
            (
                "/source_facts/teams/0/players/0/champion_id",
                serde_json::json!(103),
            ),
            (
                "/evidence/timing/received_at",
                serde_json::json!("2026-02-30T12:00:00Z"),
            ),
            ("/evidence/timing/received_at", serde_json::json!("fixture")),
        ] {
            // Insertions and replacements both cross the persisted LR boundary.
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            let key = key.replace("~1", "/");
            let prior = serialized
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(key.clone(), value);
            let corrupted: ResultFile = serde_json::from_value(serialized.clone()).unwrap();
            assert!(!corrupted.valid_for(&b.media_id, &b.game_id), "{pointer}");
            let fields = serialized
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap();
            if let Some(prior) = prior {
                fields.insert(key, prior);
            } else {
                fields.remove(&key);
            }
        }
        let mut corrupted = decoded;
        corrupted
            .source_facts
            .local_player
            .as_mut()
            .unwrap()
            .stats
            .as_mut()
            .unwrap()
            .extras
            .insert("unlistedSecret".into(), 0);
        assert!(!corrupted.valid_for(&b.media_id, &b.game_id));
    }

    #[test]
    fn every_early_flag_keeps_its_own_presence_and_value() {
        let b = binding();
        for (source, target) in [
            ("invalid", "invalid"),
            ("gameEndedInEarlySurrender", "game_ended_in_early_surrender"),
            ("teamEarlySurrendered", "team_early_surrendered"),
            ("causedEarlySurrender", "caused_early_surrender"),
            ("earlySurrenderAccomplice", "early_surrender_accomplice"),
        ] {
            for value in [
                serde_json::json!(false),
                serde_json::json!(true),
                serde_json::Value::Null,
                serde_json::json!("invalid"),
            ] {
                let mut body = serde_json::json!({"gameId": b.game_id.parse::<u64>().unwrap()});
                body[source] = value.clone();
                let r = parse(body).confirm(&b).unwrap();
                assert_eq!(r.ended_early, value == true);
                let fields = serde_json::to_value(&r.source_facts).unwrap();
                if value.is_boolean() {
                    assert_eq!(fields[target], value);
                } else {
                    assert!(fields.get(target).is_none());
                    assert_eq!(
                        r.source_facts.issues[target],
                        if value.is_null() {
                            FieldIssue::Unavailable
                        } else {
                            FieldIssue::Invalid
                        }
                    );
                }
            }
        }
    }
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
        assert!(serde_json::from_str::<Eog>(r#"{"gameId":18446744073709551614,"localPlayer":{"stats":{"WIN":1,"kills":3,"kills":4}}}"#).is_err());
        let eog = parse(
            serde_json::json!({"gameId":18446744073709551614_u64,"teams":[{"players":[null,{"gameId":7}]}]}),
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
