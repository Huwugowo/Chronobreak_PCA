//! Notifications are bounded wake-up hints; preparation rereads authoritative files.
use notify::{Event, EventKind, RecursiveMode, Watcher, event::ModifyKind};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
pub(super) struct Changes {
    pub rescan: bool,
    // true also discovers a newly published bundle; telemetry alone does not.
    pub bundles: BTreeMap<String, bool>,
}
impl Changes {
    pub fn all() -> Self {
        Self {
            rescan: true,
            ..Self::default()
        }
    }
    pub fn is_empty(&self) -> bool {
        !self.rescan && self.bundles.is_empty()
    }
    pub fn merge(&mut self, other: Self) {
        if self.rescan || other.rescan {
            *self = Self::all();
            return;
        }
        for (bundle, discover) in other.bundles {
            *self.bundles.entry(bundle).or_default() |= discover;
        }
        if self.bundles.len() > 256 {
            *self = Self::all();
        }
    }
    fn event(&mut self, root: &Path, database: &Path, event: &Event) {
        if event.need_rescan() {
            *self = Self::all();
        }
        if self.rescan || matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        for path in &event.paths {
            if path == database {
                // Ordinary SQLite/WAL writes are not source changes. Replacement is.
                if matches!(
                    event.kind,
                    EventKind::Create(_)
                        | EventKind::Remove(_)
                        | EventKind::Modify(ModifyKind::Name(_))
                ) {
                    *self = Self::all();
                }
                continue;
            }
            if path == root || path == &root.join("games") {
                if matches!(
                    event.kind,
                    EventKind::Create(_)
                        | EventKind::Remove(_)
                        | EventKind::Modify(ModifyKind::Name(_))
                ) {
                    *self = Self::all();
                }
                continue;
            }
            let Ok(relative) = path.strip_prefix(root.join("games")) else {
                continue;
            };
            let mut parts = relative.iter();
            let Some(bundle) = parts.next().and_then(|part| part.to_str()) else {
                continue;
            };
            if !crate::library::valid_game_id(bundle) {
                continue;
            }
            let leaf = parts.next().and_then(|part| part.to_str());
            if parts.next().is_some() {
                continue;
            }
            let discover = match leaf {
                // Windows also reports the bundle directory itself as modified
                // when its live files change. Publication is signaled by the
                // final files, or by a directory entering/leaving the catalog.
                None => matches!(
                    event.kind,
                    EventKind::Any
                        | EventKind::Other
                        | EventKind::Create(_)
                        | EventKind::Remove(_)
                        | EventKind::Modify(ModifyKind::Name(_))
                ),
                Some("metadata.json" | "video.mp4") => true,
                _ => false,
            };
            if discover
                || matches!(
                    leaf,
                    Some(
                        "game_log.json"
                            | "league_observations.jsonl"
                            | "league_match.json"
                            | "league_result.json"
                            | "league_history_result.json"
                    )
                )
            {
                let mut change = Self::default();
                change.bundles.insert(bundle.into(), discover);
                self.merge(change);
            }
        }
    }
}

pub(super) struct Watch {
    _watcher: notify::RecommendedWatcher,
    pending: Arc<Mutex<Changes>>,
}
impl Watch {
    pub fn new(root: &Path, approved_root: &Path, cache: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(cache)?;
        let pending = Arc::new(Mutex::new(Changes::default()));
        let changes = Arc::clone(&pending);
        let watched_root = root.to_owned();
        let database = crate::league_index::cache_path(cache, approved_root);
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(mut pending) = changes.lock() else {
                return;
            };
            match event {
                Ok(event) => pending.event(&watched_root, &database, &event),
                Err(_) => *pending = Changes::all(),
            }
        })?;
        watcher.watch(root, RecursiveMode::Recursive)?;
        // Detect replacement of the selected root itself, as well as its contents.
        if let Some(parent) = root.parent() {
            watcher.watch(parent, RecursiveMode::NonRecursive)?;
        }
        watcher.watch(cache, RecursiveMode::NonRecursive)?;
        Ok(Self {
            _watcher: watcher,
            pending,
        })
    }
    pub fn take(&self) -> Changes {
        self.pending
            .lock()
            .map(|mut pending| std::mem::take(&mut *pending))
            .unwrap_or_else(|_| Changes::all())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, RemoveKind};
    use std::path::PathBuf;
    #[test]
    fn coalesces_targets_ignores_video_growth_and_recovers_ambiguous_changes() {
        let root = PathBuf::from("library");
        let db = PathBuf::from("cache/index.sqlite3");
        let mut changes = Changes::default();
        for _ in 0..20 {
            changes.event(
                &root,
                &db,
                &Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Any)))
                    .add_path(root.join("games/100/league_history_result.json")),
            );
        }
        changes.event(
            &root,
            &db,
            &Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Any)))
                .add_path(root.join("games/200/video.mp4.part")),
        );
        changes.event(
            &root,
            &db,
            &Event::new(EventKind::Modify(ModifyKind::Any)).add_path(root.join("games/200")),
        );
        assert_eq!(changes.bundles.len(), 1);
        assert_eq!(changes.bundles.get("100"), Some(&false));
        changes.event(
            &root,
            &db,
            &Event::new(EventKind::Create(CreateKind::File))
                .add_path(root.join("games/200/metadata.json")),
        );
        assert_eq!(changes.bundles.get("200"), Some(&true));
        changes.event(
            &root,
            &db,
            &Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(root.join("games/100")),
        );
        assert_eq!(changes.bundles.get("100"), Some(&true));
        changes.event(
            &root,
            &db,
            &Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Any))).add_path(db.clone()),
        );
        assert!(!changes.rescan);
        changes.event(
            &root,
            &db,
            &Event::new(EventKind::Remove(RemoveKind::File)).add_path(db.clone()),
        );
        assert!(changes.rescan);
    }
    #[test]
    fn native_notification_targets_a_new_history_sidecar() {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let bundle = root.path().join("games/100");
        std::fs::create_dir_all(&bundle).unwrap();
        let watch = Watch::new(root.path(), root.path(), cache.path()).unwrap();
        std::fs::write(
            bundle.join("league_history_result.json"),
            b"new optional facts",
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut changes = Changes::default();
        while changes.is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            changes.merge(watch.take());
        }
        assert!(!changes.rescan);
        assert_eq!(changes.bundles.get("100"), Some(&false));
    }

    #[test]
    fn native_notification_wakes_on_approved_cache_rename() {
        let parent = tempfile::tempdir().unwrap();
        let logical = parent.path().join("library");
        std::fs::create_dir(&logical).unwrap();
        let logical = logical.join("..").join("library");
        let approved = crate::playback_file::ApprovedRoot::new(&logical).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let database = crate::league_index::cache_path(cache.path(), approved.path());
        std::fs::write(&database, b"fixture cache").unwrap();
        let watch = Watch::new(&logical, approved.path(), cache.path()).unwrap();
        std::fs::rename(&database, database.with_extension("old")).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if watch.take().rescan {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("cache rename did not request a rescan");
    }
}
