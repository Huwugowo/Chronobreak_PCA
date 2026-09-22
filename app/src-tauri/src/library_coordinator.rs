//! One process-wide blocking scan owner and revision-checked publication.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::Semaphore;

use crate::clip_duration::{self, DurationState, ObservedDuration};
use crate::library::{self, LibrarySnapshot};
use crate::playback_file::ApprovedRoot;
use crate::playback_server::{MediaRoots, OutputDirectory};

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub(crate) enum LibraryRefreshError {
    Busy,
    Superseded,
    InvalidSelection,
    InvalidBatch,
    Failed {
        message: String,
        retained_snapshot_token: Option<String>,
    },
}

struct PublishedSnapshot {
    path: PathBuf,
    root: ApprovedRoot,
    snapshot: Arc<LibrarySnapshot>,
}

#[derive(Default)]
struct Publication {
    revision: u64,
    root_epoch: u64,
    selected_token: Option<String>,
    current: Option<PublishedSnapshot>,
    active_cancel: Option<Arc<AtomicBool>>,
    duration_cancel: Option<Arc<AtomicBool>>,
    durations: HashMap<String, ObservedDuration>,
    closed: bool,
}

impl Publication {
    fn invalidate(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("library revision exhausted");
        if let Some(cancelled) = self.active_cancel.take() {
            cancelled.store(true, Ordering::Release);
        }
        if let Some(cancelled) = self.duration_cancel.take() {
            cancelled.store(true, Ordering::Release);
        }
        self.selected_token = None;
        self.durations.clear();
    }
}

struct ScanCapture {
    revision: u64,
    path: PathBuf,
    root: ApprovedRoot,
    origin: String,
    token: String,
    cancelled: Arc<AtomicBool>,
}

impl ScanCapture {
    fn check_current(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.cancelled.load(Ordering::Acquire),
            "library scan superseded"
        );
        Ok(())
    }
}

/// Dropping an async caller cancels publication, but does not release its worker slot.
struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[derive(Clone)]
pub(crate) struct LibraryCoordinator {
    publication: Arc<Mutex<Publication>>,
    scan_slot: Arc<Semaphore>,
    duration_slot: Arc<Semaphore>,
    mutation_slot: Arc<Semaphore>,
    replay_slot: Arc<Semaphore>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ClipDurationResult {
    pub clip_id: String,
    pub duration: DurationState,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ClipDurations {
    pub snapshot_token: String,
    pub clips: Vec<ClipDurationResult>,
}

#[derive(Clone, Copy)]
pub(crate) enum Selection<'a> {
    Game(&'a str),
    Clip(&'a str),
    Library,
}

pub(crate) struct RootCapture {
    pub path: PathBuf,
    root: ApprovedRoot,
    root_epoch: u64,
    token: String,
}

/// Invalidates even if a worker panics or an export future is dropped after side effects.
pub(crate) struct MutationCompletion {
    coordinator: LibraryCoordinator,
    roots: Arc<MediaRoots>,
    pub capture: RootCapture,
    // Keeps the single mutation owner until the operation's side effects and
    // completion admission have both been evaluated.
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl MutationCompletion {
    pub(crate) fn selection_is_current(&self) -> bool {
        let publication = self
            .coordinator
            .publication
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        !publication.closed
            && publication.root_epoch == self.capture.root_epoch
            && publication.selected_token.as_deref() == Some(&self.capture.token)
    }
}

impl Drop for MutationCompletion {
    fn drop(&mut self) {
        self.coordinator
            .invalidate_capture(&self.roots, &self.capture);
    }
}

impl LibraryCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            publication: Arc::new(Mutex::new(Publication::default())),
            scan_slot: Arc::new(Semaphore::new(1)),
            duration_slot: Arc::new(Semaphore::new(1)),
            mutation_slot: Arc::new(Semaphore::new(1)),
            replay_slot: Arc::new(Semaphore::new(1)),
        }
    }

    pub(crate) async fn refresh(
        &self,
        roots: &MediaRoots,
        origin: &str,
    ) -> Result<LibrarySnapshot, LibraryRefreshError> {
        self.refresh_with(roots, origin, |capture| {
            library::build_library_snapshot(
                &capture.path,
                &capture.origin,
                capture.token.clone(),
                || capture.check_current(),
            )
        })
        .await
    }

    async fn refresh_with(
        &self,
        roots: &MediaRoots,
        origin: &str,
        build: impl FnOnce(&ScanCapture) -> anyhow::Result<LibrarySnapshot> + Send + 'static,
    ) -> Result<LibrarySnapshot, LibraryRefreshError> {
        // Never queue semaphore waiters. The permit belongs to the actual OS work.
        let permit =
            Arc::clone(&self.scan_slot)
                .try_acquire_owned()
                .map_err(|error| match error {
                    tokio::sync::TryAcquireError::Closed => LibraryRefreshError::Superseded,
                    tokio::sync::TryAcquireError::NoPermits => LibraryRefreshError::Busy,
                })?;
        // Generate the opaque wire token before taking the publication guard.
        let token = crate::playback_policy::random_capability().map_err(|_| {
            LibraryRefreshError::Failed {
                message: "could not allocate library snapshot token".into(),
                retained_snapshot_token: None,
            }
        })?;
        let capture = {
            let mut publication = self.publication.lock().unwrap_or_else(|p| p.into_inner());
            if publication.closed {
                return Err(LibraryRefreshError::Superseded);
            }
            publication.invalidate();
            let (path, root) = roots.output_pair();
            let cancelled = Arc::new(AtomicBool::new(false));
            publication.active_cancel = Some(Arc::clone(&cancelled));
            ScanCapture {
                revision: publication.revision,
                path,
                root,
                origin: origin.to_owned(),
                token,
                cancelled,
            }
        };
        let capture = Arc::new(capture);
        let _cancel_on_drop = CancelOnDrop(Arc::clone(&capture.cancelled));
        let worker_capture = Arc::clone(&capture);
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            worker_capture
                .check_current()
                .and_then(|()| build(&worker_capture))
        })
        .await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("library scan worker failed")));
        let result = result.map(Arc::new);
        let (snapshot, previous) = {
            let mut publication = self.publication.lock().unwrap_or_else(|p| p.into_inner());
            let (path, root) = roots.output_pair();
            if publication.closed
                || capture.cancelled.load(Ordering::Acquire)
                || capture.revision != publication.revision
                || path != capture.path
                || root.path() != capture.root.path()
            {
                return Err(LibraryRefreshError::Superseded);
            }
            publication.active_cancel = None;
            // Preserve a complete same-root view and report its token on failure.
            let snapshot = result.map_err(|error| LibraryRefreshError::Failed {
                message: format!("{error:#}"),
                retained_snapshot_token: publication
                    .current
                    .as_ref()
                    .filter(|current| current.path == path && current.root.path() == root.path())
                    .map(|current| current.snapshot.token.clone()),
            })?;
            let previous = publication.current.replace(PublishedSnapshot {
                path: capture.path.clone(),
                root: capture.root.clone(),
                snapshot: Arc::clone(&snapshot),
            });
            publication.selected_token = Some(capture.token.clone());
            (snapshot, previous)
        };
        // Release old data and copy the IPC response outside the publication guard.
        drop(previous);
        Ok((*snapshot).clone())
    }

    /// Called only after directory validation and settings persistence succeed.
    /// Fixed lock order: coordinator, then MediaRoots. No I/O under either guard.
    pub(crate) fn publish_output_directory(&self, roots: &MediaRoots, directory: OutputDirectory) {
        let previous =
            {
                let mut publication = self.publication.lock().unwrap_or_else(|p| p.into_inner());
                roots.set_output_directory(directory);
                publication.root_epoch = publication
                    .root_epoch
                    .checked_add(1)
                    .expect("root epoch exhausted");
                publication.invalidate();
                let (path, root) = roots.output_pair();
                if publication.current.as_ref().is_some_and(|current| {
                    current.path != path || current.root.path() != root.path()
                }) {
                    publication.current.take()
                } else {
                    None
                }
            };
        drop(previous);
    }

    pub(crate) fn shutdown(&self) {
        let mut publication = self.publication.lock().unwrap_or_else(|p| p.into_inner());
        publication.closed = true;
        publication.invalidate();
        self.scan_slot.close();
        self.duration_slot.close();
        self.mutation_slot.close();
        self.replay_slot.close();
    }

    fn capture_selection(
        &self,
        roots: &MediaRoots,
        token: &str,
        selection: Selection<'_>,
    ) -> Result<RootCapture, LibraryRefreshError> {
        let publication = self.publication.lock().unwrap_or_else(|p| p.into_inner());
        if publication.closed || publication.selected_token.as_deref() != Some(token) {
            return Err(LibraryRefreshError::Superseded);
        }
        let current = publication
            .current
            .as_ref()
            .ok_or(LibraryRefreshError::Superseded)?;
        let (path, root) = roots.output_pair();
        if path != current.path || root.path() != current.root.path() {
            return Err(LibraryRefreshError::Superseded);
        }
        let valid = match selection {
            Selection::Game(id) => {
                library::valid_game_id(id)
                    && current
                        .snapshot
                        .games
                        .iter()
                        .any(|game| game.timestamp == id)
            }
            Selection::Clip(id) => {
                library::valid_clip_asset(&format!("{id}.mp4"))
                    && current
                        .snapshot
                        .clips
                        .iter()
                        .any(|clip| clip.filename == id)
            }
            Selection::Library => true,
        };
        if !valid {
            return Err(LibraryRefreshError::InvalidSelection);
        }
        Ok(RootCapture {
            path,
            root,
            root_epoch: publication.root_epoch,
            token: token.into(),
        })
    }

    /// One owned blocking read shared by descriptor and full-probe requests.
    pub(crate) async fn read_replay<T: Send + 'static>(
        &self,
        roots: &MediaRoots,
        token: &str,
        game: &str,
        build: impl FnOnce(&std::path::Path) -> anyhow::Result<T> + Send + 'static,
    ) -> Result<T, LibraryRefreshError> {
        let permit =
            Arc::clone(&self.replay_slot)
                .try_acquire_owned()
                .map_err(|error| match error {
                    tokio::sync::TryAcquireError::Closed => LibraryRefreshError::Superseded,
                    tokio::sync::TryAcquireError::NoPermits => LibraryRefreshError::Busy,
                })?;
        let capture = self.capture_selection(roots, token, Selection::Game(game))?;
        let path = capture.path.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            build(&path)
        })
        .await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("replay read worker failed")));
        let current = self.capture_selection(roots, token, Selection::Game(game))?;
        if current.root_epoch != capture.root_epoch
            || current.path != capture.path
            || current.root.path() != capture.root.path()
        {
            return Err(LibraryRefreshError::Superseded);
        }
        result.map_err(|error| LibraryRefreshError::Failed {
            message: error.to_string(),
            retained_snapshot_token: None,
        })
    }

    fn invalidate_capture(&self, roots: &MediaRoots, capture: &RootCapture) {
        let mut publication = self.publication.lock().unwrap_or_else(|p| p.into_inner());
        let (path, root) = roots.output_pair();
        // An export can finish after A/B/A or a same-root settings publication.
        // Its response is stale, but its files still changed A's current view.
        if path == capture.path && root.path() == capture.root.path() {
            publication.invalidate();
        }
    }

    pub(crate) fn mutation_permit(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, LibraryRefreshError> {
        Arc::clone(&self.mutation_slot)
            .try_acquire_owned()
            .map_err(|error| match error {
                tokio::sync::TryAcquireError::Closed => LibraryRefreshError::Superseded,
                tokio::sync::TryAcquireError::NoPermits => LibraryRefreshError::Busy,
            })
    }

    #[cfg(test)]
    pub(crate) async fn settings<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let permit = self.mutation_permit().map_err(command_error)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work()
        })
        .await
        .map_err(|_| "settings worker failed".to_owned())?
    }

    pub(crate) async fn settings_for_snapshot<T: Send + 'static>(
        &self,
        roots: &Arc<MediaRoots>,
        token: &str,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        // Report a stale snapshot deterministically before a concurrent
        // mutation can mask it as a generic busy result. Re-capture after
        // taking the permit to close the root-change race.
        self.capture_selection(roots, token, Selection::Library)
            .map_err(command_error)?;
        let permit = self.mutation_permit().map_err(command_error)?;
        // Admission is tied to the snapshot that the settings UI displayed.
        // The permit is moved into the worker and remains owned until it exits.
        self.capture_selection(roots, token, Selection::Library)
            .map_err(command_error)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work()
        })
        .await
        .map_err(|_| "settings worker failed".to_owned())?
    }

    pub(crate) fn admit_export(
        &self,
        roots: &Arc<MediaRoots>,
        token: &str,
        game: &str,
    ) -> Result<MutationCompletion, LibraryRefreshError> {
        self.capture_selection(roots, token, Selection::Game(game))?;
        let admission = self.mutation_permit()?;
        let capture = self.capture_selection(roots, token, Selection::Game(game))?;
        Ok(MutationCompletion {
            coordinator: self.clone(),
            roots: Arc::clone(roots),
            capture,
            _permit: admission,
        })
    }

    pub(crate) async fn mutate<T: Send + 'static>(
        &self,
        roots: &Arc<MediaRoots>,
        token: &str,
        selection: Selection<'_>,
        work: impl FnOnce(&std::path::Path) -> anyhow::Result<T> + Send + 'static,
    ) -> Result<T, String> {
        // Check identity before contending for the single mutation owner so a
        // stale token remains a superseded request rather than being reported
        // as busy behind an unrelated operation. Re-capture after admission
        // to close the root-change race between these checks.
        self.capture_selection(roots, token, selection)
            .map_err(command_error)?;
        let permit = self.mutation_permit().map_err(command_error)?;
        let capture = self
            .capture_selection(roots, token, selection)
            .map_err(command_error)?;
        let completion = MutationCompletion {
            coordinator: self.clone(),
            roots: Arc::clone(roots),
            capture,
            _permit: permit,
        };
        tokio::task::spawn_blocking(move || {
            let completion = completion;
            // Conservatively invalidate every admitted attempt, including partial errors.
            let result = work(&completion.capture.path).map_err(crate::error_string);
            drop(completion);
            result
        })
        .await
        .map_err(|_| "library mutation worker failed".to_owned())?
    }

    pub(crate) async fn resolve_durations(
        &self,
        roots: &MediaRoots,
        origin: &str,
        token: &str,
        ids: Vec<String>,
        ffprobe: Option<PathBuf>,
        retry_unavailable: bool,
    ) -> Result<ClipDurations, LibraryRefreshError> {
        self.resolve_durations_with(roots, token, ids, retry_unavailable, {
            let origin = origin.to_owned();
            move |capture, id, cancelled, cached| {
                clip_duration::resolve(
                    &capture.path.join("clips").join(format!("{id}.mp4")),
                    &capture.root,
                    &origin,
                    id,
                    ffprobe.as_deref(),
                    || cancelled.load(Ordering::Acquire),
                    cached,
                )
            }
        })
        .await
    }

    async fn resolve_durations_with(
        &self,
        roots: &MediaRoots,
        token: &str,
        ids: Vec<String>,
        retry_unavailable: bool,
        probe: impl Fn(&RootCapture, &str, &AtomicBool, Option<&ObservedDuration>) -> ObservedDuration
        + Send
        + 'static,
    ) -> Result<ClipDurations, LibraryRefreshError> {
        if ids.is_empty() || ids.len() > 8 || ids.iter().collect::<HashSet<_>>().len() != ids.len()
        {
            return Err(LibraryRefreshError::InvalidBatch);
        }
        let permit = Arc::clone(&self.duration_slot)
            .try_acquire_owned()
            .map_err(|error| match error {
                tokio::sync::TryAcquireError::Closed => LibraryRefreshError::Superseded,
                tokio::sync::TryAcquireError::NoPermits => LibraryRefreshError::Busy,
            })?;
        // Keep admission closed through publication as well as OS work. A dropped
        // waiter releases only its reference; the blocking worker retains ownership.
        let waiter_permit = Arc::new(permit);
        let permit = Arc::clone(&waiter_permit);
        let capture = self.capture_selection(roots, token, Selection::Library)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let (revision, cached) = {
            let mut publication = self.publication.lock().unwrap_or_else(|p| p.into_inner());
            // Capture and registration are separate short guards; reject intervening invalidation.
            if publication.closed || publication.selected_token.as_deref() != Some(token) {
                return Err(LibraryRefreshError::Superseded);
            }
            let current = publication
                .current
                .as_ref()
                .ok_or(LibraryRefreshError::Superseded)?;
            if ids.iter().any(|id| {
                !library::valid_clip_asset(&format!("{id}.mp4"))
                    || !current
                        .snapshot
                        .clips
                        .iter()
                        .any(|clip| &clip.filename == id)
            }) {
                return Err(LibraryRefreshError::InvalidSelection);
            }
            let cached: HashMap<_, _> = ids
                .iter()
                .filter_map(|id| {
                    publication
                        .durations
                        .get(id)
                        .filter(|value| {
                            !retry_unavailable
                                || matches!(value.state, DurationState::Available { .. })
                        })
                        .map(|value| (id.clone(), value.clone()))
                })
                .collect();
            publication.duration_cancel = Some(Arc::clone(&cancelled));
            (publication.revision, cached)
        };
        let _cancel_on_drop = CancelOnDrop(Arc::clone(&cancelled));
        let worker_cancel = Arc::clone(&cancelled);
        let values = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut values = Vec::with_capacity(ids.len());
            for id in ids {
                if worker_cancel.load(Ordering::Acquire) {
                    break;
                }
                let value = probe(&capture, &id, &worker_cancel, cached.get(&id));
                values.push((id, value));
            }
            values
        })
        .await
        .map_err(|_| LibraryRefreshError::Failed {
            message: "optional duration worker failed".into(),
            retained_snapshot_token: Some(token.into()),
        })?;
        let mut publication = self.publication.lock().unwrap_or_else(|p| p.into_inner());
        if publication.closed
            || publication.revision != revision
            || cancelled.load(Ordering::Acquire)
            || publication.selected_token.as_deref() != Some(token)
        {
            return Err(LibraryRefreshError::Superseded);
        }
        publication.duration_cancel = None;
        let clips = values
            .into_iter()
            .map(|(clip_id, value)| {
                let duration = value.state.clone();
                publication.durations.insert(clip_id.clone(), value);
                ClipDurationResult { clip_id, duration }
            })
            .collect();
        Ok(ClipDurations {
            snapshot_token: token.into(),
            clips,
        })
    }
}

pub(crate) fn command_error(error: LibraryRefreshError) -> String {
    match error {
        LibraryRefreshError::Busy => "library busy; retry after the active operation",
        LibraryRefreshError::Superseded => "library selection superseded; refresh and select again",
        LibraryRefreshError::InvalidSelection => "item is not in the selected library snapshot",
        _ => "library operation failed",
    }
    .into()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::sync::oneshot;

    const ORIGIN: &str = "http://127.0.0.1:9000/cap/test";

    fn build(capture: &ScanCapture) -> anyhow::Result<LibrarySnapshot> {
        library::build_library_snapshot(
            &capture.path,
            &capture.origin,
            capture.token.clone(),
            || Ok(()),
        )
    }

    fn current(coordinator: &LibraryCoordinator) -> Option<Arc<LibrarySnapshot>> {
        coordinator
            .publication
            .lock()
            .unwrap()
            .current
            .as_ref()
            .map(|value| Arc::clone(&value.snapshot))
    }

    async fn signalled<T>(ready: oneshot::Receiver<T>) -> T {
        tokio::time::timeout(Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap()
    }

    // Used only as a deterministic completion notification, never production admission.
    async fn worker_finished(coordinator: &LibraryCoordinator) {
        let permit = tokio::time::timeout(
            Duration::from_secs(5),
            Arc::clone(&coordinator.scan_slot).acquire_owned(),
        )
        .await
        .unwrap()
        .unwrap();
        drop(permit);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dropped_caller_keeps_global_slot_until_blocking_worker_finishes() {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        let roots = Arc::new(MediaRoots::new(a.path().into()).unwrap());
        let coordinator = Arc::new(LibraryCoordinator::new());
        let runtime_thread = std::thread::current().id();
        let (started, ready) = oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let task = {
            let coordinator = Arc::clone(&coordinator);
            let roots = Arc::clone(&roots);
            tokio::spawn(async move {
                coordinator
                    .refresh_with(&roots, ORIGIN, move |capture| {
                        started.send(std::thread::current().id()).unwrap();
                        blocked.recv_timeout(Duration::from_secs(5))?;
                        build(capture)
                    })
                    .await
            })
        };
        assert_ne!(signalled(ready).await, runtime_thread);
        assert_eq!(
            coordinator.refresh(&roots, ORIGIN).await.unwrap_err(),
            LibraryRefreshError::Busy
        );
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        coordinator
            .publish_output_directory(&roots, OutputDirectory::new(b.path().into()).unwrap());
        // A root switch cannot create a second scan owner.
        for _ in 0..16 {
            assert_eq!(
                coordinator.refresh(&roots, ORIGIN).await.unwrap_err(),
                LibraryRefreshError::Busy
            );
        }
        assert_eq!(coordinator.scan_slot.available_permits(), 0);
        assert!(current(&coordinator).is_none());
        release.send(()).unwrap();
        worker_finished(&coordinator).await;
        let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
        assert_eq!(current(&coordinator).unwrap().token, snapshot.token);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn root_publication_rejects_blocked_scan_and_captures_new_pair_atomically() {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        fs::create_dir_all(a.path().join("games/100")).unwrap();
        fs::create_dir_all(b.path().join("games/200")).unwrap();
        let roots = Arc::new(MediaRoots::new(a.path().into()).unwrap());
        let coordinator = Arc::new(LibraryCoordinator::new());
        let original = coordinator.refresh(&roots, ORIGIN).await.unwrap();
        assert_eq!(original.games[0].timestamp, "100");
        let (started, ready) = oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let task = {
            let coordinator = Arc::clone(&coordinator);
            let roots = Arc::clone(&roots);
            tokio::spawn(async move {
                coordinator
                    .refresh_with(&roots, ORIGIN, move |capture| {
                        let snapshot = build(capture)?;
                        started
                            .send((
                                capture.path.clone(),
                                capture.root.path().to_path_buf(),
                                capture.revision,
                            ))
                            .unwrap();
                        blocked.recv_timeout(Duration::from_secs(5))?;
                        Ok(snapshot)
                    })
                    .await
            })
        };
        let (old_path, old_root, old_revision) = signalled(ready).await;
        assert_eq!(old_path, a.path());
        assert_eq!(old_root, ApprovedRoot::new(a.path()).unwrap().path());
        coordinator
            .publish_output_directory(&roots, OutputDirectory::new(b.path().into()).unwrap());
        assert!(
            current(&coordinator).is_none(),
            "root changes clear old cards immediately"
        );
        release.send(()).unwrap();
        assert_eq!(
            task.await.unwrap().unwrap_err(),
            LibraryRefreshError::Superseded
        );
        let expected_path = b.path().to_path_buf();
        let expected_root = ApprovedRoot::new(b.path()).unwrap();
        let snapshot = coordinator
            .refresh_with(&roots, ORIGIN, move |capture| {
                assert_eq!(capture.path, expected_path);
                assert_eq!(capture.root.path(), expected_root.path());
                assert_eq!(capture.origin, ORIGIN);
                assert!(capture.revision > old_revision);
                build(capture)
            })
            .await
            .unwrap();
        assert_eq!(snapshot.games[0].timestamp, "200");
        assert_ne!(snapshot.token, original.token);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn same_path_aba_and_revision_only_invalidation_reject_old_results() {
        for switch_away in [false, true] {
            let a = tempdir().unwrap();
            let b = tempdir().unwrap();
            let roots = Arc::new(MediaRoots::new(a.path().into()).unwrap());
            let coordinator = Arc::new(LibraryCoordinator::new());
            let (started, ready) = oneshot::channel();
            let (release, blocked) = std::sync::mpsc::channel();
            let task = {
                let coordinator = Arc::clone(&coordinator);
                let roots = Arc::clone(&roots);
                tokio::spawn(async move {
                    coordinator
                        .refresh_with(&roots, ORIGIN, move |capture| {
                            started.send(()).unwrap();
                            blocked.recv_timeout(Duration::from_secs(5))?;
                            // Deliberately ignore cancellation: final revision rejection
                            // must work even when an OS call could not be interrupted.
                            capture.cancelled.store(false, Ordering::Release);
                            build(capture)
                        })
                        .await
                })
            };
            signalled(ready).await;
            if switch_away {
                coordinator.publish_output_directory(
                    &roots,
                    OutputDirectory::new(b.path().into()).unwrap(),
                );
            }
            coordinator
                .publish_output_directory(&roots, OutputDirectory::new(a.path().into()).unwrap());
            release.send(()).unwrap();
            assert_eq!(
                task.await.unwrap().unwrap_err(),
                LibraryRefreshError::Superseded
            );
            assert!(current(&coordinator).is_none());
        }
    }

    #[tokio::test]
    async fn same_root_failure_retains_complete_snapshot_and_retry_rebuilds_from_disk() {
        let directory = tempdir().unwrap();
        let roots = MediaRoots::new(directory.path().into()).unwrap();
        let coordinator = LibraryCoordinator::new();
        let original = coordinator.refresh(&roots, ORIGIN).await.unwrap();
        fs::write(directory.path().join("clips"), b"enumeration failure").unwrap();
        let error = coordinator.refresh(&roots, ORIGIN).await.unwrap_err();
        assert!(
            matches!(error, LibraryRefreshError::Failed { retained_snapshot_token: Some(token), .. } if token == original.token)
        );
        assert_eq!(*current(&coordinator).unwrap(), original);
        fs::remove_file(directory.path().join("clips")).unwrap();
        fs::create_dir(directory.path().join("clips")).unwrap();
        fs::write(directory.path().join("clips/100_200.mp4"), b"new clip").unwrap();
        let rebuilt = coordinator.refresh(&roots, ORIGIN).await.unwrap();
        assert_ne!(rebuilt.token, original.token);
        assert_eq!(rebuilt.clips[0].filename, "100_200");
        assert_eq!(rebuilt.usage.clips_bytes, 8);
    }

    #[tokio::test]
    async fn failed_settings_persistence_leaves_root_revision_and_snapshot_unchanged() {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        let roots = MediaRoots::new(a.path().into()).unwrap();
        let coordinator = LibraryCoordinator::new();
        let original = coordinator.refresh(&roots, ORIGIN).await.unwrap();
        let revision = coordinator.publication.lock().unwrap().revision;
        let (path, root) = roots.output_pair();
        let blocker = a.path().join("not-a-directory");
        fs::write(&blocker, b"fixture").unwrap();
        assert!(
            crate::persist_settings_and_publish(
                &blocker.join("config.toml"),
                &crate::config::Config::default(),
                OutputDirectory::new(b.path().into()).unwrap(),
                &roots,
                &coordinator,
            )
            .is_err()
        );
        let (after_path, after_root) = roots.output_pair();
        assert_eq!(after_path, path);
        assert_eq!(after_root.path(), root.path());
        assert_eq!(coordinator.publication.lock().unwrap().revision, revision);
        assert_eq!(*current(&coordinator).unwrap(), original);
    }

    #[tokio::test]
    async fn blocking_worker_panic_releases_slot_without_replacing_complete_view() {
        let directory = tempdir().unwrap();
        let roots = MediaRoots::new(directory.path().into()).unwrap();
        let coordinator = LibraryCoordinator::new();
        let original = coordinator.refresh(&roots, ORIGIN).await.unwrap();
        let error = coordinator
            .refresh_with(&roots, ORIGIN, |_| panic!("injected worker panic"))
            .await
            .unwrap_err();
        assert!(matches!(error, LibraryRefreshError::Failed { .. }));
        assert_eq!(*current(&coordinator).unwrap(), original);
        assert_eq!(coordinator.scan_slot.available_permits(), 1);
        assert!(coordinator.refresh(&roots, ORIGIN).await.is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_stops_admission_and_rejects_still_owned_work() {
        let directory = tempdir().unwrap();
        let roots = Arc::new(MediaRoots::new(directory.path().into()).unwrap());
        let coordinator = Arc::new(LibraryCoordinator::new());
        let (started, ready) = oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let task = {
            let coordinator = Arc::clone(&coordinator);
            let roots = Arc::clone(&roots);
            tokio::spawn(async move {
                coordinator
                    .refresh_with(&roots, ORIGIN, move |capture| {
                        started.send(()).unwrap();
                        blocked.recv_timeout(Duration::from_secs(5))?;
                        build(capture)
                    })
                    .await
            })
        };
        signalled(ready).await;
        coordinator.shutdown();
        assert_eq!(coordinator.scan_slot.available_permits(), 0);
        assert_eq!(
            coordinator.refresh(&roots, ORIGIN).await.unwrap_err(),
            LibraryRefreshError::Superseded
        );
        release.send(()).unwrap();
        assert_eq!(
            task.await.unwrap().unwrap_err(),
            LibraryRefreshError::Superseded
        );
        assert_eq!(coordinator.scan_slot.available_permits(), 1);
        assert!(current(&coordinator).is_none());
    }
    include!("library_coordinator_m2_tests.rs");
    include!("library_coordinator_replay_tests.rs");
}
