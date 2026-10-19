//! Replay selection uses each observation's committed calibration, never an
//! intercept inferred from another event or source epoch.
use super::{
    query::{IndexedEvent, MAX_RESPONSE_BYTES, MAX_WINDOW_ROWS, QueryCoverage},
    *,
};
use chronobreak_replay_time::{GameCalibrationV2, MediaTimelineV2, ReplayTick};

pub(super) const REPLAY_EVENTS_SQL: &str = "SELECT r.data,r.sequence,r.ordinal,o.data,c.data,e.replay_tick,e.source_id FROM events e
    JOIN event_revisions r ON r.media_id=e.media_id AND r.sequence=e.sequence AND r.ordinal=e.ordinal
    JOIN evidence o ON o.media_id=r.media_id AND o.run_id=r.run_id AND o.epoch=r.epoch AND o.observation_id=r.observation_id
    LEFT JOIN calibrations c ON c.media_id=o.media_id AND c.run_id=o.run_id AND c.epoch=o.epoch AND c.observation_id=o.calibration_id
    WHERE e.media_id=?1 AND e.run_id=?2 AND e.epoch=?3 AND e.replay_tick BETWEEN ?4 AND ?5 AND e.ingestion NOT IN ('invalid','conflict')
    AND (?6 IS NULL OR (e.replay_tick,e.source_id)>(?6,?7)) ORDER BY e.replay_tick,e.source_id LIMIT ?8";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplayCursor {
    pub replay_tick: ReplayTick,
    pub source_id: u64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplayWindowQuery {
    pub scope: ScopeQuery,
    pub start: ReplayTick,
    pub end: ReplayTick,
    pub at: ReplayTick,
    pub after: Option<ReplayCursor>,
    pub limit: u16,
}
#[derive(Debug, Serialize)]
pub(crate) struct ReplayRoster {
    pub source: obs::SourceEpoch,
    pub local_participant: Option<obs::ParticipantId>,
    pub game_tick: GameTick,
    pub observed_replay_tick: ReplayTick,
    pub age_replay_ticks: ReplayTick,
    pub evidence: obs::Evidence,
    pub players: Vec<obs::RosterPlayer>,
    pub coverage: QueryCoverage,
}
#[derive(Debug, Serialize)]
pub(crate) struct ReplayWindow {
    pub source: obs::SourceEpoch,
    pub local_participant: Option<obs::ParticipantId>,
    pub events: Vec<IndexedEvent>,
    pub next: Option<ReplayCursor>,
    pub roster: Option<ReplayRoster>,
    pub coverage: QueryCoverage,
}

impl Index {
    pub(crate) fn replay_window(
        &self,
        query: &ReplayWindowQuery,
        timeline: &MediaTimelineV2,
    ) -> Result<ReplayWindow> {
        ensure!(
            query.scope.media_id == timeline.media_id
                && query.start <= query.end
                && query.end <= timeline.video.replay_end
                && query.at <= timeline.video.replay_end,
            "replay query identity/interval"
        );
        ensure!(
            query.limit > 0 && usize::from(query.limit) <= MAX_WINDOW_ROWS,
            "replay window limit"
        );
        let scope = &query.scope;
        // This reports conservative whole-source coverage. A replay window with
        // no mapped rows does not claim negative evidence for unmapped input.
        let coverage = self.whole_source_coverage(scope)?;
        let mut events = Vec::new();
        let mut next = None;
        if coverage.index_status == "ready" {
            let mut statement = self.db.prepare(REPLAY_EVENTS_SQL)?;
            let mut rows = statement.query(params![
                scope.media_id.as_str(),
                scope.run_id.as_str(),
                sql_u64(scope.epoch)?,
                sql_u64(query.start.get())?,
                sql_u64(query.end.get())?,
                query
                    .after
                    .as_ref()
                    .map(|c| sql_u64(c.replay_tick.get()))
                    .transpose()?,
                query
                    .after
                    .as_ref()
                    .map(|c| sql_u64(c.source_id))
                    .transpose()?,
                u32::from(query.limit) + 1
            ])?;
            let mut bytes = 0;
            let mut last = None;
            while let Some(row) = rows.next()? {
                let json: String = row.get(0)?;
                let evidence: String = row.get(3)?;
                let calibration: Option<String> = row.get(4)?;
                let size =
                    json.len() + evidence.len() + calibration.as_ref().map_or(0, String::len);
                ensure!(size <= MAX_RESPONSE_BYTES, "replay event response capacity");
                if events.len() == usize::from(query.limit) || bytes + size > MAX_RESPONSE_BYTES {
                    next = last;
                    break;
                }
                bytes += size;
                let revision: obs::events::EventRevision = serde_json::from_str(&json)?;
                let calibration: Option<GameCalibrationV2> =
                    calibration.map(|c| serde_json::from_str(&c)).transpose()?;
                let mapped = library::map_game_tick(
                    calibration.as_ref(),
                    revision.event.game_tick.context("mapped event tick")?,
                    timeline,
                )?;
                let tick = unsigned(row, 5)?;
                ensure!(
                    matches!(&mapped,chronobreak_replay_time::MappedReplayTime::InsideMedia {replay_tick,..} if replay_tick.get()==tick),
                    "stale replay projection"
                );
                last = Some(ReplayCursor {
                    replay_tick: ReplayTick::new(tick)?,
                    source_id: unsigned(row, 6)?,
                });
                events.push(IndexedEvent {
                    revision,
                    sequence: unsigned(row, 1)?,
                    ordinal: unsigned(row, 2)?,
                    evidence: serde_json::from_str(&evidence)?,
                    mapped_replay_time: mapped,
                });
            }
        }
        let roster = if coverage.index_status == "ready" {
            self.roster_at_replay(scope, query.at)?
        } else {
            None
        };
        let output = ReplayWindow {
            source: scope.source(),
            local_participant: self.local(scope)?,
            events,
            next,
            roster,
            coverage,
        };
        ensure!(
            serde_json::to_vec(&output)?.len() <= 6 * 1024 * 1024,
            "replay window response capacity"
        );
        Ok(output)
    }

    fn roster_at_replay(&self, scope: &ScopeQuery, at: ReplayTick) -> Result<Option<ReplayRoster>> {
        let latest:Option<(u64,i64,u64,String)>=self.db.query_row("SELECT r.observation_id,r.game_tick,r.replay_tick,e.data FROM rosters r JOIN evidence e
            ON e.media_id=r.media_id AND e.run_id=r.run_id AND e.epoch=r.epoch AND e.observation_id=r.observation_id
            WHERE r.media_id=?1 AND r.run_id=?2 AND r.epoch=?3 AND r.replay_tick<=?4 ORDER BY r.replay_tick DESC,r.observation_id DESC LIMIT 1",
            params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?,sql_u64(at.get())?],|r|Ok((unsigned(r,0)?,r.get(1)?,unsigned(r,2)?,r.get(3)?))).optional()?;
        let Some((id, tick, replay, evidence)) = latest else {
            return Ok(None);
        };
        let mut players = Vec::new();
        let mut statement=self.db.prepare("SELECT p.data FROM roster_members m JOIN roster_payloads p
            ON p.media_id=m.media_id AND p.run_id=m.run_id AND p.epoch=m.epoch AND p.payload_id=m.payload_id
            WHERE m.media_id=?1 AND m.run_id=?2 AND m.epoch=?3 AND m.observation_id=?4 ORDER BY m.ordinal LIMIT 65")?;
        let mut rows = statement.query(params![
            scope.media_id.as_str(),
            scope.run_id.as_str(),
            sql_u64(scope.epoch)?,
            sql_u64(id)?
        ])?;
        while let Some(row) = rows.next()? {
            ensure!(players.len() < obs::MAX_ROSTER, "replay roster capacity");
            players.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
        }
        Ok(Some(ReplayRoster {
            source: scope.source(),
            local_participant: self.local(scope)?,
            game_tick: GameTick::new(tick)?,
            observed_replay_tick: ReplayTick::new(replay)?,
            age_replay_ticks: ReplayTick::new(at.get() - replay)?,
            evidence: serde_json::from_str(&evidence)?,
            players,
            coverage: self.coverage(
                scope,
                GameTick::new(tick)?,
                GameTick::new(tick)?,
                "player_list",
            )?,
        }))
    }
}
