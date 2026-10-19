#[test]
fn optional_match_context_is_scoped_to_the_selected_media() {
    let root = tempfile::tempdir().unwrap();
    let directory = write_game(root.path(), "1786000000", "2026-08-06T10:00:00Z", false);
    let media = MediaId::parse(MEDIA_ID).unwrap();
    let descriptor =
        replay_descriptor(root.path(), "origin", "1786000000", "token".to_owned()).unwrap();
    let facts = league_facts(root.path(), "1786000000", &media).unwrap();
    assert_eq!(facts.media_id, media);
    assert_eq!(facts.local_player_name.as_deref(), Some("Player#EUW"));
    assert!(facts.league_match.is_none());
    let bytes = serde_json::to_vec(&crate::league_match::fixture(&media)).unwrap();
    let path = directory.join("league_match.json");
    fs::write(&path, &bytes).unwrap();
    let facts = league_facts(root.path(), "1786000000", &media).unwrap();
    let context = facts.league_match.unwrap();
    assert_eq!(context.game_id, "9007199254740993");
    assert_eq!(context.local_riot_id, "Player#EUW");
    assert_eq!(list_games(root.path()).unwrap().len(), 1);
    assert_eq!(
        replay_descriptor(root.path(), "origin", "1786000000", "token".to_owned()).unwrap(),
        descriptor
    );
    save_game(root.path(), "1786000000", true).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(list_games(root.path()).unwrap()[0].saved);
    assert!(delete_game(root.path(), "1786000000").is_err());
}

pub(crate) fn result_fixture(media: &MediaId) -> chronobreak_league_data::ResultFile {
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
        schema_version: chronobreak_league_data::RESULT_SCHEMA_VERSION,
        source: "lcu_eog".into(),
        confirmed: true,
        media_id: media.clone(),
        game_id: "9007199254740993".into(),
        source_facts: chronobreak_league_data::observation::FinalFacts {
            game_id: Some("9007199254740993".into()),
            ..Default::default()
        },
        evidence: None,
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
fn result_facts_require_a_matching_match_context_without_changing_media_authority() {
    let root = tempfile::tempdir().unwrap();
    let directory = write_game(root.path(), "1786000000", "2026-08-06T10:00:00Z", false);
    let media = MediaId::parse(MEDIA_ID).unwrap();
    let result = result_fixture(&media);
    fs::write(
        directory.join(chronobreak_league_data::RESULT_FILE),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
    assert!(
        league_facts(root.path(), "1786000000", &media)
            .unwrap()
            .league_result
            .is_none()
    );
    fs::write(
        directory.join("league_match.json"),
        serde_json::to_vec(&crate::league_match::fixture(&media)).unwrap(),
    )
    .unwrap();
    let facts = league_facts(root.path(), "1786000000", &media).unwrap();
    assert_eq!(facts.league_result, Some(result));
    assert_eq!(
        list_games(root.path()).unwrap()[0]
            .league_result
            .unwrap()
            .outcome,
        chronobreak_league_data::Outcome::Win
    );
}

#[test]
fn invalid_optional_facts_never_invalidate_media_or_fabricate_results() {
    let root = tempfile::tempdir().unwrap();
    let directory = write_game(root.path(), "1786000000", "2026-08-06T10:00:00Z", false);
    let media = MediaId::parse(MEDIA_ID).unwrap();
    fs::write(
        directory.join("league_match.json"),
        serde_json::to_vec(&crate::league_match::fixture(&media)).unwrap(),
    )
    .unwrap();
    let good = serde_json::to_value(result_fixture(&media)).unwrap();
    let mut wrong_media = good.clone();
    wrong_media["media_id"] = json!(MediaId::new_v4());
    for bytes in [
        b"corrupt".to_vec(),
        serde_json::to_vec(&wrong_media).unwrap(),
        vec![b' '; 256 * 1024 + 1],
    ] {
        fs::write(directory.join(chronobreak_league_data::RESULT_FILE), bytes).unwrap();
        assert!(
            league_facts(root.path(), "1786000000", &media)
                .unwrap()
                .league_result
                .is_none()
        );
        assert!(replay_descriptor(root.path(), "origin", "1786000000", "token".into()).is_ok());
        assert!(list_games(root.path()).unwrap()[0].video_available);
    }
}
