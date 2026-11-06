//! Window-independent, resumable preparation. No media decoder or WebView is needed.
use crate::{
    league_index::{self, Index},
    library,
    playback_file::ApprovedRoot,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

mod changes;
use changes::{Changes, Watch};

#[derive(Default)]
struct Preparation {
    approved: Option<ApprovedRoot>,
    games: Option<Vec<library::GameSummary>>,
    completed: HashMap<PathBuf, String>,
}

struct Background {
    root: PathBuf,
    cache: PathBuf,
    state: Preparation,
    watch: Option<Watch>,
    approved: Option<ApprovedRoot>,
    retry: Changes,
    next_attempt: Instant,
    next_rescan: Instant,
}
impl Background {
    fn new(root: PathBuf, cache: PathBuf) -> Self {
        Self {
            watch: Watch::new(&root, &cache).ok(),
            root,
            cache,
            state: Preparation::default(),
            approved: None,
            retry: Changes::all(),
            next_attempt: Instant::now(),
            next_rescan: Instant::now() + Duration::from_secs(300),
        }
    }
    async fn tick(&mut self, stop: &Arc<AtomicBool>) -> Result<Vec<String>> {
        let now = Instant::now();
        if now < self.next_attempt {
            return Ok(Vec::new());
        }
        let mut changes = std::mem::take(&mut self.retry);
        if let Some(watch) = &self.watch {
            changes.merge(watch.take());
        }
        // Native notifications can overflow or be unavailable on network storage.
        // A rare authoritative rescan also recovers missed external edits.
        if now >= self.next_rescan {
            changes = Changes::all();
            self.watch = Watch::new(&self.root, &self.cache).ok();
            self.next_rescan = now + Duration::from_secs(300);
        }
        if !changes.rescan {
            changes.bundles.retain(|bundle, discover| {
                *discover
                    || self
                        .state
                        .games
                        .as_ref()
                        .is_some_and(|games| games.iter().any(|game| &game.timestamp == bundle))
            });
        }
        if changes.is_empty() {
            return Ok(Vec::new());
        }
        let approved = ApprovedRoot::new(&self.root);
        if let Ok(root) = &approved
            && self.approved.as_ref() != Some(root)
        {
            self.watch = Watch::new(&self.root, &self.cache).ok();
            self.approved = Some(root.clone());
            changes = Changes::all();
        }
        let result = match approved {
            Ok(_) => prepare(&self.root, &self.cache, stop, &mut self.state, &changes).await,
            Err(error) => Err(error),
        };
        if result.is_err() {
            // Keep hints across writer contention/transient errors; never spin.
            self.retry.merge(changes);
            self.next_attempt = Instant::now() + Duration::from_secs(5);
        }
        result
    }
}

pub(crate) struct Owner(Arc<AtomicBool>);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
pub(crate) fn start(app: tauri::AppHandle) -> Owner {
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = Arc::clone(&stop);
    tauri::async_runtime::spawn(async move {
        let mut background: Option<Background> = None;
        while !cancel.load(Ordering::Acquire) {
            let (root, cache) = {
                let state = app.state::<crate::AppState>();
                (
                    state.roots.output_directory(),
                    state.league_index_cache.clone(),
                )
            };
            if background
                .as_ref()
                .is_none_or(|b| b.root != root || b.cache != cache)
            {
                background = Some(Background::new(root, cache));
            }
            if let Some(background) = &mut background {
                match background.tick(&cancel).await {
                    Ok(media) if !media.is_empty() => {
                        let _ = app.emit("league-data-prepared", media);
                    }
                    Ok(_) => {}
                    Err(error) => eprintln!("Chronobreak background preparation: {error}"),
                }
            }
            // Coalesce bursts. Idle ticks do not open files or SQLite.
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    Owner(stop)
}

async fn prepare(
    root: &Path,
    cache: &Path,
    stop: &Arc<AtomicBool>,
    state: &mut Preparation,
    changes: &Changes,
) -> Result<Vec<String>> {
    if changes.is_empty() {
        return Ok(Vec::new());
    }
    let targets = changes
        .bundles
        .iter()
        .filter(|(bundle, discover)| {
            **discover
                || state
                    .games
                    .as_ref()
                    .is_some_and(|games| games.iter().any(|g| &g.timestamp == *bundle))
        })
        .map(|(bundle, _)| bundle.clone())
        .collect::<Vec<_>>();
    if !changes.rescan && targets.is_empty() {
        // Active recordings are unpublished. Their live journal writes require no preparation.
        return Ok(Vec::new());
    }
    let path = root.to_owned();
    let cancelled = Arc::clone(stop);
    let cached = state.games.clone();
    let previous_root = state.approved.clone();
    let full = changes.rescan || cached.is_none();
    let refresh = targets.clone();
    let (approved, games, logical_root, full) = tokio::task::spawn_blocking(move || {
        let approved = ApprovedRoot::new(&path)?;
        let full = full || previous_root.as_ref() != Some(&approved);
        ensure!(!cancelled.load(Ordering::Acquire), "preparation stopped");
        let games = if full {
            // Preparation does not need clip inventory, storage usage, or media probes.
            library::published_games(approved.path(), || {
                ensure!(!cancelled.load(Ordering::Acquire), "preparation stopped");
                Ok(())
            })?
        } else {
            let mut games = cached.unwrap_or_default();
            for bundle in refresh {
                ensure!(!cancelled.load(Ordering::Acquire), "preparation stopped");
                games.retain(|game| game.timestamp != bundle);
                if let Some(game) = library::published_game(approved.path(), &bundle) {
                    games.push(game);
                }
            }
            games
        };
        Ok::<_, anyhow::Error>((approved, games, path, full))
    })
    .await??;
    // Share the UI's approved identity for cache ownership and source reads.
    // Keep the configured path separately so alias retargeting still invalidates work.
    let library_root = approved.path().to_owned();
    state.approved = Some(approved.clone());
    state.games = Some(games.clone());
    let games = Arc::new(games);
    let mut changed = Vec::new();
    // Reconcile even an empty library. A fresh UI and the headless helper use
    // the same stable catalog identity, independent of UI admission tokens.
    let current = {
        let root = library_root.clone();
        let cache = cache.to_owned();
        let games = Arc::clone(&games);
        let completed = state.completed.clone();
        let targets = targets.clone();
        let approved = approved.clone();
        let logical_root = logical_root.clone();
        tokio::task::spawn_blocking(move || {
            ensure!(
                ApprovedRoot::new(&logical_root)? == approved,
                "preparation root changed"
            );
            let mut index = Index::open(&cache, root.as_path())?;
            index.reconcile(&games, "background")?;
            let mut pending = Vec::new();
            for game in games
                .iter()
                .filter(|game| full || targets.contains(&game.timestamp))
            {
                let directory = root.as_path().join("games").join(&game.timestamp);
                let stamp = match league_index::source_stamp(&directory) {
                    Ok(stamp) => stamp,
                    Err(error) => {
                        eprintln!("Chronobreak source preparation deferred: {error}");
                        continue;
                    }
                };
                let ready = index
                    .prepared_progress(root.as_path(), game)
                    .is_ok_and(|p| p.status != "pending");
                if completed.get(&directory) != Some(&stamp) || !ready {
                    pending.push((game.clone(), directory, stamp));
                }
            }
            Ok::<_, anyhow::Error>(pending)
        })
        .await??
    };
    state.completed.retain(|directory, _| {
        games
            .iter()
            .any(|g| library_root.join("games").join(&g.timestamp) == *directory)
    });
    // Make the entire catalog's inexpensive card facts available before a
    // single recording's journal/overlay can occupy the worker for seconds.
    for (game, _, _) in &current {
        let root = approved.clone();
        let logical_root = logical_root.clone();
        let cache = cache.to_owned();
        let game = game.clone();
        let cancelled = Arc::clone(stop);
        tokio::task::spawn_blocking(move || {
            let check = || {
                ensure!(!cancelled.load(Ordering::Acquire), "preparation stopped");
                ensure!(
                    ApprovedRoot::new(&logical_root)? == root,
                    "preparation root changed"
                );
                Ok(())
            };
            check()?;
            let mut index = Index::open(&cache, root.path())?;
            if let Err(error) = index.history_recording(root.path(), &game, false, &check) {
                check()?;
                eprintln!("Chronobreak card facts unavailable: {error}");
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        tokio::task::yield_now().await;
    }
    for (game, directory, stamp) in current {
        if stop.load(Ordering::Acquire) {
            break;
        }
        loop {
            let root = approved.clone();
            let library_root = library_root.clone();
            let logical_root = logical_root.clone();
            let cache = cache.to_owned();
            let media_id = game.media_id.clone();
            let game = game.clone();
            let cancelled = Arc::clone(stop);
            let result = tokio::task::spawn_blocking(move || {
                let check = || {
                    ensure!(!cancelled.load(Ordering::Acquire), "preparation stopped");
                    ensure!(
                        ApprovedRoot::new(&logical_root)? == root,
                        "preparation root changed"
                    );
                    Ok(())
                };
                check()?;
                let mut index = Index::open(&cache, library_root.as_path())?;
                match index.prepare_recording(library_root.as_path(), &game, &check) {
                    Ok(ready) => Ok(ready),
                    Err(error) => {
                        check()?;
                        index.preparation_failed(library_root.as_path(), &game)?;
                        eprintln!("Chronobreak optional recording data unavailable: {error}");
                        Ok(true)
                    }
                }
            })
            .await?;
            match result {
                Ok(true) => {
                    state.completed.insert(directory.clone(), stamp.clone());
                    if let Some(media) = &media_id {
                        changed.push(media.as_str().to_owned());
                    }
                    break;
                }
                Ok(false) => {}
                Err(error) => {
                    return Err(error);
                }
            }
            if stop.load(Ordering::Acquire) {
                break;
            }
            // The writer and connection have been released. Cooperate with
            // other work without imposing a delay for every committed prefix.
            tokio::task::yield_now().await;
        }
    }
    Ok(changed)
}

/// Called before Tauri startup, so the recorder can prepare data while the window is closed.
pub fn headless(args: &[std::ffi::OsString]) -> Result<()> {
    ensure!(
        args.len() == 2,
        "expected --prepare-library <absolute library>"
    );
    let root = PathBuf::from(&args[1]);
    ensure!(root.is_absolute(), "library must be absolute");
    let data = crate::config::preparation_data_directory()?;
    std::fs::create_dir_all(&data)?;
    let lock_path = data.join("league-preparation-owner.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    ensure!(
        !std::fs::symlink_metadata(&lock_path)?.is_symlink(),
        "preparation lock type"
    );
    if lock.try_lock().is_err() {
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("preparation runtime")?;
    runtime.block_on(async {
        let stop = Arc::new(AtomicBool::new(false));
        let mut background = Background::new(root.clone(), data.join("league-index"));
        loop {
            if let Err(error) = background.tick(&stop).await {
                eprintln!("Chronobreak headless preparation: {error}");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::league_index::{OverlayQuery, ScopeQuery};

    #[cfg(windows)]
    #[tokio::test]
    async fn preparation_matches_ui_cache_and_writer_for_path_spellings() {
        let library = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let approved = ApprovedRoot::new(library.path()).unwrap();
        let other_approved = ApprovedRoot::new(other.path()).unwrap();
        crate::library::tests::write_matched_game(
            library.path(),
            "100",
            "2026-08-05T10:00:00Z",
            false,
        );
        crate::library::tests::write_matched_game(
            other.path(),
            "200",
            "2026-08-05T10:00:00Z",
            false,
        );
        let stop = Arc::new(AtomicBool::new(false));
        let spellings = [
            library.path().to_owned(),
            PathBuf::from(library.path().to_str().unwrap().replace('\\', "/")),
            approved.path().to_owned(),
        ];
        assert_ne!(spellings[0].as_os_str(), spellings[2].as_os_str());
        for spelling in &spellings {
            let mut background = Background::new(spelling.clone(), cache.path().to_owned());
            background.tick(&stop).await.unwrap();
            let games = crate::library::list_games(approved.path()).unwrap();
            let reader = Index::reader(cache.path(), approved.path()).unwrap();
            assert_eq!(
                reader
                    .prepared_progress(approved.path(), &games[0])
                    .unwrap()
                    .status,
                "ready"
            );
            let source = reader
                .scopes(games[0].media_id.as_ref().unwrap())
                .unwrap()
                .remove(0)
                .source;
            let descriptor =
                crate::library::replay_descriptor(approved.path(), "", "100", String::new())
                    .unwrap();
            let overlay = reader
                .overlay_page(
                    &OverlayQuery {
                        scope: ScopeQuery {
                            media_id: source.media_id,
                            run_id: source.run_id,
                            epoch: source.epoch,
                        },
                        after: None,
                        revision: None,
                        limit: 128,
                        include_scoreboard: true,
                    },
                    &descriptor.media_timeline,
                )
                .unwrap();
            assert!(!overlay.samples.is_empty());
        }
        let databases = std::fs::read_dir(cache.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "sqlite3")
            })
            .count();
        assert_eq!(databases, 1);

        let _writer = Index::open(cache.path(), approved.path()).unwrap();
        for spelling in &spellings {
            assert!(
                prepare(
                    spelling,
                    cache.path(),
                    &stop,
                    &mut Preparation::default(),
                    &Changes::all()
                )
                .await
                .is_err()
            );
        }
        prepare(
            other.path(),
            cache.path(),
            &stop,
            &mut Preparation::default(),
            &Changes::all(),
        )
        .await
        .unwrap();
        assert_ne!(
            league_index::cache_path(cache.path(), approved.path()),
            league_index::cache_path(cache.path(), other_approved.path())
        );
        assert_eq!(
            Index::reader(cache.path(), other_approved.path())
                .unwrap()
                .history(
                    &crate::league_index::HistoryQuery {
                        limit: 10,
                        ..Default::default()
                    },
                    1
                )
                .unwrap()
                .rows[0]
                .game
                .timestamp,
            "200"
        );
    }

    #[tokio::test]
    async fn replacement_does_not_reuse_the_previous_library_catalog() {
        let directory = tempfile::tempdir().unwrap();
        let logical_root = directory.path().join("library");
        std::fs::create_dir(&logical_root).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let mut state = Preparation::default();
        crate::library::tests::write_matched_game(
            &logical_root,
            "100",
            "2026-08-05T10:00:00Z",
            false,
        );
        prepare(
            &logical_root,
            cache.path(),
            &stop,
            &mut state,
            &Changes::all(),
        )
        .await
        .unwrap();
        let previous = state.approved.clone().unwrap();
        std::fs::rename(&logical_root, directory.path().join("previous")).unwrap();
        std::fs::create_dir(&logical_root).unwrap();
        crate::library::tests::write_matched_game(
            &logical_root,
            "200",
            "2026-08-06T10:00:00Z",
            false,
        );
        let approved = ApprovedRoot::new(&logical_root).unwrap();
        assert_ne!(approved, previous);
        // A replacement between the background observer and preparation must
        // invalidate the cached catalog even if only an incremental hint arrives.
        let mut changes = Changes::default();
        changes.bundles.insert("200".into(), true);
        prepare(&logical_root, cache.path(), &stop, &mut state, &changes)
            .await
            .unwrap();
        assert_eq!(state.games.as_ref().unwrap().len(), 1);
        let page = Index::reader(cache.path(), approved.path())
            .unwrap()
            .history(
                &crate::league_index::HistoryQuery {
                    limit: 10,
                    ..Default::default()
                },
                1,
            )
            .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].game.timestamp, "200");
    }

    #[tokio::test]
    async fn prepares_without_a_window_survives_restart_and_recovers_a_reset_cache() {
        let directory = tempfile::tempdir().unwrap();
        let root = ApprovedRoot::new(directory.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        crate::library::tests::write_matched_game(
            root.path(),
            "100",
            "2026-08-05T10:00:00Z",
            false,
        );
        let stop = Arc::new(AtomicBool::new(false));
        let mut completed = Preparation::default();
        assert_eq!(
            prepare(
                root.path(),
                cache.path(),
                &stop,
                &mut completed,
                &Changes::all()
            )
            .await
            .unwrap()
            .len(),
            1
        );
        let games = library::list_games(root.path()).unwrap();
        let game = &games[0];
        {
            let reader = Index::reader(cache.path(), root.path()).unwrap();
            assert_eq!(
                reader.prepared_progress(root.path(), game).unwrap().status,
                "ready"
            );
            let source = reader
                .scopes(game.media_id.as_ref().unwrap())
                .unwrap()
                .remove(0)
                .source;
            let query = OverlayQuery {
                scope: ScopeQuery {
                    media_id: source.media_id,
                    run_id: source.run_id,
                    epoch: source.epoch,
                },
                after: None,
                revision: None,
                limit: 128,
                include_scoreboard: true,
            };
            let descriptor =
                library::replay_descriptor(root.path(), "", "100", String::new()).unwrap();
            let cached = reader
                .overlay_page(&query, &descriptor.media_timeline)
                .unwrap();
            assert!(!cached.samples.is_empty());
            let writer = Index::open(cache.path(), root.path()).unwrap();
            let computed = writer
                .compute_overlay_page(&query, &descriptor.media_timeline)
                .unwrap();
            assert_eq!(
                serde_json::to_value(cached).unwrap(),
                serde_json::to_value(computed).unwrap()
            );
        }
        assert!(
            prepare(
                root.path(),
                cache.path(),
                &stop,
                &mut completed,
                &Changes::all()
            )
            .await
            .unwrap()
            .is_empty()
        );
        crate::league_index::reset::reset(
            cache.path(),
            root.path(),
            &games,
            "new-ui-token",
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(
            prepare(
                root.path(),
                cache.path(),
                &stop,
                &mut completed,
                &Changes::all()
            )
            .await
            .unwrap()
            .len(),
            1
        );
        let mut restarted = Preparation::default();
        prepare(
            root.path(),
            cache.path(),
            &stop,
            &mut restarted,
            &Changes::all(),
        )
        .await
        .unwrap();
        assert_eq!(
            Index::reader(cache.path(), root.path())
                .unwrap()
                .prepared_progress(root.path(), game)
                .unwrap()
                .status,
            "ready"
        );
    }
    #[tokio::test]
    async fn idle_and_unpublished_telemetry_need_no_library_or_index_access() {
        let directory = tempfile::tempdir().unwrap();
        let root = ApprovedRoot::new(directory.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let mut background = Background::new(root.path().into(), cache.path().into());
        background.tick(&stop).await.unwrap();
        // Model idle notifications while the library cannot be read and the writer is owned.
        background.watch = None;
        background.retry = Changes::default();
        let _writer = Index::open(cache.path(), root.path()).unwrap();
        background.root = root.path().join("unavailable");
        assert!(background.tick(&stop).await.unwrap().is_empty());
        background.retry.bundles.insert("200".into(), false);
        assert!(background.tick(&stop).await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn prepares_only_changed_bundles_and_keeps_other_search_facts_ready() {
        let directory = tempfile::tempdir().unwrap();
        let root = ApprovedRoot::new(directory.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        crate::library::tests::write_matched_game(
            root.path(),
            "100",
            "2026-08-05T10:00:00Z",
            false,
        );
        let other = crate::library::tests::write_matched_game(
            root.path(),
            "101",
            "2026-08-06T10:00:00Z",
            false,
        );
        let mut state = Preparation::default();
        prepare(
            root.path(),
            cache.path(),
            &stop,
            &mut state,
            &Changes::all(),
        )
        .await
        .unwrap();
        let original = state.completed[&other].clone();
        let media = state
            .games
            .as_ref()
            .unwrap()
            .iter()
            .find(|g| g.timestamp == "100")
            .unwrap()
            .media_id
            .clone()
            .unwrap();
        // If an unrelated bundle were scanned, its invalid metadata would remove it.
        std::fs::write(other.join("metadata.json"), b"invalid unrelated metadata").unwrap();
        std::fs::write(
            root.path().join("games/100/league_history_result.json"),
            b"unavailable optional result",
        )
        .unwrap();
        let mut changes = Changes::default();
        changes.bundles.insert("100".into(), false);
        assert_eq!(
            prepare(root.path(), cache.path(), &stop, &mut state, &changes)
                .await
                .unwrap(),
            vec![media.as_str()]
        );
        assert_eq!(state.completed[&other], original);
        assert_eq!(state.games.as_ref().unwrap().len(), 2);
        // A new publication must not invalidate unchanged recordings' search readiness.
        crate::library::tests::write_matched_game(
            root.path(),
            "102",
            "2026-08-07T10:00:00Z",
            false,
        );
        changes.bundles.clear();
        changes.bundles.insert("102".into(), true);
        prepare(root.path(), cache.path(), &stop, &mut state, &changes)
            .await
            .unwrap();
        let reader = Index::reader(cache.path(), root.path()).unwrap();
        let query = crate::league_index::HistoryQuery {
            limit: 10,
            participant_riot_id: Some("Player#EUW".into()),
            ..Default::default()
        };
        let page = reader.history(&query, 3).unwrap();
        assert!(page.rows.iter().any(|row| row.media_id == media));
        drop(reader);
        changes.bundles.clear();
        changes.bundles.insert("101".into(), true);
        prepare(root.path(), cache.path(), &stop, &mut state, &changes)
            .await
            .unwrap();
        assert!(!state.completed.contains_key(&other));
        assert_eq!(state.games.as_ref().unwrap().len(), 2);
    }
    #[tokio::test]
    async fn writer_contention_retains_work_and_retries_with_backoff() {
        let directory = tempfile::tempdir().unwrap();
        let root = ApprovedRoot::new(directory.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        crate::library::tests::write_matched_game(
            root.path(),
            "100",
            "2026-08-05T10:00:00Z",
            false,
        );
        let writer = Index::open(cache.path(), root.path()).unwrap();
        let mut background = Background::new(root.path().into(), cache.path().into());
        assert!(background.tick(&stop).await.is_err());
        assert!(!background.retry.is_empty());
        assert!(background.next_attempt > Instant::now());
        assert!(background.tick(&stop).await.unwrap().is_empty());
        drop(writer);
        background.next_attempt = Instant::now();
        assert_eq!(background.tick(&stop).await.unwrap().len(), 1);
        assert!(background.retry.is_empty());
    }
    #[tokio::test]
    async fn isolates_a_corrupt_recording_and_prunes_removed_bundles() {
        let directory = tempfile::tempdir().unwrap();
        let root = ApprovedRoot::new(directory.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let bad = crate::library::tests::write_matched_game(
            root.path(),
            "101",
            "2026-08-06T10:00:00Z",
            false,
        );
        let metadata = bad.join("metadata.json");
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
        let old = json["media_id"].as_str().unwrap().to_owned();
        std::fs::write(
            &metadata,
            serde_json::to_string(&json)
                .unwrap()
                .replace(&old, "11111111-2222-4333-8444-555555555556"),
        )
        .unwrap();
        std::fs::write(bad.join("league_observations.jsonl"), b"broken").unwrap();
        crate::library::tests::write_matched_game(
            root.path(),
            "100",
            "2026-08-05T10:00:00Z",
            false,
        );
        let stop = Arc::new(AtomicBool::new(false));
        let mut completed = Preparation::default();
        prepare(
            root.path(),
            cache.path(),
            &stop,
            &mut completed,
            &Changes::all(),
        )
        .await
        .unwrap();
        let games = library::list_games(root.path()).unwrap();
        let reader = Index::reader(cache.path(), root.path()).unwrap();
        for game in &games {
            assert_eq!(
                reader.prepared_progress(root.path(), game).unwrap().status,
                if game.timestamp == "100" {
                    "ready"
                } else {
                    "unavailable"
                }
            );
        }
        drop(reader);
        std::fs::remove_dir_all(&bad).unwrap();
        prepare(
            root.path(),
            cache.path(),
            &stop,
            &mut completed,
            &Changes::all(),
        )
        .await
        .unwrap();
        assert!(!completed.completed.contains_key(&bad));
    }
}
