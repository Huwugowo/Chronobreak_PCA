#[test]
fn optional_match_context_preserves_descriptor_scan_save_and_delete_contracts() {
    let root = tempfile::tempdir().unwrap();
    let directory = write_game(root.path(), "1786000000", "2026-08-06T10:00:00Z", false);
    let media = MediaId::parse(MEDIA_ID).unwrap();
    let descriptor =
        replay_descriptor(root.path(), "origin", "1786000000", "token".to_owned()).unwrap();
    let summary = list_games(root.path()).unwrap();
    assert!(
        playback_probe(root.path(), "origin", "1786000000")
            .unwrap()
            .league_match
            .is_none()
    );
    let bytes = serde_json::to_vec(&crate::league_match::fixture(&media)).unwrap();
    let path = directory.join("league_match.json");
    fs::write(&path, &bytes).unwrap();
    let probe = playback_probe(root.path(), "origin", "1786000000").unwrap();
    let context = probe.league_match.unwrap();
    assert_eq!(context.game_id, "9007199254740993");
    assert_eq!(context.queue_id, 0);
    assert_eq!(context.local_riot_id, "Player#EUW");
    assert_eq!(
        context.status,
        crate::league_match::MatchStatus::Provisional
    );
    assert_eq!(list_games(root.path()).unwrap(), summary);
    assert_eq!(
        replay_descriptor(root.path(), "origin", "1786000000", "token".to_owned()).unwrap(),
        descriptor
    );
    save_game(root.path(), "1786000000", true).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(
        playback_probe(root.path(), "origin", "1786000000")
            .unwrap()
            .game
            .saved
    );
    assert!(delete_game(root.path(), "1786000000").is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    save_game(root.path(), "1786000000", false).unwrap();
    delete_game(root.path(), "1786000000").unwrap();
    assert!(!directory.exists());
}

fn result_fixture(media: &MediaId) -> chronobreak_league_data::ResultFile {
    use chronobreak_league_data::{Coverage, Families, Outcome, Player, ResultFile, Stats, Team};
    let player = Player {
        stats: Stats {
            kills: Some(6),
            deaths: Some(2),
            assists: Some(9),
            ..Stats::default()
        },
        ..Player::default()
    };
    ResultFile {
        schema_version: 1,
        source: "lcu_eog".into(),
        confirmed: true,
        media_id: media.clone(),
        game_id: "9007199254740993".into(),
        outcome: Outcome::Win,
        ended_early: false,
        coverage: Families {
            combat: Coverage::Partial,
            economy: Coverage::Missing,
            damage: Coverage::Missing,
            support: Coverage::Missing,
            vision: Coverage::Missing,
            objectives: Coverage::Missing,
            loadout: Coverage::Missing,
            runes: Coverage::Missing,
        },
        duration_seconds: Some(600),
        end_timestamp_ms: None,
        queue_id: None,
        map_id: None,
        game_version: None,
        game_mode: Some("CLASSIC".into()),
        game_type: None,
        queue_type: None,
        label_mismatch: false,
        local_player: Some(player.clone()),
        teams: vec![Team {
            team_id: Some(100),
            is_winning: Some(true),
            stats: Stats::default(),
            players: vec![player; 10],
        }],
    }
}
#[test]
fn optional_result_is_bound_compact_and_preserves_media_events_and_saved_state() {
    let root = tempfile::tempdir().unwrap();
    let directory = write_game(root.path(), "1786000000", "2026-08-06T10:00:00Z", false);
    let media = MediaId::parse(MEDIA_ID).unwrap();
    let baseline = playback_probe(root.path(), "origin", "1786000000").unwrap();
    let descriptor =
        replay_descriptor(root.path(), "origin", "1786000000", "token".into()).unwrap();
    let path = directory.join(chronobreak_league_data::RESULT_FILE);
    let result = result_fixture(&media);
    let bytes = serde_json::to_vec(&result).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(
        playback_probe(root.path(), "origin", "1786000000")
            .unwrap()
            .league_result
            .is_none()
    );
    fs::write(
        directory.join("league_match.json"),
        serde_json::to_vec(&crate::league_match::fixture(&media)).unwrap(),
    )
    .unwrap();
    let probe = playback_probe(root.path(), "origin", "1786000000").unwrap();
    assert_eq!(probe.league_result, Some(result));
    assert_eq!(probe.events, baseline.events);
    assert_eq!(probe.player_timeline, baseline.player_timeline);
    assert_eq!(probe.game.kills, baseline.game.kills);
    let summary = list_games(root.path()).unwrap();
    assert_eq!(
        summary[0].league_result.unwrap().outcome,
        chronobreak_league_data::Outcome::Win
    );
    assert!(serde_json::to_vec(&summary[0].league_result).unwrap().len() < 100);
    assert_eq!(
        replay_descriptor(root.path(), "origin", "1786000000", "token".into()).unwrap(),
        descriptor
    );
    save_game(root.path(), "1786000000", true).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(
        playback_probe(root.path(), "origin", "1786000000")
            .unwrap()
            .game
            .saved
    );
}
#[test]
fn invalid_result_never_invalidates_playback_or_fabricates_final_facts() {
    let root = tempfile::tempdir().unwrap();
    let directory = write_game(root.path(), "1786000000", "2026-08-06T10:00:00Z", false);
    let media = MediaId::parse(MEDIA_ID).unwrap();
    fs::write(
        directory.join("league_match.json"),
        serde_json::to_vec(&crate::league_match::fixture(&media)).unwrap(),
    )
    .unwrap();
    let good = serde_json::to_value(result_fixture(&media)).unwrap();
    let mut variants = [
        json!({"confirmed":true}),
        good.clone(),
        good.clone(),
        good.clone(),
        good.clone(),
    ];
    variants[1]["game_id"] = json!("7");
    variants[2]["media_id"] = json!(MediaId::new_v4());
    variants[3]["schema_version"] = json!(2);
    variants[4]["local_player"]["stats"]["kills"] = json!(-1);
    let mut bytes = variants
        .iter()
        .map(|v| serde_json::to_vec(v).unwrap())
        .collect::<Vec<_>>();
    bytes.push(b"corrupt".to_vec());
    bytes.push(vec![b' '; 256 * 1024 + 1]);
    for bad in bytes {
        fs::write(directory.join(chronobreak_league_data::RESULT_FILE), bad).unwrap();
        let probe = playback_probe(root.path(), "origin", "1786000000").unwrap();
        assert!(probe.league_result.is_none());
        assert!(probe.game.video_available);
        assert_eq!(probe.events.len(), 3);
        assert!(list_games(root.path()).unwrap()[0].league_result.is_none());
    }
    let mut unknown = good;
    unknown["outcome"] = json!("unknown");
    unknown["ended_early"] = json!(true);
    fs::write(
        directory.join(chronobreak_league_data::RESULT_FILE),
        serde_json::to_vec(&unknown).unwrap(),
    )
    .unwrap();
    assert_eq!(
        playback_probe(root.path(), "origin", "1786000000")
            .unwrap()
            .league_result
            .unwrap()
            .outcome,
        chronobreak_league_data::Outcome::Unknown
    );
}

#[test]
fn result_summary_scan_characterization_uses_only_compact_ipc() {
    let root = tempfile::tempdir().unwrap();
    let media = MediaId::parse(MEDIA_ID).unwrap();
    let facts = result_fixture(&media);
    let bytes = serde_json::to_vec(&facts).unwrap();
    let mut directories = Vec::new();
    for i in 0..24 {
        directories.push(write_game(
            root.path(),
            &(1786000000 + i).to_string(),
            "2026-08-06T10:00:00Z",
            false,
        ));
    }
    let start = std::time::Instant::now();
    for _ in 0..10 {
        assert_eq!(list_games(root.path()).unwrap().len(), 24);
    }
    let without_us = start.elapsed().as_micros();
    for directory in &directories {
        fs::write(
            directory.join("league_match.json"),
            serde_json::to_vec(&crate::league_match::fixture(&media)).unwrap(),
        )
        .unwrap();
        fs::write(directory.join(chronobreak_league_data::RESULT_FILE), &bytes).unwrap();
    }
    let start = std::time::Instant::now();
    for _ in 0..10 {
        assert_eq!(list_games(root.path()).unwrap().len(), 24);
    }
    let with_us = start.elapsed().as_micros();
    let summary = list_games(root.path()).unwrap();
    let wire = serde_json::to_vec(&summary[0].league_result).unwrap();
    assert!(wire.len() < 100);
    assert!(bytes.len() < 256 * 1024);
    println!(
        "result_scan bundles=24 iterations=10 without_us={without_us} with_us={with_us} result_bytes={} compact_summary_bytes={}",
        bytes.len(),
        wire.len()
    );
}

#[test]
fn invalid_optional_match_never_invalidates_valid_v2_playback() {
    let root = tempfile::tempdir().unwrap();
    let directory = write_game(root.path(), "1786000000", "2026-08-06T10:00:00Z", false);
    let media = MediaId::parse(MEDIA_ID).unwrap();
    let mut wrong_media = crate::league_match::fixture(&media);
    wrong_media["media_id"] = json!(MediaId::new_v4());
    let mut unsupported = crate::league_match::fixture(&media);
    unsupported["schema_version"] = json!(2);
    let mut unproved_final = crate::league_match::fixture(&media);
    unproved_final["status"] = json!("confirmed");
    for bytes in [
        b"corrupt".to_vec(),
        vec![b' '; 64 * 1024 + 1],
        serde_json::to_vec(&wrong_media).unwrap(),
        serde_json::to_vec(&unsupported).unwrap(),
        serde_json::to_vec(&unproved_final).unwrap(),
    ] {
        fs::write(directory.join("league_match.json"), bytes).unwrap();
        let probe = playback_probe(root.path(), "origin", "1786000000").unwrap();
        assert!(probe.league_match.is_none());
        assert!(probe.game.video_available);
        assert_eq!(probe.events.len(), 3);
    }
}
