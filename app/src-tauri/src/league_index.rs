//! App-only disposable projection of committed source facts. Recording files and
//! the opened media boundary remain authoritative. Imports and queries are bounded.
use crate::library::{self, GameSummary};
use anyhow::{Context, Result, bail, ensure};
use chronobreak_league_data::{
    ResultFile, Stats,
    observation::{self as obs, checkpoint, journal, manifest},
};
use chronobreak_replay_time::{GameTick, MediaId};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

mod actors;
mod authored;
mod coverage;
mod history_summary;
mod import;
mod overlay;
mod query;
mod replay;
pub(crate) mod reset;
pub(crate) use authored::AuthoredPage;
pub(crate) use overlay::{OverlayPage, OverlayQuery};
pub(crate) use replay::{ReplayWindow, ReplayWindowQuery};
pub(crate) use reset::ResetResult;
#[cfg(test)]
mod tests;
pub(crate) use query::{
    EventWindow, HistoryPage, HistoryQuery, RosterAt, ScopeQuery, SourceScope, WindowQuery,
};

const VERSION: i64 = 8;
const JOURNAL: &str = "league_observations.jsonl";
const MAX_JOURNAL: u64 = 4 * 1024 * 1024 * 1024;
// Keep the foreground import page within the IPC contract.
const IMPORT_BATCHES: usize = 32;

#[derive(Clone, Copy)]
pub(crate) enum ImportWork<'a> {
    Interactive,
    Background {
        deadline: Instant,
        cancelled: &'a AtomicBool,
    },
}

pub(crate) struct Index {
    db: Connection,
    _writer: Option<fs::File>,
}

/// The cache stays on the app's local data volume, even for a network library.
pub(crate) fn cache_path(cache: &Path, approved_root: &Path) -> PathBuf {
    let digest = Sha256::digest(approved_root.as_os_str().as_encoded_bytes());
    let key: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    cache.join(format!("league-{key}-v{VERSION}.sqlite3"))
}

impl Index {
    pub(crate) fn reader(cache: &Path, root: &Path) -> Result<Self> {
        let path = cache_path(cache, root);
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_file() && !meta.is_symlink(),
            "invalid index cache file"
        );
        let db = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.busy_timeout(std::time::Duration::from_millis(50))?;
        db.set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            5 * 1024 * 1024,
        )?;
        db.execute_batch(
            "PRAGMA trusted_schema=OFF; PRAGMA cache_size=-4096; PRAGMA temp_store=MEMORY;",
        )?;
        // All statements in this short-lived reader see one WAL snapshot.
        db.execute_batch("BEGIN DEFERRED")?;
        let identity: (String, i64) =
            db.query_row("SELECT root,version FROM cache_identity", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        ensure!(
            identity.0 == root.to_str().context("index root encoding")? && identity.1 == VERSION,
            "index identity/version"
        );
        Ok(Self { db, _writer: None })
    }

    pub(crate) fn open(cache: &Path, root: &Path) -> Result<Self> {
        let writer = Self::writer_lock(cache, root)?;
        Self::open_locked(cache, root, Some(writer))
    }

    pub(super) fn writer_lock(cache: &Path, root: &Path) -> Result<fs::File> {
        fs::create_dir_all(cache)?;
        let path = cache_path(cache, root).with_extension("lock");
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        ensure!(
            lock.metadata()?.is_file() && !fs::symlink_metadata(&path)?.is_symlink(),
            "index lock type"
        );
        lock.try_lock().context("index preparation already owned")?;
        Ok(lock)
    }

    pub(super) fn open_locked(cache: &Path, root: &Path, writer: Option<fs::File>) -> Result<Self> {
        let path = cache_path(cache, root);
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(
                meta.is_file() && !meta.is_symlink(),
                "invalid index cache file"
            );
        }
        let db = Connection::open(path)?;
        ensure!(
            rusqlite::version_number() >= 3_051_003,
            "SQLite WAL fix required"
        );
        db.busy_timeout(std::time::Duration::from_millis(50))?;
        db.set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            5 * 1024 * 1024,
        )?;
        db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL; PRAGMA wal_autocheckpoint=256; PRAGMA journal_size_limit=8388608;
            PRAGMA cache_size=-4096; PRAGMA temp_store=MEMORY;")?;
        let root = root.to_str().context("index root encoding")?;
        let has_identity_table: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='cache_identity')",
            [],
            |r| r.get(0),
        )?;
        let identity: Option<(String, i64)> = if has_identity_table {
            db.query_row("SELECT root,version FROM cache_identity", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?
        } else {
            None
        };
        if let Some((existing, version)) = identity {
            ensure!(
                existing == root && version == VERSION,
                "index identity/version"
            );
        } else {
            // Identity is published only after all schema statements succeed.
            // An interrupted initialization can replay the idempotent schema.
            db.execute_batch(include_str!("league_index/schema.sql"))?;
            db.execute(
                "INSERT INTO cache_identity(root,version) VALUES (?1,?2)",
                params![root, VERSION],
            )?;
        }
        let mode: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        ensure!(mode == "wal", "index requires WAL mode");
        Ok(Self {
            db,
            _writer: writer,
        })
    }

    pub(crate) fn require_catalog(&self, token: &str) -> Result<()> {
        let current: Option<String> =
            self.db
                .query_row("SELECT catalog_token FROM cache_identity", [], |r| r.get(0))?;
        ensure!(
            current.as_deref() == Some(token),
            "index catalog is pending for this snapshot"
        );
        Ok(())
    }

    pub(crate) fn require_source(&self, root: &Path, bundle: &str, media: &MediaId) -> Result<()> {
        let (accepted, stamp): (String, Option<String>) = self.db.query_row(
            "SELECT bundle,source_stamp FROM recordings WHERE media_id=?1",
            [media.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let stamp = stamp
            .filter(|_| accepted == bundle)
            .context("recording index is pending source refresh")?;
        ensure!(
            stamp == source_stamp(&root.join("games").join(bundle))?,
            "recording index is pending source refresh"
        );
        Ok(())
    }

    pub(crate) fn reconcile(&mut self, games: &[GameSummary], _token: &str) -> Result<()> {
        let token = catalog_key(games)?;
        let token = token.as_str();
        self.refresh_authored(self.root_path()?.as_path())?;
        if self.require_catalog(token).is_ok() {
            return Ok(());
        }
        // Membership comes from the accepted filesystem scan, never this cache.
        let mut counts = std::collections::HashMap::new();
        for game in games {
            if let Some(media) = &game.media_id {
                *counts.entry(media).or_insert(0usize) += 1;
            }
        }
        let tx = self.db.transaction()?;
        tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS present(bundle TEXT PRIMARY KEY,media_id TEXT UNIQUE); DELETE FROM present;")?;
        {
            let mut insert = tx.prepare_cached("INSERT INTO present VALUES (?1,?2)")?;
            for game in games {
                if let Some(media) = &game.media_id
                    && counts.get(media) == Some(&1)
                {
                    insert.execute(params![game.timestamp, media.as_str()])?;
                }
            }
        }
        tx.execute("DELETE FROM recordings WHERE NOT EXISTS(SELECT 1 FROM present p WHERE p.bundle=recordings.bundle AND p.media_id=recordings.media_id)", [])?;
        {
            let mut update = tx.prepare_cached("INSERT INTO recordings(bundle,media_id,recorded_at,champion,mode,saved,summary_json,outcome,match_group)
            VALUES (?1,?2,?3,?4,?5,?6,?7,'unknown',?8) ON CONFLICT(media_id) DO UPDATE SET
            recorded_at=excluded.recorded_at,champion=excluded.champion,mode=excluded.mode,saved=excluded.saved,summary_json=excluded.summary_json,match_group=excluded.match_group,
            facts_current=CASE WHEN recordings.summary_json=excluded.summary_json THEN recordings.facts_current ELSE 0 END")?;
            for game in games {
                if let Some(media) = &game.media_id
                    && counts.get(media) == Some(&1)
                {
                    update.execute(params![
                        game.timestamp,
                        media.as_str(),
                        game.recorded_at,
                        game.champion,
                        game.game_mode,
                        game.saved,
                        serde_json::to_string(game)?,
                        game.match_group,
                    ])?;
                }
            }
        }
        tx.execute("UPDATE cache_identity SET catalog_token=?1", [token])?;
        tx.commit()?;
        Ok(())
    }

    fn root_path(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(self.db.query_row::<String, _, _>(
            "SELECT root FROM cache_identity",
            [],
            |r| r.get(0),
        )?))
    }

    pub(crate) fn recording(
        &mut self,
        root: &Path,
        summary: &GameSummary,
        rebuild: bool,
        check: &(impl Fn() -> Result<()> + ?Sized),
    ) -> Result<ImportProgress> {
        let source = self.history_recording(root, summary, rebuild, check)?;
        self.import_recording_source(root, &source, rebuild, ImportWork::Interactive, check)
    }

    fn import_recording_source(
        &mut self,
        root: &Path,
        source: &RecordingSource,
        rebuild: bool,
        work: ImportWork<'_>,
        check: &(impl Fn() -> Result<()> + ?Sized),
    ) -> Result<ImportProgress> {
        let media = &source.timeline.media_id;
        let old_stamp: Option<String> = self.db.query_row(
            "SELECT source_stamp FROM recordings WHERE media_id=?1",
            [media.as_str()],
            |r| r.get(0),
        )?;
        if !rebuild && old_stamp.as_deref() == Some(&source.stamp) {
            return self.progress(media, 0);
        }
        let result = self.import(
            root,
            &source.directory,
            &source.timeline,
            &source.stamp,
            work,
            check,
        );
        if result.is_err() {
            // Supersession rolls back this unit without discarding its
            // previously published prefix.
            check()?;
            self.clear_telemetry(media)?;
            check()?;
            self.db.execute("UPDATE recordings SET telemetry_status='unavailable',source_stamp=?2 WHERE media_id=?1", params![media.as_str(),source.stamp])?;
        }
        self.refresh_history_summary(media)?;
        result
    }

    pub(crate) fn prepare_recording(
        &mut self,
        root: &Path,
        summary: &GameSummary,
        work: ImportWork<'_>,
        check: &(impl Fn() -> Result<()> + ?Sized),
    ) -> Result<bool> {
        let source = self.history_recording(root, summary, false, check)?;
        let progress = self.import_recording_source(root, &source, false, work, check)?;
        if progress.status == "ready" {
            return self.prepare_overlay(&source.timeline);
        }
        Ok(progress.status != "pending")
    }

    /// Publish small, validated card facts independently of journal/overlay work.
    pub(crate) fn history_recording(
        &mut self,
        root: &Path,
        summary: &GameSummary,
        rebuild: bool,
        check: &(impl Fn() -> Result<()> + ?Sized),
    ) -> Result<RecordingSource> {
        check()?;
        let descriptor = library::replay_descriptor(root, "", &summary.timestamp, String::new())?;
        let media = descriptor.media_timeline.media_id.clone();
        ensure!(
            summary.media_id.as_ref() == Some(&media),
            "recording identity changed since accepted scan"
        );
        let directory = root.join("games").join(&summary.timestamp);
        // Fingerprint before decoding facts. A result arriving during this read
        // must invalidate this version, rather than bless older decoded facts.
        let stamp = source_stamp(&directory)?;
        let timeline_json = serde_json::to_string(&descriptor.media_timeline)?;
        let summary_json = serde_json::to_string(summary)?;
        if !rebuild {
            let current: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM recordings WHERE media_id=?1 AND bundle=?2
                 AND history_stamp=?3 AND summary_json=?4 AND timeline_json=?5 AND facts_current=1)",
                params![media.as_str(), summary.timestamp, stamp, summary_json, timeline_json],
                |r| r.get(0),
            )?;
            if current {
                check()?;
                ensure!(
                    source_stamp(&directory)? == stamp,
                    "history source changed during preparation"
                );
                return Ok(RecordingSource {
                    directory,
                    timeline: descriptor.media_timeline,
                    stamp,
                });
            }
        }
        let candidate = crate::league_match::read(&directory, &media);
        ensure!(
            candidate.as_ref().map(|candidate| &candidate.group_key)
                == summary.match_group.as_ref(),
            "recording match identity changed since scan"
        );
        let result = crate::league_result::read(&directory, &media, candidate.as_ref());
        // Same media ID in two bundles is ambiguous and must never silently move.
        let previous: Option<String> = self
            .db
            .query_row(
                "SELECT bundle FROM recordings WHERE media_id=?1",
                [media.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        ensure!(
            previous.as_ref().is_none_or(|p| p == &summary.timestamp),
            "duplicate media identity in library"
        );
        let old_media: Option<String> = self
            .db
            .query_row(
                "SELECT media_id FROM recordings WHERE bundle=?1",
                [&summary.timestamp],
                |r| r.get(0),
            )
            .optional()?;
        if old_media.as_deref().is_some_and(|id| id != media.as_str()) {
            self.db.execute(
                "DELETE FROM recordings WHERE bundle=?1",
                [&summary.timestamp],
            )?;
        }
        let result_json = result.as_ref().map(serde_json::to_string).transpose()?;
        let previous_result: Option<String> = self
            .db
            .query_row(
                "SELECT result_json FROM recordings WHERE media_id=?1",
                [media.as_str()],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        self.db.execute(
            "UPDATE recordings SET facts_current=0 WHERE media_id=?1 AND (summary_json!=?2 OR result_json IS NOT ?3)",
            params![media.as_str(), summary_json, result_json],
        )?;
        self.db.execute("INSERT INTO recordings(media_id,bundle,recorded_at,champion,mode,saved,queue_id,map_id,outcome,game_id,summary_json,result_json,match_group)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
            ON CONFLICT(media_id) DO UPDATE SET recorded_at=excluded.recorded_at,champion=excluded.champion,mode=excluded.mode,saved=excluded.saved,
              queue_id=excluded.queue_id,map_id=excluded.map_id,outcome=excluded.outcome,game_id=excluded.game_id,summary_json=excluded.summary_json,result_json=excluded.result_json",
            params![media.as_str(),summary.timestamp,summary.recorded_at,summary.champion,summary.game_mode,summary.saved,
                result.as_ref().and_then(|r| r.queue_id).or_else(|| candidate.as_ref().map(|c| u64::from(c.queue_id))).map(sql_u64).transpose()?,
                result.as_ref().and_then(|r| r.map_id).or_else(|| candidate.as_ref().map(|c| u64::from(c.map_id))).map(sql_u64).transpose()?,
                result.as_ref().map(|r| name(&r.outcome)).transpose()?.unwrap_or_else(|| "unknown".into()),
                candidate.as_ref().map(|c| c.game_id.as_str()),summary_json,previous_result,summary.match_group])?;
        if previous_result != result_json {
            self.final_facts(&media, result.as_ref())?;
            self.db.execute(
                "UPDATE recordings SET result_json=?2 WHERE media_id=?1",
                params![media.as_str(), result_json],
            )?;
        }
        let old_timeline: Option<String> = self.db.query_row(
            "SELECT timeline_json FROM recordings WHERE media_id=?1",
            [media.as_str()],
            |r| r.get(0),
        )?;
        if rebuild || old_timeline.as_ref() != Some(&timeline_json) {
            self.clear_telemetry(&media)?;
        }
        self.db.execute(
            "UPDATE recordings SET timeline_json=?2 WHERE media_id=?1",
            params![media.as_str(), timeline_json],
        )?;
        check()?;
        ensure!(
            source_stamp(&directory)? == stamp,
            "history source changed during preparation"
        );
        self.db.execute(
            "UPDATE recordings SET history_stamp=?2 WHERE media_id=?1",
            params![media.as_str(), stamp],
        )?;
        self.refresh_history_summary(&media)?;
        Ok(RecordingSource {
            directory,
            timeline: descriptor.media_timeline,
            stamp,
        })
    }

    pub(crate) fn progress(&self, media: &MediaId, batches: u32) -> Result<ImportProgress> {
        self.db.query_row("SELECT telemetry_status,position_json,tail_bytes,facts_current FROM recordings WHERE media_id=?1", [media.as_str()], |r| {
            let position: Option<String> = r.get(1)?;
            Ok(ImportProgress { media_id: media.clone(), status: r.get(0)?, batches,
                position: position.and_then(|s| serde_json::from_str(&s).ok()), tail_bytes: unsigned(r,2)?, history_ready: r.get(3)? })
        }).map_err(Into::into)
    }

    pub(crate) fn prepared_progress(
        &self,
        root: &Path,
        summary: &GameSummary,
    ) -> Result<ImportProgress> {
        let media = summary
            .media_id
            .as_ref()
            .context("recording media unavailable")?;
        let mut progress = self.progress(media, 0)?;
        let history_stamp: Option<String> = self.db.query_row(
            "SELECT history_stamp FROM recordings WHERE media_id=?1",
            [media.as_str()],
            |row| row.get(0),
        )?;
        progress.history_ready &= history_stamp.is_some_and(|stamp| {
            source_stamp(&root.join("games").join(&summary.timestamp))
                .is_ok_and(|current| current == stamp)
        });
        if self
            .require_source(root, &summary.timestamp, media)
            .is_err()
        {
            progress.status = "pending".into();
            return Ok(progress);
        }
        if progress.status == "ready" {
            for source in self.scopes(media)? {
                let scope = ScopeQuery {
                    media_id: source.source.media_id,
                    run_id: source.source.run_id,
                    epoch: source.source.epoch,
                };
                let expected = serde_json::to_string(
                    &self
                        .whole_source_coverage(&scope)?
                        .committed
                        .context("source revision")?,
                )?;
                let ready:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM prepared_overlay_sources WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND complete=1 AND revision=?4)",params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?,expected],|r|r.get(0))?;
                if !ready {
                    progress.status = "pending".into();
                    break;
                }
            }
        }
        Ok(progress)
    }

    pub(crate) fn preparation_failed(&mut self, root: &Path, game: &GameSummary) -> Result<()> {
        let media = game.media_id.as_ref().context("preparation media")?;
        let stamp = source_stamp(&root.join("games").join(&game.timestamp))?;
        self.db.execute("UPDATE recordings SET telemetry_status='unavailable',source_stamp=?2 WHERE media_id=?1",params![media.as_str(),stamp])?;
        Ok(())
    }

    fn clear_telemetry(&mut self, media: &MediaId) -> Result<()> {
        let tx = self.db.transaction()?;
        for table in [
            "prepared_overlay_samples",
            "prepared_overlay_sources",
            "commits",
            "source_coverage",
            "scopes",
            "evidence",
            "records",
            "calibrations",
            "participants",
            "roster_payloads",
            "rosters",
            "roster_members",
            "event_revisions",
            "events",
            "event_actors",
            "derived_actor_links",
            "gaps",
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE media_id=?1"),
                [media.as_str()],
            )?;
        }
        tx.execute("UPDATE recordings SET source_stamp=NULL,journal_header=NULL,source_identity=NULL,position_json=NULL,checkpoint_json=NULL,telemetry_status='pending',tail_bytes=0,facts_current=0 WHERE media_id=?1", [media.as_str()])?;
        tx.commit()?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ImportProgress {
    pub media_id: MediaId,
    pub status: String,
    pub batches: u32,
    pub position: Option<journal::Position>,
    pub tail_bytes: u64,
    pub history_ready: bool,
}

pub(crate) struct RecordingSource {
    directory: PathBuf,
    timeline: chronobreak_replay_time::MediaTimelineV2,
    stamp: String,
}
fn sql_u64(value: u64) -> Result<i64> {
    value
        .try_into()
        .context("source integer exceeds SQLite range")
}
fn unsigned(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    value.try_into().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}
fn optional_unsigned(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    let value: Option<i64> = row.get(index)?;
    value
        .map(|value| {
            value.try_into().map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Integer,
                    Box::new(error),
                )
            })
        })
        .transpose()
}
fn name(value: &impl Serialize) -> Result<String> {
    let value = serde_json::to_value(value)?;
    value
        .as_str()
        .map(str::to_owned)
        .context("source enum name")
}
fn bounded(path: &Path, cap: usize) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.is_symlink() && meta.len() <= cap as u64,
        "optional source size/type"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= cap, "optional source size");
    Ok(bytes)
}
pub(crate) fn source_stamp(directory: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    // Small manifests/sidecars are cheap content fingerprints. The append-only
    // journal's size/change time avoids rereading unchanged history; explicit
    // rebuild verifies the whole checksum chain.
    for (file, cap) in [
        ("metadata.json", 64 * 1024),
        (manifest::FILE, manifest::MAX_BYTES),
        ("league_match.json", 64 * 1024),
        (
            chronobreak_league_data::RESULT_FILE,
            chronobreak_league_data::MAX_RESULT_BYTES,
        ),
        (
            chronobreak_league_data::HISTORY_RESULT_FILE,
            chronobreak_league_data::MAX_RESULT_BYTES,
        ),
    ] {
        hash.update(file.as_bytes());
        match bounded(&directory.join(file), cap) {
            Ok(bytes) => hash.update(bytes),
            Err(_) => hash.update(b"unavailable"),
        }
    }
    if let Ok(meta) = fs::symlink_metadata(directory.join(JOURNAL)) {
        hash.update(meta.len().to_le_bytes());
        if meta.is_file() && !meta.is_symlink() {
            let file = fs::File::open(directory.join(JOURNAL))?;
            hash.update(
                crate::playback_file::file_facts(&file)?
                    .generation_key()
                    .as_bytes(),
            );
        }
        if let Ok(modified) = meta.modified()?.duration_since(std::time::UNIX_EPOCH) {
            hash.update(modified.as_nanos().to_le_bytes());
        }
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Stable across headless preparation and UI refreshes; admission tokens remain separate.
pub(crate) fn catalog_key(games: &[GameSummary]) -> Result<String> {
    let mut rows = games
        .iter()
        .map(|g| {
            (
                g.timestamp.as_str(),
                g.media_id.as_ref().map(MediaId::as_str),
                serde_json::to_string(&(
                    &g.recorded_at,
                    &g.champion,
                    &g.game_mode,
                    g.saved,
                    &g.match_group,
                )),
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    let mut hash = Sha256::new();
    for (bundle, media, json) in rows {
        hash.update(bundle.as_bytes());
        hash.update(media.unwrap_or("").as_bytes());
        hash.update(json?.as_bytes());
    }
    Ok(format!("{:x}", hash.finalize()))
}
