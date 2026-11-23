use super::*;
use crate::directory_guard::{DirectoryGuard, guard_root, open_regular};
use crate::playback_file::ApprovedRoot;
use chronobreak_league_data::{MAX_RANKED_LP_BYTES, RANKED_LP_FILE, RankedLp, ranked_lp_queue};
use std::collections::BTreeSet;

pub(crate) struct RankedLpProjection {
    pub bundle: String,
    pub media: String,
    pub delta: Option<i32>,
    pub updated: bool,
}

pub(super) fn receipt_delta(
    directory: &Path,
    media: &MediaId,
    game: &str,
    queue: u32,
) -> Option<i32> {
    ranked_lp_queue(queue)?;
    let file = open_regular(&directory.join(RANKED_LP_FILE), false, false).ok()?;
    let mut bytes = Vec::new();
    file.take((MAX_RANKED_LP_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(RankedLp::decode(&bytes, media, game, queue)?.delta)
}

impl Index {
    pub(super) fn install_ranked_lp_projection(db: &mut Connection) -> Result<()> {
        let tx = db.transaction()?;
        let has_column: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('recordings') WHERE name='ranked_lp_delta')",
            [], |row| row.get(0),
        )?;
        if !has_column {
            tx.execute_batch("ALTER TABLE recordings ADD COLUMN ranked_lp_delta INTEGER;")?;
        }
        tx.execute_batch(
            "CREATE INDEX recordings_ranked_lp ON recordings(match_group,ranked_lp_delta,queue_id)
             WHERE ranked_lp_delta IS NOT NULL AND facts_current=1;",
        )?;
        tx.commit()?;
        Ok(())
    }

    /// This path does no catalog reconciliation or replay preparation. It uses
    /// the already checked binding and touches only the indicated receipt files.
    pub(crate) fn refresh_ranked_lp(
        &self,
        root: &ApprovedRoot,
        bundles: &BTreeSet<String>,
        full: bool,
    ) -> Result<Vec<RankedLpProjection>> {
        const SELECT: &str =
            "SELECT bundle,media_id,game_id,queue_id,ranked_lp_delta FROM recordings
            WHERE facts_current=1 AND queue_id IN (420,440) AND game_id IS NOT NULL";
        let read = |row: &rusqlite::Row<'_>| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, Option<i32>>(4)?,
            ))
        };
        let mut parts = Vec::new();
        if full {
            let mut statement = self.db.prepare(SELECT)?;
            for part in statement.query_map([], read)? {
                parts.push(part?);
            }
        } else {
            let sql = format!("{SELECT} AND bundle=?1");
            let mut statement = self.db.prepare(&sql)?;
            for bundle in bundles {
                if let Some(part) = statement.query_row([bundle], read).optional()? {
                    parts.push(part);
                }
            }
        }
        if parts.is_empty() {
            return Ok(Vec::new());
        }

        let guarded_root = guard_root(root)?;
        let games = guarded_root.child("games", false)?;
        let mut projections = Vec::with_capacity(parts.len());
        let mut update = self.db.prepare_cached(
            "UPDATE recordings SET ranked_lp_delta=?2
             WHERE media_id=?1 AND ranked_lp_delta IS NOT ?2",
        )?;
        for (bundle, media, game, queue, previous) in parts {
            let current = if full && previous.is_none() {
                // Most files are absent. Avoid opening a pinned directory in that case.
                let directory = games.path.join(&bundle);
                if fs::symlink_metadata(directory.join(RANKED_LP_FILE)).is_ok() {
                    guarded_delta(&games, &bundle, &media, &game, queue)
                } else {
                    None
                }
            } else {
                guarded_delta(&games, &bundle, &media, &game, queue)
            };
            let updated = current != previous;
            if updated {
                update.execute(params![media, current])?;
            }
            projections.push(RankedLpProjection {
                bundle,
                media,
                delta: current,
                updated,
            });
        }
        Ok(projections)
    }
}

fn guarded_delta(
    games: &DirectoryGuard,
    bundle: &str,
    media: &str,
    game: &str,
    queue: u32,
) -> Option<i32> {
    receipt_delta(
        &games.child(bundle, false).ok()?.path,
        &MediaId::parse(media).ok()?,
        game,
        queue,
    )
}
