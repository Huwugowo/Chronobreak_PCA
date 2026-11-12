use super::{query::*, *};
use chronobreak_replay_time::MappedReplayTime;
use obs::{Endpoint, Evidence, ObservationId, Record};
use std::io::{Cursor, Write};
use tempfile::{TempDir, tempdir};

#[path = "import_progress_tests.rs"]
mod import_progress_tests;
#[path = "preprod_tests.rs"]
mod preprod_tests;
#[cfg(windows)]
#[path = "production_path_tests.rs"]
mod production_path_tests;
#[path = "query_measurements.rs"]
mod query_measurements;

struct Fixture {
    root: TempDir,
    cache: TempDir,
    directory: PathBuf,
    summary: GameSummary,
    source: obs::SourceEpoch,
    writer: journal::Writer<fs::File>,
    state: checkpoint::State,
    events: obs::events::EventTracker,
}
impl Fixture {
    fn new(commits: usize) -> Self {
        let root = tempdir().unwrap();
        let cache = tempdir().unwrap();
        let directory =
            library::tests::write_matched_game(root.path(), "100", "2026-08-05T10:00:00Z", false);
        let summary = library::list_games(root.path()).unwrap().remove(0);
        let mut records = Vec::new();
        let recovered = journal::recover(
            Cursor::new(fs::read(directory.join(JOURNAL)).unwrap()),
            |entries, _| {
                records.extend(entries.iter().map(|e| e.record.clone()));
                Ok(())
            },
        )
        .unwrap();
        for record in &mut records {
            if let Record::EventObservation { evidence, .. } = record {
                evidence.game_stats = Some(ObservationId(4));
            }
        }
        let source = obs::SourceEpoch {
            media_id: recovered.header.media_id.clone(),
            run_id: recovered.header.run_id.clone(),
            epoch: 0,
        };
        let writer = journal::Writer::new(
            fs::File::create(directory.join(JOURNAL)).unwrap(),
            recovered.header.clone(),
        )
        .unwrap();
        let state = checkpoint::State::new(recovered.header, writer.position().clone());
        let mut events = obs::events::EventTracker::new(source.clone());
        for record in &records {
            if let Record::EventObservation { revisions, .. } = record {
                events.restore_committed(&source, revisions).unwrap();
            }
        }
        let mut fixture = Self {
            root,
            cache,
            directory,
            summary,
            source,
            writer,
            state,
            events,
        };
        fixture.commit(&records, true);
        for id in 1..commits {
            fixture.game_stats(100 + id as u64, 4_000_000 + id as i64 * 1_000_000, true);
        }
        fixture
    }
    fn evidence(&self, id: u64, endpoint: Endpoint, gs: Option<u64>) -> Evidence {
        Evidence {
            id: ObservationId(id),
            source: self.source.clone(),
            endpoint,
            timing: obs::RequestTiming {
                start_us: id * 1_000,
                end_us: id * 1_000 + 10,
                received_at: "2026-08-05T10:00:00Z".into(),
            },
            game_stats: gs.map(ObservationId),
            calibration: Some(ObservationId(1)),
        }
    }
    fn commit(&mut self, records: &[Record], update_manifest: bool) {
        let position = self.writer.commit(records).unwrap();
        self.state.apply(records, &position).unwrap();
        if update_manifest {
            fs::write(
                self.directory.join(manifest::FILE),
                manifest::Manifest::from_checkpoint(self.state.snapshot())
                    .encode()
                    .unwrap(),
            )
            .unwrap();
        }
    }
    fn game_stats(&mut self, id: u64, tick: i64, update_manifest: bool) {
        let record = Record::Observation {
            observation: Box::new(obs::Observation::Success {
                evidence: self.evidence(id, Endpoint::GameStats, None),
                payload: obs::Payload::GameStats(obs::GameStats {
                    game_tick: Some(GameTick::new(tick).unwrap()),
                    ..Default::default()
                }),
            }),
        };
        self.commit(&[record], update_manifest);
    }
    fn index(&self) -> Index {
        let mut index = Index::open(self.cache.path(), self.root.path()).unwrap();
        index
            .reconcile(std::slice::from_ref(&self.summary), "accepted")
            .unwrap();
        index
    }
    fn ready(&self, index: &mut Index) {
        for _ in 0..10 {
            if index
                .recording(self.root.path(), &self.summary, false, &|| Ok(()))
                .unwrap()
                .status
                == "ready"
            {
                assert_whole_coverage(index, &self.scope());
                return;
            }
        }
        panic!("bounded fixture did not finish");
    }
    fn scope(&self) -> ScopeQuery {
        ScopeQuery {
            media_id: self.source.media_id.clone(),
            run_id: self.source.run_id.clone(),
            epoch: self.source.epoch,
        }
    }
    fn window(&self) -> WindowQuery {
        WindowQuery {
            scope: self.scope(),
            start: GameTick::new(0).unwrap(),
            end: GameTick::new(10_000_000).unwrap(),
            after: None,
            limit: 100,
            kind: None,
            stolen: None,
            minimum_kill_streak: None,
            participant: None,
            role: None,
        }
    }
    fn timeline(&self) -> chronobreak_replay_time::MediaTimelineV2 {
        library::replay_descriptor(self.root.path(), "", "100", String::new())
            .unwrap()
            .media_timeline
    }
}

fn assert_whole_coverage(index: &Index, scope: &ScopeQuery) {
    let projected = index.whole_source_coverage(scope).unwrap();
    let expected = super::coverage::aggregate(
        &index.db,
        scope,
        GameTick::new(-86_400_000_000).unwrap(),
        GameTick::new(86_400_000_000).unwrap(),
        "events",
    )
    .unwrap();
    assert_eq!(projected.gap_attempts, expected.gap_attempts);
    assert_eq!(projected.unresolved_actors, expected.unresolved_actors);
    assert_eq!(
        projected.unmapped_observations,
        expected.unmapped_observations
    );
    assert_eq!(
        projected.successful_observations,
        expected.successful_observations
    );
    assert_eq!(
        projected.observed_from.map(GameTick::get),
        expected.observed_from
    );
    assert_eq!(
        projected.observed_through.map(GameTick::get),
        expected.observed_through
    );
}

#[test]
fn pages_commits_atomically_and_skips_unchanged_sources() {
    let f = Fixture::new(IMPORT_BATCHES + 1);
    let mut index = f.index();
    let first = index
        .recording(f.root.path(), &f.summary, false, &|| Ok(()))
        .unwrap();
    assert_eq!(
        (first.status.as_str(), first.batches),
        ("pending", IMPORT_BATCHES as u32)
    );
    assert!(
        index
            .events(&f.window(), &f.timeline())
            .unwrap()
            .events
            .is_empty()
    );
    let second = index
        .recording(f.root.path(), &f.summary, false, &|| Ok(()))
        .unwrap();
    assert_eq!((second.status.as_str(), second.batches), ("ready", 1));
    assert_eq!(
        index
            .recording(f.root.path(), &f.summary, false, &|| Ok(()))
            .unwrap()
            .batches,
        0
    );
    let reader = Index::reader(f.cache.path(), f.root.path()).unwrap();
    reader
        .require_catalog(&catalog_key(std::slice::from_ref(&f.summary)).unwrap())
        .unwrap();
    assert!(reader.require_catalog("old").is_err());
    assert_eq!(
        reader
            .events(&f.window(), &f.timeline())
            .unwrap()
            .events
            .len(),
        3
    );
    let plan: String = reader.db.query_row("EXPLAIN QUERY PLAN SELECT source_id FROM events WHERE media_id=?1 AND run_id=?2 AND epoch=0 AND game_tick BETWEEN 0 AND 10000000 ORDER BY game_tick,source_id", params![f.source.media_id.as_str(),f.source.run_id.as_str()], |r| r.get(3)).unwrap();
    assert!(plan.contains("events_window"), "{plan}");
}

#[test]
fn reused_card_facts_recheck_the_source_before_returning() {
    let mut fixture = Fixture::new(1);
    let mut index = fixture.index();
    fixture.ready(&mut index);
    let checks = std::cell::Cell::new(0);
    let changed = index.recording(fixture.root.path(), &fixture.summary, false, &|| {
        checks.set(checks.get() + 1);
        if checks.get() == 2 {
            // Change the optional source after the reuse decision but before
            // the recording can report that its checked facts are current.
            fs::write(fixture.directory.join(manifest::FILE), b"{}")?;
        }
        Ok(())
    });
    assert_eq!(
        changed.unwrap_err().to_string(),
        "history source changed during preparation"
    );
    fixture.game_stats(100, 5_000_000, true);
    fixture.ready(&mut index);
}

#[test]
fn retains_corrections_exact_actors_stale_anchor_and_rebuild_equivalence() {
    let mut f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let mut query = f.window();
    query.limit = 1;
    let first = index.events(&query, &f.timeline()).unwrap();
    let original_event = first.events[0].revision.event.clone();
    let original_unresolved = index
        .whole_source_coverage(&f.scope())
        .unwrap()
        .unresolved_actors;
    assert_eq!(first.events.len(), 1);
    query.after = first.next;
    assert_eq!(
        index.events(&query, &f.timeline()).unwrap().events[0]
            .revision
            .event
            .source_id,
        Some(1)
    );
    let event = obs::Event {
        source_id: Some(0),
        source_time: Some(0.into()),
        game_tick: Some(GameTick::new(0).unwrap()),
        name: Some("DragonKill".into()),
        stolen: Some(true),
        killer: Some("Player#NA".into()),
        actors: Some(
            [(
                obs::events::ActorRole::Killer,
                vec![obs::events::ActorReference {
                    participant: None,
                    status: obs::events::ActorStatus::Unresolved,
                }],
            )]
            .into(),
        ),
        ..Default::default()
    };
    let pending = f.events.prepare(&f.source, vec![event]).unwrap();
    let revisions = pending.revisions().to_vec();
    f.events.acknowledge(pending).unwrap();
    f.commit(
        &[Record::EventObservation {
            evidence: f.evidence(200, Endpoint::Events, Some(4)),
            revisions,
        }],
        false,
    );
    f.ready(&mut index);
    let mut query = f.window();
    query.stolen = Some(true);
    let result = index.events(&query, &f.timeline()).unwrap();
    assert_eq!(result.events.len(), 2);
    let correction = result
        .events
        .iter()
        .find(|event| event.revision.event.source_id == Some(0))
        .unwrap();
    assert_eq!(
        correction.revision.ingestion,
        obs::events::Ingestion::Correction
    );
    query.participant = Some(obs::ParticipantId(1));
    query.role = Some(obs::events::ActorRole::Killer);
    assert!(
        index
            .events(&query, &f.timeline())
            .unwrap()
            .events
            .is_empty()
    );
    let count: u64 = index
        .db
        .query_row("SELECT COUNT(*) FROM event_revisions", [], |r| {
            unsigned(r, 0)
        })
        .unwrap();
    assert_eq!(count, 4);
    let before = serde_json::to_value(index.events(&f.window(), &f.timeline()).unwrap()).unwrap();
    assert_eq!(
        index
            .recording(f.root.path(), &f.summary, true, &|| Ok(()))
            .unwrap()
            .status,
        "ready"
    );
    assert_eq!(
        serde_json::to_value(index.events(&f.window(), &f.timeline()).unwrap()).unwrap(),
        before
    );
    let history = index
        .history(
            &HistoryQuery {
                limit: 10,
                participant_riot_id: Some("Player#NA".into()),
                ..Default::default()
            },
            1,
        )
        .unwrap();
    assert!(history.rows.is_empty());
    let pending = f.events.prepare(&f.source, vec![original_event]).unwrap();
    let revisions = pending.revisions().to_vec();
    f.events.acknowledge(pending).unwrap();
    f.commit(
        &[Record::EventObservation {
            evidence: f.evidence(201, Endpoint::Events, Some(4)),
            revisions,
        }],
        false,
    );
    f.ready(&mut index);
    assert_eq!(
        index
            .whole_source_coverage(&f.scope())
            .unwrap()
            .unresolved_actors,
        original_unresolved
    );
}

#[test]
fn roster_age_and_latest_empty_roster_do_not_resurrect_players_or_stats() {
    let mut f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let roster = index
        .roster_at(&f.scope(), GameTick::new(5_000_000).unwrap())
        .unwrap();
    assert_eq!(roster.age_us, Some(2_000_000));
    assert_eq!(roster.players.len(), 1);
    let history = index
        .history(
            &HistoryQuery {
                limit: 10,
                ..Default::default()
            },
            1,
        )
        .unwrap();
    assert!(matches!(
        history.rows[0].stats.source,
        StatsSource::RecordedSnapshot
    ));
    assert_eq!(history.rows[0].stats.kills, None);
    assert_eq!(history.rows[0].game.participants.len(), 1);
    assert!(history.rows[0].stats.creep_score.is_none());
    f.game_stats(201, 6_000_000, false);
    f.commit(
        &[Record::RosterObservation {
            evidence: f.evidence(202, Endpoint::PlayerList, Some(201)),
            roster: obs::roster::RosterObservation {
                full: true,
                references: vec![],
                payloads: vec![],
            },
        }],
        true,
    );
    f.ready(&mut index);
    let roster = index
        .roster_at(&f.scope(), GameTick::new(7_000_000).unwrap())
        .unwrap();
    assert!(roster.players.is_empty());
    assert_eq!(roster.age_us, Some(1_000_000));
    assert!(matches!(
        index
            .history(
                &HistoryQuery {
                    limit: 10,
                    ..Default::default()
                },
                1
            )
            .unwrap()
            .rows[0]
            .stats
            .source,
        StatsSource::Unavailable
    ));
}

#[test]
fn tail_and_corrupt_anchor_stay_optional_to_healthy_media() {
    let mut f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    fs::OpenOptions::new()
        .append(true)
        .open(f.directory.join(JOURNAL))
        .unwrap()
        .write_all(b"{\"torn\":")
        .unwrap();
    let p = index
        .recording(f.root.path(), &f.summary, false, &|| Ok(()))
        .unwrap();
    assert_eq!(p.tail_bytes, 8);
    let mut manifest = manifest::Manifest::from_checkpoint(f.state.snapshot());
    manifest.committed.end_offset += 1;
    fs::write(f.directory.join(manifest::FILE), manifest.encode().unwrap()).unwrap();
    assert!(
        index
            .recording(f.root.path(), &f.summary, false, &|| Ok(()))
            .is_err()
    );
    assert_eq!(
        index.progress(&f.source.media_id, 0).unwrap().status,
        "unavailable"
    );
    assert!(index.scopes(&f.source.media_id).is_err());
    let retained_scopes: u64 = index
        .db
        .query_row("SELECT COUNT(*) FROM scopes", [], |r| unsigned(r, 0))
        .unwrap();
    assert_eq!(retained_scopes, 0);
    assert!(
        index
            .events(&f.window(), &f.timeline())
            .unwrap()
            .events
            .is_empty()
    );
    let descriptor = library::replay_descriptor(f.root.path(), "", "100", String::new()).unwrap();
    assert_eq!(descriptor.media_timeline.media_id, f.source.media_id);
}

#[test]
fn card_facts_survive_bounded_imports_before_replay_is_prepared() {
    let f = Fixture::new(IMPORT_BATCHES * 3 + 1);
    let result = library::tests::result_fixture(&f.source.media_id);
    fs::write(
        f.directory.join("league_match.json"),
        serde_json::to_vec(&crate::league_match::fixture(&f.source.media_id)).unwrap(),
    )
    .unwrap();
    fs::write(
        f.directory.join(chronobreak_league_data::RESULT_FILE),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
    let mut index = f.index();
    index
        .history_recording(f.root.path(), &f.summary, false, &|| Ok(()))
        .unwrap();
    let query = HistoryQuery {
        limit: 10,
        ..Default::default()
    };
    let initial = index.history(&query, 1).unwrap();
    assert!(matches!(
        initial.rows[0].stats.source,
        StatsSource::FinalEog
    ));
    let expected = serde_json::to_value(&initial.rows[0].stats).unwrap();
    assert!(index.progress(&f.source.media_id, 0).unwrap().history_ready);
    assert_eq!(initial.rows[0].index_status, "pending");
    assert!(index.scopes(&f.source.media_id).is_err());
    for _ in 0..4 {
        let check = || {
            let reader = Index::reader(f.cache.path(), f.root.path())?;
            let page = reader.history(&query, 1)?;
            assert_eq!(serde_json::to_value(&page.rows[0].stats).unwrap(), expected);
            Ok(())
        };
        index
            .recording(f.root.path(), &f.summary, false, &check)
            .unwrap();
        assert!(index.progress(&f.source.media_id, 0).unwrap().history_ready);
    }
    assert_eq!(
        index.progress(&f.source.media_id, 0).unwrap().status,
        "ready"
    );
    assert_eq!(
        index
            .prepared_progress(f.root.path(), &f.summary)
            .unwrap()
            .status,
        "pending"
    );
    assert_eq!(
        serde_json::to_value(&index.history(&query, 1).unwrap().rows[0].stats).unwrap(),
        expected
    );
    // A real final-result change must replace the displayed facts, rather than
    // preserving an older enrichment to conceal loading.
    let mut changed = result;
    changed.local_player.as_mut().unwrap().stats.kills = Some(42);
    fs::write(
        f.directory.join(chronobreak_league_data::RESULT_FILE),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert!(
        !index
            .prepared_progress(f.root.path(), &f.summary)
            .unwrap()
            .history_ready
    );
    index
        .history_recording(f.root.path(), &f.summary, false, &|| Ok(()))
        .unwrap();
    assert!(
        index
            .prepared_progress(f.root.path(), &f.summary)
            .unwrap()
            .history_ready
    );
    assert_eq!(
        index.history(&query, 1).unwrap().rows[0].stats.kills,
        Some(42)
    );
}

#[tokio::test]
async fn history_groups_recording_parts_before_filters_counts_and_pagination() {
    let f = Fixture::new(1);
    let root = f.root.path();
    let write_part = |bundle: &str, date: &str, local_account: Option<&str>, saved: bool| {
        let path = library::tests::write_game(root, bundle, date, saved);
        let media = MediaId::new_v4();
        let metadata = path.join("metadata.json");
        let json = fs::read_to_string(&metadata).unwrap();
        fs::write(
            &metadata,
            json.replace(f.source.media_id.as_str(), media.as_str()),
        )
        .unwrap();
        if let Some(account) = local_account {
            let mut candidate = crate::league_match::fixture(&media);
            candidate["local_puuid"] = serde_json::json!(account);
            fs::write(
                path.join("league_match.json"),
                serde_json::to_vec(&candidate).unwrap(),
            )
            .unwrap();
        }
        media
    };
    let middle = write_part(
        "101",
        "2026-08-06T10:00:00Z",
        Some("synthetic-local-puuid"),
        true,
    );
    let latest = write_part(
        "102",
        "2026-08-07T10:00:00Z",
        Some("synthetic-local-puuid"),
        false,
    );
    // Share a timestamp across groups to exercise the second keyset column.
    let other = write_part(
        "103",
        "2026-08-07T10:00:00Z",
        Some("different-puuid"),
        false,
    );
    let newest_metadata = root.join("games/102/metadata.json");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(&newest_metadata).unwrap()).unwrap();
    metadata["local_player_champion"] = serde_json::json!("Ahri");
    fs::write(newest_metadata, serde_json::to_vec(&metadata).unwrap()).unwrap();
    write_part("104", "2026-08-08T10:00:00Z", None, false);
    fs::write(root.join("games/104/league_match.json"), b"invalid").unwrap();

    let mut index = f.index();
    let catalog = library::list_games(root).unwrap();
    index.reconcile(&catalog, "grouped").unwrap();
    assert_eq!(
        catalog
            .iter()
            .filter(|part| part.match_group.is_none())
            .count(),
        1
    );
    assert_eq!(
        catalog
            .iter()
            .filter(|part| part.match_group == f.summary.match_group)
            .count(),
        3
    );

    let query = HistoryQuery {
        limit: 1,
        ..Default::default()
    };
    let first = index.history(&query, 2).unwrap();
    assert_eq!((first.catalog_recordings, first.indexed_recordings), (2, 2));
    let next = first.next.unwrap();
    let second = index
        .history(
            &HistoryQuery {
                after: Some(next.clone()),
                ..query.clone()
            },
            2,
        )
        .unwrap();
    let representatives = [&first.rows[0].media_id, &second.rows[0].media_id];
    assert!(representatives.contains(&&latest) && representatives.contains(&&other));
    assert!(second.next.is_none());
    let saved_ahri = HistoryQuery {
        limit: 10,
        champion: Some("Ahri".into()),
        saved_only: Some(true),
        ..Default::default()
    };
    let saved_before_results = index.history(&saved_ahri, 2).unwrap();
    assert_eq!(saved_before_results.rows.len(), 1);
    assert_eq!(saved_before_results.rows[0].media_id, latest);

    for (bundle, media, kills) in [("100", &f.source.media_id, 7), ("101", &middle, 19)] {
        let mut result = library::tests::result_fixture(media);
        result.local_player.as_mut().unwrap().stats.kills = Some(kills);
        fs::write(
            root.join("games")
                .join(bundle)
                .join(chronobreak_league_data::RESULT_FILE),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        let part = catalog
            .iter()
            .find(|part| part.media_id.as_ref() == Some(media))
            .unwrap();
        index
            .history_recording(root, part, false, &|| Ok(()))
            .unwrap();
    }
    f.ready(&mut index);
    let first_after_result = index.history(&query, 2).unwrap();
    assert_eq!(
        serde_json::to_value(first_after_result.next).unwrap(),
        serde_json::to_value(Some(next)).unwrap()
    );
    assert!(index.history(&saved_ahri, 2).unwrap().rows.is_empty());

    for query in [
        HistoryQuery {
            limit: 10,
            saved_only: Some(true),
            ..Default::default()
        },
        HistoryQuery {
            limit: 10,
            event_kind: Some("ChampionKill".into()),
            ..Default::default()
        },
    ] {
        let matches = index.history(&query, 2).unwrap();
        assert_eq!(matches.rows.len(), 1);
        assert_eq!(matches.rows[0].media_id, middle);
        assert_eq!(matches.rows[0].stats.kills, Some(19));
        assert!(matches!(
            matches.rows[0].stats.source,
            StatsSource::FinalEog
        ));
    }
    let video = include_bytes!("../../testdata/recording.mp4");
    fs::create_dir(root.join("clips")).unwrap();
    fs::write(root.join("clips/104_1.mp4"), video).unwrap();
    let roots =
        std::sync::Arc::new(crate::playback_server::MediaRoots::new(root.to_owned()).unwrap());
    let (logical, approved) = roots.output_pair();
    let snapshot =
        library::refresh_library_snapshot(&logical, &approved, "", "grouped".into(), || Ok(()))
            .unwrap();
    assert!(snapshot.archive_error.is_none());
    assert!(!root.join("games/104").exists());
    let origin = crate::playback_server::start(
        crate::playback_policy::BoundServer::bind().unwrap(),
        roots,
        Default::default(),
        root.join("ddragon"),
        #[cfg(feature = "replay-benchmark")]
        None,
        false,
    )
    .await
    .unwrap();
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .unwrap();
    let mut urls = vec![format!("{origin}/clips/104_1.mp4")];
    for (bundle, media) in [
        ("100", &f.source.media_id),
        ("101", &middle),
        ("102", &latest),
    ] {
        let descriptor =
            library::replay_descriptor(root, &origin, bundle, "grouped".into()).unwrap();
        assert_eq!(&descriptor.media_timeline.media_id, media);
        assert!(
            descriptor
                .video_url
                .ends_with(&format!("/games/{bundle}/video.mp4"))
        );
        urls.push(descriptor.video_url);
    }
    for url in urls {
        let response = client
            .get(url)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        assert_eq!(response.bytes().await.unwrap().as_ref(), video);
    }
}

#[test]
fn final_nullable_facts_replace_recorded_projection_independently() {
    let f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let mut result = library::tests::result_fixture(&f.source.media_id);
    result.local_player.as_mut().unwrap().stats.deaths = None;
    result.local_player.as_mut().unwrap().stats.minions = Some(127);
    result.local_player.as_mut().unwrap().stats.jungle_minions = None;
    result.local_player.as_mut().unwrap().secondary_style = Some(8000);
    result.duration_seconds = Some(0);
    result.queue_id = Some(400);
    result.source_facts.local_player = Some(obs::FinalPlayer {
        game_id: Some(result.game_id.clone()),
        riot_id_game_name: Some("Final".into()),
        riot_id_tag_line: Some("EUW".into()),
        items: Some(vec![Some(0), None]),
        stats: Some(obs::FinalStats {
            stats: Stats {
                kills: Some(6),
                ..Default::default()
            },
            aliases: [("kills".into(), vec!["CHAMPIONS_KILLED".into()])].into(),
            ..Default::default()
        }),
        ..Default::default()
    });
    fs::write(
        f.directory.join("league_match.json"),
        serde_json::to_vec(&crate::league_match::fixture(&f.source.media_id)).unwrap(),
    )
    .unwrap();
    fs::write(
        f.directory.join(chronobreak_league_data::RESULT_FILE),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
    f.ready(&mut index);
    let page = index
        .history(
            &HistoryQuery {
                limit: 10,
                queue_id: Some(400),
                participant_riot_id: Some("Final#EUW".into()),
                ..Default::default()
            },
            1,
        )
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    let stats = &page.rows[0].stats;
    assert!(matches!(stats.source, StatsSource::FinalEog));
    assert_eq!(
        (stats.kills, stats.deaths, stats.assists),
        (Some(6), None, Some(9))
    );
    assert_eq!(stats.items, Some(vec![Some(0), None]));
    assert!(stats.creep_score.is_none());
    assert!(stats.duration_seconds.is_none());
    assert_eq!(stats.secondary_style, Some(8000));
    let aliases: String = index
        .db
        .query_row(
            "SELECT aliases FROM final_stats WHERE owner_key='local' AND stat='kills'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(aliases.contains("CHAMPIONS_KILLED"));
}

#[test]
fn accepted_membership_discards_removed_replaced_and_ambiguous_media() {
    let f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let mut duplicate = f.summary.clone();
    duplicate.timestamp = "101".into();
    index
        .reconcile(&[f.summary.clone(), duplicate], "duplicate")
        .unwrap();
    assert_eq!(
        index
            .history(
                &HistoryQuery {
                    limit: 10,
                    ..Default::default()
                },
                2
            )
            .unwrap()
            .indexed_recordings,
        0
    );
    index
        .reconcile(std::slice::from_ref(&f.summary), "restored")
        .unwrap();
    f.ready(&mut index);
    index.reconcile(&[], "empty").unwrap();
    assert_eq!(
        index
            .history(
                &HistoryQuery {
                    limit: 10,
                    ..Default::default()
                },
                0
            )
            .unwrap()
            .indexed_recordings,
        0
    );
}

#[test]
fn reset_isolates_reused_participant_and_event_ids() {
    let mut f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let original = f.scope();
    let calibration = f.state.snapshot().calibration.as_ref().unwrap().1.clone();
    f.source.epoch = 1;
    let mut evidence = f.evidence(300, Endpoint::GameStats, None);
    evidence.calibration = None;
    f.commit(
        &[Record::SourceReset {
            evidence,
            previous_epoch: 0,
        }],
        false,
    );
    let mut gs = f.evidence(301, Endpoint::GameStats, None);
    gs.calibration = Some(gs.id);
    let local = f.evidence(302, Endpoint::PlayerList, Some(301));
    let player = obs::RosterPlayer {
        participant: Some(obs::ParticipantId(1)),
        riot_id: Some("Player#NA".into()),
        team: Some("CHAOS".into()),
        ..Default::default()
    };
    let events = obs::events::EventTracker::new(f.source.clone());
    let prepared = events
        .prepare(
            &f.source,
            vec![obs::Event {
                source_id: Some(0),
                source_time: Some(1.into()),
                game_tick: Some(GameTick::new(1_000_000).unwrap()),
                name: Some("DragonKill".into()),
                killer: Some("Player#NA".into()),
                actors: Some(
                    [(
                        obs::events::ActorRole::Killer,
                        vec![obs::events::ActorReference {
                            participant: Some(obs::ParticipantId(1)),
                            status: obs::events::ActorStatus::Resolved,
                        }],
                    )]
                    .into(),
                ),
                ..Default::default()
            }],
        )
        .unwrap();
    let mut event_evidence = f.evidence(303, Endpoint::Events, Some(301));
    event_evidence.calibration = Some(ObservationId(301));
    f.commit(
        &[
            Record::Calibration {
                evidence: gs.clone(),
                calibration,
            },
            Record::Observation {
                observation: Box::new(obs::Observation::Success {
                    evidence: gs,
                    payload: obs::Payload::GameStats(obs::GameStats {
                        game_tick: Some(GameTick::new(1_000_000).unwrap()),
                        ..Default::default()
                    }),
                }),
            },
            Record::LocalParticipant {
                evidence: local.clone(),
                participant: obs::ParticipantId(1),
            },
            Record::RosterObservation {
                evidence: local,
                roster: obs::roster::RosterObservation {
                    full: true,
                    references: vec![obs::roster::PayloadId(1)],
                    payloads: vec![obs::roster::SharedPlayer {
                        id: obs::roster::PayloadId(1),
                        player,
                    }],
                },
            },
            Record::EventObservation {
                evidence: event_evidence,
                revisions: prepared.revisions().to_vec(),
            },
        ],
        true,
    );
    f.ready(&mut index);
    assert_eq!(index.scopes(&f.source.media_id).unwrap().len(), 2);
    let mut query = f.window();
    query.scope = original;
    assert_eq!(index.events(&query, &f.timeline()).unwrap().events.len(), 3);
    query.scope = f.scope();
    query.participant = Some(obs::ParticipantId(1));
    let result = index.events(&query, &f.timeline()).unwrap();
    assert_eq!(result.events.len(), 1);
    assert_eq!(
        result.events[0].revision.event.killer.as_deref(),
        Some("Player#NA")
    );
    assert_eq!(
        index
            .roster_at(&f.scope(), GameTick::new(2_000_000).unwrap())
            .unwrap()
            .players[0]
            .riot_id
            .as_deref(),
        Some("Player#NA")
    );
}

#[test]
fn checksum_valid_source_with_fabricated_revision_digest_is_rejected() {
    let mut f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let pending = f
        .events
        .prepare(
            &f.source,
            vec![obs::Event {
                source_id: Some(9),
                source_time: Some(3.into()),
                game_tick: Some(GameTick::new(3_000_000).unwrap()),
                name: Some("FirstBlood".into()),
                ..Default::default()
            }],
        )
        .unwrap();
    let mut revisions = pending.revisions().to_vec();
    revisions[0].fingerprint[0] ^= 1;
    f.commit(
        &[Record::EventObservation {
            evidence: f.evidence(250, Endpoint::Events, Some(4)),
            revisions,
        }],
        true,
    );
    assert!(
        index
            .recording(f.root.path(), &f.summary, false, &|| Ok(()))
            .is_err()
    );
    assert_eq!(
        index.progress(&f.source.media_id, 0).unwrap().status,
        "unavailable"
    );
}

#[test]
fn negative_queries_distinguish_observed_absence_from_future_or_unresolved_coverage() {
    let f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let mut query = f.window();
    query.end = GameTick::new(2_000_000).unwrap();
    query.kind = Some("BaronKill".into());
    assert!(matches!(
        index.events(&query, &f.timeline()).unwrap().match_state,
        MatchState::UnknownCoverage
    ));
    query.start = GameTick::new(3_000_000).unwrap();
    query.end = query.start;
    assert!(matches!(
        index.events(&query, &f.timeline()).unwrap().match_state,
        MatchState::NoObservedMatch
    ));
    query.end = GameTick::new(5_000_000).unwrap();
    assert!(matches!(
        index.events(&query, &f.timeline()).unwrap().match_state,
        MatchState::UnknownCoverage
    ));
    query.start = GameTick::new(0).unwrap();
    query.end = GameTick::new(2_000_000).unwrap();
    query.participant = Some(obs::ParticipantId(99));
    assert!(matches!(
        index.events(&query, &f.timeline()).unwrap().match_state,
        MatchState::UnknownCoverage
    ));
}

#[test]
fn catalog_is_queryable_before_import_and_missing_events_are_unknown() {
    let f = Fixture::new(1);
    let mut index = f.index();
    let page = index
        .history(
            &HistoryQuery {
                limit: 10,
                ..Default::default()
            },
            1,
        )
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].index_status, "pending");
    assert!(matches!(
        page.rows[0].stats.source,
        StatsSource::Unavailable
    ));
    f.ready(&mut index);
    index.db.execute("DELETE FROM events", []).unwrap();
    index
        .db
        .execute("DELETE FROM evidence WHERE endpoint='events'", [])
        .unwrap();
    index.refresh_history_summary(&f.source.media_id).unwrap();
    let page = index
        .history(
            &HistoryQuery {
                limit: 10,
                event_kind: Some("DragonKill".into()),
                ..Default::default()
            },
            1,
        )
        .unwrap();
    assert!(page.rows.is_empty());
    assert_eq!(page.unknown_recordings, 1);
    let page = index
        .history(
            &HistoryQuery {
                limit: 10,
                mode: Some("CLASSIC".into()),
                search: Some("synd".into()),
                ..Default::default()
            },
            1,
        )
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(
        index
            .history(
                &HistoryQuery {
                    limit: 10,
                    saved_only: Some(true),
                    ..Default::default()
                },
                1
            )
            .unwrap()
            .rows
            .is_empty()
    );
}

#[test]
fn ui_refresh_keeps_unchanged_prepared_history_current() {
    let f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let query = HistoryQuery {
        limit: 10,
        event_kind: Some("ChampionKill".into()),
        ..Default::default()
    };
    assert_eq!(index.history(&query, 1).unwrap().rows.len(), 1);
    index
        .reconcile(std::slice::from_ref(&f.summary), "refreshed")
        .unwrap();
    let pending = index.history(&query, 1).unwrap();
    assert_eq!(pending.rows.len(), 1);
    assert_eq!(pending.unknown_recordings, 1);
    let basic = index
        .history(
            &HistoryQuery {
                limit: 10,
                ..Default::default()
            },
            1,
        )
        .unwrap();
    assert_eq!(basic.rows[0].index_status, "ready");
    let progress = index
        .recording(f.root.path(), &f.summary, false, &|| Ok(()))
        .unwrap();
    assert_eq!(
        progress.batches, 0,
        "unchanged refresh must not parse the journal again"
    );
    assert_eq!(index.history(&query, 1).unwrap().rows.len(), 1);
    index.reconcile(&[], "empty").unwrap();
    let empty = index.history(&query, 0).unwrap();
    assert_eq!((empty.indexed_recordings, empty.unknown_recordings), (0, 0));
}

#[test]
fn history_rejects_oversized_rows_before_deserialization() {
    let f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    index
        .db
        .execute(
            "UPDATE recordings SET summary_json=?1",
            [" ".repeat(MAX_RESPONSE_BYTES + 1)],
        )
        .unwrap();
    let error = index
        .history(
            &HistoryQuery {
                limit: 10,
                ..Default::default()
            },
            1,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("history input capacity"),
        "{error}"
    );
}

#[test]
fn interrupted_unit_keeps_the_previous_published_prefix_and_resumes() {
    let f = Fixture::new(IMPORT_BATCHES + 3);
    let mut index = f.index();
    let first = index
        .recording(f.root.path(), &f.summary, false, &|| Ok(()))
        .unwrap();
    assert_eq!(first.batches as usize, IMPORT_BATCHES);
    let prefix = index
        .db
        .query_row(
            "SELECT position_json,checkpoint_json FROM recordings WHERE media_id=?1",
            [f.source.media_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .unwrap();
    let checks = std::cell::Cell::new(0);
    let check = || {
        checks.set(checks.get() + 1);
        // Readers see the previous complete unit, including while a later
        // source commit has already been applied inside the write transaction.
        let reader = Index::reader(f.cache.path(), f.root.path())?;
        let count: i64 = reader
            .db
            .query_row("SELECT count(*) FROM commits", [], |row| row.get(0))?;
        assert_eq!(count, IMPORT_BATCHES as i64);
        ensure!(
            checks.get() < 4,
            "interrupt second commit before publication"
        );
        Ok(())
    };
    assert!(
        index
            .import(
                f.root.path(),
                &f.directory,
                &f.timeline(),
                &source_stamp(&f.directory).unwrap(),
                &check
            )
            .is_err()
    );
    assert_eq!(checks.get(), 4);
    let after = index
        .db
        .query_row(
            "SELECT position_json,checkpoint_json FROM recordings WHERE media_id=?1",
            [f.source.media_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .unwrap();
    assert_eq!(prefix, after);
    assert_eq!(
        index
            .db
            .query_row("SELECT count(*) FROM commits", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        IMPORT_BATCHES as i64
    );
    f.ready(&mut index);
    assert_eq!(
        index
            .db
            .query_row("SELECT count(*) FROM commits", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        (IMPORT_BATCHES + 3) as i64
    );
}

#[test]
fn canceled_import_can_resume_on_a_new_accepted_snapshot() {
    let f = Fixture::new(5);
    let mut index = f.index();
    let calls = std::cell::Cell::new(0);
    let check = || {
        calls.set(calls.get() + 1);
        ensure!(calls.get() < 3, "superseded test job");
        Ok(())
    };
    assert!(
        index
            .recording(f.root.path(), &f.summary, false, &check)
            .is_err()
    );
    assert_eq!(
        index.progress(&f.source.media_id, 0).unwrap().status,
        "pending"
    );
    let published: u64 = index
        .db
        .query_row("SELECT COUNT(*) FROM source_coverage", [], |row| {
            unsigned(row, 0)
        })
        .unwrap();
    assert_eq!(published, 0);
    index
        .reconcile(std::slice::from_ref(&f.summary), "resumed")
        .unwrap();
    f.ready(&mut index);
    assert!(!index.scopes(&f.source.media_id).unwrap().is_empty());
}

#[test]
fn reader_snapshot_and_source_stamp_reject_stale_changes() {
    let mut f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let reader = Index::reader(f.cache.path(), f.root.path()).unwrap();
    reader
        .require_source(f.root.path(), "100", &f.source.media_id)
        .unwrap();
    index.reconcile(&[], "new-membership").unwrap();
    reader
        .require_catalog(&catalog_key(std::slice::from_ref(&f.summary)).unwrap())
        .unwrap();
    assert_eq!(
        reader
            .history(
                &HistoryQuery {
                    limit: 10,
                    ..Default::default()
                },
                1
            )
            .unwrap()
            .rows
            .len(),
        1
    );
    f.game_stats(900, 9_000_000, true);
    assert!(
        reader
            .require_source(f.root.path(), "100", &f.source.media_id)
            .is_err()
    );
}

#[test]
fn replay_windows_follow_each_ingestion_calibration_and_keep_roster_age() {
    use chronobreak_replay_time::{ReplayTick, SignedReplayTick};
    let mut f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let mut query = ReplayWindowQuery {
        scope: f.scope(),
        start: ReplayTick::new(0).unwrap(),
        end: ReplayTick::new(100_000_000).unwrap(),
        at: ReplayTick::new(144_000_000).unwrap(),
        after: None,
        limit: 1,
    };
    let first = index.replay_window(&query, &f.timeline()).unwrap();
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].revision.event.source_id, Some(1));
    assert_eq!(first.roster.as_ref().unwrap().game_tick.get(), 3_000_000);
    assert_eq!(first.roster.unwrap().age_replay_ticks.get(), 48_000_000);
    query.after = first.next;
    let second = index.replay_window(&query, &f.timeline()).unwrap();
    assert_eq!(second.events[0].revision.event.source_id, Some(2));
    let MappedReplayTime::InsideMedia { replay_tick, .. } = second.events[0].mapped_replay_time
    else {
        panic!("fixture event should map");
    };
    let ending_at_event = index
        .clip_events(
            &f.timeline(),
            ReplayTick::new(replay_tick.get() - 1).unwrap(),
            replay_tick,
            &|| Ok(()),
        )
        .unwrap();
    assert!(ending_at_event.events.is_empty());
    let beginning_at_event = index
        .clip_events(
            &f.timeline(),
            replay_tick,
            ReplayTick::new(replay_tick.get() + 1).unwrap(),
            &|| Ok(()),
        )
        .unwrap();
    assert!(
        beginning_at_event
            .events
            .iter()
            .any(|event| event.event.source_id == Some(2))
    );
    let mut calibration = f.state.snapshot().calibration.as_ref().unwrap().1.clone();
    calibration.replay_tick_at_game_zero = Some(SignedReplayTick::new(48_000_000).unwrap());
    let mut e = f.evidence(400, Endpoint::GameStats, None);
    e.calibration = Some(e.id);
    f.commit(
        &[Record::Calibration {
            evidence: e.clone(),
            calibration,
        }],
        false,
    );
    let pending = f
        .events
        .prepare(
            &f.source,
            vec![obs::Event {
                source_id: Some(8),
                source_time: Some(1.into()),
                game_tick: Some(GameTick::new(1_000_000).unwrap()),
                name: Some("DragonKill".into()),
                ..Default::default()
            }],
        )
        .unwrap();
    let mut event_evidence = f.evidence(401, Endpoint::Events, Some(4));
    event_evidence.calibration = Some(ObservationId(400));
    f.commit(
        &[Record::EventObservation {
            evidence: event_evidence,
            revisions: pending.revisions().to_vec(),
        }],
        true,
    );
    f.ready(&mut index);
    query.start = ReplayTick::new(90_000_000).unwrap();
    query.end = ReplayTick::new(100_000_000).unwrap();
    query.after = None;
    query.limit = 100;
    let window = index.replay_window(&query, &f.timeline()).unwrap();
    assert_eq!(window.events.len(), 1);
    assert_eq!(window.events[0].revision.event.source_id, Some(8));
    assert!(
        matches!(window.events[0].mapped_replay_time,chronobreak_replay_time::MappedReplayTime::InsideMedia {replay_tick,..} if replay_tick.get()==96_000_000)
    );
    let plan:String=index.db.query_row("EXPLAIN QUERY PLAN SELECT source_id FROM events WHERE media_id=?1 AND run_id=?2 AND epoch=0 AND replay_tick BETWEEN 0 AND 100000000 ORDER BY replay_tick,source_id",params![f.source.media_id.as_str(),f.source.run_id.as_str()],|r|r.get(3)).unwrap();
    assert!(plan.contains("events_replay_window"), "{plan}");
}

#[test]
fn replay_identity_context_omits_conflicting_descriptors_and_stays_inside_its_source() {
    use chronobreak_replay_time::ReplayTick;
    let f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let query = ReplayWindowQuery {
        scope: f.scope(),
        start: ReplayTick::new(0).unwrap(),
        end: ReplayTick::new(100_000_000).unwrap(),
        at: ReplayTick::new(0).unwrap(),
        after: None,
        limit: 512,
    };
    let read = |index: &Index| index.replay_window(&query, &f.timeline()).unwrap();
    let initial = read(&index);
    assert!(initial.roster.is_none());
    assert!(!initial.identities.is_empty());
    let participant = initial.identities[0].participant;
    let team = initial.identities[0].team.clone();
    let params = params![
        f.source.media_id.as_str(),
        f.source.run_id.as_str(),
        sql_u64(f.source.epoch).unwrap()
    ];
    // A descriptor in another epoch must not affect this source's context.
    index.db.execute(
        "INSERT INTO roster_payloads SELECT media_id,run_id,epoch+1,payload_id,json_set(data,'$.team','different')
        FROM roster_payloads WHERE media_id=?1 AND run_id=?2 AND epoch=?3", params,
    ).unwrap();
    assert_eq!(
        read(&index)
            .identities
            .iter()
            .find(|t| t.participant == participant)
            .unwrap()
            .team,
        team
    );
    let conflicting = serde_json::json!({"participant":participant,"team":"different", "champion_name":"different", "riot_id":"different#ID"}).to_string();
    index
        .db
        .execute(
            "INSERT INTO roster_payloads VALUES (?1,?2,?3,999999,?4)",
            params![
                f.source.media_id.as_str(),
                f.source.run_id.as_str(),
                sql_u64(f.source.epoch).unwrap(),
                conflicting
            ],
        )
        .unwrap();
    let conflicted = read(&index);
    let identity = conflicted
        .identities
        .iter()
        .find(|t| t.participant == participant)
        .unwrap();
    assert!(identity.team.is_none());
    assert!(identity.champion_name.is_none());
    assert!(identity.riot_id.is_none());
    index.db.execute("UPDATE roster_payloads SET data=json_remove(data,'$.team') WHERE media_id=?1 AND run_id=?2 AND epoch=?3", params).unwrap();
    assert!(
        read(&index)
            .identities
            .iter()
            .all(|identity| identity.team.is_none())
    );
}
