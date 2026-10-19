//! Service-owned finite post-game enrichment with a restart-safe atomic ledger.
use super::{
    association::ProvisionalCandidate,
    post_game_ledger::{self as ledger, Ledger, SavedJob, Store, Terminal},
    result::{Binding, Eog, Rejection},
    sidecar::DirectoryGuard,
    transport::{self, Metrics, Transport},
};
use chronobreak_replay_time::MediaId;
use std::{
    collections::VecDeque,
    future::Future,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::Instant,
};
const CADENCE: Duration = Duration::from_secs(10);
const EXPIRY: Duration = Duration::from_secs(180);
use ledger::{MAX_ATTEMPTS, MAX_PENDING};
enum Command {
    Submit(Box<Binding>, DirectoryGuard, Instant, i64),
    Published(MediaId),
    Cancel(MediaId),
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    struct Fake {
        calls: Arc<Mutex<Vec<Instant>>>,
        response: serde_json::Value,
    }
    #[tokio::test]
    async fn saturated_lifecycle_channel_durably_cancels_pending_work_without_http() {
        let root = tempfile::tempdir().unwrap();
        let games = root.path().join("games");
        std::fs::create_dir(&games).unwrap();
        let (send, receive) = mpsc::channel(1);
        let media = submit(&send, &games.join("100"));
        let (stop, cancel) = watch::channel(false);
        let control_failed = Arc::new(AtomicBool::new(false));
        let handle = Handle {
            send,
            stop,
            control_failed: Arc::clone(&control_failed),
        };
        handle.published(media);
        assert!(control_failed.load(Ordering::Acquire));
        let (store, saved) = Store::open(root.path(), ledger::wall_ms().unwrap()).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let totals = work_state(
            receive,
            cancel,
            Fake {
                calls: Arc::clone(&calls),
                response: serde_json::json!({}),
            },
            VecDeque::new(),
            Durability {
                store: Some(store),
                saved,
                clock: Clock::real(),
                failed: false,
                control_failed,
            },
        )
        .await;
        assert_eq!(totals.cancelled, 1);
        assert!(calls.lock().unwrap().is_empty());
        let (_, saved) = Store::open(root.path(), ledger::wall_ms().unwrap()).unwrap();
        assert!(matches!(saved.jobs[0].terminal, Some(Terminal::Cancelled)));
    }
    struct CheckReservation {
        ledger: std::path::PathBuf,
        stop: watch::Sender<bool>,
        game_id: u64,
    }
    impl Source for CheckReservation {
        async fn acquire(
            &mut self,
            _: &mut watch::Receiver<bool>,
            _: Instant,
            _: &DirectoryGuard,
        ) -> Option<Eog> {
            let state: Ledger =
                serde_json::from_slice(&std::fs::read(&self.ledger).unwrap()).unwrap();
            assert_eq!(state.jobs.len(), 1);
            assert_eq!(state.jobs[0].attempts, 1);
            assert!(state.jobs[0].next_ms > state.jobs[0].closed_ms);
            self.stop.send_replace(true);
            serde_json::from_value(serde_json::json!({"gameId": self.game_id})).ok()
        }
        fn totals(&self) -> (u64, u64) {
            (1, 0)
        }
    }

    #[tokio::test]
    async fn coordinator_saves_reservation_before_http_and_confirmation_before_shutdown() {
        let root = tempfile::tempdir().unwrap();
        let games = root.path().join("games");
        std::fs::create_dir(&games).unwrap();
        let (send, receive) = mpsc::channel(16);
        let media = submit(&send, &games.join("100"));
        let binding = super::super::association::fixture_candidate(media).result_binding();
        let (stop, cancel) = watch::channel(false);
        let now = ledger::wall_ms().unwrap();
        let (store, saved) = Store::open(root.path(), now).unwrap();
        let totals = work_state(
            receive,
            cancel,
            CheckReservation {
                ledger: root.path().join(ledger::FILE),
                stop,
                game_id: binding.game_id.parse().unwrap(),
            },
            VecDeque::new(),
            Durability {
                store: Some(store),
                saved,
                clock: Clock::real(),
                failed: false,
                control_failed: Arc::new(AtomicBool::new(false)),
            },
        )
        .await;
        assert_eq!(totals.attempts, 1);
        assert_eq!(totals.confirmed, 1);
        let (_, saved) = Store::open(root.path(), ledger::wall_ms().unwrap()).unwrap();
        assert_eq!(saved.jobs[0].attempts, 1);
        assert!(saved.jobs[0].result.is_some());
        assert!(!saved.jobs[0].published);
    }

    #[tokio::test]
    async fn recovery_reconciles_publication_and_already_installed_result_without_http() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("games").join("100");
        std::fs::create_dir_all(&game).unwrap();
        let media = MediaId::new_v4();
        let result = super::super::sidecar::tests::result_fixture(&game, &media);
        let binding = super::super::association::fixture_candidate(media.clone()).result_binding();
        let directory = DirectoryGuard::capture(&game, media).unwrap();
        let now = ledger::wall_ms().unwrap();
        let (store, mut saved) = Store::open(root.path(), now).unwrap();
        let mut job = SavedJob::new(binding, directory.locator().unwrap(), now).unwrap();
        job.result = Some(result);
        saved.jobs.push(job);
        store.save(&saved).unwrap();
        drop(store);
        drop(directory);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut installed = None;
        for run in 0..2 {
            let now = ledger::wall_ms().unwrap();
            let (store, mut saved) = Store::open(root.path(), now).unwrap();
            if run == 1 {
                // Model crash after LR rename but before completion was saved.
                saved.jobs[0].terminal = None;
                store.save(&saved).unwrap();
            }
            let jobs = restore_jobs(&saved, now);
            assert!(jobs[0].saved.published);
            let (send, receive) = mpsc::channel(16);
            let (_stop, cancel) = watch::channel(false);
            drop(send);
            let totals = work_state(
                receive,
                cancel,
                Fake {
                    calls: Arc::clone(&calls),
                    response: serde_json::json!({}),
                },
                jobs,
                Durability {
                    store: Some(store),
                    saved,
                    clock: Clock::real(),
                    failed: false,
                    control_failed: Arc::new(AtomicBool::new(false)),
                },
            )
            .await;
            assert_eq!(totals.writes, 1);
            assert_eq!(totals.attempts, 0);
            let bytes = std::fs::read(game.join(chronobreak_league_data::RESULT_FILE)).unwrap();
            if let Some(previous) = &installed {
                assert_eq!(previous, &bytes);
            }
            installed = Some(bytes);
            let (store, saved) = Store::open(root.path(), ledger::wall_ms().unwrap()).unwrap();
            assert!(matches!(saved.jobs[0].terminal, Some(Terminal::Completed)));
            drop(store);
        }
        assert!(calls.lock().unwrap().is_empty());
    }
    impl Source for Fake {
        async fn acquire(
            &mut self,
            _: &mut watch::Receiver<bool>,
            _: Instant,
            _: &DirectoryGuard,
        ) -> Option<Eog> {
            self.calls.lock().unwrap().push(Instant::now());
            serde_json::from_value(self.response.clone()).ok()
        }
        fn totals(&self) -> (u64, u64) {
            (self.calls.lock().unwrap().len() as u64, 0)
        }
    }
    async fn settle() {
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
    }
    fn submit(send: &mpsc::Sender<Command>, path: &std::path::Path) -> MediaId {
        std::fs::create_dir(path).unwrap();
        let media = MediaId::new_v4();
        let c = super::super::association::fixture_candidate(media.clone());
        send.try_send(Command::Submit(
            Box::new(c.result_binding()),
            DirectoryGuard::capture(path, media.clone()).unwrap(),
            Instant::now(),
            ledger::wall_ms().unwrap(),
        ))
        .unwrap();
        media
    }
    struct Waiting {
        joined: Arc<std::sync::atomic::AtomicBool>,
    }
    impl Source for Waiting {
        async fn acquire(
            &mut self,
            cancel: &mut watch::Receiver<bool>,
            _: Instant,
            _: &DirectoryGuard,
        ) -> Option<Eog> {
            let _ = cancel.changed().await;
            self.joined.store(true, std::sync::atomic::Ordering::SeqCst);
            None
        }
        fn totals(&self) -> (u64, u64) {
            (0, 0)
        }
    }
    #[tokio::test(start_paused = true)]
    async fn active_acquisition_is_cancelled_and_joined_on_failure_deletion_expiry_and_shutdown() {
        for action in 0..4 {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("game");
            let (send, recv) = mpsc::channel(16);
            let (stop, cancel) = watch::channel(false);
            let joined = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let media = submit(&send, &path);
            let worker = tokio::spawn(work(
                recv,
                cancel,
                Waiting {
                    joined: joined.clone(),
                },
            ));
            settle().await;
            match action {
                0 => send.try_send(Command::Cancel(media)).unwrap(),
                1 => {
                    std::fs::remove_dir(&path).unwrap();
                    tokio::time::advance(Duration::from_secs(1)).await;
                }
                2 => tokio::time::advance(EXPIRY).await,
                _ => {
                    stop.send_replace(true);
                }
            }
            settle().await;
            assert!(joined.load(std::sync::atomic::Ordering::SeqCst));
            stop.send_replace(true);
            let totals = worker.await.unwrap();
            assert_eq!(totals.cancelled, u64::from(action == 0));
            assert_eq!(totals.deleted, u64::from(action == 1));
            assert_eq!(totals.expired, u64::from(action == 2));
        }
    }
    #[tokio::test(start_paused = true)]
    async fn virtual_hour_caps_attempts_cadence_pending_and_oldest_overflow() {
        let root = tempfile::tempdir().unwrap();
        let (send, recv) = mpsc::channel(16);
        let (stop, cancel) = watch::channel(false);
        let calls = Arc::new(Mutex::new(Vec::new()));
        for i in 0..5 {
            submit(&send, &root.path().join(i.to_string()));
        }
        let worker = tokio::spawn(work(
            recv,
            cancel,
            Fake {
                calls: calls.clone(),
                response: serde_json::json!({"gameId":7}),
            },
        ));
        settle().await;
        for _ in 0..3600 {
            tokio::time::advance(Duration::from_secs(1)).await;
            settle().await;
        }
        stop.send_replace(true);
        let totals = worker.await.unwrap();
        assert_eq!(totals.peak, MAX_PENDING);
        assert_eq!(totals.dropped, 1);
        assert_eq!(totals.expired, MAX_PENDING as u64);
        assert_eq!(
            totals.attempts,
            (MAX_PENDING as u64) * u64::from(MAX_ATTEMPTS)
        );
        assert_eq!(totals.not_ready_responses, totals.attempts);
        let times = calls.lock().unwrap();
        assert_eq!(times.len(), 18);
        for batch in times.windows(2) {
            assert!(batch[1].duration_since(batch[0]) >= CADENCE);
        }
    }
    #[tokio::test(start_paused = true)]
    async fn cancellation_deletion_and_failed_publication_end_work() {
        for action in 0..3 {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("game");
            let (send, recv) = mpsc::channel(16);
            let (stop, cancel) = watch::channel(false);
            let calls = Arc::new(Mutex::new(Vec::new()));
            let media = submit(&send, &path);
            let worker = tokio::spawn(work(
                recv,
                cancel,
                Fake {
                    calls: calls.clone(),
                    response: serde_json::json!({}),
                },
            ));
            settle().await;
            assert_eq!(calls.lock().unwrap().len(), 1);
            if action == 0 {
                send.try_send(Command::Cancel(media)).unwrap();
            } else if action == 1 {
                std::fs::remove_dir(&path).unwrap();
            } else {
                stop.send_replace(true);
            }
            settle().await;
            tokio::time::advance(EXPIRY).await;
            settle().await;
            stop.send_replace(true);
            worker.await.unwrap();
            assert_eq!(calls.lock().unwrap().len(), 1);
        }
    }
    #[tokio::test(start_paused = true)]
    async fn confirmation_waits_for_publication_and_stops_requests() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("game");
        let (send, recv) = mpsc::channel(16);
        let (stop, cancel) = watch::channel(false);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let media = submit(&send, &path);
        let b = super::super::association::fixture_candidate(media).result_binding();
        let worker = tokio::spawn(work(
            recv,
            cancel,
            Fake {
                calls: calls.clone(),
                response: serde_json::json!({"gameId":b.game_id.parse::<u64>().unwrap()}),
            },
        ));
        settle().await;
        for _ in 0..17 {
            tokio::time::advance(CADENCE).await;
            settle().await;
        }
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert!(!path.join(chronobreak_league_data::RESULT_FILE).exists());
        stop.send_replace(true);
        worker.await.unwrap();
    }
}
#[derive(Clone)]
pub(crate) struct Handle {
    send: mpsc::Sender<Command>,
    control_failed: Arc<AtomicBool>,
    stop: watch::Sender<bool>,
}
impl Handle {
    fn send(&self, command: Command) {
        if self.send.try_send(command).is_err() {
            // Never silently lose publication/cancellation and never queue an
            // unbounded sender behind disk I/O. Saturation closes all jobs.
            self.control_failed.store(true, Ordering::Release);
            self.stop.send_replace(true);
            tracing::warn!(
                code = "eog_control_unavailable",
                "post-game control closed; pending jobs cancelled"
            );
        }
    }
    pub(crate) fn submit(&self, candidate: &ProvisionalCandidate, directory: DirectoryGuard) {
        let Ok(closed_ms) = ledger::wall_ms() else {
            return;
        };
        self.send(Command::Submit(
            Box::new(candidate.result_binding()),
            directory,
            Instant::now(),
            closed_ms,
        ));
    }
    pub(crate) fn published(&self, media: MediaId) {
        self.send(Command::Published(media));
    }
    pub(crate) fn cancel(&self, media: MediaId) {
        self.send(Command::Cancel(media));
    }
}
pub(crate) struct Coordinator {
    handle: Handle,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<Totals>>,
    runtime: tokio::runtime::Handle,
}
impl Coordinator {
    pub(crate) fn start(root: &Path) -> Result<Self, &'static str> {
        let runtime = tokio::runtime::Handle::current();
        if runtime.runtime_flavor() != tokio::runtime::RuntimeFlavor::MultiThread {
            return Err("result_runtime");
        }
        let (send, receive) = mpsc::channel(16);
        let (stop, cancel) = watch::channel(false);
        let control_failed = Arc::new(AtomicBool::new(false));
        let task_failed = Arc::clone(&control_failed);
        let source = LocalSource {
            transport: Transport::new()?,
            metrics: Metrics::default(),
        };
        let root = root.to_owned();
        let task = tokio::spawn(async move {
            let loaded = tokio::task::spawn_blocking(move || {
                let now = ledger::wall_ms()?;
                let (store, saved) = Store::open(&root, now)?;
                let jobs = restore_jobs(&saved, now);
                Ok::<_, &'static str>((store, saved, jobs))
            })
            .await;
            match loaded {
                Ok(Ok((store, saved, jobs))) => {
                    work_state(
                        receive,
                        cancel,
                        source,
                        jobs,
                        Durability {
                            store: Some(store),
                            saved,
                            clock: Clock::real(),
                            failed: false,
                            control_failed: task_failed,
                        },
                    )
                    .await
                }
                _ => {
                    tracing::warn!(
                        code = "eog_ledger_start",
                        "optional post-game recovery unavailable"
                    );
                    Totals::default()
                }
            }
        });
        Ok(Self {
            handle: Handle {
                send,
                control_failed,
                stop: stop.clone(),
            },
            stop,
            task: Some(task),
            runtime,
        })
    }
    pub(crate) fn handle(&self) -> Handle {
        self.handle.clone()
    }
    pub(crate) async fn stop(mut self) {
        self.stop.send_replace(true);
        if let Some(task) = self.task.as_mut() {
            let _ = task.await;
        }
        self.task.take();
    }
}
impl Drop for Coordinator {
    fn drop(&mut self) {
        self.stop.send_replace(true);
        if let Some(task) = self.task.take() {
            tokio::task::block_in_place(|| {
                let _ = self.runtime.block_on(task);
            });
        }
    }
}
trait Source: Send {
    fn acquire(
        &mut self,
        cancel: &mut watch::Receiver<bool>,
        expires: Instant,
        directory: &DirectoryGuard,
    ) -> impl Future<Output = Option<Eog>> + Send;
    fn totals(&self) -> (u64, u64);
    fn last_http_status(&self) -> Option<u16> {
        None
    }
}
struct LocalSource {
    transport: Transport,
    metrics: Metrics,
}
impl Source for LocalSource {
    async fn acquire(
        &mut self,
        cancel: &mut watch::Receiver<bool>,
        expires: Instant,
        directory: &DirectoryGuard,
    ) -> Option<Eog> {
        // Always join the OS worker; HTTP cancellation owns no blocking worker.
        let discovered = transport::discover().await.ok()?;
        if *cancel.borrow() || Instant::now() >= expires {
            return None;
        }
        tokio::select! {biased; _=cancel.changed()=>None, result=tokio::time::timeout_at(expires,self.transport.eog(&discovered,&mut self.metrics,directory))=>result.ok().and_then(Result::ok)}
    }
    fn totals(&self) -> (u64, u64) {
        (self.metrics.requests, self.metrics.response_bytes)
    }
    fn last_http_status(&self) -> Option<u16> {
        self.metrics.last_http_status
    }
}
struct Job {
    saved: SavedJob,
    directory: Option<DirectoryGuard>,
    expires: Instant,
    next: Instant,
}
impl Job {
    fn active(&self) -> bool {
        self.saved.terminal.is_none()
    }
}

struct Clock {
    #[cfg(test)]
    virtual_origin: Option<(i64, Instant)>,
}
impl Clock {
    fn real() -> Self {
        Self {
            #[cfg(test)]
            virtual_origin: None,
        }
    }
    fn now(&self) -> Result<i64, &'static str> {
        #[cfg(test)]
        if let Some((wall, instant)) = self.virtual_origin {
            return Ok(
                wall + i64::try_from(instant.elapsed().as_millis()).map_err(|_| "ledger_clock")?
            );
        }
        ledger::wall_ms()
    }
}
struct Durability {
    store: Option<Arc<Store>>,
    saved: Ledger,
    clock: Clock,
    failed: bool,
    control_failed: Arc<AtomicBool>,
}
impl Durability {
    async fn persist(&mut self, jobs: &mut VecDeque<Job>) -> Result<(), &'static str> {
        if self.failed {
            return Err("ledger_failed");
        }
        let result = self.persist_inner(jobs).await;
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    async fn persist_inner(&mut self, jobs: &mut VecDeque<Job>) -> Result<(), &'static str> {
        let now = self.clock.now()?;
        self.saved.jobs = jobs.iter().map(|j| j.saved.clone()).collect();
        let rollback = now < self.saved.last_wall_ms;
        self.saved.reconcile_time(now)?;
        for (job, saved) in jobs.iter_mut().zip(&self.saved.jobs) {
            job.saved.terminal = saved.terminal;
            if !job.active() {
                job.directory.take();
            }
        }
        if let Some(store) = &self.store {
            store.persist(self.saved.clone()).await?;
        }
        if rollback {
            return Err("ledger_clock_rollback");
        }
        Ok(())
    }
}
fn restore_jobs(saved: &Ledger, now_ms: i64) -> VecDeque<Job> {
    let now = Instant::now();
    saved
        .jobs
        .iter()
        .map(|saved| {
            let mut saved = saved.clone();
            let directory = if saved.terminal.is_none() {
                match DirectoryGuard::recover(&saved.directory) {
                    Ok(directory) => {
                        saved.published |= directory.result_published(&saved.binding.game_id);
                        Some(directory)
                    }
                    Err(_) => {
                        saved.terminal = Some(Terminal::Deleted);
                        None
                    }
                }
            } else {
                None
            };
            Job {
                expires: now
                    + Duration::from_millis(saved.expires_ms.saturating_sub(now_ms).max(0) as u64),
                next: now
                    + Duration::from_millis(saved.next_ms.saturating_sub(now_ms).max(0) as u64),
                saved,
                directory,
            }
        })
        .collect()
}
#[derive(Default)]
struct Totals {
    attempts: u64,
    confirmed: u64,
    primary_win_one: u64,
    primary_win_zero: u64,
    rejected: u64,
    expired: u64,
    deleted: u64,
    cancelled: u64,
    dropped: u64,
    abandoned: u64,
    writes: u64,
    result_bytes: u64,
    peak: usize,
    latency_ms: u128,
    unavailable_attempts: u64,
    not_ready_responses: u64,
}
impl Totals {
    fn terminal_count(&self) -> u64 {
        self.rejected
            + self.expired
            + self.deleted
            + self.cancelled
            + self.dropped
            + self.abandoned
            + self.writes
    }
}
fn retain_available(jobs: &mut VecDeque<Job>, now: Instant, totals: &mut Totals) {
    for job in jobs.iter_mut().filter(|job| job.active()) {
        let terminal = if now >= job.expires {
            totals.expired += 1;
            Some(Terminal::Expired)
        } else if !job
            .directory
            .as_ref()
            .is_some_and(DirectoryGuard::available)
        {
            totals.deleted += 1;
            Some(Terminal::Deleted)
        } else {
            None
        };
        if let Some(terminal) = terminal {
            job.saved.terminal = Some(terminal);
            job.directory.take();
        }
    }
}
fn pending(jobs: &VecDeque<Job>) -> usize {
    jobs.iter().filter(|j| j.active()).count()
}
fn log_totals(source: &impl Source, totals: &Totals, pending: usize, joined: bool) {
    let (requests, response_bytes) = source.totals();
    tracing::info!(
        requests, response_bytes, pending,
        attempts = totals.attempts,
        confirmed = totals.confirmed,
        primary_win_one = totals.primary_win_one,
        primary_win_zero = totals.primary_win_zero,
        rejected = totals.rejected,
        expired = totals.expired,
        deleted = totals.deleted,
        cancelled = totals.cancelled,
        dropped = totals.dropped,
        abandoned = totals.abandoned,
        writes = totals.writes,
        result_bytes = totals.result_bytes,
        peak_pending = totals.peak,
        latency_ms = totals.latency_ms,
        unavailable_attempts = totals.unavailable_attempts,
        not_ready_responses = totals.not_ready_responses,
        last_http_status = ?source.last_http_status(),
        "{}", if joined { "post-game coordinator joined" } else { "post-game coordinator totals" }
    );
}
fn log_terminal_change(source: &impl Source, totals: &Totals, pending: usize, last: &mut u64) {
    let terminal = totals.terminal_count();
    if terminal != *last {
        log_totals(source, totals, pending, false);
        *last = terminal;
    }
}
fn command(jobs: &mut VecDeque<Job>, cmd: Command, totals: &mut Totals) {
    match cmd {
        Command::Submit(binding, directory, closed, closed_ms) => {
            if jobs
                .iter()
                .any(|j| j.saved.binding.media_id == binding.media_id)
            {
                return;
            }
            let Ok(locator) = directory.locator() else {
                totals.abandoned += 1;
                return;
            };
            let Ok(saved) = SavedJob::new(*binding, locator, closed_ms) else {
                totals.abandoned += 1;
                return;
            };
            if jobs.len() == MAX_PENDING {
                let remove = jobs.iter().position(|j| !j.active()).unwrap_or(0);
                if jobs.remove(remove).is_some_and(|j| j.active()) {
                    totals.dropped += 1;
                }
            }
            jobs.push_back(Job {
                saved,
                directory: Some(directory),
                expires: closed + EXPIRY,
                next: Instant::now(),
            });
            totals.peak = totals.peak.max(pending(jobs));
        }
        Command::Published(media) => {
            for job in jobs
                .iter_mut()
                .filter(|j| j.active() && j.saved.binding.media_id == media)
            {
                job.saved.published = true;
            }
        }
        Command::Cancel(media) => {
            for job in jobs
                .iter_mut()
                .filter(|j| j.active() && j.saved.binding.media_id == media)
            {
                job.saved.terminal = Some(Terminal::Cancelled);
                job.directory.take();
                totals.cancelled += 1;
            }
        }
    }
}
#[cfg(test)]
async fn work(
    receive: mpsc::Receiver<Command>,
    cancel: watch::Receiver<bool>,
    source: impl Source,
) -> Totals {
    let wall = ledger::wall_ms().unwrap();
    work_state(
        receive,
        cancel,
        source,
        VecDeque::new(),
        Durability {
            store: None,
            saved: Ledger::empty(wall),
            clock: Clock {
                virtual_origin: Some((wall, Instant::now())),
            },
            failed: false,
            control_failed: Arc::new(AtomicBool::new(false)),
        },
    )
    .await
}
async fn work_state(
    mut receive: mpsc::Receiver<Command>,
    mut cancel: watch::Receiver<bool>,
    mut source: impl Source,
    mut jobs: VecDeque<Job>,
    mut durability: Durability,
) -> Totals {
    let mut totals = Totals::default();
    let mut logged_terminal = 0;
    loop {
        while let Ok(cmd) = receive.try_recv() {
            command(&mut jobs, cmd, &mut totals);
        }
        if durability.control_failed.load(Ordering::Acquire) {
            for job in jobs.iter_mut().filter(|j| j.active()) {
                job.saved.terminal = Some(Terminal::Cancelled);
                job.directory.take();
                totals.cancelled += 1;
            }
        }
        let now = Instant::now();
        retain_available(&mut jobs, now, &mut totals);
        // Every transition is saved before another request or result install.
        // Shutdown preserves accepted pending work at its original deadline.
        if let Err(code) = durability.persist(&mut jobs).await {
            tracing::warn!(
                code,
                "post-game durability unavailable; acquisition stopped"
            );
            break;
        }
        if *cancel.borrow() {
            break;
        }
        log_terminal_change(&source, &totals, pending(&jobs), &mut logged_terminal);
        if let Some(index) = jobs
            .iter()
            .position(|j| j.active() && j.saved.published && j.saved.result.is_some())
        {
            let job = &mut jobs[index];
            let result = job.saved.result.clone().expect("confirmed facts");
            let directory = job.directory.take().expect("owned directory");
            // The confirmed result remains in the ledger until completion is
            // durable. A crash after installation rechecks it idempotently.
            job.saved.terminal = Some(
                match tokio::task::spawn_blocking(move || directory.install_result(&result)).await {
                    Ok(Ok(bytes)) => {
                        totals.writes += 1;
                        totals.result_bytes += bytes as u64;
                        Terminal::Completed
                    }
                    _ => {
                        totals.abandoned += 1;
                        Terminal::PublicationFailed
                    }
                },
            );
            continue;
        }
        let Ok(now_ms) = durability.clock.now() else {
            break;
        };
        let mut eligible = Vec::new();
        for job in jobs.iter_mut().filter(|j| {
            j.active()
                && j.saved.result.is_none()
                && j.next <= now
                && j.saved.attempts < MAX_ATTEMPTS
        }) {
            if job.saved.reserve(now_ms) {
                job.next = now + CADENCE;
                eligible.push(job.saved.binding.media_id.clone());
            } else {
                job.next = now
                    + Duration::from_millis(job.saved.next_ms.saturating_sub(now_ms).max(1) as u64);
            }
        }
        if !eligible.is_empty() {
            // A reservation consumes the attempt even if the process dies before
            // dispatch or before a response; all eligible jobs share one response.
            if let Err(code) = durability.persist(&mut jobs).await {
                tracing::warn!(code, "post-game reservation failed; request suppressed");
                break;
            }
            if *cancel.borrow() {
                continue;
            }
            let dispatch_now = Instant::now();
            let Ok(dispatch_wall) = durability.clock.now() else {
                break;
            };
            eligible.retain(|media| {
                jobs.iter().any(|j| {
                    j.active()
                        && &j.saved.binding.media_id == media
                        && dispatch_now < j.expires
                        && dispatch_wall < j.saved.expires_ms
                })
            });
            let Some(media) = eligible.first().cloned() else {
                continue;
            };
            totals.attempts += eligible.len() as u64;
            let began = Instant::now();
            let expires = jobs
                .iter()
                .filter(|j| eligible.contains(&j.saved.binding.media_id))
                .map(|j| j.expires)
                .min()
                .expect("eligible job");
            let response = acquire_owned(
                &mut source,
                &mut receive,
                &mut cancel,
                &mut jobs,
                Attempt {
                    media: &media,
                    expires,
                },
                &mut totals,
                &mut durability,
            )
            .await;
            totals.latency_ms += began.elapsed().as_millis();
            let Ok(ended_ms) = durability.clock.now() else {
                break;
            };
            for job in jobs
                .iter_mut()
                .filter(|j| j.active() && eligible.contains(&j.saved.binding.media_id))
            {
                job.next = Instant::now() + CADENCE;
                job.saved.next_ms = ended_ms
                    .saturating_add(ledger::CADENCE_MS)
                    .min(job.saved.expires_ms + ledger::CADENCE_MS);
                if let Some(eog) = &response {
                    match eog.confirm(&job.saved.binding) {
                        Ok(result) => {
                            totals.primary_win_one += u64::from(eog.primary_win() == Some(1));
                            totals.primary_win_zero += u64::from(eog.primary_win() == Some(0));
                            job.saved.result = Some(result);
                            totals.confirmed += 1;
                        }
                        Err(Rejection::Association) => {
                            job.saved.terminal = Some(Terminal::Rejected);
                            job.directory.take();
                            totals.rejected += 1;
                        }
                        Err(Rejection::NotReady) => totals.not_ready_responses += 1,
                    }
                } else {
                    totals.unavailable_attempts += 1;
                }
            }
            continue;
        }
        let deadline = jobs
            .iter()
            .filter(|j| j.active())
            .map(|j| {
                if j.saved.result.is_some() || j.saved.attempts == MAX_ATTEMPTS {
                    j.expires
                } else {
                    j.next.min(j.expires)
                }
            })
            .min();
        tokio::select! {biased;
            _=cancel.changed()=>{},
            cmd=receive.recv()=>match cmd {Some(cmd)=>command(&mut jobs,cmd,&mut totals),None=>break},
            _=async {match deadline {Some(deadline)=>tokio::time::sleep_until(deadline).await,None=>std::future::pending().await}}=>{}
        }
    }
    log_totals(&source, &totals, pending(&jobs), true);
    totals
}

// Commands, expiry and deletion remain observable while discovery is in flight.
// Cancelling a job signals acquisition, then joins it; no OS worker is detached.
struct Attempt<'a> {
    media: &'a MediaId,
    expires: Instant,
}

async fn acquire_owned(
    source: &mut impl Source,
    receive: &mut mpsc::Receiver<Command>,
    cancel: &mut watch::Receiver<bool>,
    jobs: &mut VecDeque<Job>,
    attempt: Attempt<'_>,
    totals: &mut Totals,
    durability: &mut Durability,
) -> Option<Eog> {
    let Attempt { media, expires } = attempt;
    let (stop, mut own_cancel) = watch::channel(false);
    let directory = jobs
        .iter()
        .find(|j| j.active() && &j.saved.binding.media_id == media)?
        .directory
        .as_ref()?
        .try_clone()
        .ok()?;
    let acquisition = source.acquire(&mut own_cancel, expires, &directory);
    tokio::pin!(acquisition);
    loop {
        tokio::select! {biased;
            _=cancel.changed()=>{stop.send_replace(true);let _=acquisition.await;return None;}
            cmd=receive.recv()=>{
                match cmd {Some(cmd)=>command(jobs,cmd,totals),None=>{stop.send_replace(true);let _=acquisition.await;return None;}}
                if durability.persist(jobs).await.is_err() { stop.send_replace(true); let _=acquisition.await; return None; }
                if !jobs.iter().any(|j| j.active() && &j.saved.binding.media_id==media){stop.send_replace(true);let _=acquisition.await;return None;}
            }
            _=tokio::time::sleep_until((Instant::now()+Duration::from_secs(1)).min(expires))=>{
                retain_available(jobs, Instant::now(), totals);
                if durability.persist(jobs).await.is_err() { stop.send_replace(true); let _=acquisition.await; return None; }
                if Instant::now() >= expires || !jobs.iter().any(|j| j.active() && &j.saved.binding.media_id==media) {
                    stop.send_replace(true);let _=acquisition.await;return None;
                }
            }
            response=&mut acquisition=>return response,
        }
    }
}
