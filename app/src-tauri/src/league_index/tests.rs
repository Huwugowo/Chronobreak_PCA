use super::{query::*, *};
use obs::{Endpoint, Evidence, ObservationId, Record};
use std::io::{Cursor, Write};
use tempfile::{TempDir, tempdir};

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
            library::tests::write_game(root.path(), "100", "2026-08-05T10:00:00Z", false);
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
    let f = Fixture::new(5);
    let mut index = f.index();
    let first = index
        .recording(f.root.path(), &f.summary, false, &|| Ok(()))
        .unwrap();
    assert_eq!((first.status.as_str(), first.batches), ("pending", 4));
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
    reader.require_catalog("accepted").unwrap();
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
    let f = Fixture::new(1);
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
fn final_nullable_facts_replace_recorded_projection_independently() {
    let f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let mut result = library::tests::result_fixture(&f.source.media_id);
    result.local_player.as_mut().unwrap().stats.deaths = None;
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
fn refresh_marks_history_facts_unknown_until_each_source_is_checked() {
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
    assert!(pending.rows.is_empty());
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
    assert_eq!(basic.rows[0].index_status, "pending");
    assert!(matches!(
        basic.rows[0].stats.source,
        StatsSource::Unavailable
    ));
    let progress = index
        .recording(f.root.path(), &f.summary, false, &|| Ok(()))
        .unwrap();
    assert_eq!(
        progress.batches, 0,
        "unchanged refresh must not parse the journal again"
    );
    assert_eq!(index.history(&query, 1).unwrap().rows.len(), 1);
    index.reconcile(&[], "empty").unwrap();
    let counts: u64 = index
        .db
        .query_row("SELECT SUM(count) FROM history_counts", [], |r| {
            unsigned(r, 0)
        })
        .unwrap();
    assert_eq!(counts, 0);
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
    reader.require_catalog("accepted").unwrap();
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
    assert_eq!(
        index.replay_window(&query, &f.timeline()).unwrap().events[0]
            .revision
            .event
            .source_id,
        Some(2)
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
