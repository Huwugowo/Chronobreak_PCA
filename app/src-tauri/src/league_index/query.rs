use super::*;
use chronobreak_replay_time::{GameCalibrationV2, MappedReplayTime, MediaTimelineV2};
use obs::events::{ActorRole, EventRevision};

pub(super) const MAX_WINDOW_ROWS: usize = 512;
pub(super) const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub(super) const HISTORY_SQL: &str = "SELECT r.media_id,r.summary_json,r.result_json,r.telemetry_status,r.recorded_at,r.facts_current FROM recordings r
    WHERE (?1 IS NULL OR r.recorded_at<?1 OR (r.recorded_at=?1 AND r.media_id>?2))
    AND ((?3 IS NULL AND ?4 IS NULL AND ?6 IS NULL AND ?7=0) OR r.facts_current=1)
    AND (?3 IS NULL OR r.queue_id=?3) AND (?4 IS NULL OR r.outcome=?4) AND (?5 IS NULL OR r.champion=?5)
    AND (?6 IS NULL OR EXISTS(SELECT 1 FROM final_players f WHERE f.media_id=r.media_id AND f.riot_id=?6)
        OR (r.telemetry_status='ready' AND EXISTS(SELECT 1 FROM participants p WHERE p.media_id=r.media_id AND p.riot_id=?6)))
    AND (?7=0 OR (r.telemetry_status='ready' AND EXISTS(SELECT 1 FROM events e
        WHERE e.media_id=r.media_id AND e.ingestion NOT IN ('invalid','conflict')
        AND (?8 IS NULL OR e.kind=?8) AND (?9 IS NULL OR e.stolen=?9) AND (?10 IS NULL OR e.kill_streak>=?10)
        AND ((?11 IS NULL AND ?6 IS NULL) OR EXISTS(SELECT 1 FROM effective_event_actors a
            JOIN participants p ON p.media_id=a.media_id AND p.run_id=a.run_id AND p.epoch=a.epoch AND p.participant_id=a.participant_id
            WHERE a.media_id=e.media_id AND a.sequence=e.sequence AND a.revision_ordinal=e.ordinal AND a.status='resolved'
            AND (?11 IS NULL OR a.role=?11) AND (?6 IS NULL OR p.riot_id=?6))))))
    AND (?13 IS NULL OR r.mode=?13) AND (COALESCE(?14,0)=0 OR r.saved=1)
    AND (?15 IS NULL OR instr(lower(r.champion),lower(?15))>0 OR instr(lower(r.mode),lower(?15))>0)
    ORDER BY r.recorded_at DESC,r.media_id LIMIT ?12";
pub(super) const GAME_TICK_ROSTER_LOOKUP_SQL: &str = "SELECT r.observation_id,r.game_tick,e.data FROM rosters r JOIN evidence e
    ON e.media_id=r.media_id AND e.run_id=r.run_id AND e.epoch=r.epoch AND e.observation_id=r.observation_id
    WHERE r.media_id=?1 AND r.run_id=?2 AND r.epoch=?3 AND r.game_tick<=?4 ORDER BY r.game_tick DESC,r.observation_id DESC LIMIT 1";

struct HistoryRosterRow {
    run: String,
    epoch: u64,
    observation: u64,
    local: Option<u64>,
    tick: Option<i64>,
    evidence: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScopeQuery {
    pub media_id: MediaId,
    pub run_id: MediaId,
    pub epoch: u64,
}
impl ScopeQuery {
    pub(super) fn source(&self) -> obs::SourceEpoch {
        obs::SourceEpoch {
            media_id: self.media_id.clone(),
            run_id: self.run_id.clone(),
            epoch: self.epoch,
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WindowQuery {
    pub scope: ScopeQuery,
    pub start: GameTick,
    pub end: GameTick,
    pub after: Option<EventCursor>,
    pub limit: u16,
    pub kind: Option<String>,
    pub stolen: Option<bool>,
    pub minimum_kill_streak: Option<u64>,
    pub participant: Option<obs::ParticipantId>,
    pub role: Option<ActorRole>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventCursor {
    pub game_tick: GameTick,
    pub source_id: u64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MatchState {
    ObservedMatch,
    NoObservedMatch,
    UnknownCoverage,
}
#[derive(Debug, Serialize)]
pub(crate) struct IndexedEvent {
    pub revision: EventRevision,
    pub effective_actors: Option<obs::events::EventActors>,
    pub sequence: u64,
    pub ordinal: u64,
    pub evidence: obs::Evidence,
    pub mapped_replay_time: MappedReplayTime,
}
#[derive(Debug, Serialize)]
pub(crate) struct QueryCoverage {
    pub index_status: String,
    pub committed: Option<journal::Position>,
    pub uncommitted_tail_bytes: u64,
    pub gap_attempts: u64,
    pub unresolved_actors: u64,
    pub unmapped_observations: u64,
    pub successful_observations: u64,
    pub observed_from: Option<GameTick>,
    pub observed_through: Option<GameTick>,
}
#[derive(Debug, Serialize)]
pub(crate) struct EventWindow {
    pub source: obs::SourceEpoch,
    pub local_participant: Option<obs::ParticipantId>,
    pub events: Vec<IndexedEvent>,
    pub next: Option<EventCursor>,
    pub match_state: MatchState,
    pub coverage: QueryCoverage,
}
#[derive(Debug, Serialize)]
pub(crate) struct RosterAt {
    pub source: obs::SourceEpoch,
    pub local_participant: Option<obs::ParticipantId>,
    pub game_tick: Option<GameTick>,
    pub age_us: Option<u64>,
    pub evidence: Option<obs::Evidence>,
    pub players: Vec<obs::RosterPlayer>,
    pub coverage: QueryCoverage,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryCursor {
    pub recorded_at: String,
    pub media_id: MediaId,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryQuery {
    pub after: Option<HistoryCursor>,
    pub limit: u16,
    pub queue_id: Option<u64>,
    pub outcome: Option<chronobreak_league_data::Outcome>,
    pub champion: Option<String>,
    pub mode: Option<String>,
    pub saved_only: Option<bool>,
    pub search: Option<String>,
    pub participant_riot_id: Option<String>,
    pub event_kind: Option<String>,
    pub stolen: Option<bool>,
    pub minimum_kill_streak: Option<u64>,
    pub role: Option<ActorRole>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StatsSource {
    FinalEog,
    RecordedSnapshot,
    Unavailable,
}
#[derive(Debug, Serialize)]
pub(crate) struct HistoryStats {
    pub source: StatsSource,
    pub kills: Option<u64>,
    pub deaths: Option<u64>,
    pub assists: Option<u64>,
    pub items: Option<Vec<Option<u64>>>,
    pub keystone_id: Option<u64>,
    pub secondary_style: Option<u64>,
    pub summoner_spells: Option<[Option<String>; 2]>,
    pub creep_score: Option<u64>,
    pub duration_seconds: Option<u64>,
    pub evidence: Option<obs::Evidence>,
    pub final_evidence: Option<chronobreak_league_data::ResultEvidence>,
    pub game_tick: Option<GameTick>,
}
impl HistoryStats {
    fn unavailable() -> Self {
        Self {
            source: StatsSource::Unavailable,
            kills: None,
            deaths: None,
            assists: None,
            items: None,
            keystone_id: None,
            secondary_style: None,
            summoner_spells: None,
            creep_score: None,
            duration_seconds: None,
            evidence: None,
            final_evidence: None,
            game_tick: None,
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct HistoryRow {
    pub media_id: MediaId,
    pub game: GameSummary,
    pub stats: HistoryStats,
    pub index_status: String,
}
#[derive(Debug, Serialize)]
pub(crate) struct HistoryPage {
    pub rows: Vec<HistoryRow>,
    pub next: Option<HistoryCursor>,
    pub indexed_recordings: u64,
    pub catalog_recordings: u64,
    /// Excluded/unfinished sources are unknown, never negative query evidence.
    pub unknown_recordings: u64,
}
#[derive(Debug, Serialize)]
pub(crate) struct SourceScope {
    pub source: obs::SourceEpoch,
    pub local_participant: Option<obs::ParticipantId>,
}

impl Index {
    pub(crate) fn scopes(&self, media: &MediaId) -> Result<Vec<SourceScope>> {
        let ready: bool = self.db.query_row(
            "SELECT telemetry_status='ready' FROM recordings WHERE media_id=?1",
            [media.as_str()],
            |r| r.get(0),
        )?;
        ensure!(ready, "source index unavailable or pending");
        let mut statement = self.db.prepare("SELECT run_id,epoch,local_participant FROM scopes WHERE media_id=?1 ORDER BY epoch LIMIT 257")?;
        let mut rows = statement.query([media.as_str()])?;
        let mut output = Vec::new();
        while let Some(row) = rows.next()? {
            ensure!(output.len() < 256, "source epoch query capacity");
            let run: String = row.get(0)?;
            output.push(SourceScope {
                source: obs::SourceEpoch {
                    media_id: media.clone(),
                    run_id: MediaId::parse(&run)?,
                    epoch: unsigned(row, 1)?,
                },
                local_participant: optional_unsigned(row, 2)?.map(obs::ParticipantId),
            });
        }
        Ok(output)
    }

    pub(super) fn local(&self, scope: &ScopeQuery) -> Result<Option<obs::ParticipantId>> {
        Ok(self
            .db
            .query_row(
                "SELECT local_participant FROM scopes WHERE media_id=?1 AND run_id=?2 AND epoch=?3",
                params![
                    scope.media_id.as_str(),
                    scope.run_id.as_str(),
                    sql_u64(scope.epoch)?
                ],
                |r| optional_unsigned(r, 0),
            )
            .optional()?
            .flatten()
            .map(obs::ParticipantId))
    }
    pub(super) fn coverage(
        &self,
        scope: &ScopeQuery,
        start: GameTick,
        end: GameTick,
        endpoint: &str,
    ) -> Result<QueryCoverage> {
        let progress = self.progress(&scope.media_id, 0)?;
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM scopes WHERE media_id=?1 AND run_id=?2 AND epoch=?3)",
            params![
                scope.media_id.as_str(),
                scope.run_id.as_str(),
                sql_u64(scope.epoch)?
            ],
            |r| r.get(0),
        )?;
        let totals = if !exists {
            super::coverage::Totals::default()
        } else if progress.status == "ready" {
            let mut totals = super::coverage::cached(&self.db, scope, endpoint)?
                .context("missing ready coverage projection")?;
            let (gaps, unresolved) =
                super::coverage::interval(&self.db, scope, start, end, endpoint)?;
            totals.gap_attempts = gaps;
            totals.unresolved_actors = unresolved;
            totals
        } else {
            super::coverage::aggregate(&self.db, scope, start, end, endpoint)?
        };
        Ok(QueryCoverage {
            index_status: if exists {
                progress.status
            } else {
                "unavailable".into()
            },
            committed: progress.position,
            uncommitted_tail_bytes: progress.tail_bytes,
            gap_attempts: totals.gap_attempts,
            unresolved_actors: totals.unresolved_actors,
            unmapped_observations: totals.unmapped_observations,
            successful_observations: totals.successful_observations,
            observed_from: totals.observed_from.map(GameTick::new).transpose()?,
            observed_through: totals.observed_through.map(GameTick::new).transpose()?,
        })
    }

    pub(crate) fn events(
        &self,
        query: &WindowQuery,
        timeline: &MediaTimelineV2,
    ) -> Result<EventWindow> {
        ensure!(
            query.scope.media_id == timeline.media_id && query.start <= query.end,
            "event window media/interval"
        );
        ensure!(
            query.limit > 0 && usize::from(query.limit) <= MAX_WINDOW_ROWS,
            "event window limit"
        );
        validate_text(query.kind.as_deref())?;
        let scope = &query.scope;
        let coverage = self.coverage(scope, query.start, query.end, "events")?;
        let mut events = Vec::new();
        let mut more = false;
        if coverage.index_status == "ready" {
            let mut statement = self.db.prepare("SELECT r.data,r.sequence,r.ordinal,o.data,c.data FROM events e
                JOIN event_revisions r ON r.media_id=e.media_id AND r.sequence=e.sequence AND r.ordinal=e.ordinal
                JOIN evidence o ON o.media_id=r.media_id AND o.run_id=r.run_id AND o.epoch=r.epoch AND o.observation_id=r.observation_id
                LEFT JOIN calibrations c ON c.media_id=o.media_id AND c.run_id=o.run_id AND c.epoch=o.epoch AND c.observation_id=o.calibration_id
                WHERE e.media_id=?1 AND e.run_id=?2 AND e.epoch=?3 AND e.game_tick BETWEEN ?4 AND ?5
                AND e.ingestion NOT IN ('invalid','conflict')
                AND (?6 IS NULL OR e.game_tick>?6 OR (e.game_tick=?6 AND e.source_id>?7))
                AND (?8 IS NULL OR e.kind=?8) AND (?9 IS NULL OR e.stolen=?9) AND (?10 IS NULL OR e.kill_streak>=?10)
                AND ((?11 IS NULL AND ?12 IS NULL) OR EXISTS(SELECT 1 FROM effective_event_actors a
                    WHERE a.media_id=e.media_id AND a.sequence=e.sequence AND a.revision_ordinal=e.ordinal AND a.status='resolved'
                    AND (?11 IS NULL OR a.participant_id=?11) AND (?12 IS NULL OR a.role=?12)))
                ORDER BY e.game_tick,e.source_id LIMIT ?13")?;
            let mut rows = statement.query(params![
                scope.media_id.as_str(),
                scope.run_id.as_str(),
                sql_u64(scope.epoch)?,
                query.start.get(),
                query.end.get(),
                query.after.as_ref().map(|c| c.game_tick.get()),
                query
                    .after
                    .as_ref()
                    .map(|c| sql_u64(c.source_id))
                    .transpose()?,
                query.kind,
                query.stolen,
                query.minimum_kill_streak.map(sql_u64).transpose()?,
                query.participant.map(|p| sql_u64(p.0)).transpose()?,
                query.role.as_ref().map(name).transpose()?,
                u32::from(query.limit) + 1
            ])?;
            let mut bytes = 0;
            while let Some(row) = rows.next()? {
                if events.len() == usize::from(query.limit) {
                    more = true;
                    break;
                }
                let json: String = row.get(0)?;
                let evidence: String = row.get(3)?;
                let calibration: Option<String> = row.get(4)?;
                let size =
                    json.len() + evidence.len() + calibration.as_ref().map_or(0, String::len);
                ensure!(size <= MAX_RESPONSE_BYTES, "event response capacity");
                if bytes + size > MAX_RESPONSE_BYTES {
                    more = true;
                    break;
                }
                bytes += size;
                let revision: EventRevision = serde_json::from_str(&json)?;
                let calibration: Option<GameCalibrationV2> =
                    calibration.map(|c| serde_json::from_str(&c)).transpose()?;
                let tick = revision
                    .event
                    .game_tick
                    .context("indexed event game tick")?;
                events.push(IndexedEvent {
                    effective_actors: self.effective_actors(
                        &scope.media_id,
                        unsigned(row, 1)?,
                        unsigned(row, 2)?,
                        &revision.event.actors,
                    )?,
                    revision,
                    sequence: unsigned(row, 1)?,
                    ordinal: unsigned(row, 2)?,
                    evidence: serde_json::from_str(&evidence)?,
                    mapped_replay_time: library::map_game_tick(
                        calibration.as_ref(),
                        tick,
                        timeline,
                    )?,
                });
            }
        }
        let next = if more {
            events
                .last()
                .and_then(|e| e.revision.event.game_tick.zip(e.revision.event.source_id))
                .map(|(game_tick, source_id)| EventCursor {
                    game_tick,
                    source_id,
                })
        } else {
            None
        };
        let match_state = if !events.is_empty() {
            MatchState::ObservedMatch
        } else if coverage.index_status != "ready"
            || coverage.gap_attempts > 0
            || coverage.uncommitted_tail_bytes > 0
            || coverage.unmapped_observations > 0
            || coverage.successful_observations == 0
            || ((query.participant.is_some() || query.role.is_some())
                && coverage.unresolved_actors > 0)
            || coverage.observed_from.is_none_or(|from| from > query.start)
            || coverage
                .observed_through
                .is_none_or(|through| through < query.end)
        {
            MatchState::UnknownCoverage
        } else {
            MatchState::NoObservedMatch
        };
        Ok(EventWindow {
            source: scope.source(),
            local_participant: self.local(scope)?,
            events,
            next,
            match_state,
            coverage,
        })
    }

    pub(crate) fn roster_at(&self, scope: &ScopeQuery, tick: GameTick) -> Result<RosterAt> {
        let coverage = self.coverage(scope, tick, tick, "player_list")?;
        let mut output = RosterAt {
            source: scope.source(),
            local_participant: self.local(scope)?,
            game_tick: None,
            age_us: None,
            evidence: None,
            players: Vec::new(),
            coverage,
        };
        if output.coverage.index_status != "ready" {
            return Ok(output);
        }
        let latest: Option<(u64, i64, String)> = self
            .db
            .query_row(
                GAME_TICK_ROSTER_LOOKUP_SQL,
                params![
                    scope.media_id.as_str(),
                    scope.run_id.as_str(),
                    sql_u64(scope.epoch)?,
                    tick.get()
                ],
                |r| Ok((unsigned(r, 0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((id, observed, evidence)) = latest {
            output.game_tick = Some(GameTick::new(observed)?);
            output.age_us = Some(u64::try_from(tick.get() - observed)?);
            output.evidence = Some(serde_json::from_str(&evidence)?);
            let mut statement = self.db.prepare("SELECT p.data FROM roster_members m JOIN roster_payloads p
                ON p.media_id=m.media_id AND p.run_id=m.run_id AND p.epoch=m.epoch AND p.payload_id=m.payload_id
                WHERE m.media_id=?1 AND m.run_id=?2 AND m.epoch=?3 AND m.observation_id=?4 ORDER BY m.ordinal LIMIT 65")?;
            let mut rows = statement.query(params![
                scope.media_id.as_str(),
                scope.run_id.as_str(),
                sql_u64(scope.epoch)?,
                sql_u64(id)?
            ])?;
            while let Some(row) = rows.next()? {
                ensure!(
                    output.players.len() < obs::MAX_ROSTER,
                    "roster query capacity"
                );
                output
                    .players
                    .push(serde_json::from_str(&row.get::<_, String>(0)?)?);
            }
        }
        Ok(output)
    }

    pub(crate) fn history(&self, query: &HistoryQuery, catalog_size: usize) -> Result<HistoryPage> {
        ensure!(query.limit > 0 && query.limit <= 50, "history page limit");
        for text in [
            query.champion.as_deref(),
            query.mode.as_deref(),
            query.search.as_deref(),
            query.participant_riot_id.as_deref(),
            query.event_kind.as_deref(),
            query.after.as_ref().map(|c| c.recorded_at.as_str()),
        ] {
            validate_text(text)?;
        }
        if let Some(riot) = &query.participant_riot_id {
            ensure!(
                obs::decode::full_riot_id(riot),
                "history identity must be exact"
            );
        }
        let event_filter = query.event_kind.is_some()
            || query.stolen.is_some()
            || query.minimum_kill_streak.is_some()
            || query.role.is_some();
        let mut statement = self.db.prepare(HISTORY_SQL)?;
        let mut rows = statement.query(params![
            query.after.as_ref().map(|c| c.recorded_at.as_str()),
            query.after.as_ref().map(|c| c.media_id.as_str()),
            query.queue_id.map(sql_u64).transpose()?,
            query.outcome.as_ref().map(name).transpose()?,
            query.champion,
            query.participant_riot_id,
            event_filter,
            query.event_kind,
            query.stolen,
            query.minimum_kill_streak.map(sql_u64).transpose()?,
            query.role.as_ref().map(name).transpose()?,
            u32::from(query.limit) + 1,
            query.mode,
            query.saved_only,
            query.search
        ])?;
        let mut output = Vec::new();
        let mut next = None;
        let mut input_bytes = 0;
        let mut output_bytes = 0;
        while let Some(row) = rows.next()? {
            if output.len() == usize::from(query.limit) {
                if let Some(last) = output.last() {
                    let last: &HistoryRow = last;
                    next = Some(HistoryCursor {
                        recorded_at: last.game.recorded_at.clone(),
                        media_id: last.media_id.clone(),
                    });
                }
                break;
            }
            let media = MediaId::parse(&row.get::<_, String>(0)?)?;
            // Check SQLite text lengths before allocating/deserializing the row.
            input_bytes += row.get_ref(1)?.as_str()?.len();
            if let rusqlite::types::ValueRef::Text(bytes) = row.get_ref(2)? {
                input_bytes += bytes.len();
            }
            ensure!(input_bytes <= MAX_RESPONSE_BYTES, "history input capacity");
            let current: bool = row.get(5)?;
            let result: Option<String> = if current { row.get(2)? } else { None };
            let result: Option<ResultFile> =
                result.map(|r| serde_json::from_str(&r)).transpose()?;
            let stats = if current {
                self.history_stats(&media, result.as_ref())?
            } else {
                HistoryStats::unavailable()
            };
            let mut game: GameSummary = serde_json::from_str(&row.get::<_, String>(1)?)?;
            game.participants = if current {
                self.history_participants(&media, result.as_ref())?
            } else {
                Vec::new()
            };
            let item = HistoryRow {
                media_id: media,
                game,
                stats,
                index_status: if current {
                    row.get(3)?
                } else {
                    "pending".into()
                },
            };
            output_bytes += serde_json::to_vec(&item)?.len();
            ensure!(
                output_bytes <= MAX_RESPONSE_BYTES,
                "history response capacity"
            );
            output.push(item);
        }
        use super::history_summary::*;
        let mut mask = BASE_UNKNOWN | SOURCE_UNCHECKED;
        if query.queue_id.is_some() {
            mask |= QUEUE_UNKNOWN;
        }
        if query.outcome.is_some() {
            mask |= OUTCOME_UNKNOWN;
        }
        if query.participant_riot_id.is_some() || query.role.is_some() {
            mask |= ACTOR_UNKNOWN;
        }
        if event_filter {
            mask |= EVENTS_UNKNOWN;
        }
        let (indexed, uncertain): (u64, u64) = self.db.query_row(
            "SELECT COALESCE(SUM(count),0),COALESCE(SUM(CASE WHEN (mask & ?1)!=0 THEN count ELSE 0 END),0) FROM history_counts",
            [mask], |r| Ok((unsigned(r,0)?,unsigned(r,1)?)),
        )?;
        Ok(HistoryPage {
            rows: output,
            next,
            indexed_recordings: indexed,
            catalog_recordings: catalog_size as u64,
            unknown_recordings: (catalog_size as u64).saturating_sub(indexed) + uncertain,
        })
    }

    fn history_participants(
        &self,
        media: &MediaId,
        result: Option<&ResultFile>,
    ) -> Result<Vec<library::ReplayParticipant>> {
        if let Some(result) = result.filter(|r| r.teams.iter().any(|t| !t.players.is_empty())) {
            let local_team = result.local_player.as_ref().and_then(|p| p.team_id);
            return Ok(result
                .teams
                .iter()
                .flat_map(|team| {
                    team.players.iter().map(move |player| {
                        let team_id = match (team.team_id, player.team_id) {
                            (Some(a), Some(b)) if a != b => None,
                            (a, b) => a.or(b),
                        };
                        library::ReplayParticipant {
                            summoner_name: player
                                .riot_id
                                .clone()
                                .unwrap_or_else(|| "Unresolved player".into()),
                            champion: player
                                .champion_name
                                .clone()
                                .unwrap_or_else(|| "Unknown".into()),
                            relation: match (local_team, team_id) {
                                (Some(a), Some(b)) if a == b => library::EventRelation::Ally,
                                (Some(_), Some(_)) => library::EventRelation::Enemy,
                                _ => library::EventRelation::Neutral,
                            },
                        }
                    })
                })
                .collect());
        }
        // One complete observed roster from one source; never combine epochs or
        // infer a team from names. Pending imports cannot supply partial hints.
        let latest: Option<(String, u64, u64, Option<u64>)> = self.db.query_row(
            "SELECT r.run_id,r.epoch,r.observation_id,s.local_participant
            FROM rosters r JOIN scopes s ON s.media_id=r.media_id AND s.run_id=r.run_id AND s.epoch=r.epoch
            JOIN evidence e ON e.media_id=r.media_id AND e.run_id=r.run_id AND e.epoch=r.epoch AND e.observation_id=r.observation_id
            JOIN recordings g ON g.media_id=r.media_id AND g.telemetry_status='ready'
            WHERE r.media_id=?1 ORDER BY e.end_us DESC,r.observation_id DESC LIMIT 1",
            [media.as_str()], |r| Ok((r.get(0)?, unsigned(r,1)?, unsigned(r,2)?, optional_unsigned(r,3)?)),
        ).optional()?;
        let Some((run, epoch, observation, local)) = latest else {
            return Ok(Vec::new());
        };
        let mut statement = self.db.prepare("SELECT p.data FROM roster_members m JOIN roster_payloads p
            ON p.media_id=m.media_id AND p.run_id=m.run_id AND p.epoch=m.epoch AND p.payload_id=m.payload_id
            WHERE m.media_id=?1 AND m.run_id=?2 AND m.epoch=?3 AND m.observation_id=?4 ORDER BY m.ordinal LIMIT 65")?;
        let mut rows = statement.query(params![
            media.as_str(),
            run,
            sql_u64(epoch)?,
            sql_u64(observation)?
        ])?;
        let mut players: Vec<obs::RosterPlayer> = Vec::new();
        while let Some(row) = rows.next()? {
            ensure!(players.len() < obs::MAX_ROSTER, "history roster capacity");
            players.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
        }
        let local_team = players
            .iter()
            .find(|p| p.participant.map(|p| p.0) == local && local.is_some())
            .and_then(|p| p.team.as_ref());
        Ok(players
            .iter()
            .map(|player| library::ReplayParticipant {
                summoner_name: player
                    .riot_id
                    .clone()
                    .unwrap_or_else(|| "Unresolved player".into()),
                champion: player
                    .champion_name
                    .clone()
                    .unwrap_or_else(|| "Unknown".into()),
                relation: match (local_team, player.team.as_ref()) {
                    (Some(a), Some(b)) if a == b => library::EventRelation::Ally,
                    (Some(_), Some(_)) => library::EventRelation::Enemy,
                    _ => library::EventRelation::Neutral,
                },
            })
            .collect())
    }

    fn history_stats(&self, media: &MediaId, result: Option<&ResultFile>) -> Result<HistoryStats> {
        if let Some(result) = result {
            let p = result.local_player.as_ref();
            return Ok(HistoryStats {
                source: StatsSource::FinalEog,
                summoner_spells: p.map(|p| {
                    [p.spell1_id, p.spell2_id].map(|id| {
                        id.filter(|id| *id > 0 && u32::try_from(*id).is_ok())
                            .map(|id| id.to_string())
                    })
                }),
                kills: p.and_then(|p| p.stats.kills),
                deaths: p.and_then(|p| p.stats.deaths),
                assists: p.and_then(|p| p.stats.assists),
                keystone_id: p
                    .and_then(|p| p.perks.first())
                    .and_then(|perk| perk.id)
                    .filter(|id| *id > 0 && u32::try_from(*id).is_ok()),
                secondary_style: p
                    .and_then(|p| p.secondary_style)
                    .filter(|id| *id > 0 && u32::try_from(*id).is_ok()),
                creep_score: p
                    .and_then(|p| p.stats.minions.zip(p.stats.jungle_minions))
                    .and_then(|(lane, neutral)| lane.checked_add(neutral))
                    .filter(|total| *total <= chronobreak_league_data::MAX_COUNTER),
                duration_seconds: result.duration_seconds.filter(|seconds| *seconds > 0),
                items: result
                    .source_facts
                    .local_player
                    .as_ref()
                    .and_then(|p| p.items.clone()),
                evidence: None,
                final_evidence: result.evidence.clone(),
                game_tick: None,
            });
        }
        let latest = self.db.query_row("SELECT r.run_id,r.epoch,r.observation_id,s.local_participant,r.game_tick,e.data
            FROM rosters r JOIN scopes s ON s.media_id=r.media_id AND s.run_id=r.run_id AND s.epoch=r.epoch
            JOIN evidence e ON e.media_id=r.media_id AND e.run_id=r.run_id AND e.epoch=r.epoch AND e.observation_id=r.observation_id
            JOIN recordings g ON g.media_id=r.media_id AND g.telemetry_status='ready'
            WHERE r.media_id=?1 ORDER BY e.end_us DESC,r.observation_id DESC LIMIT 1", [media.as_str()],
            |r| Ok(HistoryRosterRow {
                run: r.get(0)?, epoch: unsigned(r, 1)?, observation: unsigned(r, 2)?,
                local: optional_unsigned(r, 3)?, tick: r.get(4)?, evidence: r.get(5)?,
            })).optional()?;
        let Some(HistoryRosterRow {
            run,
            epoch,
            observation,
            local: Some(local),
            tick,
            evidence,
        }) = latest
        else {
            return Ok(HistoryStats::unavailable());
        };
        let mut statement = self.db.prepare("SELECT p.data FROM roster_members m JOIN roster_payloads p
            ON p.media_id=m.media_id AND p.run_id=m.run_id AND p.epoch=m.epoch AND p.payload_id=m.payload_id
            WHERE m.media_id=?1 AND m.run_id=?2 AND m.epoch=?3 AND m.observation_id=?4 AND m.participant_id=?5 LIMIT 2")?;
        let mut rows = statement.query(params![
            media.as_str(),
            run,
            sql_u64(epoch)?,
            sql_u64(observation)?,
            sql_u64(local)?
        ])?;
        let Some(row) = rows.next()? else {
            return Ok(HistoryStats::unavailable());
        };
        let player: obs::RosterPlayer = serde_json::from_str(&row.get::<_, String>(0)?)?;
        if rows.next()?.is_some() {
            return Ok(HistoryStats::unavailable());
        }
        let scores = player.scores.as_ref();
        Ok(HistoryStats {
            source: StatsSource::RecordedSnapshot,
            summoner_spells: player.summoner_spells.as_ref().map(|spells| {
                [&spells.one, &spells.two].map(|spell| {
                    spell.as_ref().and_then(|spell| {
                        spell
                            .id
                            .filter(|id| *id > 0 && u32::try_from(*id).is_ok())
                            .map(|id| id.to_string())
                            .or_else(|| spell.display_name.clone())
                    })
                })
            }),
            kills: scores.and_then(|s| s.kills),
            deaths: scores.and_then(|s| s.deaths),
            assists: scores.and_then(|s| s.assists),
            keystone_id: None,
            secondary_style: None,
            creep_score: None,
            duration_seconds: None,
            items: player
                .items
                .map(|items| items.into_iter().map(|i| i.item_id).collect()),
            evidence: Some(serde_json::from_str(&evidence)?),
            final_evidence: None,
            game_tick: tick.map(GameTick::new).transpose()?,
        })
    }
}

fn validate_text(value: Option<&str>) -> Result<()> {
    ensure!(
        value.is_none_or(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)),
        "query text"
    );
    Ok(())
}
