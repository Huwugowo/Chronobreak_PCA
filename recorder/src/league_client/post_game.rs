//! Service-owned finite, in-memory post-game enrichment. No persisted jobs.
use super::{
    association::ProvisionalCandidate,
    result::{Binding, Eog, Rejection},
    sidecar::DirectoryGuard,
    transport::{self, Metrics, Transport},
};
use chronobreak_league_data::ResultFile;
use chronobreak_replay_time::MediaId;
use std::{collections::VecDeque, future::Future, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::Instant,
};
const CADENCE: Duration = Duration::from_secs(10);
const EXPIRY: Duration = Duration::from_secs(180);
const MAX_ATTEMPTS: u32 = 18;
const MAX_PENDING: usize = 4;
enum Command {
    Submit(Box<Binding>, DirectoryGuard, Instant),
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
            worker.await.unwrap();
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
        worker.await.unwrap();
        let times = calls.lock().unwrap();
        assert_eq!(times.len(), 4 * 18);
        for batch in times.chunks_exact(4).collect::<Vec<_>>().windows(2) {
            assert!(batch[1][0].duration_since(batch[0][0]) >= CADENCE);
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
pub(crate) struct Handle(mpsc::Sender<Command>);
impl Handle {
    pub(crate) fn submit(&self, candidate: &ProvisionalCandidate, directory: DirectoryGuard) {
        let _ = self.0.try_send(Command::Submit(
            Box::new(candidate.result_binding()),
            directory,
            Instant::now(),
        ));
    }
    pub(crate) fn published(&self, media: MediaId) {
        let _ = self.0.try_send(Command::Published(media));
    }
    pub(crate) fn cancel(&self, media: MediaId) {
        let _ = self.0.try_send(Command::Cancel(media));
    }
}
pub(crate) struct Coordinator {
    handle: Handle,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
    runtime: tokio::runtime::Handle,
}
impl Coordinator {
    pub(crate) fn start() -> Result<Self, &'static str> {
        let runtime = tokio::runtime::Handle::current();
        if runtime.runtime_flavor() != tokio::runtime::RuntimeFlavor::MultiThread {
            return Err("result_runtime");
        }
        let (send, receive) = mpsc::channel(16);
        let (stop, cancel) = watch::channel(false);
        let task = tokio::spawn(work(
            receive,
            cancel,
            LocalSource {
                transport: Transport::new()?,
                metrics: Metrics::default(),
            },
        ));
        Ok(Self {
            handle: Handle(send),
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
}
struct Job {
    binding: Binding,
    directory: Option<DirectoryGuard>,
    expires: Instant,
    next: Instant,
    attempts: u32,
    result: Option<ResultFile>,
    published: bool,
}
#[derive(Default)]
struct Totals {
    attempts: u64,
    confirmed: u64,
    primary_win_one: u64,
    primary_win_zero: u64,
    rejected: u64,
    expired: u64,
    dropped: u64,
    abandoned: u64,
    writes: u64,
    result_bytes: u64,
    peak: usize,
    latency_ms: u128,
}
fn command(jobs: &mut VecDeque<Job>, cmd: Command, totals: &mut Totals) {
    match cmd {
        Command::Submit(binding, directory, closed) => {
            if jobs.len() == MAX_PENDING {
                jobs.pop_front();
                totals.dropped += 1;
            }
            let now = Instant::now();
            jobs.push_back(Job {
                binding: *binding,
                directory: Some(directory),
                expires: closed + EXPIRY,
                next: now,
                attempts: 0,
                result: None,
                published: false,
            });
            totals.peak = totals.peak.max(jobs.len());
        }
        Command::Published(media) => {
            for job in jobs {
                if job.binding.media_id == media {
                    job.published = true;
                }
            }
        }
        Command::Cancel(media) => jobs.retain(|job| job.binding.media_id != media),
    }
}
async fn work(
    mut receive: mpsc::Receiver<Command>,
    mut cancel: watch::Receiver<bool>,
    mut source: impl Source,
) {
    let mut jobs = VecDeque::<Job>::new();
    let mut totals = Totals::default();
    loop {
        if *cancel.borrow() {
            break;
        }
        while let Ok(cmd) = receive.try_recv() {
            command(&mut jobs, cmd, &mut totals);
        }
        let now = Instant::now();
        jobs.retain(|j| {
            let keep =
                now < j.expires && j.directory.as_ref().is_some_and(DirectoryGuard::available);
            if !keep {
                totals.expired += 1;
            }
            keep
        });
        if let Some(index) = jobs.iter().position(|j| j.published && j.result.is_some()) {
            let mut job = jobs.remove(index).expect("job exists");
            let result = job.result.take().expect("confirmed facts");
            let directory = job.directory.take().expect("owned directory");
            // Joined filesystem ownership survives cancellation of the coordinator.
            match tokio::task::spawn_blocking(move || directory.install_result(&result)).await {
                Ok(Ok(bytes)) => {
                    totals.writes += 1;
                    totals.result_bytes += bytes as u64;
                }
                _ => totals.abandoned += 1,
            }
            continue;
        }
        if let Some(index) = jobs
            .iter()
            .position(|j| j.result.is_none() && j.next <= now && j.attempts < MAX_ATTEMPTS)
        {
            let began = Instant::now();
            jobs[index].next = began + CADENCE;
            jobs[index].attempts += 1;
            let media = jobs[index].binding.media_id.clone();
            let expires = jobs[index].expires;
            totals.attempts += 1;
            let response = acquire_owned(
                &mut source,
                &mut receive,
                &mut cancel,
                &mut jobs,
                &media,
                expires,
                &mut totals,
            )
            .await;
            totals.latency_ms += began.elapsed().as_millis();
            let Some(index) = jobs.iter().position(|j| j.binding.media_id == media) else {
                continue;
            };
            let job = &mut jobs[index];
            job.next = Instant::now() + CADENCE;
            if let Some(eog) = response {
                match eog.confirm(&job.binding) {
                    Ok(result) => {
                        totals.primary_win_one += u64::from(eog.primary_win() == Some(1));
                        totals.primary_win_zero += u64::from(eog.primary_win() == Some(0));
                        job.result = Some(result);
                        totals.confirmed += 1;
                        tracing::info!(
                            confirmed = totals.confirmed,
                            primary_win_one = totals.primary_win_one,
                            primary_win_zero = totals.primary_win_zero,
                            "post-game confirmation totals"
                        );
                    }
                    Err(Rejection::Association) => {
                        jobs.remove(index);
                        totals.rejected += 1;
                    }
                    Err(Rejection::NotReady) => {}
                }
            }
            continue;
        }
        let deadline = jobs
            .iter()
            .map(|j| {
                if j.result.is_some() || j.attempts == MAX_ATTEMPTS {
                    j.expires
                } else {
                    j.next.min(j.expires)
                }
            })
            .min();
        tokio::select! {biased;
            _=cancel.changed()=>break,
            cmd=receive.recv()=>match cmd {Some(cmd)=>command(&mut jobs,cmd,&mut totals),None=>break},
            _=async {match deadline {Some(deadline)=>tokio::time::sleep_until(deadline).await,None=>std::future::pending().await}}=>{}
        }
    }
    let (requests, response_bytes) = source.totals();
    tracing::info!(
        requests,
        response_bytes,
        attempts = totals.attempts,
        confirmed = totals.confirmed,
        primary_win_one = totals.primary_win_one,
        primary_win_zero = totals.primary_win_zero,
        rejected = totals.rejected,
        expired = totals.expired,
        dropped = totals.dropped,
        abandoned = totals.abandoned,
        writes = totals.writes,
        result_bytes = totals.result_bytes,
        peak_pending = totals.peak,
        latency_ms = totals.latency_ms,
        "post-game coordinator joined"
    );
}

// Commands, expiry and deletion remain observable while discovery is in flight.
// Cancelling a job signals acquisition, then joins it; no OS worker is detached.
async fn acquire_owned(
    source: &mut impl Source,
    receive: &mut mpsc::Receiver<Command>,
    cancel: &mut watch::Receiver<bool>,
    jobs: &mut VecDeque<Job>,
    media: &MediaId,
    expires: Instant,
    totals: &mut Totals,
) -> Option<Eog> {
    let (stop, mut own_cancel) = watch::channel(false);
    let directory = jobs
        .iter()
        .find(|j| &j.binding.media_id == media)?
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
                if !jobs.iter().any(|j|&j.binding.media_id==media){stop.send_replace(true);let _=acquisition.await;return None;}
            }
            _=tokio::time::sleep_until((Instant::now()+Duration::from_secs(1)).min(expires))=>{
                if Instant::now()>=expires || !jobs.iter().find(|j|&j.binding.media_id==media).is_some_and(|j|j.directory.as_ref().is_some_and(DirectoryGuard::available)) {
                    jobs.retain(|j|&j.binding.media_id!=media);stop.send_replace(true);let _=acquisition.await;return None;
                }
            }
            response=&mut acquisition=>return response,
        }
    }
}
