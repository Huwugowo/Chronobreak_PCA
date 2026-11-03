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
    time::Duration,
};
use tauri::{Emitter, Manager};

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
        let mut completed = HashMap::new();
        while !cancel.load(Ordering::Acquire) {
            let (root, cache) = {
                let state = app.state::<crate::AppState>();
                (
                    state.roots.output_directory(),
                    state.league_index_cache.clone(),
                )
            };
            match prepare(&root, &cache, &cancel, &mut completed).await {
                Ok(media) => {
                    if !media.is_empty() {
                        let _ = app.emit("league-data-prepared", media);
                    }
                }
                Err(error) => eprintln!("Chronobreak background preparation: {error}"),
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
    Owner(stop)
}

pub(crate) async fn prepare(
    root: &Path,
    cache: &Path,
    stop: &Arc<AtomicBool>,
    completed: &mut HashMap<PathBuf, String>,
) -> Result<Vec<String>> {
    let path = root.to_owned();
    let cancelled = Arc::clone(stop);
    let (approved, snapshot, library_root) = tokio::task::spawn_blocking(move || {
        let approved = ApprovedRoot::new(&path)?;
        let snapshot =
            library::build_library_snapshot(approved.path(), "", "background".into(), || {
                ensure!(!cancelled.load(Ordering::Acquire), "preparation stopped");
                Ok(())
            })?;
        Ok::<_, anyhow::Error>((approved, snapshot, path))
    })
    .await??;
    let snapshot = Arc::new(snapshot);
    let mut changed = Vec::new();
    // Reconcile even an empty library. A fresh UI and the headless helper use
    // the same stable catalog identity, independent of UI admission tokens.
    let current = {
        let root = library_root.clone();
        let cache = cache.to_owned();
        let snapshot = Arc::clone(&snapshot);
        let completed = completed.clone();
        tokio::task::spawn_blocking(move || {
            let mut index = Index::open(&cache, root.as_path())?;
            index.reconcile(&snapshot.games, &snapshot.token)?;
            let mut pending = Vec::new();
            for game in &snapshot.games {
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
    completed.retain(|directory, _| {
        snapshot
            .games
            .iter()
            .any(|g| library_root.join("games").join(&g.timestamp) == *directory)
    });
    for (game, directory, stamp) in current {
        if stop.load(Ordering::Acquire) {
            break;
        }
        loop {
            let root = approved.clone();
            let library_root = library_root.clone();
            let cache = cache.to_owned();
            let media_id = game.media_id.clone();
            let game = game.clone();
            let snapshot = Arc::clone(&snapshot);
            let cancelled = Arc::clone(stop);
            let result = tokio::task::spawn_blocking(move || {
                let check = || {
                    ensure!(!cancelled.load(Ordering::Acquire), "preparation stopped");
                    ensure!(
                        ApprovedRoot::new(root.path())? == root,
                        "preparation root changed"
                    );
                    Ok(())
                };
                check()?;
                let mut index = Index::open(&cache, library_root.as_path())?;
                index.reconcile(&snapshot.games, &snapshot.token)?;
                // Another owner may already have prepared this recording.
                if index
                    .prepared_progress(library_root.as_path(), &game)
                    .is_ok_and(|p| p.status != "pending")
                {
                    return Ok(true);
                }
                let progress = match index.recording(library_root.as_path(), &game, false, &check) {
                    Ok(progress) => progress,
                    Err(error) => {
                        check()?;
                        index.preparation_failed(library_root.as_path(), &game)?;
                        eprintln!("Chronobreak optional recording data unavailable: {error}");
                        return Ok(true);
                    }
                };
                if progress.status == "ready" {
                    let descriptor = library::replay_descriptor(
                        library_root.as_path(),
                        "",
                        &game.timestamp,
                        String::new(),
                    )?;
                    return index.prepare_overlay(&descriptor.media_timeline);
                }
                Ok(progress.status != "pending")
            })
            .await?;
            match result {
                Ok(true) => {
                    completed.insert(directory.clone(), stamp.clone());
                    if let Some(media) = &media_id {
                        changed.push(media.as_str().to_owned());
                    }
                    break;
                }
                Ok(false) => {}
                Err(error) => {
                    eprintln!("Chronobreak recording preparation deferred: {error}");
                    break;
                }
            }
            if stop.load(Ordering::Acquire) {
                break;
            }
            // Work is bounded by a journal batch or one overlay page. Yield between batches.
            tokio::time::sleep(Duration::from_millis(25)).await;
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
        let mut completed = HashMap::new();
        loop {
            if let Err(error) =
                prepare(&root, &data.join("league-index"), &stop, &mut completed).await
            {
                eprintln!("Chronobreak headless preparation: {error}");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::league_index::{OverlayQuery, ScopeQuery};
    #[tokio::test]
    async fn prepares_without_a_window_survives_restart_and_recovers_a_reset_cache() {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        crate::library::tests::write_game(root.path(), "100", "2026-08-05T10:00:00Z", false);
        let stop = Arc::new(AtomicBool::new(false));
        let mut completed = HashMap::new();
        assert_eq!(
            prepare(root.path(), cache.path(), &stop, &mut completed)
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
            prepare(root.path(), cache.path(), &stop, &mut completed)
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
            prepare(root.path(), cache.path(), &stop, &mut completed)
                .await
                .unwrap()
                .len(),
            1
        );
        let mut restarted = HashMap::new();
        prepare(root.path(), cache.path(), &stop, &mut restarted)
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
    async fn isolates_a_corrupt_recording_and_prunes_removed_bundles() {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let bad =
            crate::library::tests::write_game(root.path(), "101", "2026-08-06T10:00:00Z", false);
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
        crate::library::tests::write_game(root.path(), "100", "2026-08-05T10:00:00Z", false);
        let stop = Arc::new(AtomicBool::new(false));
        let mut completed = HashMap::new();
        prepare(root.path(), cache.path(), &stop, &mut completed)
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
        prepare(root.path(), cache.path(), &stop, &mut completed)
            .await
            .unwrap();
        assert!(!completed.contains_key(&bad));
    }
}
