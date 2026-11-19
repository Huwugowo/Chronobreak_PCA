//! Coverage work belongs to the bounded importer, not every history page.
use super::*;

pub(super) const BASE_UNKNOWN: u8 = 1;
pub(super) const QUEUE_UNKNOWN: u8 = 2;
pub(super) const OUTCOME_UNKNOWN: u8 = 4;
pub(super) const ACTOR_UNKNOWN: u8 = 8;
pub(super) const EVENTS_UNKNOWN: u8 = 16;
pub(super) const SOURCE_UNCHECKED: u8 = 32;

impl Index {
    /// Backfill existing projections once, without rebuilding source facts.
    pub(super) fn install_history_coverage(db: &mut Connection) -> Result<()> {
        let installed: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='history_coverage_counts')",
            [],
            |row| row.get(0),
        )?;
        if !installed {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("history_coverage.sql"))?;
            tx.commit()?;
        }
        Ok(())
    }

    pub(super) fn history_counts(&self, mask: u8) -> Result<(u64, u64)> {
        let sql = if self.history_coverage_ready {
            "SELECT COALESCE(SUM(matches),0),
                COALESCE(SUM(matches) FILTER (WHERE (flags & ?1)!=0),0)
             FROM history_coverage_counts"
        } else {
            // A reader admitted before the first writer upgrades an existing cache
            // still sees the old schema for its entire read transaction.
            "SELECT COUNT(*),COALESCE(SUM(unknown),0) FROM (
                SELECT MAX(facts_current=0 OR (history_flags & ?1)!=0) AS unknown
                FROM recordings WHERE match_group IS NOT NULL GROUP BY match_group
            )"
        };
        Ok(self.db.query_row(sql, [mask], |row| {
            Ok((unsigned(row, 0)?, unsigned(row, 1)?))
        })?)
    }

    /// Materialize one recording's coverage classes after a source-stamp check.
    /// The 64 possible masks have incrementally maintained catalog counts.
    pub(super) fn refresh_history_summary(db: &Connection, media: &MediaId) -> Result<()> {
        let pending: bool = db.query_row(
            "SELECT telemetry_status='pending' FROM recordings WHERE media_id=?1",
            [media.as_str()],
            |r| r.get(0),
        )?;
        if pending {
            db.execute(
                "UPDATE recordings SET history_flags=31 WHERE media_id=?1 AND history_flags!=31",
                [media.as_str()],
            )?;
            return Ok(());
        }
        let flags: u8 = db.query_row(
            "SELECT
                CASE WHEN r.telemetry_status!='ready' OR r.tail_bytes>0 OR EXISTS(SELECT 1 FROM gaps g WHERE g.media_id=r.media_id) THEN 1 ELSE 0 END
                | CASE WHEN r.queue_id IS NULL THEN 2 ELSE 0 END
                | CASE WHEN r.outcome='unknown' THEN 4 ELSE 0 END
                | CASE WHEN EXISTS(SELECT 1 FROM effective_event_actors a WHERE a.media_id=r.media_id AND a.status!='resolved') THEN 8 ELSE 0 END
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
        db.execute(
            "UPDATE recordings SET history_flags=?2 WHERE media_id=?1",
            params![media.as_str(), flags],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_coverage_backfill_and_updates_share_the_recording_transaction() {
        let library = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let (cache, root) = (cache.path(), library.path());
        let old_schema = Connection::open(cache_path(cache, root)).unwrap();
        old_schema
            .execute_batch("PRAGMA journal_mode=WAL;")
            .unwrap();
        old_schema
            .execute_batch(include_str!("schema.sql"))
            .unwrap();
        old_schema
            .execute(
                "INSERT INTO cache_identity(root,version,catalog_token) VALUES (?1,?2,'existing')",
                params![root.to_str().unwrap(), VERSION],
            )
            .unwrap();
        old_schema.execute_batch(
            "INSERT INTO recordings(media_id,bundle,recorded_at,champion,mode,saved,outcome,summary_json,match_group,facts_current,history_flags)
             VALUES ('a','a','','','',0,'unknown','{}','first',1,2),
                    ('b','b','','','',0,'unknown','{}','first',0,4),
                    ('c','c','','','',0,'unknown','{}','second',1,8),
                    ('d','d','','','',0,'unknown','{}',NULL,0,31);"
        ).unwrap();
        drop(old_schema);

        let base = BASE_UNKNOWN | SOURCE_UNCHECKED;
        let legacy_reader = Index::reader(cache, root).unwrap();
        assert!(!legacy_reader.history_coverage_ready);
        assert_eq!(legacy_reader.history_counts(base).unwrap(), (2, 1));

        let mut writer = Index::open(cache, root).unwrap();
        writer.require_catalog("existing").unwrap();
        let reader_before = Index::reader(cache, root).unwrap();
        assert!(reader_before.history_coverage_ready);
        assert_eq!(
            reader_before.history_counts(base | ACTOR_UNKNOWN).unwrap(),
            (2, 2)
        );

        let tx = writer.db.transaction().unwrap();
        tx.execute(
            "UPDATE recordings SET match_group='second', facts_current=1 WHERE media_id='b'",
            [],
        )
        .unwrap();
        tx.execute("DELETE FROM recordings WHERE media_id='c'", [])
            .unwrap();
        let during_write = Index::reader(cache, root).unwrap();
        assert_eq!(during_write.history_counts(base).unwrap(), (2, 1));
        tx.commit().unwrap();

        let reader_after = Index::reader(cache, root).unwrap();
        assert_eq!(
            reader_after.history_counts(base | ACTOR_UNKNOWN).unwrap(),
            (2, 0)
        );
        assert_eq!(
            reader_after.history_counts(base | QUEUE_UNKNOWN).unwrap(),
            (2, 1)
        );
        assert_eq!(
            reader_after.history_counts(base | OUTCOME_UNKNOWN).unwrap(),
            (2, 1)
        );
        assert_eq!(
            reader_after
                .history_counts(base | OUTCOME_UNKNOWN | QUEUE_UNKNOWN)
                .unwrap(),
            (2, 2)
        );
        assert_eq!(reader_before.history_counts(base).unwrap(), (2, 1));
        assert_eq!(legacy_reader.history_counts(base).unwrap(), (2, 1));

        let tx = writer.db.transaction().unwrap();
        tx.execute("DELETE FROM recordings", []).unwrap();
        tx.rollback().unwrap();
        assert_eq!(writer.history_counts(base).unwrap(), (2, 0));
        writer
            .db
            .execute("DELETE FROM recordings WHERE media_id='a'", [])
            .unwrap();
        assert_eq!(writer.history_counts(base | QUEUE_UNKNOWN).unwrap(), (1, 0));
    }
}
