//! Byte-bounded nonblocking acquisition admission and an isolated disk worker.
//! No capture thread owns serialization, queue waits, writes or checkpoints.
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chronobreak_league_data::observation::{checkpoint, journal, *};
use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

use super::acquisition::Sink;
use crate::league_client::sidecar::DirectoryGuard;

pub const JOURNAL_FILE: &str = "league_observations.jsonl";
pub const CHECKPOINT_FILE: &str = "league_checkpoint.json";
pub const QUEUE_BYTES: usize = 4 * 1024 * 1024;
const CONTROL_BYTES: usize = 64 * 1024;
const QUEUE_SLOTS: usize = 128;
const CONTROL_SLOTS: usize = 8;
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(60);
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);
const FLUSH_INTERVAL: Duration = Duration::from_secs(2);
const FLUSH_BYTES: usize = 256 * 1024;
const MAX_WRITERS: usize = 4;
// Serde derives fixed arrays through length 32 in this workspace. The final
// bucket saturates at 2^30 microseconds (about 18 minutes), far beyond a
// healthy worker's expected commit lag.
const COMMIT_LAG_BUCKETS: usize = 32;

/// A timed-out OS write cannot be aborted. Keep its task owned until it exits,
/// and keep its permit until then, so repeated recordings cannot grow workers.
/// Only already-finished handles are discarded. No I/O runs under this mutex.
struct WriterSupervisor {
    permits: Arc<Semaphore>,
    retired: Mutex<Vec<tokio::task::JoinHandle<WriterReport>>>,
}
impl WriterSupervisor {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            permits: Arc::new(Semaphore::new(MAX_WRITERS)),
            retired: Mutex::new(Vec::new()),
        })
    }
    fn global() -> Arc<Self> {
        static SUPERVISOR: OnceLock<Arc<WriterSupervisor>> = OnceLock::new();
        Arc::clone(SUPERVISOR.get_or_init(Self::new))
    }
    fn reserve(&self) -> Result<OwnedSemaphorePermit, journal::Error> {
        self.retired
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|task| !task.is_finished());
        Arc::clone(&self.permits)
            .try_acquire_owned()
            .map_err(|_| journal::Error::Capacity)
    }
    fn retain(&self, task: tokio::task::JoinHandle<WriterReport>) {
        // Abort prevents a queued blocking task from starting. An executing one
        // observes Budget::abandoned between operations and stays owned here.
        task.abort();
        let mut retired = self.retired.lock().unwrap_or_else(|e| e.into_inner());
        retired.retain(|task| !task.is_finished());
        retired.push(task);
    }
}

struct Budget {
    admission: Mutex<()>,
    bytes: AtomicUsize,
    slots: AtomicUsize,
    maximum_bytes: AtomicUsize,
    rejected: AtomicU64,
    closing: AtomicBool,
    abandoned: AtomicBool,
    failed: AtomicBool,
}
impl Budget {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            admission: Mutex::new(()),
            bytes: AtomicUsize::new(0),
            slots: AtomicUsize::new(0),
            maximum_bytes: AtomicUsize::new(0),
            rejected: AtomicU64::new(0),
            closing: AtomicBool::new(false),
            abandoned: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        })
    }
    fn close(&self) {
        // Admission holds this only across reserve/try_send, never serialization
        // or I/O. Once closing is visible, every successful admission is queued.
        let _gate = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        self.closing.store(true, Ordering::Release);
    }
    fn ensure_owned(&self) -> Result<(), journal::Error> {
        if self.abandoned.load(Ordering::Acquire) {
            Err(journal::Error::Invalid(
                "journal owner cancelled; durability tail unknown",
            ))
        } else {
            Ok(())
        }
    }
    fn reserve(self: &Arc<Self>, bytes: usize, control: bool) -> Option<Lease> {
        if self.closing.load(Ordering::Acquire) || self.failed.load(Ordering::Acquire) {
            return None;
        }
        let slot_limit = if control {
            QUEUE_SLOTS
        } else {
            QUEUE_SLOTS - CONTROL_SLOTS
        };
        let byte_limit = if control {
            QUEUE_BYTES
        } else {
            QUEUE_BYTES - CONTROL_BYTES
        };
        self.slots
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < slot_limit).then_some(n + 1)
            })
            .ok()?;
        let previous = self
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                n.checked_add(bytes).filter(|n| *n <= byte_limit)
            });
        let Ok(previous) = previous else {
            self.slots.fetch_sub(1, Ordering::Release);
            return None;
        };
        self.maximum_bytes
            .fetch_max(previous + bytes, Ordering::Relaxed);
        Some(Lease {
            budget: Arc::clone(self),
            bytes,
        })
    }
}
struct Lease {
    budget: Arc<Budget>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.budget.bytes.fetch_sub(self.bytes, Ordering::Release);
        self.budget.slots.fetch_sub(1, Ordering::Release);
    }
}
struct Packet {
    records: Vec<Record>,
    charged_bytes: usize,
    admitted: Instant,
    _lease: Lease,
}

pub struct QueueSink {
    sender: SyncSender<Packet>,
    budget: Arc<Budget>,
}
impl QueueSink {
    pub fn failed(&self) -> bool {
        self.budget.failed.load(Ordering::Acquire)
    }
}
impl Sink for QueueSink {
    fn try_records(&mut self, rows: Vec<Record>) -> Result<(), Vec<Record>> {
        let accepted = self.try_admit(rows);
        if accepted.is_err() {
            self.budget.rejected.fetch_add(1, Ordering::Relaxed);
        }
        accepted
    }
}
impl QueueSink {
    fn try_admit(&mut self, rows: Vec<Record>) -> Result<(), Vec<Record>> {
        if self.budget.closing.load(Ordering::Acquire) || self.failed() {
            return Err(rows);
        }
        let control = rows.iter().all(|row| match row {
            Record::GapInterval { .. }
            | Record::Boundary { .. }
            | Record::SourceReset { .. }
            | Record::CalibrationInvalidated { .. } => true,
            Record::Observation { observation } => {
                matches!(observation.as_ref(), Observation::Gap { .. })
            }
            _ => false,
        });
        let limit = if control {
            CONTROL_BYTES
        } else {
            QUEUE_BYTES - CONTROL_BYTES
        };
        let bytes = queue_charge(&rows);
        if rows.is_empty() || rows.len() > journal::MAX_BATCH_RECORDS || bytes > limit {
            return Err(rows);
        }
        let Ok(_gate) = self.budget.admission.try_lock() else {
            return Err(rows);
        };
        let Some(lease) = self.budget.reserve(bytes, control) else {
            return Err(rows);
        };
        let packet = Packet {
            records: rows,
            charged_bytes: bytes,
            admitted: Instant::now(),
            _lease: lease,
        };
        self.sender.try_send(packet).map_err(|error| match error {
            mpsc::TrySendError::Full(packet) | mpsc::TrySendError::Disconnected(packet) => {
                packet.records
            }
        })
    }
}

fn queue_charge(rows: &Vec<Record>) -> usize {
    rows.iter().fold(
        rows.capacity()
            .saturating_mul(std::mem::size_of::<Record>())
            .saturating_add(128),
        |bytes, row| bytes.saturating_add(row.queue_charge()),
    )
}

#[derive(Debug, Default, Serialize)]
pub struct WriterReport {
    pub accepted_batches: u64,
    pub committed_batches: u64,
    pub committed_records: u64,
    pub journal_bytes: u64,
    /// Logical bytes supplied to atomic sidecar replacements. These are not a
    /// filesystem-level write-amplification measurement.
    pub checkpoint_bytes: u64,
    pub manifest_bytes: u64,
    /// Journal commit and in-memory checkpoint update wall time. Atomic sidecar
    /// replacements and queue waits are excluded; this is not CPU time.
    pub write_wall_us: u64,
    pub maximum_commit_lag_us: u64,
    /// Bounded histogram of per-admission durability lag. Bucket zero is zero
    /// microseconds; bucket N is [2^(N-1), 2^N - 1] microseconds. The final
    /// bucket is saturated.
    pub commit_lag_histogram: [u64; COMMIT_LAG_BUCKETS],
    /// Conservative owned-record admission credits, including spare capacity;
    /// these are no longer pre-serialized JSON byte counts.
    pub queue_high_water_bytes: usize,
    pub rejected_batches: u64,
    pub checkpoint_writes: u64,
    pub checkpoint_failures: u64,
    pub manifest_writes: u64,
    pub manifest_failures: u64,
    pub failed: bool,
    pub failure: Option<String>,
}
impl WriterReport {
    fn record_commit_lag(&mut self, lag_us: u64) {
        let bucket = if lag_us == 0 {
            0
        } else {
            usize::try_from(u64::BITS - lag_us.leading_zeros())
                .unwrap_or(COMMIT_LAG_BUCKETS - 1)
                .min(COMMIT_LAG_BUCKETS - 1)
        };
        self.commit_lag_histogram[bucket] = self.commit_lag_histogram[bucket].saturating_add(1);
    }
}

pub struct WriterSession {
    budget: Arc<Budget>,
    task: Option<tokio::task::JoinHandle<WriterReport>>,
    supervisor: Arc<WriterSupervisor>,
}
impl WriterSession {
    /// Exercise the production writer against an existing temporary fixture bundle.
    /// The normal exclusive-create and pinned-directory checks still apply.
    #[cfg(all(windows, feature = "replay-time-fixture"))]
    pub async fn start_fixture(
        directory: &std::path::Path,
        scope: SourceEpoch,
    ) -> Result<(Self, QueueSink), journal::Error> {
        let directory = DirectoryGuard::capture(directory, scope.media_id.clone())
            .map_err(journal::Error::Invalid)?;
        Self::start(directory, scope).await
    }

    /// Directory creation and media authorization belong to the recording owner.
    /// Opening a journal is exclusive and never truncates an existing bundle.
    pub(crate) async fn start(
        directory: DirectoryGuard,
        scope: SourceEpoch,
    ) -> Result<(Self, QueueSink), journal::Error> {
        let supervisor = WriterSupervisor::global();
        let permit = supervisor.reserve()?;
        let (sender, receiver) = mpsc::sync_channel(QUEUE_SLOTS);
        let budget = Budget::new();
        let task_budget = Arc::clone(&budget);
        let (ready_tx, ready_rx) = oneshot::channel();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let header = journal::Header::new(&scope);
            let writer = match task_budget.ensure_owned().and_then(|()| {
                directory
                    .create_journal(&scope.media_id)
                    .map_err(journal::Error::Invalid)
                    .and_then(|file| journal::Writer::new(file, header.clone()))
            }) {
                Ok(writer) => writer,
                Err(error) => {
                    task_budget.failed.store(true, Ordering::Release);
                    let reason = error.to_string();
                    let _ = ready_tx.send(Err(error));
                    return WriterReport {
                        failed: true,
                        failure: Some(reason),
                        ..Default::default()
                    };
                }
            };
            let state = checkpoint::State::new(header, writer.position().clone());
            let initial_manifest = task_budget
                .ensure_owned()
                .and_then(|()| manifest::Manifest::from_checkpoint(state.snapshot()).encode())
                .and_then(|bytes| {
                    let byte_count = bytes.len() as u64;
                    directory
                        .replace_telemetry(true, &bytes)
                        .map_err(journal::Error::Invalid)
                        .map(|()| byte_count)
                });
            let initial_manifest_bytes = match initial_manifest {
                Ok(bytes) => bytes,
                Err(error) => {
                    task_budget.failed.store(true, Ordering::Release);
                    let reason = error.to_string();
                    let _ = ready_tx.send(Err(error));
                    return WriterReport {
                        failed: true,
                        failure: Some(reason),
                        ..Default::default()
                    };
                }
            };
            if ready_tx.send(Ok(())).is_err() {
                return WriterReport::default();
            }
            let mut report = run_worker(
                journal::RetainingWriter::new(writer),
                state,
                receiver,
                task_budget,
                Some(directory),
            );
            report.manifest_writes = report.manifest_writes.saturating_add(1);
            report.manifest_bytes = report.manifest_bytes.saturating_add(initial_manifest_bytes);
            report
        });
        let session = Self {
            budget: Arc::clone(&budget),
            task: Some(task),
            supervisor,
        };
        match tokio::time::timeout(SHUTDOWN_DEADLINE, ready_rx).await {
            Ok(Ok(Ok(()))) => Ok((session, QueueSink { sender, budget })),
            Ok(Ok(Err(error))) => Err(error),
            _ => Err(journal::Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "journal initialization did not complete",
            ))),
        }
    }

    pub async fn stop(mut self) -> WriterReport {
        self.budget.close();
        let Some(task) = self.task.as_mut() else {
            return WriterReport::default();
        };
        match tokio::time::timeout(SHUTDOWN_DEADLINE, &mut *task).await {
            Ok(Ok(report)) => {
                self.task.take();
                report
            }
            Ok(Err(error)) => {
                self.task.take();
                WriterReport {
                    failed: true,
                    failure: Some(error.to_string()),
                    ..Default::default()
                }
            }
            Err(_) => WriterReport {
                failed: true,
                failure: Some("journal shutdown deadline exceeded; durability tail unknown".into()),
                ..Default::default()
            },
        }
    }
}
impl Drop for WriterSession {
    fn drop(&mut self) {
        self.budget.close();
        if let Some(task) = self.task.take() {
            self.budget.abandoned.store(true, Ordering::Release);
            self.supervisor.retain(task);
        }
    }
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn run_worker<W: journal::DurableWrite>(
    mut writer: journal::RetainingWriter<W>,
    mut state: checkpoint::State,
    receiver: Receiver<Packet>,
    budget: Arc<Budget>,
    checkpoint_output: Option<DirectoryGuard>,
) -> WriterReport {
    let mut report = WriterReport::default();
    let mut checkpoint_at = Instant::now();
    let mut deferred = None;
    loop {
        if let Err(error) = budget.ensure_owned() {
            report.failed = true;
            report.failure = Some(error.to_string());
            break;
        }
        let first = match deferred
            .take()
            .map(Ok)
            .unwrap_or_else(|| receiver.recv_timeout(Duration::from_millis(100)))
        {
            Ok(packet) => packet,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if budget.closing.load(Ordering::Acquire) {
                    break;
                }
                continue;
            }
        };
        let mut bytes = first.charged_bytes;
        let mut count = first.records.len();
        let deadline = first.admitted + FLUSH_INTERVAL;
        let oldest = first.admitted;
        let mut packets = vec![first];
        while bytes < FLUSH_BYTES && count < journal::MAX_BATCH_RECORDS {
            let closing = budget.closing.load(Ordering::Acquire);
            let wait = if closing {
                Duration::ZERO
            } else {
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(100))
            };
            if wait.is_zero() && !closing {
                break;
            }
            match receiver.recv_timeout(wait) {
                Ok(packet) => {
                    if count + packet.records.len() > journal::MAX_BATCH_RECORDS
                        || bytes + packet.charged_bytes > QUEUE_BYTES - CONTROL_BYTES
                    {
                        deferred = Some(packet);
                        break;
                    }
                    bytes += packet.charged_bytes;
                    count += packet.records.len();
                    packets.push(packet);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if closing || Instant::now() >= deadline {
                        break;
                    }
                }
            }
        }
        let start = Instant::now();
        let mut checkpoint_result = Ok(());
        let result = budget.ensure_owned().and_then(|()| {
            // Keep packet leases until commit completes, even though their
            // records move into this bounded worker batch.
            let mut records = Vec::with_capacity(count);
            for packet in &mut packets {
                records.append(&mut packet.records);
            }
            writer.commit_owned_with(records, |committed, position| {
                checkpoint_result = state.apply(committed, position);
            })
        });
        report.write_wall_us = report.write_wall_us.saturating_add(micros(start.elapsed()));
        let oldest_lag_us = micros(oldest.elapsed());
        report.maximum_commit_lag_us = report.maximum_commit_lag_us.max(oldest_lag_us);
        match result {
            Ok(position) => {
                report.accepted_batches += packets.len() as u64;
                report.committed_batches += 1;
                report.committed_records = position.last_sequence;
                report.journal_bytes = position.end_offset;
                for packet in &packets {
                    report.record_commit_lag(micros(packet.admitted.elapsed()));
                }
            }
            Err(error) => {
                budget.failed.store(true, Ordering::Release);
                report.failed = true;
                report.failure = Some(error.to_string());
                break;
            }
        }
        if let Err(error) = checkpoint_result {
            // The journal is committed but this derived checkpoint is unusable.
            // Stop before advertising a partial checkpoint; recovery can replay.
            budget.failed.store(true, Ordering::Release);
            report.failed = true;
            report.failure = Some(error.to_string());
            break;
        }
        if let Err(error) = budget.ensure_owned() {
            report.failed = true;
            report.failure = Some(error.to_string());
            break;
        }
        if checkpoint_at.elapsed() >= CHECKPOINT_INTERVAL {
            persist_checkpoint(&state, &checkpoint_output, &budget, &mut report);
            persist_manifest(&state, &checkpoint_output, &budget, &mut report);
            checkpoint_at = Instant::now();
        }
    }
    if !report.failed {
        persist_checkpoint(&state, &checkpoint_output, &budget, &mut report);
        persist_manifest(&state, &checkpoint_output, &budget, &mut report);
    }
    report.queue_high_water_bytes = budget.maximum_bytes.load(Ordering::Relaxed);
    report.rejected_batches = budget.rejected.load(Ordering::Relaxed);
    report
}

// The manifest is a recovery hint, not an acknowledgement. A valid stale anchor
// is recoverable through the journal; publish it with periodic checkpoints and
// after a graceful drain rather than replacing/syncing it for every commit.
fn persist_manifest(
    state: &checkpoint::State,
    output: &Option<DirectoryGuard>,
    budget: &Budget,
    report: &mut WriterReport,
) {
    let Some(directory) = output else { return };
    let result = budget
        .ensure_owned()
        .and_then(|()| manifest::Manifest::from_checkpoint(state.snapshot()).encode())
        .and_then(|bytes| {
            let count = bytes.len() as u64;
            directory
                .replace_telemetry(true, &bytes)
                .map_err(journal::Error::Invalid)
                .map(|()| count)
        });
    match result {
        Ok(bytes) => {
            report.manifest_writes += 1;
            report.manifest_bytes = report.manifest_bytes.saturating_add(bytes);
        }
        Err(_) => report.manifest_failures += 1,
    }
}

fn persist_checkpoint(
    state: &checkpoint::State,
    output: &Option<DirectoryGuard>,
    budget: &Budget,
    report: &mut WriterReport,
) {
    let Some(directory) = output else {
        return;
    };
    let result = budget
        .ensure_owned()
        .and_then(|()| state.snapshot().encode())
        .and_then(|bytes| {
            let byte_count = bytes.len() as u64;
            directory
                .replace_telemetry(false, &bytes)
                .map_err(journal::Error::Invalid)
                .map(|()| byte_count)
        });
    match result {
        Ok(bytes) => {
            report.checkpoint_writes += 1;
            report.checkpoint_bytes = report.checkpoint_bytes.saturating_add(bytes);
        }
        Err(_) => {
            report.checkpoint_failures += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chronobreak_replay_time::MediaId;
    use std::io::Write;
    #[cfg(windows)]
    use std::path::Path;

    fn scope() -> SourceEpoch {
        SourceEpoch {
            media_id: MediaId::parse("10000000-0000-4000-8000-000000000001").unwrap(),
            run_id: MediaId::parse("10000000-0000-4000-8000-000000000002").unwrap(),
            epoch: 0,
        }
    }
    fn row(id: u64, text: usize) -> Record {
        Record::Observation {
            observation: Box::new(Observation::Success {
                evidence: Evidence {
                    id: ObservationId(id),
                    source: scope(),
                    endpoint: Endpoint::GameStats,
                    timing: RequestTiming {
                        start_us: id,
                        end_us: id,
                        received_at: "2026-10-15T00:00:00Z".into(),
                    },
                    game_stats: None,
                    calibration: None,
                },
                payload: Payload::GameStats(GameStats {
                    map_name: Some("x".repeat(text)),
                    ..Default::default()
                }),
            }),
        }
    }
    fn gap(id: u64) -> Record {
        let Record::Observation { observation } = row(id, 0) else {
            unreachable!()
        };
        let Observation::Success { evidence, .. } = *observation else {
            unreachable!()
        };
        Record::Observation {
            observation: Box::new(Observation::Gap {
                evidence,
                reason: GapReason::Backpressure,
            }),
        }
    }
    fn channel() -> (QueueSink, Receiver<Packet>, Arc<Budget>) {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_SLOTS);
        let budget = Budget::new();
        (
            QueueSink {
                sender,
                budget: Arc::clone(&budget),
            },
            receiver,
            budget,
        )
    }

    #[test]
    fn saturation_reserves_bytes_and_slots_for_gap_accounting() {
        let (mut sink, receiver, budget) = channel();
        let mut accepted = 0;
        while sink.try_records(vec![row(accepted, 120_000)]).is_ok() {
            accepted += 1;
        }
        assert!(accepted > 0);
        assert!(budget.bytes.load(Ordering::Relaxed) <= QUEUE_BYTES - CONTROL_BYTES);
        assert!(sink.try_records(vec![gap(accepted)]).is_ok());
        assert!(budget.bytes.load(Ordering::Relaxed) <= QUEUE_BYTES);
        drop(receiver);
        assert_eq!(budget.bytes.load(Ordering::Relaxed), 0);
        assert_eq!(budget.slots.load(Ordering::Relaxed), 0);

        let (mut sink, receiver, budget) = channel();
        for id in 0..(QUEUE_SLOTS - CONTROL_SLOTS) {
            assert!(sink.try_records(vec![row(id as u64, 0)]).is_ok());
        }
        assert!(sink.try_records(vec![row(1000, 0)]).is_err());
        for id in 0..CONTROL_SLOTS {
            assert!(sink.try_records(vec![gap(2000 + id as u64)]).is_ok());
        }
        assert!(sink.try_records(vec![gap(3000)]).is_err());
        drop(receiver);
        assert_eq!(budget.slots.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn closed_or_disconnected_queue_rejects_atomically_and_releases_budget() {
        let (mut sink, receiver, budget) = channel();
        drop(receiver);
        assert!(sink.try_records(vec![row(1, 10)]).is_err());
        assert_eq!(budget.bytes.load(Ordering::Relaxed), 0);
        budget.close();
        assert!(sink.try_records(vec![gap(2)]).is_err());
        assert_eq!(budget.slots.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn owned_admission_accounts_for_spare_capacity_and_returns_rejected_records() {
        let (mut sink, receiver, budget) = channel();
        let mut record = row(1, 0);
        if let Record::Observation { observation } = &mut record
            && let Observation::Success {
                payload: Payload::GameStats(gs),
                ..
            } = observation.as_mut()
        {
            let mut name = String::with_capacity(QUEUE_BYTES);
            name.push('x');
            gs.map_name = Some(name);
        }
        let rejected = sink.try_records(vec![record]).unwrap_err();
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].queue_charge() >= QUEUE_BYTES);
        assert_eq!(budget.bytes.load(Ordering::Relaxed), 0);
        assert!(receiver.try_recv().is_err());

        let mut rows = Vec::with_capacity(QUEUE_BYTES / std::mem::size_of::<Record>() + 1);
        rows.push(gap(2));
        assert!(queue_charge(&rows) > QUEUE_BYTES);
        assert_eq!(sink.try_records(rows).unwrap_err().len(), 1);
        assert_eq!(budget.slots.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn closure_waits_for_an_admitted_sender_before_worker_drain() {
        let (mut sink, receiver, budget) = channel();
        // Pause an admission inside the gate, immediately before try_send. Stop
        // must not become visible to the draining worker until this packet lands.
        let gate = budget.admission.lock().unwrap();
        let closing_budget = Arc::clone(&budget);
        let (started_tx, started_rx) = mpsc::channel();
        let (closed_tx, closed_rx) = mpsc::channel();
        let closer = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            closing_budget.close();
            closed_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(closed_rx.recv_timeout(Duration::from_millis(50)).is_err());
        assert!(!budget.closing.load(Ordering::Acquire));
        let records = vec![row(1, 10)];
        let charged_bytes = queue_charge(&records);
        let lease = budget.reserve(charged_bytes, false).unwrap();
        sink.sender
            .try_send(Packet {
                records,
                charged_bytes,
                admitted: Instant::now(),
                _lease: lease,
            })
            .unwrap();
        drop(gate);
        closed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        closer.join().unwrap();
        let header = journal::Header::new(&scope());
        let writer = journal::Writer::new(
            FaultDisk {
                fail: Arc::new(AtomicBool::new(false)),
            },
            header.clone(),
        )
        .unwrap();
        let state = checkpoint::State::new(header, writer.position().clone());
        let report = run_worker(
            journal::RetainingWriter::new(writer),
            state,
            receiver,
            Arc::clone(&budget),
            None,
        );
        assert!(!report.failed);
        assert_eq!(report.committed_records, 1);
        assert!(sink.try_records(vec![row(2, 0)]).is_err());
        assert_eq!(budget.bytes.load(Ordering::Relaxed), 0);
    }

    struct FaultDisk {
        fail: Arc<AtomicBool>,
    }
    impl Write for FaultDisk {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.fail.load(Ordering::Relaxed) {
                Err(std::io::Error::other("injected disk full"))
            } else {
                Ok(bytes.len())
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl journal::DurableWrite for FaultDisk {
        fn sync_data(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct BlockedDisk {
        entered: Option<oneshot::Sender<()>>,
        release: Receiver<()>,
        syncs: usize,
    }
    impl Write for BlockedDisk {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl journal::DurableWrite for BlockedDisk {
        fn sync_data(&mut self) -> std::io::Result<()> {
            self.syncs += 1;
            if self.syncs == 2 {
                let _ = self.entered.take().unwrap().send(());
                self.release.recv().map_err(std::io::Error::other)?;
            }
            Ok(())
        }
    }

    async fn interrupted_writer(timeout: bool) {
        let supervisor = WriterSupervisor::new();
        let permit = supervisor.reserve().unwrap();
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (report_tx, report_rx) = oneshot::channel();
        let header = journal::Header::new(&scope());
        let writer = journal::Writer::new(
            BlockedDisk {
                entered: Some(entered_tx),
                release: release_rx,
                syncs: 0,
            },
            header.clone(),
        )
        .unwrap();
        let state = checkpoint::State::new(header, writer.position().clone());
        let (mut sink, receiver, budget) = channel();
        assert!(sink.try_records(vec![row(1, 10)]).is_ok());
        budget.close();
        let worker_budget = Arc::clone(&budget);
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let report = run_worker(
                journal::RetainingWriter::new(writer),
                state,
                receiver,
                worker_budget,
                None,
            );
            let _ = report_tx.send((
                report.failed,
                report.committed_records,
                report.checkpoint_writes,
            ));
            report
        });
        let session = WriterSession {
            budget: Arc::clone(&budget),
            task: Some(task),
            supervisor: Arc::clone(&supervisor),
        };
        entered_rx.await.unwrap();
        let mut stop = Box::pin(session.stop());
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(stop.as_mut().poll(cx).is_pending()))
                .await
        );
        if timeout {
            tokio::time::advance(SHUTDOWN_DEADLINE).await;
            assert!(stop.await.failed);
        } else {
            // Poll stop once, then abandon its future while sync_data is blocked.
            drop(stop);
        }
        assert!(budget.abandoned.load(Ordering::Acquire));
        assert!(sink.try_records(vec![row(2, 0)]).is_err());
        assert_eq!(supervisor.retired.lock().unwrap().len(), 1);
        assert_eq!(supervisor.permits.available_permits(), MAX_WRITERS - 1);
        let remaining: Vec<_> = (1..MAX_WRITERS)
            .map(|_| supervisor.reserve().unwrap())
            .collect();
        assert!(supervisor.reserve().is_err());
        release_tx.send(()).unwrap();
        assert_eq!(report_rx.await.unwrap(), (true, 1, 0));
        // Await the actual task retained by the supervisor, proving ownership
        // survives both dropping stop and its deadline, with no subsequent work.
        let retired = supervisor.retired.lock().unwrap().pop().unwrap();
        assert!(retired.await.unwrap().failed);
        assert_eq!(budget.bytes.load(Ordering::Relaxed), 0);
        drop(remaining);
        assert_eq!(supervisor.permits.available_permits(), MAX_WRITERS);
    }

    #[tokio::test(start_paused = true)]
    async fn dropped_stop_hands_blocked_writer_to_bounded_owner() {
        interrupted_writer(false).await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_timeout_hands_blocked_writer_to_bounded_owner() {
        interrupted_writer(true).await;
    }

    #[test]
    fn disk_failure_stops_admission_releases_queued_bytes_and_keeps_unknown_tail() {
        let fail = Arc::new(AtomicBool::new(false));
        let header = journal::Header::new(&scope());
        let writer = journal::Writer::new(
            FaultDisk {
                fail: Arc::clone(&fail),
            },
            header.clone(),
        )
        .unwrap();
        let state = checkpoint::State::new(header, writer.position().clone());
        let (mut sink, receiver, budget) = channel();
        assert!(sink.try_records(vec![row(1, 10)]).is_ok());
        assert!(sink.try_records(vec![row(2, 10)]).is_ok());
        fail.store(true, Ordering::Relaxed);
        let report = run_worker(
            journal::RetainingWriter::new(writer),
            state,
            receiver,
            Arc::clone(&budget),
            None,
        );
        assert!(report.failed);
        assert_eq!(report.committed_records, 0);
        assert!(sink.failed());
        assert!(sink.try_records(vec![gap(3)]).is_err());
        assert_eq!(budget.bytes.load(Ordering::Relaxed), 0);
        assert_eq!(budget.slots.load(Ordering::Relaxed), 0);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn real_worker_drains_accepted_batches_and_checkpoints_exact_committed_position() {
        let temp = tempfile::tempdir().unwrap();
        let directory = DirectoryGuard::capture(temp.path(), scope().media_id).unwrap();
        let (session, mut sink) = WriterSession::start(directory, scope()).await.unwrap();
        for id in 1..=20 {
            assert!(sink.try_records(vec![row(id, 10)]).is_ok());
        }
        let report = session.stop().await;
        assert!(!report.failed, "{:?}", report.failure);
        assert_eq!(report.committed_records, 20);
        assert_eq!(report.checkpoint_writes, 1);
        assert_eq!(report.manifest_writes, 2); // initial hint + graceful drain
        assert_eq!(report.checkpoint_failures, 0);
        assert_eq!(report.manifest_failures, 0);
        assert_eq!(
            report.commit_lag_histogram.iter().sum::<u64>(),
            report.accepted_batches
        );
        assert_eq!(
            report.checkpoint_bytes,
            std::fs::metadata(temp.path().join(CHECKPOINT_FILE))
                .unwrap()
                .len()
        );
        assert!(
            report.manifest_bytes
                >= std::fs::metadata(temp.path().join("game_log.json"))
                    .unwrap()
                    .len()
        );
        assert!(sink.try_records(vec![row(21, 0)]).is_err());
        let checkpoint: checkpoint::Checkpoint =
            serde_json::from_slice(&std::fs::read(temp.path().join(CHECKPOINT_FILE)).unwrap())
                .unwrap();
        let recovery = journal::recover(
            std::io::BufReader::new(std::fs::File::open(temp.path().join(JOURNAL_FILE)).unwrap()),
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(checkpoint.position, recovery.position);
        assert_eq!(checkpoint.game_stats.unwrap().0.id, ObservationId(20));
        assert_eq!(recovery.tail_bytes, 0);
    }

    #[cfg(windows)]
    fn file_bytes(path: &Path) -> u64 {
        std::fs::metadata(path).unwrap().len()
    }

    #[cfg(windows)]
    fn commit_lag_percentile_upper_bound_us(report: &WriterReport, percentile: u64) -> u64 {
        let total: u64 = report.commit_lag_histogram.iter().sum();
        assert!(total > 0);
        let rank = total.saturating_mul(percentile).saturating_add(99) / 100;
        let mut seen = 0_u64;
        for (bucket, count) in report.commit_lag_histogram.iter().enumerate() {
            seen = seen.saturating_add(*count);
            if seen >= rank {
                return if bucket + 1 == COMMIT_LAG_BUCKETS {
                    u64::MAX
                } else if bucket == 0 {
                    0
                } else {
                    (1_u64 << bucket) - 1
                };
            }
        }
        unreachable!("histogram total was checked")
    }

    #[cfg(windows)]
    #[test]
    fn durability_histogram_reports_zero_and_saturated_lags_honestly() {
        let mut report = WriterReport::default();
        report.record_commit_lag(0);
        assert_eq!(commit_lag_percentile_upper_bound_us(&report, 100), 0);
        report.record_commit_lag(8);
        assert_eq!(commit_lag_percentile_upper_bound_us(&report, 100), 15);
        report.record_commit_lag(u64::MAX);
        assert_eq!(commit_lag_percentile_upper_bound_us(&report, 100), u64::MAX);
        assert_eq!(report.commit_lag_histogram.iter().sum::<u64>(), 3);
    }

    #[cfg(windows)]
    async fn send_generated_records(sink: &mut QueueSink, count: u64) {
        const PACKET_RECORDS: usize = 32;
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut first_id = 1_u64;
        while first_id <= count {
            let remaining = count.saturating_sub(first_id).saturating_add(1);
            let packet_count = usize::try_from(remaining)
                .unwrap_or(PACKET_RECORDS)
                .min(PACKET_RECORDS);
            let mut rows: Vec<_> = (0..packet_count)
                .map(|offset| row(first_id + offset as u64, 192))
                .collect();
            while let Err(rejected) = sink.try_records(rows) {
                rows = rejected;
                // The real producer is nonblocking. This benchmark retries a
                // complete rejected packet so it can measure sustained worker
                // throughput without converting rejected admissions into loss.
                assert!(!sink.failed(), "generated persistence worker failed");
                assert!(
                    Instant::now() < deadline,
                    "generated persistence worker stalled"
                );
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            first_id = first_id.saturating_add(packet_count as u64);
        }
    }

    #[cfg(windows)]
    async fn persistence_burst_profile(virtual_seconds: u64) {
        // Candidate steady cadence: one GameStats and Events observation each
        // second plus PlayerList every two seconds. The rows are generated
        // allowlisted observations and are written through WriterSession, but
        // enqueued as fast as the production queue permits.
        let virtual_records = virtual_seconds.saturating_mul(5) / 2;
        let temp = tempfile::tempdir().unwrap();
        let directory = DirectoryGuard::capture(temp.path(), scope().media_id).unwrap();
        let (session, mut sink) = WriterSession::start(directory, scope()).await.unwrap();
        let elapsed = Instant::now();
        send_generated_records(&mut sink, virtual_records).await;
        let report = session.stop().await;
        assert!(!report.failed, "{:?}", report.failure);
        assert_eq!(report.committed_records, virtual_records);
        assert_eq!(
            report.checkpoint_failures, 0,
            "checkpoint replacement failed"
        );
        assert_eq!(report.manifest_failures, 0, "manifest replacement failed");
        assert!(report.committed_batches > 0);
        assert!(report.checkpoint_writes >= 1);
        assert!(report.queue_high_water_bytes <= QUEUE_BYTES);

        let journal_final_bytes = file_bytes(&temp.path().join(JOURNAL_FILE));
        let checkpoint_final_bytes = file_bytes(&temp.path().join(CHECKPOINT_FILE));
        let manifest_final_bytes = file_bytes(&temp.path().join("game_log.json"));
        assert_eq!(journal_final_bytes, report.journal_bytes);
        println!(
            "contract_persistence_burst {}",
            serde_json::json!({
                "measurement": "accelerated synthetic production persistence; elapsed wall time, not CPU",
                "limitations": [
                    "uses generated GameStats-shaped allowlisted records rather than Live endpoint timing",
                    "compressed enqueue does not exercise the 2 s flush or 60 s checkpoint cadence",
                    "does not measure private memory, long-run RSS, or League game impact"
                ],
                "virtual_seconds": virtual_seconds,
                "virtual_records_at_2_5_per_second": virtual_records,
                "elapsed_wall_us": elapsed.elapsed().as_micros(),
                "journal_commit_wall_us": report.write_wall_us,
                "durability_batches": report.committed_batches,
                "accepted_packets": report.accepted_batches,
                "admission_retry_attempts": report.rejected_batches,
                "queue_high_water_bytes": report.queue_high_water_bytes,
                "commit_lag_percentile_upper_bound_us": {
                    "p50": commit_lag_percentile_upper_bound_us(&report, 50),
                    "p95": commit_lag_percentile_upper_bound_us(&report, 95),
                    "p99": commit_lag_percentile_upper_bound_us(&report, 99),
                    "max": report.maximum_commit_lag_us
                },
                "logical_write_bytes": {
                    "journal_append": report.journal_bytes,
                    "manifest_replacements": report.manifest_bytes,
                    "checkpoint_replacements": report.checkpoint_bytes,
                    "total": report.journal_bytes.saturating_add(report.manifest_bytes).saturating_add(report.checkpoint_bytes)
                },
                "final_file_bytes": {
                    "journal": journal_final_bytes,
                    "manifest": manifest_final_bytes,
                    "checkpoint": checkpoint_final_bytes,
                    "total": journal_final_bytes.saturating_add(manifest_final_bytes).saturating_add(checkpoint_final_bytes)
                },
                "checkpoint_writes": report.checkpoint_writes,
                "manifest_writes": report.manifest_writes,
                "checkpoint_failures": report.checkpoint_failures,
                "manifest_failures": report.manifest_failures
            })
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "release-mode accelerated production persistence burst measurements; no CPU, RSS, cadence, or game-impact claim"]
    async fn contract_persistence_burst_profiles() {
        for virtual_seconds in [600_u64, 3_600, 7_200] {
            persistence_burst_profile(virtual_seconds).await;
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "release-mode 24-hour virtual input boundedness through the production persistence worker; no long-run RSS or cadence claim"]
    async fn contract_persistence_virtual_day_boundedness() {
        persistence_burst_profile(86_400).await;
    }
}
