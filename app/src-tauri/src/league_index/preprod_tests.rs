//! Filesystem contract fixtures: no acquisition, fake API or prebuilt database.
use super::*;
use chronobreak_replay_time::{MappedReplayTime, ReplayTick};
use obs::events::{ActorRole, ActorStatus};

#[test]
fn real_preprod_sources_import_query_and_rebuild_without_rewriting_evidence() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/preprod-library/library/games");
    let root = tempdir().unwrap();
    let cache = tempdir().unwrap();
    for id in ["1792431584", "1792432630"] {
        let target = root.path().join("games").join(id);
        fs::create_dir_all(&target).unwrap();
        for entry in fs::read_dir(fixtures.join(id)).unwrap() {
            let file = entry.unwrap();
            fs::copy(file.path(), target.join(file.file_name())).unwrap();
        }
    }
    let summaries = library::list_games(root.path()).unwrap();
    assert_eq!(summaries.len(), 2);
    let mut index = Index::open(cache.path(), root.path()).unwrap();
    index.reconcile(&summaries, "real-preprod").unwrap();
    for summary in &summaries {
        let id = summary.timestamp.to_string();
        let timeline = library::replay_descriptor(root.path(), "", &id, String::new())
            .unwrap()
            .media_timeline;
        let mut previous = None;
        for rebuild in [false, true] {
            let first = index
                .recording(root.path(), summary, rebuild, &|| Ok(()))
                .unwrap();
            if first.status != "ready" {
                assert!(index.scopes(&timeline.media_id).is_err());
                assert!((0..1024).any(|_| {
                    index
                        .recording(root.path(), summary, false, &|| Ok(()))
                        .unwrap()
                        .status
                        == "ready"
                }));
            }
            let scope = index.scopes(&timeline.media_id).unwrap().remove(0);
            let history = index
                .history(
                    &HistoryQuery {
                        limit: 50,
                        ..Default::default()
                    },
                    2,
                )
                .unwrap();
            let card = history
                .rows
                .iter()
                .find(|row| row.media_id == timeline.media_id)
                .unwrap();
            let (cs, secondary, duration, allies, enemies) = if id == "1792431584" {
                (136, 8000, 985, 5, 5)
            } else {
                (0, 8400, 1290, 1, 0)
            };
            assert_eq!(card.stats.creep_score, Some(cs));
            assert_eq!(card.stats.secondary_style, Some(secondary));
            assert_eq!(card.stats.duration_seconds, Some(duration));
            assert_eq!(
                card.stats.summoner_spells,
                Some([
                    Some("4".into()),
                    Some(if id == "1792431584" { "6" } else { "12" }.into())
                ])
            );
            assert_eq!(
                card.game
                    .participants
                    .iter()
                    .filter(|p| p.relation == library::EventRelation::Ally)
                    .count(),
                allies
            );
            assert_eq!(
                card.game
                    .participants
                    .iter()
                    .filter(|p| p.relation == library::EventRelation::Enemy)
                    .count(),
                enemies
            );
            let query_scope = ScopeQuery {
                media_id: timeline.media_id.clone(),
                run_id: scope.source.run_id.clone(),
                epoch: scope.source.epoch,
            };
            let query = WindowQuery {
                scope: query_scope.clone(),
                start: GameTick::new(0).unwrap(),
                end: GameTick::new(86_400_000_000).unwrap(),
                after: None,
                limit: 512,
                kind: None,
                stolen: None,
                minimum_kill_streak: None,
                participant: None,
                role: None,
            };
            let all = index.events(&query, &timeline).unwrap();
            let replay = index
                .replay_window(
                    &ReplayWindowQuery {
                        scope: query_scope.clone(),
                        start: ReplayTick::new(0).unwrap(),
                        end: timeline.video.replay_end,
                        at: ReplayTick::new(60 * 48_000_000).unwrap(),
                        after: None,
                        limit: 512,
                    },
                    &timeline,
                )
                .unwrap();
            let mut overlay_query = OverlayQuery {
                scope: query_scope.clone(),
                after: None,
                revision: None,
                limit: 128,
            };
            let mut samples = Vec::new();
            let mut pages = 0;
            let mut bytes = 0;
            loop {
                let page = index.overlay_page(&overlay_query, &timeline).unwrap();
                let size = serde_json::to_vec(&page).unwrap().len();
                assert!(size <= 2 * 1024 * 1024);
                bytes += size;
                pages += 1;
                assert_eq!(page.source, scope.source);
                if pages == 1 {
                    assert_eq!(
                        page.identities.as_ref().unwrap().len(),
                        if id == "1792431584" { 10 } else { 1 }
                    );
                } else {
                    assert!(page.identities.is_none());
                }
                samples.extend(page.samples);
                overlay_query.revision = Some(page.revision);
                overlay_query.after = page.next;
                if overlay_query.after.is_none() {
                    break;
                }
            }
            assert_eq!(samples.len(), if id == "1792431584" { 488 } else { 32 });
            assert_eq!(pages, if id == "1792431584" { 4 } else { 1 });
            assert!(bytes < 1024 * 1024);
            assert!(
                samples
                    .windows(2)
                    .all(|pair| (pair[0].replay_tick, pair[0].observation_id)
                        < (pair[1].replay_tick, pair[1].observation_id))
            );
            assert!(
                samples
                    .iter()
                    .all(|s| s.replay_tick <= timeline.video.replay_end)
            );
            // Continuations cannot combine two different committed prefixes.
            overlay_query.after = Some(super::overlay::OverlayCursor {
                replay_tick: samples[0].replay_tick,
                observation_id: samples[0].observation_id,
            });
            overlay_query.revision.as_mut().unwrap().checksum[0] ^= 1;
            assert!(index.overlay_page(&overlay_query, &timeline).is_err());
            overlay_query.after = None;
            overlay_query.revision = None;
            overlay_query.limit = 129;
            assert!(index.overlay_page(&overlay_query, &timeline).is_err());
            if id == "1792431584" {
                assert_eq!(all.events.len(), 108);
                assert_eq!(replay.events.len(), 107);
                assert_eq!(replay.roster.as_ref().unwrap().players.len(), 10);
                let initial = index
                    .replay_window(
                        &ReplayWindowQuery {
                            scope: query_scope.clone(),
                            start: ReplayTick::new(0).unwrap(),
                            end: timeline.video.replay_end,
                            at: ReplayTick::new(0).unwrap(),
                            after: None,
                            limit: 512,
                        },
                        &timeline,
                    )
                    .unwrap();
                assert!(initial.roster.is_none());
                assert_eq!(initial.identities.len(), 10);
                assert!(
                    initial
                        .identities
                        .iter()
                        .all(|player| player.champion_name.is_some() && player.riot_id.is_some())
                );
                assert!(
                    initial
                        .identities
                        .iter()
                        .any(|team| Some(team.participant) == scope.local_participant)
                );
                assert_eq!(
                    all.events
                        .iter()
                        .filter(|e| matches!(
                            e.mapped_replay_time,
                            MappedReplayTime::InsideMedia { .. }
                        ))
                        .count(),
                    107
                );
                let local = scope.local_participant.unwrap();
                let mut local_query = query.clone();
                local_query.participant = Some(local);
                local_query.role = Some(ActorRole::Killer);
                let local_events = index.events(&local_query, &timeline).unwrap();
                assert!(!local_events.events.is_empty());
                let event = &local_events.events[0];
                assert_eq!(
                    event.revision.event.actors.as_ref().unwrap()[&ActorRole::Killer][0].status,
                    ActorStatus::Unresolved
                );
                assert_eq!(
                    event.effective_actors.as_ref().unwrap()[&ActorRole::Killer][0].participant,
                    Some(local)
                );
                let history = index
                    .history(
                        &HistoryQuery {
                            limit: 50,
                            participant_riot_id: Some("SUPERSTAR#VOID".into()),
                            event_kind: Some("ChampionKill".into()),
                            role: Some(ActorRole::Killer),
                            ..Default::default()
                        },
                        2,
                    )
                    .unwrap();
                assert_eq!(history.rows.len(), 1);
            } else {
                assert_eq!(all.events.len(), 3);
                assert!(
                    all.events.iter().any(|e| matches!(
                        e.mapped_replay_time,
                        MappedReplayTime::AfterMedia { .. }
                    ))
                );
                assert!(replay.events.len() < 3);
            }
            assert_whole_coverage(&index, &query_scope);
            let snapshot = serde_json::to_value((&all, &replay)).unwrap();
            if let Some(previous) = previous {
                assert_eq!(snapshot, previous);
            }
            previous = Some(snapshot);
        }
        assert_eq!(
            fs::read(root.path().join("games").join(&id).join(JOURNAL)).unwrap(),
            fs::read(fixtures.join(id).join(JOURNAL)).unwrap()
        );
    }
}
