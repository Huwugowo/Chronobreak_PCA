#[test]
fn optional_match_context_preserves_descriptor_scan_save_and_delete_contracts() {
    let root = tempfile::tempdir().unwrap();
    let directory = write_game(root.path(), "1786000000", "2026-08-06T10:00:00Z", false);
    let media = MediaId::parse(MEDIA_ID).unwrap();
    let descriptor = replay_descriptor(root.path(), "origin", "1786000000", "token".to_owned()).unwrap();
    let summary = list_games(root.path()).unwrap();
    assert!(playback_probe(root.path(), "origin", "1786000000").unwrap().league_match.is_none());
    let bytes = serde_json::to_vec(&crate::league_match::fixture(&media)).unwrap();
    let path = directory.join("league_match.json");
    fs::write(&path, &bytes).unwrap();
    let probe = playback_probe(root.path(), "origin", "1786000000").unwrap();
    let context = probe.league_match.unwrap();
    assert_eq!(context.game_id, "9007199254740993");
    assert_eq!(context.queue_id, 0);
    assert_eq!(context.local_riot_id, "Player#EUW");
    assert_eq!(context.status, crate::league_match::MatchStatus::Provisional);
    assert_eq!(list_games(root.path()).unwrap(), summary);
    assert_eq!(replay_descriptor(root.path(), "origin", "1786000000", "token".to_owned()).unwrap(), descriptor);
    save_game(root.path(), "1786000000", true).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(playback_probe(root.path(), "origin", "1786000000").unwrap().game.saved);
    assert!(delete_game(root.path(), "1786000000").is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    save_game(root.path(), "1786000000", false).unwrap();
    delete_game(root.path(), "1786000000").unwrap();
    assert!(!directory.exists());
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
