use super::*;

pub(super) fn fixture() -> (LcuRound, LiveIdentity) {
    let round = LcuRound {
        summoner: serde_json::from_str(
            r#"{"puuid":"fixture-local","gameName":"Example","tagLine":"TEST"}"#,
        )
        .unwrap(),
        gameflow: serde_json::from_str(
            r#"{
            "phase":"InProgress","map":{"id":11,"gameMode":"CLASSIC"},
            "gameData":{"gameId":9007199254740993,
            "queue":{"id":0,"mapId":11,"gameMode":"SWIFTPLAY"}}}
        "#,
        )
        .unwrap(),
    };
    let live = LiveIdentity {
        active: "Example#TEST".into(),
        map_id: Some(11),
        game_mode: Some("SWIFTPLAY".into()),
    };
    (round, live)
}

#[test]
fn exact_local_identity_and_queue_mode_supply_provisional_evidence_without_roster() {
    let (round, live) = fixture();
    let result = compare(&round, Some(&live));
    assert!(result.coherent());
    let candidate = evidence(&round, Some(&live)).unwrap();
    assert_eq!(candidate.game_id, 9_007_199_254_740_993);
    assert_eq!(candidate.queue_id, 0);
    // Broad map CLASSIC does not contradict the comparable queue/live SWIFTPLAY mode.
    assert_eq!(candidate.lcu_map_mode.as_deref(), Some("CLASSIC"));
    let json = serde_json::to_string(&result).unwrap();
    for secret in ["fixture-local", "Example", "SWIFTPLAY", "9007199254740993"] {
        assert!(!json.contains(secret));
    }
}

#[test]
fn partial_absent_empty_and_malformed_rosters_cannot_gate_provisional_identity() {
    let (round, live) = fixture();
    let expected = evidence(&round, Some(&live)).unwrap();
    // The observed bot shape is intentionally not required to be complete.
    for roster in [
        serde_json::json!({
            "teamOne": [{"puuid":"fixture-local","championId":62},{},{},{}],
            "teamTwo": [], "playerChampionSelections": [{},{},{},{},{}]
        }),
        serde_json::json!({"teamOne": [], "teamTwo": [], "playerChampionSelections":[]}),
        serde_json::json!({"teamOne": {"unexpected":true}, "teamTwo":42,
                          "playerChampionSelections":"mode-dependent"}),
        serde_json::json!({"teamOne":vec![serde_json::json!({"championId":"invalid"});100]}),
        serde_json::json!({}),
    ] {
        let mut value = serde_json::json!({
            "phase":"InProgress", "map":{"id":11,"gameMode":"CLASSIC"},
            "gameData":{"gameId":9007199254740993_u64,
                "queue":{"id":0,"mapId":11,"gameMode":"SWIFTPLAY"}}
        });
        for (key, value_roster) in roster.as_object().unwrap() {
            value["gameData"][key] = value_roster.clone();
        }
        let candidate = LcuRound {
            summoner: round.summoner.clone(),
            gameflow: serde_json::from_value(value).unwrap(),
        };
        assert!(evidence(&candidate, Some(&live)).unwrap() == expected);
    }
}

#[test]
fn queue_zero_is_explicit_but_missing_and_malformed_ids_never_supply_evidence() {
    let (mut round, live) = fixture();
    assert!(evidence(&round, Some(&live)).is_some());
    round.gameflow.game_data.queue.id = None;
    assert!(evidence(&round, Some(&live)).is_none());
    for value in ["null", "-1", "1.5", "\"0\"", "4294967296"] {
        let parsed = serde_json::from_str::<Queue>(&format!("{{\"id\":{value}}}"));
        assert!(parsed.is_err() || parsed.unwrap().id.is_none());
    }
    let (mut round, live) = fixture();
    for game_id in [None, Some(0)] {
        round.gameflow.game_data.game_id = game_id;
        assert!(evidence(&round, Some(&live)).is_none());
    }
    for value in ["-1", "1.5", "\"42\"", "18446744073709551616"] {
        assert!(serde_json::from_str::<GameData>(&format!("{{\"gameId\":{value}}}")).is_err());
    }
}

#[test]
fn exact_full_tag_and_valid_local_identity_are_required() {
    let (mut round, mut live) = fixture();
    for identity in [
        "Example#DIFFERENT",
        "example#TEST",
        "Example",
        "Example#",
        "Example#TEST#EXTRA",
    ] {
        live.active = identity.into();
        assert!(evidence(&round, Some(&live)).is_none());
    }
    live.active = "Example#TEST".into();
    for puuid in [
        None,
        Some("".into()),
        Some("bad\nvalue".into()),
        Some("x".repeat(257)),
    ] {
        round.summoner.puuid = puuid;
        assert!(evidence(&round, Some(&live)).is_none());
    }
    let (mut round, live) = fixture();
    round.summoner.game_name = Some("Example#TEST".into());
    round.summoner.tag_line = Some("EXTRA".into());
    assert!(evidence(&round, Some(&live)).is_none());
}

#[test]
fn missing_context_and_conflicting_source_maps_or_modes_never_supply_evidence() {
    for change in 0..9 {
        let (mut round, mut live) = fixture();
        match change {
            0 => round.gameflow.phase = Some("EndOfGame".into()),
            1 => {
                round.gameflow.map.id = None;
                round.gameflow.game_data.queue.map_id = None;
            }
            2 => live.map_id = None,
            3 => round.gameflow.game_data.queue.map_id = Some(12),
            4 => live.map_id = Some(0),
            5 => round.gameflow.game_data.queue.game_mode = None,
            6 => live.game_mode = None,
            7 => live.game_mode = Some("OTHER".into()),
            _ => round.gameflow.map.id = Some(0),
        }
        assert!(evidence(&round, Some(&live)).is_none());
    }
    let (round, live) = fixture();
    assert!(evidence(&round, None).is_none());
    // One map source is sufficient if the other is absent, never if contradictory.
    let mut round = round;
    round.gameflow.map.id = None;
    assert!(evidence(&round, Some(&live)).is_some());
    // No fallback to broad map mode if the queue mode is missing.
    round.gameflow.map.game_mode = Some("SWIFTPLAY".into());
    round.gameflow.game_data.queue.game_mode = None;
    assert_eq!(compare(&round, Some(&live)).queue_mode_agrees, None);
    assert!(evidence(&round, Some(&live)).is_none());
}

#[test]
fn overlong_or_control_source_strings_are_rejected_before_history_admission() {
    for change in 0..3 {
        let (mut round, live) = fixture();
        match change {
            0 => round.gameflow.map.game_mode = Some("x".repeat(257)),
            1 => round.gameflow.phase = Some("InProgress\n".into()),
            _ => round.summoner.game_name = Some("Example\n".into()),
        }
        assert!(round.validate_strings().is_err());
        assert!(evidence(&round, Some(&live)).is_none());
    }
}
