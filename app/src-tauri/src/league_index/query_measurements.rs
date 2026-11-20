//! Representative local query measurements for the disposable League projection.
//!
//! This deliberately keeps the journal fixture small.  The catalog and event rows
//! are projected into SQLite after one real import, so query measurements exercise
//! the production query APIs and indexes without manufacturing ten thousand journals.
use super::super::{
    query::{GAME_TICK_ROSTER_LOOKUP_SQL, HISTORY_SQL},
    replay::REPLAY_EVENTS_SQL,
};
use super::*;
use std::time::{Duration, Instant};

const CATALOG_ROWS: usize = 10_000;
const DENSE_EVENTS: u64 = 2_048;
const WARM_SAMPLES: usize = 80;

fn synthetic_media(index: usize) -> String {
    format!("10000000-0000-4000-8000-{index:012}")
}

fn source_event_row(index: &Index) -> (String, Vec<u8>, Vec<u8>, i64) {
    index.db.query_row(
        "SELECT r.data,r.fingerprint,r.response_fingerprint,r.observation_id FROM events e
         JOIN event_revisions r ON r.media_id=e.media_id AND r.sequence=e.sequence AND r.ordinal=e.ordinal
         LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).unwrap()
}

fn revised_event_json(template: &str, source_id: u64, game_tick: i64) -> String {
    let mut value: serde_json::Value = serde_json::from_str(template).unwrap();
    value["event"]["source_id"] = serde_json::json!(source_id);
    value["event"]["game_tick"] = serde_json::json!(game_tick.to_string());
    serde_json::to_string(&value).unwrap()
}

/// Populate a representative catalog in the existing index.  Every catalog entry
/// has a semantic event/participant predicate, while one real imported source gets
/// a denser event corpus for selected replay-window measurements.
fn seed_measurement_projection(fixture: &Fixture, index: &mut Index) {
    let (event_json, fingerprint, response_fingerprint, observation_id) = source_event_row(index);
    let base = fixture.summary.clone();
    let source = &fixture.source;
    let tx = index.db.transaction().unwrap();
    {
        let mut recording = tx.prepare_cached(
            "INSERT INTO recordings(media_id,bundle,recorded_at,champion,mode,saved,outcome,summary_json,telemetry_status,match_group,queue_id,facts_current)
             VALUES (?1,?2,?3,?4,?5,0,'unknown',?6,'ready',?7,420,1)",
        ).unwrap();
        let mut scope = tx
            .prepare_cached(
                "INSERT INTO scopes(media_id,run_id,epoch,local_participant) VALUES (?1,?2,0,1)",
            )
            .unwrap();
        let mut participant = tx.prepare_cached("INSERT INTO participants(media_id,run_id,epoch,participant_id,riot_id,champion,team) VALUES (?1,?2,0,1,'Measure#NA','Syndra','ORDER')").unwrap();
        let mut event = tx.prepare_cached("INSERT INTO events(media_id,run_id,epoch,source_id,sequence,ordinal,game_tick,kind,stolen,kill_streak,ingestion,replay_tick) VALUES (?1,?2,0,1,1,0,3000000,'DragonKill',1,1,'accepted',48000000)").unwrap();
        let mut actor = tx.prepare_cached("INSERT INTO event_actors(media_id,run_id,epoch,sequence,revision_ordinal,role,ordinal,participant_id,status) VALUES (?1,?2,0,1,0,'killer',0,1,'resolved')").unwrap();
        for row in 0..(CATALOG_ROWS - 1) {
            let media = synthetic_media(row);
            let mut summary = base.clone();
            summary.media_id = Some(MediaId::parse(&media).unwrap());
            summary.timestamp = format!("m{row:05}");
            summary.recorded_at = format!("2026-08-{:02}T{:02}:00:00Z", row % 28 + 1, row % 24);
            summary.match_group = Some(format!("{row:064x}"));
            recording
                .execute(params![
                    media,
                    summary.timestamp,
                    summary.recorded_at,
                    summary.champion,
                    summary.game_mode,
                    serde_json::to_string(&summary).unwrap(),
                    summary.match_group,
                ])
                .unwrap();
            scope
                .execute(params![media, source.run_id.as_str()])
                .unwrap();
            participant
                .execute(params![media, source.run_id.as_str()])
                .unwrap();
            event
                .execute(params![media, source.run_id.as_str()])
                .unwrap();
            actor
                .execute(params![media, source.run_id.as_str()])
                .unwrap();
        }
    }
    // The source is already imported through its real journal.  Add a dense but
    // bounded projected corpus so replay and keyset reads decode real revisions.
    let mut revision = tx.prepare_cached("INSERT INTO event_revisions(media_id,run_id,epoch,sequence,ordinal,source_id,fingerprint,response_fingerprint,observation_id,game_tick,kind,stolen,kill_streak,ingestion,data) VALUES (?1,?2,0,?3,0,?4,?5,?6,?7,?8,'DragonKill',0,1,'accepted',?9)").unwrap();
    let mut event = tx.prepare_cached("INSERT INTO events(media_id,run_id,epoch,source_id,sequence,ordinal,game_tick,kind,stolen,kill_streak,ingestion,replay_tick) VALUES (?1,?2,0,?3,?4,0,?5,'DragonKill',0,1,'accepted',?6)").unwrap();
    // Reuse the committed observation and calibration JSON, both of which already
    // belong to this exact source epoch.  This measures decoding and query work.
    let calibration_json: String = tx
        .query_row(
            "SELECT c.data FROM evidence e JOIN calibrations c ON c.media_id=e.media_id AND c.run_id=e.run_id AND c.epoch=e.epoch AND c.observation_id=e.calibration_id WHERE e.media_id=?1 AND e.observation_id=?2",
            params![source.media_id.as_str(), observation_id],
            |row| row.get(0),
        )
        .unwrap();
    let calibration = serde_json::from_str(&calibration_json).unwrap();
    for offset in 0..DENSE_EVENTS {
        let source_id = 10_000 + offset;
        let sequence = 20_000 + offset;
        let game_tick = 1_000_000 + i64::try_from(offset % 180).unwrap() * 50_000;
        let replay_tick = match library::map_game_tick(
            Some(&calibration),
            GameTick::new(game_tick).unwrap(),
            &fixture.timeline(),
        )
        .unwrap()
        {
            chronobreak_replay_time::MappedReplayTime::InsideMedia { replay_tick, .. } => {
                replay_tick.get()
            }
            _ => continue,
        };
        revision
            .execute(params![
                source.media_id.as_str(),
                source.run_id.as_str(),
                sql_u64(sequence).unwrap(),
                sql_u64(source_id).unwrap(),
                fingerprint,
                response_fingerprint,
                observation_id,
                game_tick,
                revised_event_json(&event_json, source_id, game_tick)
            ])
            .unwrap();
        event
            .execute(params![
                source.media_id.as_str(),
                source.run_id.as_str(),
                sql_u64(source_id).unwrap(),
                sql_u64(sequence).unwrap(),
                game_tick,
                sql_u64(replay_tick).unwrap()
            ])
            .unwrap();
    }
    drop(event);
    drop(revision);
    super::super::coverage::refresh(&tx, &fixture.scope()).unwrap();
    // Synthetic rows stand in for sources already checked in this catalog.
    // Preserve the same summary-count maintenance used by the real importer.
    tx.execute(
        "UPDATE recordings SET facts_current=1,history_flags=0 WHERE media_id!=?1",
        [source.media_id.as_str()],
    )
    .unwrap();
    tx.commit().unwrap();
}

fn percentile(sorted: &[Duration], hundredth: usize) -> Duration {
    let index = (sorted.len() - 1) * hundredth / 100;
    sorted[index]
}

fn measure(label: &str, mut query: impl FnMut()) {
    // Each call uses the complete public query implementation.  A first call warms
    // SQLite's bounded page cache, then timing begins.
    query();
    let mut samples = Vec::with_capacity(WARM_SAMPLES);
    for _ in 0..WARM_SAMPLES {
        let started = Instant::now();
        query();
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    eprintln!(
        "league-index measurement {label}: p50={:?} p95={:?} p99={:?} (provisional: history <=50ms, replay/roster <=25ms)",
        percentile(&samples, 50),
        percentile(&samples, 95),
        percentile(&samples, 99)
    );
}

fn plan(index: &Index, label: &str, sql: &str, bindings: &[&dyn rusqlite::ToSql]) {
    let mut statement = index
        .db
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap();
    let details: Vec<String> = statement
        .query_map(bindings, |row| row.get(3))
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    eprintln!("league-index plan {label}: {}", details.join(" | "));
}

#[test]
fn measurement_fixture_keeps_history_keyset_and_exact_actor_identity() {
    let f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    let (template, ..) = source_event_row(&index);
    let dense_revision: obs::events::EventRevision =
        serde_json::from_str(&revised_event_json(&template, 1000, 1_000_000)).unwrap();
    assert_eq!(dense_revision.event.game_tick.unwrap().get(), 1_000_000);
    let summary = f.summary.clone();
    let media = "10000000-0000-4000-8000-999999999999";
    let mut semantic = summary.clone();
    semantic.media_id = Some(MediaId::parse(media).unwrap());
    semantic.timestamp = "measurement-keyset".into();
    semantic.recorded_at = "2026-08-28T10:00:00Z".into();
    semantic.match_group = Some("0".repeat(64));
    index.db.execute("INSERT INTO recordings(media_id,bundle,recorded_at,champion,mode,saved,outcome,summary_json,telemetry_status,facts_current,history_flags,match_group) VALUES (?1,'measurement-keyset',?2,?3,?4,0,'unknown',?5,'ready',1,0,?6)", params![media,semantic.recorded_at,semantic.champion,semantic.game_mode,serde_json::to_string(&semantic).unwrap(),semantic.match_group]).unwrap();
    index
        .db
        .execute(
            "INSERT INTO scopes VALUES (?1,?2,0,1)",
            params![media, f.source.run_id.as_str()],
        )
        .unwrap();
    index
        .db
        .execute(
            "INSERT INTO participants VALUES (?1,?2,0,1,'Exact#NA','Syndra','ORDER')",
            params![media, f.source.run_id.as_str()],
        )
        .unwrap();
    index.db.execute("INSERT INTO events(media_id,run_id,epoch,source_id,sequence,ordinal,game_tick,kind,stolen,kill_streak,ingestion,replay_tick) VALUES (?1,?2,0,1,1,0,3000000,'DragonKill',0,1,'accepted',48000000)", params![media,f.source.run_id.as_str()]).unwrap();
    index
        .db
        .execute(
            "INSERT INTO event_actors VALUES (?1,?2,0,1,0,'killer',0,1,'resolved')",
            params![media, f.source.run_id.as_str()],
        )
        .unwrap();
    let first = index
        .history(
            &HistoryQuery {
                limit: 1,
                participant_riot_id: Some("Exact#NA".into()),
                event_kind: Some("DragonKill".into()),
                role: Some(obs::events::ActorRole::Killer),
                ..Default::default()
            },
            2,
        )
        .unwrap();
    assert_eq!(first.rows.len(), 1);
    assert_eq!(first.rows[0].media_id.as_str(), media);
    assert!(
        first.next.is_none(),
        "exact actor result must not leak another source epoch into keyset pagination"
    );
}

#[test]
#[ignore = "run the representative local SQL measurement in release mode"]
fn representative_local_sql_query_measurements() {
    let f = Fixture::new(1);
    let mut index = f.index();
    f.ready(&mut index);
    seed_measurement_projection(&f, &mut index);

    let default_history = HistoryQuery {
        limit: 50,
        ..Default::default()
    };
    let semantic_history = HistoryQuery {
        limit: 50,
        participant_riot_id: Some("Measure#NA".into()),
        event_kind: Some("DragonKill".into()),
        role: Some(obs::events::ActorRole::Killer),
        ..Default::default()
    };
    let roster_tick = GameTick::new(5_000_000).unwrap();
    let replay = ReplayWindowQuery {
        scope: f.scope(),
        start: chronobreak_replay_time::ReplayTick::new(16_000_000).unwrap(),
        end: chronobreak_replay_time::ReplayTick::new(64_000_000).unwrap(),
        at: chronobreak_replay_time::ReplayTick::new(64_000_000).unwrap(),
        after: None,
        limit: 50,
    };

    let no_after: Option<&str> = None;
    let no_string: Option<String> = None;
    let no_i64: Option<i64> = None;
    let no_bool: Option<bool> = None;
    let history_limit = 51u32;
    plan(
        &index,
        "constituent/history/default",
        HISTORY_SQL,
        &[
            &no_after,
            &no_after,
            &no_i64,
            &no_string,
            &no_string,
            &no_string,
            &false,
            &no_string,
            &no_bool,
            &no_i64,
            &no_string,
            &history_limit,
            &no_string,
            &false,
            &no_string,
            &no_string,
            &no_string,
            &no_string,
            &no_string,
            &no_i64,
            &no_i64,
            &no_string,
            &no_i64,
        ],
    );
    let semantic_role = "killer";
    plan(
        &index,
        "constituent/history/semantic",
        HISTORY_SQL,
        &[
            &no_after,
            &no_after,
            &no_i64,
            &no_string,
            &no_string,
            &semantic_history.participant_riot_id,
            &true,
            &semantic_history.event_kind,
            &no_bool,
            &no_i64,
            &semantic_role,
            &history_limit,
            &no_string,
            &false,
            &no_string,
            &no_string,
            &no_string,
            &no_string,
            &no_string,
            &no_i64,
            &no_i64,
            &no_string,
            &no_i64,
        ],
    );
    plan(
        &index,
        "constituent/replay/events",
        REPLAY_EVENTS_SQL,
        &[
            &f.source.media_id.as_str(),
            &f.source.run_id.as_str(),
            &sql_u64(replay.scope.epoch).unwrap(),
            &sql_u64(replay.start.get()).unwrap(),
            &sql_u64(replay.end.get()).unwrap(),
            &no_i64,
            &no_i64,
            &(u32::from(replay.limit) + 1),
        ],
    );
    plan(
        &index,
        "constituent/roster/game-tick-latest",
        GAME_TICK_ROSTER_LOOKUP_SQL,
        &[
            &f.source.media_id.as_str(),
            &f.source.run_id.as_str(),
            &sql_u64(f.scope().epoch).unwrap(),
            &roster_tick.get(),
        ],
    );

    measure("history/default/50-row", || {
        assert_eq!(
            index
                .history(&default_history, CATALOG_ROWS)
                .unwrap()
                .rows
                .len(),
            50
        );
    });
    let snapshot = seed_clip_count_measurement(&f, &mut index);
    measure("history/clip-counts/50-row", || {
        let mut page = index.history(&default_history, CATALOG_ROWS).unwrap();
        index.attach_clip_counts(&snapshot, &mut page).unwrap();
        assert_eq!(page.rows.len(), 50);
        assert!(page.rows.iter().all(|row| row.clip_count == Some(1)));
    });
    measure("history/semantic-filter/50-row", || {
        assert_eq!(
            index
                .history(&semantic_history, CATALOG_ROWS)
                .unwrap()
                .rows
                .len(),
            50
        );
    });
    index
        .db
        .execute(
            "INSERT INTO match_tag_effective SELECT DISTINCT match_group,'builtin:pentakill'
         FROM recordings WHERE match_group IS NOT NULL",
            [],
        )
        .unwrap();
    let toolbar_history = HistoryQuery {
        limit: 50,
        queue: Some(HistoryQueue::RankedSoloDuo),
        played_champions: vec![f.summary.champion.clone(), "Ahri".into()],
        player_riot_id: Some("Measure#NA".into()),
        personal_moments: vec![PersonalMoment::DragonSteal],
        tags: vec![crate::tags::TagId::parse("builtin:quadra_kill").unwrap()],
        ..Default::default()
    };
    measure("history/combined-toolbar-filter/50-row", || {
        assert_eq!(
            index
                .history_for_snapshot(&toolbar_history, CATALOG_ROWS, &snapshot)
                .unwrap()
                .rows
                .len(),
            50
        );
    });
    measure("replay-window/50-row", || {
        assert_eq!(
            index
                .replay_window(&replay, &f.timeline())
                .unwrap()
                .events
                .len(),
            50
        );
    });
    measure("roster/latest", || {
        assert!(
            !index
                .roster_at(&f.scope(), roster_tick)
                .unwrap()
                .players
                .is_empty()
        );
    });
}

fn seed_clip_count_measurement(fixture: &Fixture, index: &mut Index) -> library::LibrarySnapshot {
    use crate::clip_bundle::{ClipId, EVENTS, HEADER, THUMBNAIL};

    let id = ClipId::new();
    let directory = fixture.root.path().join("clips").join(id.as_str());
    fs::create_dir_all(&directory).unwrap();
    for asset in ["Fixture.mp4", THUMBNAIL, HEADER, EVENTS] {
        fs::write(directory.join(asset), b"fixture").unwrap();
    }
    let snapshot = library::build_library_snapshot(
        fixture.root.path(),
        "",
        "clip-count-measurement".into(),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(snapshot.managed_clip_count, 1);
    while !index.reconcile_clips(&snapshot, &|| Ok(())).unwrap() {}
    // Only History counts are timed. The projected rows represent one indexed clip per game.
    index
        .db
        .execute(
            "INSERT INTO clip_projection
         SELECT bundle,'fixture',match_group,match_group,0,0,0,'','','','','','{}'
         FROM recordings WHERE match_group IS NOT NULL",
            [],
        )
        .unwrap();
    snapshot
}

#[test]
#[ignore = "targeted warmed Match History tag query benchmark"]
fn representative_match_tag_queries() {
    let fixture = Fixture::new(1);
    let mut index = fixture.index();
    fixture.ready(&mut index);
    seed_measurement_projection(&fixture, &mut index);
    index
        .db
        .execute_batch(
            "INSERT INTO match_tag_effective SELECT match_group,'builtin:pentakill' FROM recordings
         WHERE match_group IS NOT NULL AND rowid%5=0;
         INSERT INTO match_tag_effective SELECT match_group,'builtin:baron_steal' FROM recordings
         WHERE match_group IS NOT NULL AND rowid%43=0;
         ANALYZE;",
        )
        .unwrap();
    let query = |names: &[&str]| HistoryQuery {
        limit: 50,
        tags: names
            .iter()
            .map(|name| crate::tags::TagId::parse(name).unwrap())
            .collect(),
        ..Default::default()
    };
    let first = query(&["builtin:quadra_kill"]);
    let page = index.history(&first, CATALOG_ROWS).unwrap();
    assert_eq!(page.rows.len(), 50);
    assert!(
        page.rows
            .iter()
            .all(|row| row.tags.iter().any(|tag| tag.tier == Some(5)))
    );
    let cases = [
        ("unfiltered", query(&[]), 50),
        ("implied", first.clone(), 50),
        (
            "implied continuation",
            HistoryQuery {
                after: page.next,
                ..first
            },
            50,
        ),
        (
            "implied and exact",
            query(&["builtin:quadra_kill", "builtin:baron_steal"]),
            46,
        ),
        ("missing tag", query(&["builtin:dragon_steal"]), 0),
    ];
    for (label, query, expected) in cases {
        measure(&format!("history/tags/{label}"), || {
            assert_eq!(
                index.history(&query, CATALOG_ROWS).unwrap().rows.len(),
                expected
            );
        });
    }
}

#[test]
#[cfg(feature = "history-command-benchmark")]
#[ignore = "measure the 10k Match History Tauri command in release mode"]
fn representative_history_command_measurements() {
    use crate::{AppState, MediaRuntimeState, league_index_commands};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex, RwLock};
    use tauri::{Manager, ipc, test, webview::InvokeRequest};

    let fixture = Fixture::new(1);
    let roots =
        Arc::new(crate::playback_server::MediaRoots::new(fixture.root.path().into()).unwrap());
    let (_, root) = roots.output_pair();
    let mut index = Index::open(fixture.cache.path(), root.path()).unwrap();
    index
        .reconcile(std::slice::from_ref(&fixture.summary), "fixture")
        .unwrap();
    fixture.ready(&mut index);
    seed_measurement_projection(&fixture, &mut index);
    seed_clip_count_measurement(&fixture, &mut index);
    index.db.execute_batch(
        "UPDATE recordings SET recorded_at='2022-01-01T12:00:00Z',
             summary_json=json_set(summary_json,'$.recorded_at','2022-01-01T12:00:00Z')
         WHERE bundle BETWEEN 'm00000' AND 'm00011';
         UPDATE recordings SET saved=1,summary_json=json_set(summary_json,'$.saved',json('true'))
         WHERE CAST(substr(bundle,2) AS INTEGER)%8=1;
         INSERT INTO events(media_id,run_id,epoch,source_id,sequence,ordinal,game_tick,kind,stolen,kill_streak,ingestion,replay_tick)
         SELECT r.media_id,s.run_id,0,2,2,0,4000000,'Multikill',0,
             CASE WHEN r.bundle BETWEEN 'm00000' AND 'm00002' THEN 5 ELSE 3 END,'accepted',64000000
         FROM recordings r JOIN scopes s ON s.media_id=r.media_id WHERE r.bundle LIKE 'm%';
         INSERT INTO event_actors(media_id,run_id,epoch,sequence,revision_ordinal,role,ordinal,participant_id,status)
         SELECT e.media_id,e.run_id,e.epoch,e.sequence,e.ordinal,'killer',0,1,'resolved'
         FROM events e WHERE e.kind='Multikill' AND e.sequence=2;
         INSERT INTO match_tag_effective SELECT DISTINCT match_group,'builtin:pentakill'
         FROM recordings WHERE match_group IS NOT NULL;"
    ).unwrap();
    let games = {
        let mut statement = index
            .db
            .prepare("SELECT summary_json FROM recordings")
            .unwrap();
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|row| serde_json::from_str::<GameSummary>(&row.unwrap()).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(games.len(), CATALOG_ROWS);
    index
        .db
        .execute(
            "UPDATE cache_identity SET catalog_token=?1",
            [catalog_key(&games).unwrap()],
        )
        .unwrap();
    drop(index);

    let state = tauri::async_runtime::block_on(async {
        let dir = fixture.cache.path();
        AppState {
            config_mutations: Arc::default(),
            recorder: crate::recorder_supervisor::RecorderSupervisor::start(true),
            config_path: dir.join("config.toml"),
            config: Arc::new(RwLock::new(crate::config::Config::default())),
            roots,
            library: crate::library_coordinator::LibraryCoordinator::new(),
            clip_draft: Default::default(),
            playback_origin: String::new(),
            playback_metrics: Arc::default(),
            hevc_probe_path: dir.join("probe"),
            league_index_cache: dir.to_owned(),
            preparation: Mutex::new(None),
            ddragon_cache: dir.join("ddragon"),
            ddragon_status: Arc::new(RwLock::new(crate::ddragon::DdragonStatus::loading(dir))),
            music_directory: dir.join("music"),
            media_runtime: MediaRuntimeState::Unavailable(Arc::new(
                queueback_media_runtime::resolve_root(&dir.join("missing"))
                    .await
                    .unwrap_err(),
            )),
            #[cfg(feature = "replay-benchmark")]
            benchmark: None,
            #[cfg(feature = "replay-benchmark")]
            benchmark_requests: None,
        }
    });
    let accepted =
        tauri::async_runtime::block_on(state.library.refresh_fixture(&state.roots, games)).unwrap();
    assert_eq!(accepted.games.len(), CATALOG_ROWS);
    let app = test::mock_builder()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            league_index_commands::query_league_history
        ])
        .build(test::mock_context(test::noop_assets()))
        .unwrap();
    assert_eq!(
        app.state::<AppState>().league_index_cache,
        fixture.cache.path()
    );
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for (label, query, expected_rows) in [
        ("browse", json!({ "limit": 50 }), 50),
        (
            "queue-champion-favorites",
            json!({
                "limit": 50, "queue": "ranked_solo_duo", "played_champions": ["Syndra"], "saved_only": true
            }),
            50,
        ),
        (
            "combined",
            json!({
                "limit": 50, "queue": "ranked_solo_duo", "played_champions": ["Syndra", "Ahri"],
                "player_riot_id": "Measure#NA", "personal_moments": ["dragon_steal"], "tags": ["builtin:quadra_kill"]
            }),
            50,
        ),
        (
            "rare-pentakill",
            json!({ "limit": 50, "personal_moments": ["pentakill"] }),
            3,
        ),
        (
            "absent-quadra",
            json!({ "limit": 50, "personal_moments": ["quadra_kill"] }),
            0,
        ),
        (
            "nonexistent-player",
            json!({ "limit": 50, "player_riot_id": "Missing#NA" }),
            0,
        ),
        (
            "old-date",
            json!({ "limit": 50, "recorded_from": 1640995200_i64, "recorded_before": 1641081600_i64 }),
            12,
        ),
    ] {
        let invoke = || {
            test::get_ipc_response(
                &window,
                InvokeRequest {
                    cmd: "query_league_history".into(),
                    callback: ipc::CallbackFn(0),
                    error: ipc::CallbackFn(1),
                    url: if cfg!(windows) {
                        "http://tauri.localhost"
                    } else {
                        "tauri://localhost"
                    }
                    .parse()
                    .unwrap(),
                    body: ipc::InvokeBody::Json(
                        json!({ "snapshotToken": accepted.token, "query": query }),
                    ),
                    headers: Default::default(),
                    invoke_key: test::INVOKE_KEY.into(),
                },
            )
            .unwrap()
        };
        let page: Value = invoke().deserialize().unwrap();
        assert_eq!(
            page["rows"].as_array().unwrap().len(),
            expected_rows,
            "{label}"
        );
        assert_eq!(page["catalog_recordings"], CATALOG_ROWS);
        assert_eq!(page["indexed_recordings"], CATALOG_ROWS);
        assert!(
            page["rows"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["clip_count"] == 1),
            "{label}"
        );
        measure(label, || {
            std::hint::black_box(invoke());
        });
    }
}

#[test]
#[ignore = "release-mode real-journal rebuild throughput; no cold-disk or UI latency claim"]
fn representative_journal_rebuild_measurements() {
    const EVENT_COMMITS: u64 = 64;
    const EVENTS_PER_COMMIT: u64 = 32;
    let mut f = Fixture::new(1);
    let mut template = None;
    journal::recover(
        Cursor::new(fs::read(f.directory.join(JOURNAL)).unwrap()),
        |entries, _| {
            for entry in entries {
                if let Record::EventObservation { revisions, .. } = &entry.record {
                    template = revisions.first().map(|revision| revision.event.clone());
                }
            }
            Ok(())
        },
    )
    .unwrap();
    let template = template.unwrap();
    for commit in 0..EVENT_COMMITS {
        let events = (0..EVENTS_PER_COMMIT)
            .map(|ordinal| {
                let offset = commit * EVENTS_PER_COMMIT + ordinal;
                let mut event = template.clone();
                event.source_id = Some(10_000 + offset);
                let seconds = 1.0 + (offset % 180) as f64 * 0.05;
                // Live values enter through JSON parsing. Retain that exact
                // representation before computing a revision fingerprint.
                event.source_time = Some(serde_json::from_str(&seconds.to_string()).unwrap());
                event.game_tick = Some(GameTick::from_seconds(seconds).unwrap());
                event
            })
            .collect();
        let pending = f.events.prepare(&f.source, events).unwrap();
        let revisions = pending.revisions().to_vec();
        f.commit(
            &[Record::EventObservation {
                evidence: f.evidence(1_000 + commit, Endpoint::Events, Some(4)),
                revisions,
            }],
            commit + 1 == EVENT_COMMITS,
        );
        f.events.acknowledge(pending).unwrap();
    }
    let source_bytes = fs::metadata(f.directory.join(JOURNAL)).unwrap().len();
    let started = Instant::now();
    let mut index = f.index();
    let create_wall = started.elapsed();
    for rebuild in [false, true] {
        let started = Instant::now();
        let mut pages = Vec::new();
        let mut batches = 0;
        loop {
            let page_started = Instant::now();
            let progress = index
                .recording(
                    f.root.path(),
                    &f.summary,
                    rebuild && pages.is_empty(),
                    &|| Ok(()),
                )
                .unwrap();
            pages.push(page_started.elapsed());
            assert!(progress.batches as usize <= IMPORT_BATCHES);
            batches += u64::from(progress.batches);
            if progress.status == "ready" {
                break;
            }
            assert_eq!(progress.status, "pending");
            assert!(pages.len() <= 100);
        }
        let wall = started.elapsed();
        assert_eq!(batches, EVENT_COMMITS + 1);
        let events: u64 = index
            .db
            .query_row(
                "SELECT COUNT(*) FROM events WHERE source_id>=10000",
                [],
                |row| unsigned(row, 0),
            )
            .unwrap();
        assert_eq!(events, EVENT_COMMITS * EVENTS_PER_COMMIT);
        pages.sort_unstable();
        let db_path = cache_path(f.cache.path(), f.root.path());
        let retained_cache_bytes: u64 = [
            db_path.clone(),
            PathBuf::from(format!("{}-wal", db_path.display())),
            PathBuf::from(format!("{}-shm", db_path.display())),
        ]
        .iter()
        .filter_map(|path| fs::metadata(path).ok().map(|metadata| metadata.len()))
        .sum();
        println!(
            "league-index rebuild {}",
            serde_json::json!({
                "rebuild": rebuild, "source_bytes": source_bytes, "events": events,
                "commits": batches, "pages": pages.len(), "cache_create_wall_us": create_wall.as_micros(),
                "import_wall_us": wall.as_micros(),
                "source_mib_per_second": source_bytes as f64 / (1024.0 * 1024.0) / wall.as_secs_f64(),
                "page_p50_us": percentile(&pages, 50).as_micros(),
                "page_p95_us": percentile(&pages, 95).as_micros(),
                "page_p99_us": percentile(&pages, 99).as_micros(),
                "retained_cache_bytes": retained_cache_bytes,
            })
        );
    }
    index
        .reconcile(std::slice::from_ref(&f.summary), "refreshed")
        .unwrap();
    measure("unchanged-source/accepted-refresh", || {
        let progress = index
            .recording(f.root.path(), &f.summary, false, &|| Ok(()))
            .unwrap();
        assert_eq!(progress.status, "ready");
        assert_eq!(progress.batches, 0);
    });
}
