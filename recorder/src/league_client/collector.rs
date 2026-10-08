//! Service-owned LCU collection; Live requests remain owned by the existing poller.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chronobreak_replay_time::MediaId;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::association::{Association, CADENCE, ProvisionalCandidate, RoundInput};
use super::transport::{self, Epoch, Metrics, Transport};
use super::{LcuRound, LiveObservation};

struct LcuObservation {
    round: LcuRound,
    epoch: Epoch,
    started: Instant,
    finished: Instant,
}

trait RoundSource: Send {
    fn acquire(
        &mut self,
        cancellation: &mut watch::Receiver<bool>,
    ) -> impl Future<Output = Option<LcuObservation>> + Send;
    fn totals(&self) -> (u64, u64);
}

struct LocalSource {
    transport: Transport,
    metrics: Metrics,
    next_http_round: Option<tokio::time::Instant>,
}

impl RoundSource for LocalSource {
    async fn acquire(
        &mut self,
        cancellation: &mut watch::Receiver<bool>,
    ) -> Option<LcuObservation> {
        if *cancellation.borrow() {
            return None;
        }
        // Never select/drop an owned blocking discovery/recheck worker.
        let discovered = transport::discover().await.ok()?;
        if *cancellation.borrow() {
            return None;
        }
        let epoch = discovered.epoch();
        // Discovery latency must not compress the spacing of actual HTTP rounds.
        if !wait_for_round_slot(&mut self.next_http_round, cancellation).await {
            return None;
        }
        let started = Instant::now();
        let round = tokio::select! {
            biased;
            _ = cancelled(cancellation) => return None,
            round = self.transport.round(&discovered, &mut self.metrics) => round.ok()?,
        };
        let finished = Instant::now();
        let unchanged = transport::unchanged(discovered).await.ok()?;
        if !unchanged || *cancellation.borrow() {
            return None;
        }
        Some(LcuObservation {
            round,
            epoch,
            started,
            finished,
        })
    }

    fn totals(&self) -> (u64, u64) {
        (self.metrics.requests, self.metrics.response_bytes)
    }
}

pub(crate) struct ContextSession {
    state: Arc<Mutex<Association>>,
    cancellation: watch::Sender<bool>,
    task: Option<JoinHandle<(u64, u64)>>,
    runtime: tokio::runtime::Handle,
}

pub(crate) struct ContextResult {
    pub candidate: Option<ProvisionalCandidate>,
    pub requests: u64,
    pub response_bytes: u64,
}

impl ContextSession {
    pub fn start(
        media_id: MediaId,
        recording_started: Instant,
        latest: watch::Receiver<Option<LiveObservation>>,
    ) -> Result<Self, &'static str> {
        // Production service uses its two-worker runtime. Synchronous abandoned-owner
        // cleanup can join here without stalling the executor that must reap workers.
        if tokio::runtime::Handle::current().runtime_flavor()
            != tokio::runtime::RuntimeFlavor::MultiThread
        {
            return Err("collector_runtime");
        }
        let source = LocalSource {
            transport: Transport::new()?,
            metrics: Metrics::default(),
            next_http_round: None,
        };
        Ok(Self::start_with_source(
            media_id,
            recording_started,
            latest,
            source,
        ))
    }

    fn start_with_source(
        media_id: MediaId,
        recording_started: Instant,
        latest: watch::Receiver<Option<LiveObservation>>,
        source: impl RoundSource + 'static,
    ) -> Self {
        let state = Arc::new(Mutex::new(Association::new(media_id, recording_started)));
        let (cancellation, receiver) = watch::channel(false);
        let task_state = Arc::clone(&state);
        let task = tokio::spawn(collect(source, latest, receiver, task_state));
        Self {
            state,
            cancellation,
            task: Some(task),
            runtime: tokio::runtime::Handle::current(),
        }
    }

    /// Synchronous admission boundary; the worker never admits after this returns.
    pub fn close_admission(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.close();
        }
        self.cancellation.send_replace(true);
    }

    pub(crate) fn close_and_snapshot(&self) -> Option<ProvisionalCandidate> {
        let candidate = self.state.lock().ok().and_then(|mut state| {
            state.close();
            state.snapshot()
        });
        self.cancellation.send_replace(true);
        candidate
    }

    pub async fn stop(mut self) -> Result<ContextResult, &'static str> {
        self.close_admission();
        // Await rather than abort: a blocking OS worker may extend this join.
        // Retain ownership while awaiting: dropping this stop future must still
        // invoke Drop's close-and-join path, including an in-flight OS worker.
        let joined = self.task.as_mut().ok_or("collector_task")?.await;
        self.task.take();
        let (requests, response_bytes) = joined.map_err(|_| "collector_task")?;
        let candidate = self.state.lock().map_err(|_| "collector_state")?.finish();
        Ok(ContextResult {
            candidate,
            requests,
            response_bytes,
        })
    }
}

impl Drop for ContextSession {
    fn drop(&mut self) {
        self.close_admission();
        if let Some(task) = self.task.take() {
            // Never abort an owner that may be joining a blocking OS worker.
            // Normal stop joins asynchronously; this path handles abandoned owners.
            tokio::task::block_in_place(|| {
                if self.runtime.block_on(task).is_err() {
                    tracing::warn!(
                        code = "collector_task",
                        "abandoned context collector join failed"
                    );
                }
            });
        }
    }
}

async fn cancelled(cancellation: &mut watch::Receiver<bool>) {
    loop {
        if *cancellation.borrow_and_update() {
            return;
        }
        if cancellation.changed().await.is_err() {
            return;
        }
    }
}

async fn wait_for_round_slot(
    next_http_round: &mut Option<tokio::time::Instant>,
    cancellation: &mut watch::Receiver<bool>,
) -> bool {
    if let Some(deadline) = *next_http_round {
        tokio::select! {
            biased;
            _ = cancelled(cancellation) => return false,
            _ = tokio::time::sleep_until(deadline) => {}
        }
    }
    if *cancellation.borrow() {
        return false;
    }
    *next_http_round = Some(tokio::time::Instant::now() + CADENCE);
    true
}

async fn collect(
    mut source: impl RoundSource,
    mut latest: watch::Receiver<Option<LiveObservation>>,
    mut cancellation: watch::Receiver<bool>,
    state: Arc<Mutex<Association>>,
) -> (u64, u64) {
    let mut last_sequence = 0;
    loop {
        if *cancellation.borrow() || !state.lock().is_ok_and(|state| state.accepting()) {
            break;
        }
        // Wait for a recent round as well as a new sequence, avoiding a permanent
        // phase offset after fast snapshot retries. No extra Live requests.
        let available = tokio::select! {
            biased;
            _ = cancelled(&mut cancellation) => break,
            value = latest.wait_for(|value| value.as_ref().is_some_and(|v|
                v.sequence > last_sequence
                && tokio::time::Instant::now().into_std().saturating_duration_since(v.started)
                    <= Duration::from_secs(2)))
                => value.is_ok(),
        };
        if !available {
            break;
        }
        let attempted_at = tokio::time::Instant::now();
        let observation = source.acquire(&mut cancellation).await;
        let next_attempt = observation.as_ref().map_or(attempted_at + CADENCE, |o| {
            tokio::time::Instant::from_std(o.started) + CADENCE
        });
        let live = latest.borrow_and_update().clone();
        if let Some(live) = &live {
            last_sequence = live.sequence;
        }
        let now = tokio::time::Instant::now().into_std();
        {
            let Ok(mut state) = state.lock() else {
                break;
            };
            if *cancellation.borrow() {
                state.close();
            }
            state.observe(
                RoundInput {
                    lcu: observation.as_ref().map(|o| &o.round),
                    live: live.as_ref(),
                    epoch: observation.as_ref().map(|o| &o.epoch),
                    epoch_unchanged: observation.is_some(),
                    lcu_started: observation.as_ref().map_or(now, |o| o.started),
                    lcu_finished: observation.as_ref().map_or(now, |o| o.finished),
                },
                now,
            );
            if !state.accepting() {
                break;
            }
        }
        tokio::select! {
            biased;
            _ = cancelled(&mut cancellation) => break,
            _ = tokio::time::sleep_until(next_attempt) => {}
        }
    }
    source.totals()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Virtual-clock harness exercises the real collect future. Production Session
    // requires a multi-thread executor for synchronous abandoned-owner joins.
    struct ClockCollector {
        state: Arc<Mutex<Association>>,
        cancellation: watch::Sender<bool>,
        task: JoinHandle<(u64, u64)>,
    }
    impl ClockCollector {
        fn start(
            media: MediaId,
            receiver: watch::Receiver<Option<LiveObservation>>,
            source: impl RoundSource + 'static,
        ) -> Self {
            let state = Arc::new(Mutex::new(Association::new(
                media,
                tokio::time::Instant::now().into_std(),
            )));
            let (cancellation, stop) = watch::channel(false);
            let task = tokio::spawn(collect(source, receiver, stop, Arc::clone(&state)));
            Self {
                state,
                cancellation,
                task,
            }
        }
        fn close_admission(&self) {
            self.state.lock().unwrap().close();
            self.cancellation.send_replace(true);
        }
        async fn stop(self) -> ContextResult {
            self.close_admission();
            let (requests, response_bytes) = self.task.await.unwrap();
            let candidate = self.state.lock().unwrap().finish();
            ContextResult {
                candidate,
                requests,
                response_bytes,
            }
        }
    }

    struct FixtureSource {
        calls: u64,
        coherent: bool,
    }
    impl RoundSource for FixtureSource {
        async fn acquire(&mut self, _: &mut watch::Receiver<bool>) -> Option<LcuObservation> {
            self.calls += 1;
            let (mut round, _) = crate::league_client::tests::fixture();
            if !self.coherent {
                round.gameflow.phase = None;
            }
            let now = tokio::time::Instant::now().into_std();
            Some(LcuObservation {
                round,
                epoch: transport::fixture_epoch(false),
                started: now,
                finished: now,
            })
        }
        fn totals(&self) -> (u64, u64) {
            (self.calls * 2, self.calls * 64)
        }
    }

    fn live(media_id: &MediaId, sequence: u64) -> LiveObservation {
        let (_, identity) = crate::league_client::tests::fixture();
        let now = tokio::time::Instant::now().into_std();
        LiveObservation {
            media_id: media_id.clone(),
            sequence,
            started: now,
            finished: now,
            identity: Some(identity),
        }
    }

    async fn settle() {
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_existing_live_rounds_binds_once_and_stops_requests() {
        let media = MediaId::new_v4();
        let (latest, receiver) = watch::channel(None);
        let session = ClockCollector::start(
            media.clone(),
            receiver,
            FixtureSource {
                calls: 0,
                coherent: true,
            },
        );
        settle().await;
        latest.send_replace(Some(live(&media, 1)));
        settle().await;
        assert!(session.state.lock().unwrap().accepting());
        // Many producer updates before cadence may not trigger request bursts.
        latest.send_replace(Some(live(&media, 2)));
        tokio::time::advance(Duration::from_secs(9)).await;
        settle().await;
        assert!(session.state.lock().unwrap().accepting());
        tokio::time::advance(Duration::from_secs(1)).await;
        latest.send_replace(Some(live(&media, 3)));
        settle().await;
        let skipped_round_did_not_bind = session.state.lock().unwrap().accepting();
        tokio::time::advance(CADENCE).await;
        latest.send_replace(Some(live(&media, 4)));
        settle().await;
        let now_bound = !session.state.lock().unwrap().accepting();
        for sequence in 5..365 {
            tokio::time::advance(CADENCE).await;
            latest.send_replace(Some(live(&media, sequence)));
            settle().await;
        }
        let result = session.stop().await;
        assert!(result.candidate.is_some());
        assert!(skipped_round_did_not_bind);
        assert!(now_bound);
        assert_eq!(result.requests, 6);
    }

    #[tokio::test(start_paused = true)]
    async fn virtual_hour_is_bounded_and_closed_admission_stops_all_submissions() {
        let media = MediaId::new_v4();
        let (latest, receiver) = watch::channel(None);
        let session = ClockCollector::start(
            media.clone(),
            receiver,
            FixtureSource {
                calls: 0,
                coherent: false,
            },
        );
        for sequence in 1..=360 {
            latest.send_replace(Some(live(&media, sequence)));
            settle().await;
            tokio::time::advance(CADENCE).await;
        }
        session.close_admission();
        for sequence in 361..721 {
            latest.send_replace(Some(live(&media, sequence)));
            tokio::time::advance(CADENCE).await;
        }
        let result = session.stop().await;
        assert!(result.candidate.is_none());
        assert_eq!(result.requests, 720);
        assert_eq!(result.response_bytes, 23040);
    }

    #[tokio::test(start_paused = true)]
    async fn intervening_failed_live_round_requires_a_new_pair_after_recovery() {
        let media = MediaId::new_v4();
        let (latest, receiver) = watch::channel(Some(live(&media, 1)));
        let session = ClockCollector::start(
            media.clone(),
            receiver,
            FixtureSource {
                calls: 0,
                coherent: true,
            },
        );
        settle().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        let mut failed = live(&media, 2);
        failed.identity = None;
        latest.send_replace(Some(failed));
        tokio::time::advance(Duration::from_secs(1)).await;
        latest.send_replace(Some(live(&media, 3)));
        tokio::time::advance(Duration::from_secs(8)).await;
        settle().await;
        let no_bind_from_skipped_failure = session.state.lock().unwrap().accepting();
        tokio::time::advance(Duration::from_secs(2)).await;
        latest.send_replace(Some(live(&media, 4)));
        settle().await;
        let recovery_needs_another_round = session.state.lock().unwrap().accepting();
        tokio::time::advance(CADENCE).await;
        latest.send_replace(Some(live(&media, 5)));
        settle().await;
        let result = session.stop().await;
        assert!(no_bind_from_skipped_failure && recovery_needs_another_round);
        assert!(result.candidate.is_some());
        assert_eq!(result.requests, 6);
    }

    struct JoinedWorkerSource {
        started: Option<tokio::sync::oneshot::Sender<()>>,
        release: Option<std::sync::mpsc::Receiver<()>>,
    }
    impl RoundSource for JoinedWorkerSource {
        async fn acquire(&mut self, _: &mut watch::Receiver<bool>) -> Option<LcuObservation> {
            let started = self.started.take().unwrap();
            let release = self.release.take().unwrap();
            tokio::task::spawn_blocking(move || {
                let _ = started.send(());
                release.recv().unwrap();
            })
            .await
            .unwrap();
            None
        }
        fn totals(&self) -> (u64, u64) {
            (0, 0)
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_closes_admission_while_owned_blocking_worker_is_joined() {
        let media = MediaId::new_v4();
        let (latest, receiver) = watch::channel(Some(live(&media, 1)));
        let (started, began) = tokio::sync::oneshot::channel();
        let (release, worker) = std::sync::mpsc::channel();
        let session = ContextSession::start_with_source(
            media,
            Instant::now() - Duration::from_secs(1),
            receiver,
            JoinedWorkerSource {
                started: Some(started),
                release: Some(worker),
            },
        );
        began.await.unwrap();
        session.close_admission();
        assert!(!session.state.lock().unwrap().accepting());
        assert!(!session.task.as_ref().unwrap().is_finished());
        release.send(()).unwrap();
        assert!(session.stop().await.unwrap().candidate.is_none());
        drop(latest);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn abandoned_owner_closes_admission_and_joins_instead_of_detaching() {
        let media = MediaId::new_v4();
        let (_latest, receiver) = watch::channel(Some(live(&media, 1)));
        let (started, began) = tokio::sync::oneshot::channel();
        let (release, worker) = std::sync::mpsc::channel();
        let session = ContextSession::start_with_source(
            media,
            Instant::now() - Duration::from_secs(1),
            receiver,
            JoinedWorkerSource {
                started: Some(started),
                release: Some(worker),
            },
        );
        began.await.unwrap();
        let state = Arc::clone(&session.state);
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            release.send(()).unwrap();
        });
        drop(session); // returns only after the owned worker and collector join
        releaser.join().unwrap();
        let mut state = state.lock().unwrap();
        assert!(!state.accepting());
        assert!(state.finish().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dropping_stop_future_still_joins_its_blocked_worker() {
        let media = MediaId::new_v4();
        let (_latest, receiver) = watch::channel(Some(live(&media, 1)));
        let (started, began) = tokio::sync::oneshot::channel();
        let (release, worker) = std::sync::mpsc::channel();
        let session = ContextSession::start_with_source(
            media,
            Instant::now() - Duration::from_secs(1),
            receiver,
            JoinedWorkerSource {
                started: Some(started),
                release: Some(worker),
            },
        );
        began.await.unwrap();
        let state = Arc::clone(&session.state);
        let mut stop = Box::pin(session.stop());
        tokio::select! {
            biased;
            _ = &mut stop => panic!("worker must still be owned while blocked"),
            _ = std::future::ready(()) => {}
        }
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            release.send(()).unwrap();
        });
        drop(stop);
        releaser.join().unwrap();
        let mut state = state.lock().unwrap();
        assert!(!state.accepting());
        assert!(state.finish().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn slow_then_fast_discovery_cannot_compress_http_round_spacing() {
        let (_cancel, mut cancellation) = watch::channel(false);
        let base = tokio::time::Instant::now();
        let mut deadline = None;
        tokio::time::advance(Duration::from_secs(7)).await; // first discovery
        assert!(wait_for_round_slot(&mut deadline, &mut cancellation).await);
        tokio::time::advance(Duration::from_secs(3)).await; // fast next discovery
        let second = tokio::spawn(async move {
            assert!(wait_for_round_slot(&mut deadline, &mut cancellation).await);
            tokio::time::Instant::now()
        });
        tokio::time::advance(Duration::from_secs(6)).await;
        settle().await;
        let still_throttled = !second.is_finished();
        tokio::time::advance(Duration::from_secs(1)).await;
        let began = second.await.unwrap();
        assert!(still_throttled);
        assert_eq!(began.duration_since(base), Duration::from_secs(17));
    }
}
