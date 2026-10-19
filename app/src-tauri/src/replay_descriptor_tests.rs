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
    assert_eq!(
        serde_json::to_value(&descriptor)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        4
    );
    let mut metadata: serde_json::Value = read_json(&game.join(METADATA_JSON)).unwrap();
    metadata.as_object_mut().unwrap().remove("capture");
    fs::write(
        game.join(METADATA_JSON),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    let unchanged = replay_descriptor(
        root.path(),
        "http://127.0.0.1:9000/cap/test",
        "1786000000",
        "snapshot".into(),
    )
    .unwrap();
    assert_eq!(descriptor, unchanged);
    assert_eq!(fs::read(game.join(VIDEO_MP4)).unwrap(), original);
}

#[test]
fn descriptor_rejects_invalid_bundles_and_unavailable_media() {
    let root = tempdir().unwrap();
    for (key, value) in [
        ("schema_version", json!(1)),
        ("media_id", json!("11111111-2222-4333-8444-555555555555")),
        ("media_timeline", json!({})),
    ] {
        let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", false);
        let path = game.join(METADATA_JSON);
        let mut document: serde_json::Value = read_json(&path).unwrap();
        document[key] = value;
        fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        assert!(
            replay_descriptor(root.path(), "origin", "1786000000", "token".into()).is_err(),
            "{key}"
        );
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

#[test]
fn exact_media_and_export_coordinates_ignore_optional_source_files() {
    let root = tempdir().unwrap();
    let game = write_game(root.path(), "1786000000", "2026-08-05T10:00:00Z", true);
    let expected = timeline();
    assert_eq!(
        replay_descriptor(root.path(), "origin", "1786000000", "token".into())
            .unwrap()
            .media_timeline,
        expected
    );
    assert_eq!(
        recording_export_authority(&game).unwrap().media_timeline,
        expected
    );
    assert!(read_metadata(&game.join(METADATA_JSON)).unwrap().saved);
    let summary = read_game_summary(&game, "1786000000".into());
    assert!(summary.saved && !summary.incomplete && summary.video_available);
    assert!(delete_game(root.path(), "1786000000").is_err());
    let result = run_auto_delete_at(
        root.path(),
        1,
        OffsetDateTime::parse("2026-10-15T00:00:00Z", &Rfc3339).unwrap(),
    )
    .unwrap();
    assert_eq!(result.deleted_count, 0);
    fs::write(game.join(METADATA_JSON), vec![b' '; 64 * 1024 + 1]).unwrap();
    assert!(recording_export_authority(&game).is_err());
    assert!(replay_descriptor(root.path(), "origin", "1786000000", "token".into()).is_err());
}
