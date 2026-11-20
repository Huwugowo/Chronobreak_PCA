//! Read the committed mapped interval without scanning journals or loading replay rosters.
use super::*;
use crate::clip_bundle::{
    Attribution, ClipEvent, ClipEvents, EVENTS_LIMIT, EventAvailability, MAX_EVENTS,
};
use chronobreak_replay_time::{GameCalibrationV2, MappedReplayTime, MediaTimelineV2, ReplayTick};
use obs::events::ActorStatus;
use std::collections::HashMap;

struct Retained {
    event: ClipEvent,
    order: (ReplayTick, u64, u64),
}

impl Index {
    pub(crate) fn clip_events(
        &self,
        timeline: &MediaTimelineV2,
        start: ReplayTick,
        end: ReplayTick,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<ClipEvents> {
        ensure!(
            start < end && end <= timeline.video.replay_end,
            "invalid clip event range"
        );
        let scopes = self.scopes(&timeline.media_id)?;
        if scopes.is_empty() {
            return Ok(ClipEvents::unavailable());
        }
        let mut records = Vec::new();
        let mut partial = false;
        let mut bytes = 128;
        let mut statement = self.db.prepare(
            "SELECT r.data,r.sequence,r.ordinal,c.data,e.replay_tick,e.source_id FROM events e
             JOIN event_revisions r ON r.media_id=e.media_id AND r.sequence=e.sequence AND r.ordinal=e.ordinal
             JOIN evidence o ON o.media_id=r.media_id AND o.run_id=r.run_id AND o.epoch=r.epoch AND o.observation_id=r.observation_id
             JOIN calibrations c ON c.media_id=o.media_id AND c.run_id=o.run_id AND c.epoch=o.epoch AND c.observation_id=o.calibration_id
             WHERE e.media_id=?1 AND e.run_id=?2 AND e.epoch=?3 AND e.replay_tick>=?4 AND e.replay_tick<?5
             AND e.ingestion IN ('accepted','correction') ORDER BY e.replay_tick,e.source_id LIMIT ?6",
        )?;
        'scopes: for source in scopes {
            check()?;
            let scope = ScopeQuery {
                media_id: timeline.media_id.clone(),
                run_id: source.source.run_id,
                epoch: source.source.epoch,
            };
            let coverage = self.whole_source_coverage(&scope)?;
            partial |= coverage.gap_attempts > 0
                || coverage.unmapped_observations > 0
                || coverage.uncommitted_tail_bytes > 0;
            let mut rows = statement.query(params![
                scope.media_id.as_str(),
                scope.run_id.as_str(),
                sql_u64(scope.epoch)?,
                sql_u64(start.get())?,
                sql_u64(end.get())?,
                (MAX_EVENTS + 1) as i64,
            ])?;
            let Some(mut row) = rows.next()? else {
                continue;
            };
            let identities = self.replay_identities(&scope)?;
            let identities: HashMap<_, _> =
                identities.iter().map(|i| (i.participant.0, i)).collect();
            let local = source.local_participant;
            let local_team = local
                .and_then(|id| identities.get(&id.0))
                .and_then(|p| p.team.as_deref());
            loop {
                if records.len().is_multiple_of(64) {
                    check()?;
                }
                let mut event =
                    serde_json::from_str::<obs::events::EventRevision>(&row.get::<_, String>(0)?)?
                        .event;
                if event.known_kind() {
                    let calibration: GameCalibrationV2 =
                        serde_json::from_str(&row.get::<_, String>(3)?)?;
                    let tick = ReplayTick::new(unsigned(row, 4)?)?;
                    ensure!(
                        matches!(
                            library::map_game_tick(Some(&calibration), event.game_tick.context("clip event time")?, timeline)?,
                            MappedReplayTime::InsideMedia { replay_tick, .. } if replay_tick == tick
                        ),
                        "stale clip event projection"
                    );
                    event.actors = self.effective_actors(
                        &scope.media_id,
                        unsigned(row, 1)?,
                        unsigned(row, 2)?,
                        &event.actors,
                    )?;
                    let role = if event.name.as_deref() == Some("FirstBlood") {
                        obs::events::ActorRole::Recipient
                    } else {
                        obs::events::ActorRole::Killer
                    };
                    let actor = event
                        .actors
                        .as_ref()
                        .and_then(|actors| actors.get(&role))
                        .and_then(|refs| {
                            if let [actor] = refs.as_slice()
                                && actor.status == ActorStatus::Resolved
                            {
                                actor.participant
                            } else {
                                None
                            }
                        });
                    let identity = actor.and_then(|id| identities.get(&id.0));
                    let attribution = match (
                        actor,
                        local,
                        identity.and_then(|p| p.team.as_deref()),
                        local_team,
                    ) {
                        (Some(actor), Some(local), _, _) if actor == local => Attribution::You,
                        (Some(_), _, Some(team), Some(local)) if team == local => Attribution::Ally,
                        (Some(_), _, Some(_), Some(_)) => Attribution::Enemy,
                        _ => Attribution::Unknown,
                    };
                    let label = identity
                        .and_then(|p| p.riot_id.clone().or_else(|| p.champion_name.clone()));
                    let event = ClipEvent {
                        replay_tick: tick,
                        event,
                        attribution,
                        actor: label,
                    };
                    bytes += serde_json::to_vec(&event)?.len() + 1;
                    if records.len() == MAX_EVENTS || bytes as u64 > EVENTS_LIMIT {
                        partial = true;
                        break 'scopes;
                    }
                    records.push(Retained {
                        event,
                        order: (tick, scope.epoch, unsigned(row, 5)?),
                    });
                }
                let Some(next) = rows.next()? else { break };
                row = next;
            }
        }
        Ok(ordered_events(records, partial))
    }
}

fn ordered_events(mut records: Vec<Retained>, partial: bool) -> ClipEvents {
    records.sort_by_key(|row| row.order);
    let availability = if partial {
        EventAvailability::Partial
    } else {
        EventAvailability::Available
    };
    ClipEvents {
        availability,
        events: records.into_iter().map(|row| row.event).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_the_observed_event_timeline_for_all_actors() {
        let multikill = |tick, actor: Option<&str>, attribution, tier| Retained {
            event: ClipEvent {
                replay_tick: ReplayTick::new(tick).unwrap(),
                event: obs::Event {
                    name: Some("Multikill".into()),
                    kill_streak: Some(tier),
                    ..Default::default()
                },
                attribution,
                actor: actor.map(str::to_owned),
            },
            order: (ReplayTick::new(tick).unwrap(), 0, tick),
        };
        let dragon = |tick, attribution, stolen| Retained {
            event: ClipEvent {
                replay_tick: ReplayTick::new(tick).unwrap(),
                event: obs::Event {
                    name: Some("DragonKill".into()),
                    stolen,
                    ..Default::default()
                },
                attribution,
                actor: None,
            },
            order: (ReplayTick::new(tick).unwrap(), 0, tick),
        };
        let events = ordered_events(
            vec![
                multikill(5, Some("Local"), Attribution::You, 4),
                multikill(1, Some("Local"), Attribution::You, 2),
                multikill(4, Some("Local"), Attribution::You, 4),
                multikill(3, Some("Enemy"), Attribution::Enemy, 3),
                multikill(6, None, Attribution::Unknown, 5),
                multikill(2, None, Attribution::Unknown, 3),
                dragon(7, Attribution::You, None),
                dragon(8, Attribution::You, Some(false)),
                dragon(9, Attribution::Ally, Some(true)),
                dragon(10, Attribution::You, Some(true)),
                dragon(11, Attribution::You, Some(true)),
            ],
            true,
        );
        assert_eq!(events.availability, EventAvailability::Partial);
        assert_eq!(
            events
                .detected_tags()
                .iter()
                .map(|tag| tag.definition.id.as_str())
                .collect::<Vec<_>>(),
            ["builtin:quadra_kill", "builtin:dragon_steal"]
        );
        assert_eq!(
            events
                .events
                .iter()
                .map(|event| event.replay_tick.get())
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
        );
    }
}
