//! Whole-source replay metadata is published with a completed projection.
//! Interval-specific coverage continues to query the underlying ranges.
use super::*;
use query::{QueryCoverage, ScopeQuery};

const WHOLE_START: i64 = -86_400_000_000;
const WHOLE_END: i64 = 86_400_000_000;
#[derive(Default)]
pub(super) struct Totals {
    pub gap_attempts: u64,
    pub unresolved_actors: u64,
    pub unmapped_observations: u64,
    pub successful_observations: u64,
    pub observed_from: Option<i64>,
    pub observed_through: Option<i64>,
}

pub(super) fn interval(
    db: &Connection,
    scope: &ScopeQuery,
    start: GameTick,
    end: GameTick,
    endpoint: &str,
) -> Result<(u64, u64)> {
    let gaps = db.query_row("SELECT COALESCE(SUM(attempts),0) FROM gaps WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND endpoint=?4 AND (game_tick IS NULL OR game_tick BETWEEN ?5 AND ?6)",
            params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?,endpoint,start.get(),end.get()], |r| unsigned(r,0))?;
    let unresolved = db.query_row("SELECT COUNT(*) FROM events e JOIN effective_event_actors a ON a.media_id=e.media_id AND a.sequence=e.sequence AND a.revision_ordinal=e.ordinal
            WHERE e.media_id=?1 AND e.run_id=?2 AND e.epoch=?3 AND e.game_tick BETWEEN ?4 AND ?5 AND a.status!='resolved'",
            params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?,start.get(),end.get()], |r| unsigned(r,0))?;
    Ok((gaps, unresolved))
}

pub(super) fn aggregate(
    db: &Connection,
    scope: &ScopeQuery,
    start: GameTick,
    end: GameTick,
    endpoint: &str,
) -> Result<Totals> {
    let (gaps, unresolved) = interval(db, scope, start, end, endpoint)?;
    let unmapped = db.query_row("SELECT COUNT(*) FROM evidence e LEFT JOIN calibrations c ON c.media_id=e.media_id AND c.run_id=e.run_id AND c.epoch=e.epoch AND c.observation_id=e.calibration_id
            LEFT JOIN evidence g ON g.media_id=e.media_id AND g.run_id=e.run_id AND g.epoch=e.epoch AND g.observation_id=e.game_stats_id AND g.endpoint='game_stats'
            WHERE e.media_id=?1 AND e.run_id=?2 AND e.epoch=?3 AND e.endpoint=?4 AND e.observed=1 AND (COALESCE(json_extract(c.data,'$.status'),'unavailable')!='available' OR g.game_tick IS NULL)",
            params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?,endpoint], |r| unsigned(r,0))?;
    let successful = db.query_row("SELECT COUNT(*) FROM evidence WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND endpoint=?4 AND observed=1",
            params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?,endpoint], |r| unsigned(r,0))?;
    let (from,through):(Option<i64>,Option<i64>)=db.query_row("SELECT MIN(g.game_tick),MAX(g.game_tick) FROM evidence e JOIN evidence g ON g.media_id=e.media_id AND g.run_id=e.run_id AND g.epoch=e.epoch AND g.observation_id=e.game_stats_id AND g.endpoint='game_stats'
            WHERE e.media_id=?1 AND e.run_id=?2 AND e.epoch=?3 AND e.endpoint=?4 AND e.observed=1",
            params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?,endpoint],|r|Ok((r.get(0)?,r.get(1)?)))?;
    Ok(Totals {
        gap_attempts: gaps,
        unresolved_actors: unresolved,
        unmapped_observations: unmapped,
        successful_observations: successful,
        observed_from: from,
        observed_through: through,
    })
}

pub(super) fn refresh(db: &Connection, scope: &ScopeQuery) -> Result<()> {
    for endpoint in [
        "game_stats",
        "active_player",
        "player_list",
        "events",
        "gameflow",
        "summoner",
        "end_of_game",
    ] {
        let totals = aggregate(
            db,
            scope,
            GameTick::new(WHOLE_START)?,
            GameTick::new(WHOLE_END)?,
            endpoint,
        )?;
        db.execute("INSERT INTO source_coverage VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
        ON CONFLICT(media_id,run_id,epoch,endpoint) DO UPDATE SET gap_attempts=excluded.gap_attempts,
        unresolved_actors=excluded.unresolved_actors,unmapped_observations=excluded.unmapped_observations,
        successful_observations=excluded.successful_observations,observed_from=excluded.observed_from,observed_through=excluded.observed_through",
        params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?,endpoint,sql_u64(totals.gap_attempts)?,
        sql_u64(totals.unresolved_actors)?,sql_u64(totals.unmapped_observations)?,sql_u64(totals.successful_observations)?,totals.observed_from,totals.observed_through])?;
    }
    Ok(())
}

pub(super) fn refresh_recording(db: &Connection, media: &MediaId) -> Result<()> {
    let mut statement = db.prepare("SELECT run_id,epoch FROM scopes WHERE media_id=?1")?;
    let mut rows = statement.query([media.as_str()])?;
    while let Some(row) = rows.next()? {
        let run: String = row.get(0)?;
        refresh(
            db,
            &ScopeQuery {
                media_id: media.clone(),
                run_id: MediaId::parse(&run)?,
                epoch: unsigned(row, 1)?,
            },
        )?;
    }
    Ok(())
}

pub(super) fn cached(
    db: &Connection,
    scope: &ScopeQuery,
    endpoint: &str,
) -> Result<Option<Totals>> {
    Ok(db.query_row("SELECT gap_attempts,unresolved_actors,unmapped_observations,successful_observations,observed_from,observed_through
            FROM source_coverage WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND endpoint=?4",
            params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?,endpoint], |row| Ok(Totals {
                gap_attempts: unsigned(row,0)?, unresolved_actors: unsigned(row,1)?,
                unmapped_observations: unsigned(row,2)?, successful_observations: unsigned(row,3)?,
                observed_from: row.get(4)?, observed_through: row.get(5)?,
            })).optional()?)
}

impl Index {
    pub(super) fn whole_source_coverage(&self, scope: &ScopeQuery) -> Result<QueryCoverage> {
        let progress = self.progress(&scope.media_id, 0)?;
        if progress.status != "ready" {
            // A resumable partial import has no published whole-source summary.
            // Keep its existing coverage fields while all matches stay unknown.
            return self.coverage(
                scope,
                GameTick::new(WHOLE_START)?,
                GameTick::new(WHOLE_END)?,
                "events",
            );
        }
        let totals = cached(&self.db, scope, "events")?;
        let status = if totals.is_some() {
            progress.status
        } else {
            "unavailable".into()
        };
        let totals = totals.unwrap_or_default();
        Ok(QueryCoverage {
            index_status: status,
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
}
