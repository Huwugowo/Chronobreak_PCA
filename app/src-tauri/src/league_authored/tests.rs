use super::*;
use chronobreak_replay_time::{FrameBoundary, ReplayTick};
use tempfile::tempdir;
const ID: &str = "123e4567-e89b-42d3-a456-426614174000";
fn record() -> AuthoredRecord {
    let media = MediaId::parse(ID).unwrap();
    let source = SourceEpoch {
        media_id: media.clone(),
        run_id: MediaId::parse("123e4567-e89b-42d3-a456-426614174001").unwrap(),
        epoch: 2,
    };
    AuthoredRecord {
        id: MediaId::parse("123e4567-e89b-42d3-a456-426614174002").unwrap(),
        kind: RecordKind::Recipe,
        created_at: "2026-10-16T10:00:00Z".into(),
        reason: "Synthetic recap input".into(),
        derivation: Derivation {
            rule: "fixture-rule".into(),
            version: "1".into(),
            parameters: [("threshold".into(), Parameter::Integer(3))].into(),
        },
        inputs: vec![
            InputReference::Event {
                source: source.clone(),
                source_id: 7,
                revision: Some(EventRevisionReference {
                    sequence: 11,
                    ordinal: 0,
                    fingerprint: [3; 32],
                }),
            },
            InputReference::Participant {
                source,
                participant: ParticipantId(1),
            },
        ],
        anchors: vec![Anchor {
            media_id: media.clone(),
            replay: Some(ReplayTimeReference {
                media_id: media.clone(),
                replay_tick: ReplayTick::new(48_000_000).unwrap(),
            }),
            game_tick: Some(GameTick::new(1_000_000).unwrap()),
            evidence: None,
            mapping: Mapping::Estimated {
                uncertainty_game_ticks: 1_000,
            },
        }],
        ranges: vec![
            ClipRange::new(
                media.clone(),
                FrameBoundary::new(90).unwrap(),
                FrameBoundary::new(120).unwrap(),
                FrameBoundary::new(600).unwrap(),
            )
            .unwrap(),
            ClipRange::new(
                media,
                FrameBoundary::new(0).unwrap(),
                FrameBoundary::new(30).unwrap(),
                FrameBoundary::new(600).unwrap(),
            )
            .unwrap(),
        ],
    }
}
#[test]
fn durable_round_trip_keeps_order_exact_time_and_unresolved_source_references() {
    let root = tempdir().unwrap();
    let value = record();
    let stored = put(root.path(), 0, value.clone(), &|| Ok(())).unwrap();
    assert_eq!(stored.revision, 1);
    assert_eq!(load(root.path()).unwrap().records, vec![value.clone()]);
    assert_eq!(stored.records[0].ranges[0].start_frame.get(), 90);
    assert!(put(root.path(), 0, value, &|| Ok(())).is_err());
    assert_eq!(load(root.path()).unwrap(), stored);
}
#[test]
fn invalid_duplicate_oversize_and_unsupported_records_fail_closed() {
    let mut store = Store {
        records: vec![record()],
        ..Default::default()
    };
    let bytes = store.encode().unwrap();
    for mutation in ["version", "duplicate", "media", "range", "date"] {
        let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        match mutation {
            "version" => v["schema_version"] = 99.into(),
            "duplicate" => {
                let r = v["records"][0].clone();
                v["records"].as_array_mut().unwrap().push(r);
            }
            "media" => {
                v["records"][0]["anchors"][0]["media_id"] =
                    "123e4567-e89b-42d3-a456-426614174009".into()
            }
            "range" => v["records"][0]["ranges"][0]["end_frame_exclusive"] = "90".into(),
            _ => v["records"][0]["created_at"] = "not a date".into(),
        }
        assert!(
            Store::decode(&serde_json::to_vec(&v).unwrap()).is_err(),
            "{mutation}"
        );
    }
    store.records[0].reason = "x".repeat(1025);
    assert!(store.encode().is_err());
    assert!(Store::decode(&vec![b' '; MAX_BYTES + 1]).is_err());
}
#[test]
fn failed_publication_and_invalid_existing_file_preserve_authored_data() {
    let root = tempdir().unwrap();
    let value = record();
    put(root.path(), 0, value.clone(), &|| Ok(())).unwrap();
    let bytes = fs::read(root.path().join(FILE)).unwrap();
    let count = std::sync::atomic::AtomicUsize::new(0);
    assert!(
        put(root.path(), 1, value.clone(), &|| {
            ensure!(
                count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0,
                "fixture cancellation before publication"
            );
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(root.path().join(FILE)).unwrap(), bytes);
    fs::write(root.path().join(FILE), b"broken fixture").unwrap();
    assert!(put(root.path(), 1, value, &|| Ok(())).is_err());
    assert_eq!(fs::read(root.path().join(FILE)).unwrap(), b"broken fixture");
}
#[test]
fn exclusive_store_owner_prevents_concurrent_lost_update() {
    let root = tempdir().unwrap();
    let owner = lock(root.path()).unwrap();
    assert!(put(root.path(), 0, record(), &|| Ok(())).is_err());
    drop(owner);
    assert!(put(root.path(), 0, record(), &|| Ok(())).is_ok());
}

#[test]
fn deleting_and_rebuilding_sql_preserves_authored_contents_and_relationships() {
    use crate::{
        league_index::{Index, cache_path},
        library,
    };
    let root = tempdir().unwrap();
    let cache = tempdir().unwrap();
    library::tests::write_game(root.path(), "100", "2026-08-05T10:00:00Z", false);
    let games = library::list_games(root.path()).unwrap();
    put(root.path(), 0, record(), &|| Ok(())).unwrap();
    let bytes = fs::read(root.path().join(FILE)).unwrap();
    let mut index = Index::open(cache.path(), root.path()).unwrap();
    index.reconcile(&games, "one").unwrap();
    let projected = serde_json::to_value(index.authored(None, 32).unwrap()).unwrap();
    assert_eq!(projected["records"][0]["inputs"][0]["source"]["epoch"], 2);
    drop(index);
    fs::remove_file(cache_path(cache.path(), root.path())).unwrap();
    let mut index = Index::open(cache.path(), root.path()).unwrap();
    index.reconcile(&games, "two").unwrap();
    assert_eq!(
        serde_json::to_value(index.authored(None, 32).unwrap()).unwrap(),
        projected
    );
    assert_eq!(fs::read(root.path().join(FILE)).unwrap(), bytes);
    index.reconcile(&[], "recording-deleted").unwrap();
    assert_eq!(
        serde_json::to_value(index.authored(None, 32).unwrap()).unwrap(),
        projected
    );
}

#[test]
fn authored_query_reconciliation_observes_saves_without_recording_import() {
    use crate::league_index::Index;
    let root = tempdir().unwrap();
    let cache = tempdir().unwrap();
    let mut index = Index::open(cache.path(), root.path()).unwrap();
    index.reconcile(&[], "accepted").unwrap();
    assert_eq!(index.authored(None, 32).unwrap().revision, 0);
    let mut value = record();
    put(root.path(), 0, value.clone(), &|| Ok(())).unwrap();
    index.reconcile(&[], "refreshed").unwrap();
    let page = index.authored(None, 32).unwrap();
    assert_eq!(page.revision, 1);
    assert_eq!(page.records, vec![value.clone()]);
    value.reason = "Updated synthetic recipe".into();
    put(root.path(), 1, value.clone(), &|| Ok(())).unwrap();
    index.reconcile(&[], "refreshed").unwrap();
    let page = index.authored(None, 32).unwrap();
    assert_eq!(page.revision, 2);
    assert_eq!(page.records, vec![value]);
}
