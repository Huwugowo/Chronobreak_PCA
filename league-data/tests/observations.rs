use chronobreak_league_data::observation::{decode, *};
use serde_json::{Value, json};

fn fixture(endpoint: Endpoint, key: &str) -> Payload {
    let source: Value = serde_json::from_str(include_str!("fixtures/live-endpoints.json")).unwrap();
    decode::live(endpoint, &serde_json::to_vec(&source[key]).unwrap()).unwrap()
}

#[test]
fn retained_roster_fields_and_nested_errors_are_distinct_from_empty_and_zero() {
    let Payload::PlayerList(players) = fixture(Endpoint::PlayerList, "pl") else {
        panic!()
    };
    let local = &players[0];
    assert_eq!(local.scores.as_ref().unwrap().kills, Some(0));
    assert_eq!(local.scores.as_ref().unwrap().ward_score, Some(0.5));
    assert_eq!(local.skin_name.as_deref(), Some(""));
    assert_eq!(local.items.as_ref().unwrap()[0].count, Some(2));
    assert_eq!(local.items.as_ref().unwrap()[0].can_use, Some(true));
    assert_eq!(
        local.runes.as_ref().unwrap().secondary.as_ref().unwrap().id,
        Some(8200)
    );
    assert_eq!(
        local
            .summoner_spells
            .as_ref()
            .unwrap()
            .one
            .as_ref()
            .unwrap()
            .raw_display_name
            .as_deref(),
        Some("SummonerFlash")
    );
    let dummy = &players[1];
    assert_eq!(dummy.items, None);
    assert_eq!(dummy.issues["items"], FieldIssue::SourceError);
    assert_eq!(dummy.issues["level"], FieldIssue::Unavailable);
    let scores = dummy.scores.as_ref().unwrap();
    assert_eq!(scores.issues["kills"], FieldIssue::Unavailable);
    assert_eq!(scores.issues["deaths"], FieldIssue::Invalid);
    assert!(!scores.issues.contains_key("assists")); // absent, not invalid or zero
    assert_eq!(dummy.respawn_timer, Some(12.5));
    assert_eq!(players[2].items, Some(vec![]));
    assert_eq!(players[2].team.as_deref(), Some("TEAM_THREE"));
    assert!(players.iter().all(|p| p.participant.is_none()));
}

#[test]
fn source_envelopes_unknown_kinds_and_allowlist_round_trip() {
    for (endpoint, key) in [
        (Endpoint::GameStats, "gs"),
        (Endpoint::ActivePlayer, "ap"),
        (Endpoint::PlayerList, "pl"),
        (Endpoint::Events, "ev"),
    ] {
        let payload = fixture(endpoint, key);
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert_eq!(serde_json::from_slice::<Payload>(&bytes).unwrap(), payload);
        let encoded = String::from_utf8(bytes).unwrap();
        assert!(!encoded.contains("must not persist"));
        assert!(!encoded.contains("currentGold"));
        assert!(!encoded.contains("currentHealth"));
    }
    let Payload::Events(events) = fixture(Endpoint::Events, "ev") else {
        panic!()
    };
    assert_eq!(events[0].source_id, Some(0));
    assert_eq!(events[2].recipient.as_deref(), Some("Fixture#TEST"));
    assert_eq!(events[2].game_tick.unwrap().get(), 120_250_000);
    assert_eq!(events[3].stolen, Some(false));
    assert!(events[4].valid_envelope());
    assert!(!events[4].known_kind());
}

#[test]
fn malformed_values_are_retained_as_issues_not_defaults() {
    let rows = json!([null, {"items":[null, {"itemID":-1}, {"itemID":2003}],"level":-1,"isDead":0,"respawnTimer":-1,"riotId":"Tagless"}]);
    let Payload::PlayerList(players) =
        decode::live(Endpoint::PlayerList, &serde_json::to_vec(&rows).unwrap()).unwrap()
    else {
        panic!()
    };
    assert_eq!(players[0].issues["$"], FieldIssue::Unavailable);
    for field in ["level", "is_dead", "respawn_timer", "riot_id"] {
        assert_eq!(players[1].issues[field], FieldIssue::Invalid);
    }
    let items = players[1].items.as_ref().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].issues["$"], FieldIssue::Unavailable);
    assert_eq!(items[1].issues["item_id"], FieldIssue::Invalid);
    assert_eq!(items[2].item_id, Some(2003));
}

#[test]
fn invalid_event_time_retains_source_value_without_inventing_a_game_tick() {
    let bytes = br#"{"Events":[{"EventID":7,"EventTime":-2,"EventName":"FirstBlood"},{"EventID":7,"EventTime":2,"EventName":"FirstBlood"}]}"#;
    let Payload::Events(events) = decode::live(Endpoint::Events, bytes).unwrap() else {
        panic!()
    };
    assert!(!events[0].valid_envelope());
    assert_eq!(events[0].source_time.as_ref().unwrap().as_i64(), Some(-2));
    assert_eq!(events[0].issues["game_tick"], FieldIssue::Invalid);
    assert!(events[1].valid_envelope());
}

#[test]
fn source_caps_and_shapes_fail_explicitly() {
    assert_eq!(
        decode::live(Endpoint::Events, b"{}"),
        Err(GapReason::InvalidShape)
    );
    assert_eq!(
        decode::live(Endpoint::PlayerList, b"["),
        Err(GapReason::InvalidJson)
    );
    assert_eq!(
        decode::live(Endpoint::GameStats, &vec![b' '; MAX_BODY_BYTES + 1]),
        Err(GapReason::BodyLimit)
    );
    assert_eq!(
        decode::live(
            Endpoint::PlayerList,
            &serde_json::to_vec(&vec![json!({}); MAX_ROSTER + 1]).unwrap()
        ),
        Err(GapReason::Capacity)
    );
}

#[test]
fn full_identity_is_not_inferred_or_tag_stripped() {
    for identity in [
        "",
        "Tagless",
        "#TAG",
        "Name#",
        "Name#TAG#x",
        " Name#TAG",
        "Name#TAG\n",
    ] {
        let payload = decode::live(
            Endpoint::ActivePlayer,
            &serde_json::to_vec(&json!({"riotId":identity})).unwrap(),
        )
        .unwrap();
        let Payload::ActivePlayer(p) = payload else {
            panic!()
        };
        assert_eq!(p.riot_id, None);
        assert_eq!(p.issues["riot_id"], FieldIssue::Invalid);
    }
    assert!(decode::full_riot_id("Fixture#TEST"));
    assert!(decode::full_riot_id("Fixture#OTHER"));
}

fn scope() -> SourceEpoch {
    SourceEpoch {
        media_id: chronobreak_replay_time::MediaId::new_v4(),
        run_id: chronobreak_replay_time::MediaId::new_v4(),
        epoch: 0,
    }
}
fn source_event(time: f64) -> Event {
    Event {
        source_id: Some(7),
        source_time: Some(serde_json::Number::from_f64(time).unwrap()),
        game_tick: chronobreak_replay_time::GameTick::from_seconds(time)
            .ok()
            .filter(|_| time >= 0.0),
        name: Some("FirstBlood".into()),
        ..Default::default()
    }
}

#[test]
fn event_progress_waits_for_commit_and_corrections_and_reversions_survive() {
    use events::{EventTracker, Ingestion};
    let scope = scope();
    let mut tracker = EventTracker::new(scope.clone());
    let first = tracker.prepare(&scope, vec![source_event(-2.0)]).unwrap();
    assert_eq!(first.revisions()[0].ingestion, Ingestion::Invalid);
    assert_eq!(tracker.retained_ids(), 0);
    drop(first); // failed/unflushed write: a retry must still contain the event
    let retry = tracker.prepare(&scope, vec![source_event(-2.0)]).unwrap();
    assert_eq!(retry.revisions().len(), 1);
    tracker.acknowledge(retry).unwrap();
    assert_eq!(tracker.retained_ids(), 1);
    for time in [2.0, -2.0, 2.0] {
        let change = tracker.prepare(&scope, vec![source_event(time)]).unwrap();
        assert_eq!(change.revisions().len(), 1);
        assert_eq!(change.revisions()[0].ingestion, Ingestion::Correction);
        tracker.acknowledge(change).unwrap();
    }
    assert!(
        tracker
            .prepare(&scope, vec![source_event(2.0)])
            .unwrap()
            .revisions()
            .is_empty()
    );
    assert_eq!(tracker.retained_ids(), 1);
}

#[test]
fn fractional_source_times_keep_committed_revision_fingerprints_after_json_roundtrip() {
    let scope = scope();
    let mut writer = events::EventTracker::new(scope.clone());
    let mut recovered = events::EventTracker::new(scope.clone());
    for step in 0..4_000 {
        let time = 1.0 + f64::from(step) * 0.05;
        let body = serde_json::to_vec(&json!({"Events": [{
            "EventID": 7, "EventTime": time, "EventName": "FirstBlood"
        }]}))
        .unwrap();
        let Payload::Events(events) = decode::live(Endpoint::Events, &body).unwrap() else {
            panic!("event response expected");
        };
        let pending = writer.prepare(&scope, events).unwrap();
        let bytes = serde_json::to_vec(pending.revisions()).unwrap();
        let committed: Vec<events::EventRevision> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(pending.revisions(), committed, "source time {time}");
        recovered.restore_committed(&scope, &committed).unwrap();
        writer.acknowledge(pending).unwrap();
    }
}

#[test]
fn duplicate_id_conflicts_are_order_independent_bounded_and_epoch_scoped() {
    use events::{EventTracker, Ingestion};
    let scope = scope();
    let mut tracker = EventTracker::with_capacity(scope.clone(), 1);
    let pending = tracker
        .prepare(
            &scope,
            vec![source_event(1.0), source_event(2.0), source_event(1.0)],
        )
        .unwrap();
    assert_eq!(pending.revisions().len(), 2);
    assert!(
        pending
            .revisions()
            .iter()
            .all(|r| r.ingestion == Ingestion::Conflict)
    );
    tracker.acknowledge(pending).unwrap();
    assert!(
        tracker
            .prepare(&scope, vec![source_event(2.0), source_event(1.0)])
            .unwrap()
            .revisions()
            .is_empty()
    );
    let other = Event {
        source_id: Some(9),
        ..source_event(2.0)
    };
    assert!(matches!(
        tracker.prepare(&scope, vec![other]),
        Err(GapReason::Capacity)
    ));
    let other_epoch = SourceEpoch {
        epoch: 1,
        ..scope.clone()
    };
    assert!(matches!(
        tracker.prepare(&other_epoch, vec![source_event(2.0)]),
        Err(GapReason::SourceReset)
    ));
    assert_eq!(tracker.retained_ids(), 1);
}

#[test]
fn stale_event_acknowledgements_do_not_change_progress() {
    let scope = scope();
    let mut tracker = events::EventTracker::new(scope.clone());
    let a = tracker.prepare(&scope, vec![source_event(1.0)]).unwrap();
    let b = tracker.prepare(&scope, vec![source_event(2.0)]).unwrap();
    tracker.acknowledge(a).unwrap();
    assert_eq!(tracker.acknowledge(b), Err(GapReason::SourceReset));
    assert!(
        tracker
            .prepare(&scope, vec![source_event(1.0)])
            .unwrap()
            .revisions()
            .is_empty()
    );
}

#[test]
fn participant_references_require_exact_unique_identity_and_keep_first_local_match() {
    let scope = scope();
    let mut participants = identity::Participants::new(scope.clone());
    let Payload::PlayerList(mut roster) = fixture(Endpoint::PlayerList, "pl") else {
        panic!()
    };
    participants
        .observe(&scope, &mut roster, Some("Fixture#TEST"))
        .unwrap();
    let local = roster[0].participant.unwrap();
    assert_eq!(participants.local(), Some(local));
    assert_ne!(roster[2].participant, Some(local));
    assert_eq!(roster[1].participant, None);
    assert_eq!(participants.resolve_actor("Fixture"), None);
    roster.swap(0, 2);
    participants
        .observe(&scope, &mut roster, Some("Fixture#OTHER"))
        .unwrap();
    assert_eq!(roster[2].participant, Some(local));
    assert_eq!(participants.local(), Some(local));
    let mut duplicate = vec![roster[2].clone(), roster[2].clone()];
    participants.observe(&scope, &mut duplicate, None).unwrap();
    assert!(
        duplicate
            .iter()
            .all(|row| row.participant.is_none()
                && row.issues["participant"] == FieldIssue::Conflict)
    );
    let mut conflict = vec![roster[2].clone()];
    conflict[0].riot_id_tag_line = Some("CONTRADICTION".into());
    participants.observe(&scope, &mut conflict, None).unwrap();
    assert_eq!(conflict[0].participant, None);
}

#[test]
fn final_facts_keep_source_identifiers_flags_slots_and_stat_alias_reasons() {
    let source = json!({"gameId":12345678901_u64,"invalid":false,"gameEndedInEarlySurrender":true,
        "teamEarlySurrendered":false,"causedEarlySurrender":null,"earlySurrenderAccomplice":"bad",
        "localPlayer":{"puuid":"synthetic-puuid","riotIdGameName":"Fixture","riotIdTagLine":"TEST","items":[0,null,"bad",2003],
        "stats":{"CHAMPIONS_KILLED":5,"kills":5,"NUM_DEATHS":1,"deaths":2,"ASSISTS":null,"goldSpent":{"error":"synthetic"},
            "WIN":0,"PERK0":8112,"PERK0_VAR1":0,"PLAYER_AUGMENT1":123,"secret":789}},
        "teams":[{"teamId":300,"isWinningTeam":false,"players":[],"stats":{"GOLD_EARNED":100}}]});
    let Payload::EndOfGame(facts) =
        decode::live(Endpoint::EndOfGame, &serde_json::to_vec(&source).unwrap()).unwrap()
    else {
        panic!()
    };
    assert_eq!(facts.game_id.as_deref(), Some("12345678901"));
    assert_eq!(facts.invalid, Some(false));
    assert_eq!(facts.game_ended_in_early_surrender, Some(true));
    assert_eq!(facts.team_early_surrendered, Some(false));
    assert_eq!(
        facts.issues["caused_early_surrender"],
        FieldIssue::Unavailable
    );
    assert_eq!(
        facts.issues["early_surrender_accomplice"],
        FieldIssue::Invalid
    );
    let player = facts.local_player.as_ref().unwrap();
    assert_eq!(player.puuid.as_deref(), Some("synthetic-puuid"));
    assert_eq!(
        player.items.as_ref().unwrap(),
        &[Some(0), None, None, Some(2003)]
    );
    assert_eq!(player.issues["items/1"], FieldIssue::Unavailable);
    assert_eq!(player.issues["items/2"], FieldIssue::Invalid);
    let stats = player.stats.as_ref().unwrap();
    assert_eq!(stats.stats.kills, Some(5));
    assert_eq!(stats.aliases["kills"], ["CHAMPIONS_KILLED", "kills"]);
    assert_eq!(stats.stats.deaths, None);
    assert_eq!(stats.issues["deaths"], FieldIssue::Conflict);
    assert_eq!(stats.issues["assists"], FieldIssue::Unavailable);
    assert_eq!(stats.issues["gold_spent"], FieldIssue::SourceError);
    assert_eq!(stats.extras["WIN"], 0); // retained source value, no normalized LOSS claim
    assert_eq!(stats.extras["PERK0_VAR1"], 0);
    assert!(!stats.extras.contains_key("secret"));
    assert_eq!(facts.teams.as_ref().unwrap()[0].team_id, Some(300));
    let bytes = serde_json::to_vec(&facts).unwrap();
    assert_eq!(
        serde_json::from_slice::<FinalFacts>(&bytes).unwrap(),
        *facts
    );
}

fn evidence_at(scope: &SourceEpoch, endpoint: Endpoint, id: u64, us: u64) -> Evidence {
    Evidence {
        id: ObservationId(id),
        source: scope.clone(),
        endpoint,
        timing: RequestTiming {
            start_us: us,
            end_us: us + 10_000,
            received_at: "2026-10-15T00:00:00Z".into(),
        },
        game_stats: None,
        calibration: None,
    }
}

#[test]
fn identity_rounds_wait_for_independent_gs_and_do_not_reuse_ap_as_fresh_evidence() {
    let scope = scope();
    let mut rounds = identity::IdentityRounds::default();
    rounds.observe_ap(
        evidence_at(&scope, Endpoint::ActivePlayer, 1, 0),
        Some("Fixture#TEST".into()),
    );
    assert!(rounds.take_ready(10_000).is_none());
    rounds.observe_gs(
        evidence_at(&scope, Endpoint::GameStats, 2, 50_000),
        GameStats {
            map_number: Some(11),
            game_mode: Some("PRACTICETOOL".into()),
            ..Default::default()
        },
    );
    let first = rounds.take_ready(60_000).unwrap();
    assert_eq!(first.sequence, 1);
    assert_eq!(first.riot_id.as_deref(), Some("Fixture#TEST"));
    assert_eq!(first.active_player.id, ObservationId(1));
    assert_eq!(first.game_stats.unwrap().id, ObservationId(2));
    assert!(rounds.take_ready(70_000).is_none());
    rounds.observe_ap(
        evidence_at(&scope, Endpoint::ActivePlayer, 3, 10_000_000),
        Some("Fixture#TEST".into()),
    );
    assert!(rounds.take_ready(10_010_000).is_none());
    let expired = rounds.take_ready(12_010_000).unwrap();
    assert_eq!(expired.sequence, 2);
    assert_eq!(expired.riot_id, None);
    rounds.missed_attempt();
    rounds.observe_ap(
        evidence_at(&scope, Endpoint::ActivePlayer, 4, 30_000_000),
        None,
    );
    assert_eq!(rounds.take_ready(30_010_000).unwrap().sequence, 4);
    rounds.clear_context();
    rounds.observe_ap(
        evidence_at(&scope, Endpoint::ActivePlayer, 5, 40_000_000),
        None,
    );
    assert_eq!(rounds.take_ready(40_010_000).unwrap().sequence, 6);
}

#[test]
fn actor_references_preserve_exact_unresolved_and_conflicting_roles() {
    use events::{ActorRole, ActorStatus};
    let scope = scope();
    let mut participants = identity::Participants::new(scope.clone());
    let mut roster = vec![RosterPlayer {
        riot_id: Some("Fixture#TEST".into()),
        ..Default::default()
    }];
    participants.observe(&scope, &mut roster, None).unwrap();
    let mut event = source_event(1.0);
    event.killer = Some("Fixture#TEST".into());
    event.victim = Some("Fixture".into());
    event.assisters = Some(vec!["Fixture#OTHER".into(), "Fixture#TEST".into()]);
    event.actors = Some(events::resolve_actors(&event, &participants));
    let actors = event.actors.as_ref().unwrap();
    assert_eq!(
        actors[&ActorRole::Killer][0].participant,
        roster[0].participant
    );
    assert_eq!(
        actors[&ActorRole::Victim][0].status,
        ActorStatus::Unresolved
    );
    assert_eq!(
        actors[&ActorRole::Assister][0].status,
        ActorStatus::Unresolved
    );
    assert_eq!(
        actors[&ActorRole::Assister][1].status,
        ActorStatus::Resolved
    );
    let mut tracker = events::EventTracker::new(scope.clone());
    let pending = tracker.prepare(&scope, vec![event.clone()]).unwrap();
    // The allocation-free hash must keep the prior JSON fingerprint contract,
    // while the retained event still includes the exact ingestion-time actors.
    let mut source = event.clone();
    source.actors = None;
    use sha2::{Digest, Sha256};
    let fingerprint: [u8; 32] = Sha256::digest(serde_json::to_vec(&source).unwrap()).into();
    assert_eq!(pending.revisions()[0].fingerprint, fingerprint);
    assert_eq!(pending.revisions()[0].event, event);
    tracker.acknowledge(pending).unwrap();
    let mut duplicate = vec![roster[0].clone(), roster[0].clone()];
    participants.observe(&scope, &mut duplicate, None).unwrap();
    event.actors = Some(events::resolve_actors(&event, &participants));
    assert_eq!(
        event.actors.as_ref().unwrap()[&ActorRole::Killer][0].status,
        ActorStatus::Conflict
    );
    assert_eq!(
        event.actors.as_ref().unwrap()[&ActorRole::Killer][0].participant,
        None
    );
    // A changed derived join is not a source correction.
    assert!(
        tracker
            .prepare(&scope, vec![event])
            .unwrap()
            .revisions()
            .is_empty()
    );
}

#[test]
fn participant_capacity_rejection_is_transactional() {
    let scope = scope();
    let mut participants = identity::Participants::new(scope.clone());
    for batch in 0..4 {
        let mut rows: Vec<_> = (0..MAX_ROSTER)
            .map(|i| RosterPlayer {
                riot_id: Some(format!("P{}#TEST", batch * MAX_ROSTER + i)),
                ..Default::default()
            })
            .collect();
        participants.observe(&scope, &mut rows, None).unwrap();
    }
    let prior = participants.resolve_actor("P0#TEST").unwrap();
    let mut rejected = vec![
        RosterPlayer {
            riot_id: Some("P0#TEST".into()),
            riot_id_game_name: Some("Mismatch".into()),
            riot_id_tag_line: Some("TEST".into()),
            ..Default::default()
        },
        RosterPlayer {
            riot_id: Some("Overflow#TEST".into()),
            ..Default::default()
        },
    ];
    let original = rejected.clone();
    assert_eq!(
        participants.observe(&scope, &mut rejected, None),
        Err(GapReason::Capacity)
    );
    assert_eq!(rejected, original);
    assert_eq!(participants.resolve_actor("P0#TEST"), Some(prior));
}
