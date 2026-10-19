//! The production data path with a simulated Live transport and fixture media.
use super::*;
use chronobreak_replay_time::{MappedReplayTime, REPLAY_TICKS_PER_SECOND, ReplayTick};
use league_replay_recorder::poller::{
    acquisition::{self, Source},
    persistence::{CHECKPOINT_FILE, JOURNAL_FILE, WriterSession},
};
use obs::events::{ActorRole, ActorStatus, Ingestion};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{sync::watch, time::Instant};

struct Responses {
    origin: Instant,
    fixture: Value,
}
impl Source for Responses {
    async fn fetch(&self, endpoint: Endpoint) -> std::result::Result<Vec<u8>, obs::GapReason> {
        let elapsed = self.origin.elapsed();
        let mut body = self.fixture[match endpoint {
            Endpoint::GameStats => "gs",
            Endpoint::ActivePlayer => "ap",
            Endpoint::PlayerList => "pl",
            Endpoint::Events => "ev",
            _ => unreachable!(),
        }]
        .clone();
        if endpoint == Endpoint::GameStats {
            body["gameTime"] = json!(120.0 + elapsed.as_secs_f64());
        }
        if endpoint == Endpoint::Events {
            if (2..3).contains(&elapsed.as_secs()) {
                return Err(obs::GapReason::Http);
            }
            // Existing cumulative rows precede calibration. A later event must
            // use its own acquired calibration and exact participant reference.
            if elapsed >= Duration::from_secs(4) {
                body["Events"].as_array_mut().unwrap().push(json!({
                    "EventID": 5, "EventName": "DragonKill", "EventTime": 124.0,
                    "KillerName": "Fixture#TEST", "Assisters": [],
                    "DragonType": "Fire", "Stolen": elapsed >= Duration::from_secs(6)
                }));
            }
        }
        Ok(serde_json::to_vec(&body).unwrap())
    }
}

#[tokio::test]
async fn simulated_live_responses_reach_durable_sql_and_application_queries() {
    let root = tempdir().unwrap();
    let cache = tempdir().unwrap();
    let directory = library::tests::write_game(root.path(), "100", "2026-10-19T10:00:00Z", false);
    let timeline = library::replay_descriptor(root.path(), "", "100", String::new())
        .unwrap()
        .media_timeline;
    // Replace only this temporary bundle's hand-built telemetry. All subsequent
    // records, sidecars and commit positions come from the production recorder.
    for name in [JOURNAL_FILE, CHECKPOINT_FILE, manifest::FILE] {
        fs::remove_file(directory.join(name)).unwrap();
    }
    let source = obs::SourceEpoch {
        media_id: timeline.media_id.clone(),
        run_id: MediaId::parse("123e4567-e89b-42d3-a456-426614174099").unwrap(),
        epoch: 0,
    };
    let (writer, mut queue) = WriterSession::start_fixture(&directory, source.clone())
        .await
        .unwrap();
    // Pause only acquisition time. Writer startup/drain keep real deadlines and
    // the blocking disk worker uses its unchanged production clock and policy.
    tokio::time::pause();
    let origin = Instant::now();
    let responses = Arc::new(Responses {
        origin,
        fixture: serde_json::from_str(include_str!(
            "../../../../league-data/tests/fixtures/live-endpoints.json"
        ))
        .unwrap(),
    });
    let (cancel, cancelled) = watch::channel(false);
    let (_identity, identity_open) = watch::channel(true);
    let acquired_source = source.clone();
    let acquisition = tokio::spawn(async move {
        acquisition::run(
            responses,
            &mut queue,
            acquired_source,
            origin,
            cancelled,
            identity_open,
        )
        .await
    });
    for _ in 0..33 {
        // Explicit advancement works while the real blocking writer is alive.
        // Yield through request dispatch and completion before advancing again.
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_millis(250)).await;
    }
    cancel.send(true).unwrap();
    let acquisition = acquisition.await.unwrap();
    tokio::time::resume();
    let report = writer.stop().await;
    assert!(!report.failed, "{:?}", report.failure);
    assert_eq!(report.rejected_batches, 0);
    assert_eq!(
        (report.checkpoint_failures, report.manifest_failures),
        (0, 0)
    );
    assert_eq!(acquisition.unreported_gaps, 0);
    assert_eq!(acquisition.unreported_identity_rounds, 0);
    assert_eq!(acquisition.unreported_source_resets, 0);
    assert!(acquisition.endpoints[&Endpoint::Events].failures > 0);
    let mut recovered_rows = 0;
    let recovered = journal::recover(
        std::io::BufReader::new(fs::File::open(directory.join(JOURNAL_FILE)).unwrap()),
        |rows, _| {
            if recovered_rows == 0 {
                assert!(matches!(
                    &rows[0].record,
                    Record::Boundary { evidence, active: true }
                        if evidence.id == ObservationId(0)
                ));
            }
            recovered_rows += rows.len() as u64;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(recovered_rows, report.committed_records);
    assert_eq!(recovered.tail_bytes, 0);

    let summary = library::list_games(root.path()).unwrap().remove(0);
    let mut index = Index::open(cache.path(), root.path()).unwrap();
    index
        .reconcile(std::slice::from_ref(&summary), "accepted")
        .unwrap();
    assert!((0..16).any(|_| {
        index
            .recording(root.path(), &summary, false, &|| Ok(()))
            .unwrap()
            .status
            == "ready"
    }));
    let scopes = index.scopes(&source.media_id).unwrap();
    assert_eq!(scopes.len(), 1);
    assert_eq!(scopes[0].source, source);
    let local = scopes[0].local_participant.unwrap();
    let scope = ScopeQuery {
        media_id: source.media_id.clone(),
        run_id: source.run_id.clone(),
        epoch: 0,
    };
    let roster = index
        .roster_at(&scope, GameTick::new(128_000_000).unwrap())
        .unwrap();
    let player = roster
        .players
        .iter()
        .find(|p| p.participant == Some(local))
        .unwrap();
    assert_eq!(player.riot_id.as_deref(), Some("Fixture#TEST"));
    assert_eq!(player.champion_name.as_deref(), Some("Ahri"));
    assert_eq!(player.scores.as_ref().unwrap().kills, Some(0));
    assert_eq!(player.scores.as_ref().unwrap().ward_score, Some(0.5));
    let dummy = roster
        .players
        .iter()
        .find(|p| p.summoner_name.as_deref() == Some("PracticeDummy"))
        .unwrap();
    assert!(dummy.items.is_none());
    assert_eq!(dummy.issues["items"], obs::FieldIssue::SourceError);
    assert_eq!(dummy.scores.as_ref().unwrap().kills, None);

    let query = WindowQuery {
        scope: scope.clone(),
        start: GameTick::new(120_000_000).unwrap(),
        end: GameTick::new(128_000_000).unwrap(),
        after: None,
        limit: 100,
        kind: Some("DragonKill".into()),
        stolen: Some(true),
        minimum_kill_streak: None,
        participant: Some(local),
        role: Some(ActorRole::Killer),
    };
    let events = index.events(&query, &timeline).unwrap();
    assert_eq!(events.events.len(), 1);
    let event = &events.events[0];
    assert_eq!(event.revision.event.source_id, Some(5));
    assert_eq!(event.revision.ingestion, Ingestion::Correction);
    assert_eq!(event.evidence.source, source);
    assert!(event.evidence.calibration.is_some());
    let actor = &event.revision.event.actors.as_ref().unwrap()[&ActorRole::Killer][0];
    assert_eq!(actor.participant, Some(local));
    assert_eq!(actor.status, ActorStatus::Resolved);
    assert!(
        matches!(event.mapped_replay_time, MappedReplayTime::InsideMedia { replay_tick, .. } if replay_tick.get() == 4 * REPLAY_TICKS_PER_SECOND)
    );
    assert!(events.coverage.gap_attempts > 0);
    assert!(matches!(events.match_state, MatchState::ObservedMatch));
    assert_whole_coverage(&index, &scope);

    let replay_query = replay::ReplayWindowQuery {
        scope,
        start: ReplayTick::new(0).unwrap(),
        end: timeline.video.replay_end,
        at: ReplayTick::new(8 * REPLAY_TICKS_PER_SECOND).unwrap(),
        after: None,
        limit: 100,
    };
    let replay = index.replay_window(&replay_query, &timeline).unwrap();
    assert!(
        replay
            .events
            .iter()
            .any(|e| e.revision.event.source_id == Some(5))
    );
    assert!(replay.roster.is_some());
    assert!(replay.coverage.unmapped_observations > 0);
    let history_query = HistoryQuery {
        limit: 50,
        participant_riot_id: Some("Fixture#TEST".into()),
        event_kind: Some("DragonKill".into()),
        stolen: Some(true),
        role: Some(ActorRole::Killer),
        ..Default::default()
    };
    let history = index.history(&history_query, 1).unwrap();
    assert_eq!(history.rows.len(), 1);
    assert!(matches!(
        history.rows[0].stats.source,
        StatsSource::RecordedSnapshot
    ));
    assert_eq!(history.rows[0].stats.kills, Some(0));
    assert_eq!(
        index
            .recording(root.path(), &summary, false, &|| Ok(()))
            .unwrap()
            .batches,
        0
    );

    let expected = serde_json::to_value((&replay, &history)).unwrap();
    index
        .reconcile(std::slice::from_ref(&summary), "accepted")
        .unwrap();
    assert!((0..16).any(|step| {
        index
            .recording(root.path(), &summary, step == 0, &|| Ok(()))
            .unwrap()
            .status
            == "ready"
    }));
    assert_eq!(
        serde_json::to_value((
            index.replay_window(&replay_query, &timeline).unwrap(),
            index.history(&history_query, 1).unwrap()
        ))
        .unwrap(),
        expected
    );
}
