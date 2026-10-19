//! Coverage work belongs to the bounded importer, not every history page.
use super::*;

pub(super) const BASE_UNKNOWN: u8 = 1;
pub(super) const QUEUE_UNKNOWN: u8 = 2;
pub(super) const OUTCOME_UNKNOWN: u8 = 4;
pub(super) const ACTOR_UNKNOWN: u8 = 8;
pub(super) const EVENTS_UNKNOWN: u8 = 16;
pub(super) const SOURCE_UNCHECKED: u8 = 32;

impl Index {
    /// Materialize one recording's coverage classes after a source-stamp check.
    /// The 64 possible masks have incrementally maintained catalog counts.
    pub(super) fn refresh_history_summary(&self, media: &MediaId) -> Result<()> {
        let pending: bool = self.db.query_row(
            "SELECT telemetry_status='pending' FROM recordings WHERE media_id=?1",
            [media.as_str()],
            |r| r.get(0),
        )?;
        if pending {
            self.db.execute(
                "UPDATE recordings SET history_flags=31,facts_current=1 WHERE media_id=?1",
                [media.as_str()],
            )?;
            return Ok(());
        }
        let flags: u8 = self.db.query_row(
            "SELECT
                CASE WHEN r.telemetry_status!='ready' OR r.tail_bytes>0 OR EXISTS(SELECT 1 FROM gaps g WHERE g.media_id=r.media_id) THEN 1 ELSE 0 END
                | CASE WHEN r.queue_id IS NULL THEN 2 ELSE 0 END
                | CASE WHEN r.outcome='unknown' THEN 4 ELSE 0 END
                | CASE WHEN EXISTS(SELECT 1 FROM event_actors a WHERE a.media_id=r.media_id AND a.status!='resolved') THEN 8 ELSE 0 END
                | CASE WHEN NOT EXISTS(SELECT 1 FROM scopes s WHERE s.media_id=r.media_id) OR EXISTS(
                    SELECT 1 FROM scopes s WHERE s.media_id=r.media_id AND (
                        NOT EXISTS(SELECT 1 FROM evidence e WHERE e.media_id=s.media_id AND e.run_id=s.run_id AND e.epoch=s.epoch AND e.endpoint='events' AND e.observed=1)
                        OR EXISTS(SELECT 1 FROM evidence e LEFT JOIN calibrations c ON c.media_id=e.media_id AND c.run_id=e.run_id AND c.epoch=e.epoch AND c.observation_id=e.calibration_id
                            LEFT JOIN evidence g ON g.media_id=e.media_id AND g.run_id=e.run_id AND g.epoch=e.epoch AND g.observation_id=e.game_stats_id AND g.endpoint='game_stats'
                            WHERE e.media_id=s.media_id AND e.run_id=s.run_id AND e.epoch=s.epoch AND e.endpoint='events' AND e.observed=1 AND (g.game_tick IS NULL OR COALESCE(json_extract(c.data,'$.status'),'unavailable')!='available'))
                        OR (SELECT MIN(g.game_tick) FROM evidence e JOIN evidence g ON g.media_id=e.media_id AND g.run_id=e.run_id AND g.epoch=e.epoch AND g.observation_id=e.game_stats_id
                            WHERE e.media_id=s.media_id AND e.run_id=s.run_id AND e.epoch=s.epoch AND e.endpoint='events' AND e.observed=1)
                            > (SELECT MIN(g.game_tick) FROM evidence g WHERE g.media_id=s.media_id AND g.run_id=s.run_id AND g.epoch=s.epoch AND g.endpoint='game_stats' AND g.observed=1)
                        OR (SELECT MAX(g.game_tick) FROM evidence e JOIN evidence g ON g.media_id=e.media_id AND g.run_id=e.run_id AND g.epoch=e.epoch AND g.observation_id=e.game_stats_id
                            WHERE e.media_id=s.media_id AND e.run_id=s.run_id AND e.epoch=s.epoch AND e.endpoint='events' AND e.observed=1)
                            < (SELECT MAX(g.game_tick) FROM evidence g WHERE g.media_id=s.media_id AND g.run_id=s.run_id AND g.epoch=s.epoch AND g.endpoint='game_stats' AND g.observed=1)
                    )) THEN 16 ELSE 0 END
             FROM recordings r WHERE r.media_id=?1",
            [media.as_str()], |r| r.get(0),
        )?;
        self.db.execute(
            "UPDATE recordings SET history_flags=?2,facts_current=1 WHERE media_id=?1",
            params![media.as_str(), flags],
        )?;
        Ok(())
    }
}
