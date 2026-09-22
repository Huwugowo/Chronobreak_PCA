#[test]
fn descriptor_uses_strict_media_authority_without_semantic_projection() {
    let root = tempdir().unwrap();
    let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
    let original = fs::read(game.join(VIDEO_MP4)).unwrap();
    let descriptor = replay_descriptor(
        root.path(),
        "http://127.0.0.1:9000/cap/test",
        "1786000000",
        "snapshot".into(),
    )
    .unwrap();
    let probe =
        playback_probe(root.path(), "http://127.0.0.1:9000/cap/test", "1786000000").unwrap();
    assert_eq!(descriptor.media_timeline, probe.media_timeline);
    assert_eq!(descriptor.video_url, probe.video_url);
    assert_eq!(
        serde_json::to_value(&descriptor)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        4
    );
    let mut log: serde_json::Value = read_json(&game.join(GAME_LOG_JSON)).unwrap();
    log["calibration"] = serde_json::Value::Null;
    log["events"] = json!([]);
    log["snapshots"] = json!([]);
    fs::write(game.join(GAME_LOG_JSON), serde_json::to_vec(&log).unwrap()).unwrap();
    let mut metadata: serde_json::Value = read_json(&game.join(METADATA_JSON)).unwrap();
    metadata.as_object_mut().unwrap().remove("capture");
    fs::write(
        game.join(METADATA_JSON),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    let empty = replay_descriptor(
        root.path(),
        "http://127.0.0.1:9000/cap/test",
        "1786000000",
        "snapshot".into(),
    )
    .unwrap();
    assert_eq!(descriptor, empty);
    assert_eq!(fs::read(game.join(VIDEO_MP4)).unwrap(), original);
}

#[test]
fn descriptor_rejects_invalid_bundles_and_unavailable_media() {
    let root = tempdir().unwrap();
    for (file, key, value) in [
        (METADATA_JSON, "schema_version", json!(1)),
        (GAME_LOG_JSON, "schema_version", json!(1)),
        (GAME_LOG_JSON, "legacy_time_ms", json!(0)),
        (
            GAME_LOG_JSON,
            "media_id",
            json!("11111111-2222-4333-8444-555555555555"),
        ),
        (METADATA_JSON, "media_timeline", json!({})),
        (
            GAME_LOG_JSON,
            "events",
            json!([{"type":"ChampionKill","game_tick":"01"}]),
        ),
    ] {
        let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
        let path = game.join(file);
        let mut document: serde_json::Value = read_json(&path).unwrap();
        document[key] = value;
        fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        assert!(
            replay_descriptor(root.path(), "origin", "1786000000", "token".into()).is_err(),
            "{file}/{key}"
        );
        assert!(playback_probe(root.path(), "origin", "1786000000").is_err());
    }
    let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
    fs::write(game.join(VIDEO_MP4), []).unwrap();
    assert!(replay_descriptor(root.path(), "origin", "1786000000", "token".into()).is_err());
    fs::remove_file(game.join(VIDEO_MP4)).unwrap();
    assert!(replay_descriptor(root.path(), "origin", "1786000000", "token".into()).is_err());
    fs::create_dir(game.join(VIDEO_MP4)).unwrap();
    assert!(replay_descriptor(root.path(), "origin", "1786000000", "token".into()).is_err());
    assert!(replay_descriptor(root.path(), "origin", "../1786000000", "token".into()).is_err());
}
