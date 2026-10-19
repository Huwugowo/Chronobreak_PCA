//! Contract acquisition exercised through a fixture sink before M2 cutover.
//! One scheduler owns all Live endpoints; capture and persistence are not dependencies.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chronobreak_league_data::observation::{decode, *};
use chronobreak_replay_time::{GameCalibrationV2, GameTick};
use serde::Serialize;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::Instant;

use super::{CalibrationWindow, ClockSample};

const ENDPOINTS: [Endpoint; 4] = [
    Endpoint::GameStats,
    Endpoint::Events,
    Endpoint::PlayerList,
    Endpoint::ActivePlayer,
];
pub const NETWORK_DEADLINE: Duration = Duration::from_secs(2);

/// A transport returns a bounded body, never credentials or a raw response archive.
pub trait Source: Send + Sync + 'static {
    fn fetch(&self, endpoint: Endpoint) -> impl Future<Output = Result<Vec<u8>, GapReason>> + Send;
}

/// Nonblocking, atomic owned batch admission. Rejection returns every record.
/// The durable writer will own flushing/acknowledgement, not this acquisition task.
pub trait Sink {
    fn try_records(&mut self, records: Vec<Record>) -> Result<(), Vec<Record>>;
}

/// Per-endpoint aggregate diagnostics; these contain no player identifiers.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Counters {
    pub requests: u64,
    pub bytes: u64,
    pub failures: u64,
    pub skipped: u64,
    pub response_us: u64,
    pub maximum_response_us: u64,
    pub parse_us: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub endpoints: std::collections::BTreeMap<Endpoint, Counters>,
    pub maximum_in_flight: usize,
    pub unreported_gaps: u64,
    pub unreported_identity_rounds: u64,
    pub unreported_source_resets: u64,
}

/// The same focused routes as the current poller, with a streaming cap for all bodies.
pub struct HttpSource {
    http: reqwest::Client,
    base: String,
}

impl HttpSource {
    pub fn local() -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .connect_timeout(NETWORK_DEADLINE)
                .timeout(NETWORK_DEADLINE)
                .build()?,
            base: super::LIVE_CLIENT_BASE_URL.to_owned(),
        })
    }
}

pub fn body_limit(endpoint: Endpoint) -> usize {
    match endpoint {
        Endpoint::GameStats => 64 * 1024,
        Endpoint::ActivePlayer => 256 * 1024,
        Endpoint::PlayerList => 1024 * 1024,
        Endpoint::Events => MAX_BODY_BYTES,
        Endpoint::Gameflow | Endpoint::Summoner | Endpoint::EndOfGame => 0,
    }
}

impl Source for HttpSource {
    async fn fetch(&self, endpoint: Endpoint) -> Result<Vec<u8>, GapReason> {
        let route = match endpoint {
            Endpoint::GameStats => "gamestats",
            Endpoint::ActivePlayer => "activeplayer",
            Endpoint::PlayerList => "playerlist",
            Endpoint::Events => "eventdata",
            _ => return Err(GapReason::InvalidShape),
        };
        let map_error = |e: reqwest::Error| {
            if e.is_timeout() {
                GapReason::Timeout
            } else {
                GapReason::Transport
            }
        };
        let mut response = self
            .http
            .get(format!("{}/{route}", self.base))
            .send()
            .await
            .map_err(map_error)?;
        if !response.status().is_success() {
            return Err(GapReason::Http);
        }
        let limit = body_limit(endpoint);
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(GapReason::BodyLimit);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(map_error)? {
            if chunk.len() > limit - bytes.len() {
                return Err(GapReason::BodyLimit);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

struct Slot {
    next: Instant,
    in_flight: Option<Flight>,
    pending_gap: Option<(Evidence, Evidence, u64)>,
    stopped: bool,
}

struct Flight {
    id: ObservationId,
    started: Instant,
    source: SourceEpoch,
    refs: (Option<ObservationId>, Option<ObservationId>),
}

struct Completed {
    index: usize,
    id: ObservationId,
    started: Instant,
    ended: Instant,
    received_at: String,
    bytes: usize,
    parse_us: u64,
    payload: Result<Payload, GapReason>,
    source: SourceEpoch,
    refs: (Option<ObservationId>, Option<ObservationId>),
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}
fn wall_time() -> String {
    // The standard format is always representable for the system clock; formatting
    // failure is explicit unavailable provenance, never a replay timestamp.
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn evidence(
    source: &SourceEpoch,
    id: ObservationId,
    endpoint: Endpoint,
    interval: (Instant, Instant),
    origin: Instant,
    received_at: String,
    refs: (Option<ObservationId>, Option<ObservationId>),
) -> Evidence {
    Evidence {
        id,
        source: source.clone(),
        endpoint,
        timing: RequestTiming {
            start_us: micros(interval.0.saturating_duration_since(origin)),
            end_us: micros(interval.1.saturating_duration_since(origin)),
            received_at,
        },
        game_stats: refs.0,
        calibration: refs.1,
    }
}

fn record(sink: &mut impl Sink, slot: &mut Slot, evidence: &Evidence, row: Record) -> bool {
    record_batch(sink, slot, evidence, vec![row])
}

fn record_batch(
    sink: &mut impl Sink,
    slot: &mut Slot,
    evidence: &Evidence,
    rows: Vec<Record>,
) -> bool {
    if let Some((first, last, attempts)) = &slot.pending_gap
        && sink
            .try_records(vec![Record::GapInterval {
                first: first.clone(),
                last: last.clone(),
                attempts: *attempts,
                reason: GapReason::Backpressure,
            }])
            .is_ok()
    {
        slot.pending_gap = None;
    }
    if slot.pending_gap.is_none() && sink.try_records(rows).is_ok() {
        return true;
    }
    match &mut slot.pending_gap {
        Some((_, last, attempts)) => {
            *last = evidence.clone();
            *attempts = attempts.saturating_add(1);
        }
        None => slot.pending_gap = Some((evidence.clone(), evidence.clone(), 1)),
    }
    false
}

fn retry_pending(sink: &mut impl Sink, pending: &mut Option<Record>) {
    if let Some(row) = pending.take()
        && let Err(rows) = sink.try_records(vec![row])
    {
        *pending = rows.into_iter().next();
    }
}

/// Runs until cancellation; AP admission closes independently. Endpoint failures
/// never terminate other acquisition, and overdue intervals never burst to catch up.
/// This is the M1 fixture boundary; the service switches over only with the M2 writer.
pub async fn run<S: Source>(
    source: Arc<S>,
    sink: &mut impl Sink,
    mut scope: SourceEpoch,
    video_started: Instant,
    mut cancel: watch::Receiver<bool>,
    mut identity_open: watch::Receiver<bool>,
) -> Summary {
    let origin = video_started;
    let mut slots: [Slot; 4] = std::array::from_fn(|_| Slot {
        next: origin,
        in_flight: None,
        pending_gap: None,
        stopped: false,
    });
    let mut summary = Summary::default();
    let mut tasks = JoinSet::new();
    let mut sequence = 0_u64;
    let mut calibration_window = CalibrationWindow::default();
    let mut calibration: Option<(ObservationId, GameCalibrationV2)> = None;
    let mut last_clock: Option<GameTick> = None;
    let mut latest_gs = None;
    let mut identity_rounds = identity::IdentityRounds::default();
    let mut participants = identity::Participants::new(scope.clone());
    let mut local_identity = None;
    let mut pending_identity = None;
    let mut pending_reset = None;
    for (index, endpoint) in ENDPOINTS.iter().enumerate() {
        let e = evidence(
            &scope,
            ObservationId(sequence),
            *endpoint,
            (origin, origin),
            origin,
            wall_time(),
            (None, None),
        );
        sequence += 1;
        record(
            sink,
            &mut slots[index],
            &e,
            Record::Boundary {
                evidence: e.clone(),
                active: true,
            },
        );
    }
    loop {
        if *cancel.borrow() {
            break;
        }
        retry_pending(sink, &mut pending_reset);
        if !*identity_open.borrow() && !slots[3].stopped {
            // An already dispatched AP attempt remains owned and is accounted for.
            slots[3].stopped = true;
            let now = Instant::now();
            let e = evidence(
                &scope,
                ObservationId(sequence),
                Endpoint::ActivePlayer,
                (now, now),
                origin,
                wall_time(),
                (latest_gs, calibration.as_ref().map(|c| c.0)),
            );
            sequence += 1;
            record(
                sink,
                &mut slots[3],
                &e,
                Record::Boundary {
                    evidence: e.clone(),
                    active: false,
                },
            );
        }
        retry_pending(sink, &mut pending_identity);
        if pending_identity.is_none()
            && !slots[3].stopped
            && let Some(round) =
                identity_rounds.take_ready(micros(Instant::now().saturating_duration_since(origin)))
        {
            let row = Record::IdentityRound {
                round: Box::new(round),
            };
            pending_identity = Some(row);
            retry_pending(sink, &mut pending_identity);
        }
        let now = Instant::now();
        for (index, slot) in slots.iter_mut().enumerate() {
            if slot.stopped || slot.in_flight.is_some() || slot.next > now {
                continue;
            }
            let endpoint = ENDPOINTS[index];
            let id = ObservationId(sequence);
            sequence += 1;
            // Do not acquire a second source transition before admitting the
            // first. This bounds pending epoch state without losing a boundary
            // or emitting data whose predecessor the writer never received.
            if pending_reset.is_some() {
                let e = evidence(
                    &scope,
                    id,
                    endpoint,
                    (now, now),
                    origin,
                    wall_time(),
                    (None, None),
                );
                match &mut slot.pending_gap {
                    Some((_, last, attempts)) => {
                        *last = e;
                        *attempts = attempts.saturating_add(1);
                    }
                    None => slot.pending_gap = Some((e.clone(), e, 1)),
                }
                summary.endpoints.entry(endpoint).or_default().skipped += 1;
                if endpoint == Endpoint::ActivePlayer {
                    identity_rounds.missed_attempt();
                }
                slot.next = now + cadence(endpoint, false, false);
                continue;
            }
            // A saturated sink holds only one aggregate gap per endpoint. Retry
            // its admission at cadence without fetching more disposable payloads.
            if let Some((first, last, attempts)) = &slot.pending_gap {
                if sink
                    .try_records(vec![Record::GapInterval {
                        first: first.clone(),
                        last: last.clone(),
                        attempts: *attempts,
                        reason: GapReason::Backpressure,
                    }])
                    .is_ok()
                {
                    slot.pending_gap = None;
                } else {
                    let e = evidence(
                        &scope,
                        id,
                        endpoint,
                        (now, now),
                        origin,
                        wall_time(),
                        (latest_gs, calibration.as_ref().map(|c| c.0)),
                    );
                    if let Some((_, last, attempts)) = &mut slot.pending_gap {
                        *last = e;
                        *attempts = attempts.saturating_add(1);
                    }
                    summary.endpoints.entry(endpoint).or_default().skipped += 1;
                    if endpoint == Endpoint::ActivePlayer {
                        identity_rounds.missed_attempt();
                    }
                    slot.next = now + cadence(endpoint, calibration.is_some(), false);
                    continue;
                }
            }
            summary.endpoints.entry(endpoint).or_default().requests += 1;
            let source = Arc::clone(&source);
            let source_scope = scope.clone();
            let refs = (latest_gs, calibration.as_ref().map(|c| c.0));
            slot.in_flight = Some(Flight {
                id,
                started: now,
                source: source_scope.clone(),
                refs,
            });
            tasks.spawn(async move {
                let result = tokio::time::timeout(NETWORK_DEADLINE, source.fetch(endpoint))
                    .await
                    .unwrap_or(Err(GapReason::Timeout));
                let ended = Instant::now();
                let received_at = wall_time();
                let bytes = result.as_ref().map_or(0, Vec::len);
                let parse_start = std::time::Instant::now();
                let payload = result.and_then(|bytes| {
                    if bytes.len() > body_limit(endpoint) {
                        Err(GapReason::BodyLimit)
                    } else {
                        decode::live(endpoint, &bytes)
                    }
                });
                Completed {
                    index,
                    id,
                    started: now,
                    ended,
                    received_at,
                    bytes,
                    parse_us: micros(parse_start.elapsed()),
                    payload,
                    source: source_scope,
                    refs,
                }
            });
        }
        summary.maximum_in_flight = summary.maximum_in_flight.max(tasks.len());
        let next = slots
            .iter()
            .filter(|s| !s.stopped && s.in_flight.is_none())
            .map(|s| s.next)
            .min();
        tokio::select! {
            biased;
            _ = cancel.changed() => { if *cancel.borrow() || cancel.has_changed().is_err() { break; } }
            // Once closed, do not spin on a dropped admission sender.
            _ = identity_open.changed(), if !slots[3].stopped => { if identity_open.has_changed().is_err() { slots[3].next = Instant::now(); identity_open = watch::channel(false).1; } }
            completed = tasks.join_next(), if !tasks.is_empty() => {
                let Some(Ok(mut completed)) = completed else { break; };
                let endpoint = ENDPOINTS[completed.index];
                slots[completed.index].in_flight = None;
                let counters = summary.endpoints.entry(endpoint).or_default();
                counters.bytes = counters.bytes.saturating_add(completed.bytes as u64);
                let elapsed = micros(completed.ended.saturating_duration_since(completed.started));
                counters.response_us = counters.response_us.saturating_add(elapsed);
                counters.maximum_response_us = counters.maximum_response_us.max(elapsed);
                counters.parse_us = counters.parse_us.saturating_add(completed.parse_us);
                if completed.source.epoch != scope.epoch { completed.payload = Err(GapReason::SourceReset); }
                let mut e = evidence(&completed.source, completed.id, endpoint, (completed.started, completed.ended), origin, completed.received_at, completed.refs);
                let mut new_calibration = None;
                let mut invalidate_calibration = false;
                if endpoint == Endpoint::GameStats {
                    let tick = match &completed.payload { Ok(Payload::GameStats(gs)) => gs.game_tick, _ => None };
                    if let Some(tick) = tick {
                        if last_clock.is_some_and(|last| tick < last) {
                            let previous_epoch = scope.epoch;
                            scope.epoch = scope.epoch.saturating_add(1);
                            e.source = scope.clone();
                            calibration = None; calibration_window.clear(); latest_gs = None;
                            identity_rounds.clear_context();
                            if pending_identity.take().is_some() { summary.unreported_identity_rounds += 1; }
                            participants = identity::Participants::new(scope.clone());
                            local_identity = None;
                            e.calibration = None; e.game_stats = None;
                            let mut boundary = e.clone();
                            boundary.id = ObservationId(sequence);
                            sequence += 1;
                            pending_reset = Some(Record::SourceReset { evidence: boundary, previous_epoch });
                        } else if last_clock == Some(tick) {
                            // Non-advancing time invalidates calibration but does not
                            // establish a different match/source epoch (e.g. loading).
                            invalidate_calibration = calibration.is_some();
                            calibration = None; calibration_window.clear();
                            e.calibration = None;
                        }
                        last_clock = Some(tick);
                        if calibration.is_none() {
                            new_calibration = calibration_window.push(ClockSample {
                                midpoint: super::request_midpoint(completed.started.into_std(), completed.ended.into_std()),
                                game_tick: tick, round_trip: completed.ended.duration_since(completed.started),
                            }, video_started.into_std());
                        }
                    } else { calibration_window.clear(); }
                }
                let valid_gs = matches!(&completed.payload, Ok(Payload::GameStats(gs)) if gs.game_tick.is_some());
                let mut staged_participants = None;
                if let Ok(Payload::PlayerList(roster)) = &mut completed.payload {
                    let mut staged = participants.clone();
                    match staged.observe(&e.source, roster, local_identity.as_deref()) {
                        Ok(()) => staged_participants = Some(staged),
                        Err(reason) => completed.payload = Err(reason),
                    }
                }
                if let Ok(Payload::Events(events)) = &mut completed.payload {
                    for event in events { event.actors = Some(events::resolve_actors(event, &participants)); }
                }
                let failed = completed.payload.is_err();
                if failed { counters.failures += 1; }
                let identity_context = match &completed.payload {
                    Ok(Payload::GameStats(gs)) => Some(gs.clone()), _ => None,
                };
                let active_id = match &completed.payload {
                    Ok(Payload::ActivePlayer(ap)) => ap.riot_id.clone(), _ => None,
                };
                let observation = match completed.payload {
                    Ok(payload) => Observation::Success { evidence: e.clone(), payload },
                    Err(reason) => Observation::Gap { evidence: e.clone(), reason },
                };
                let mut rows = Vec::with_capacity(4);
                if let Some(reset) = &pending_reset { rows.push(reset.clone()); }
                rows.push(Record::Observation { observation: Box::new(observation) });
                if invalidate_calibration { rows.push(Record::CalibrationInvalidated { evidence: e.clone() }); }
                if let Some(value) = &new_calibration { rows.push(Record::Calibration { evidence: e.clone(), calibration: value.clone() }); }
                if let Some(staged) = &staged_participants
                    && staged.local() != participants.local()
                    && let Some(participant) = staged.local() {
                    rows.push(Record::LocalParticipant { evidence: e.clone(), participant });
                }
                let accepted = record_batch(sink, &mut slots[completed.index], &e, rows);
                if accepted {
                    pending_reset = None;
                    if let Some(staged) = staged_participants { participants = staged; }
                    if endpoint == Endpoint::ActivePlayer && participants.local().is_none() { local_identity = active_id.clone(); }
                }
                if endpoint == Endpoint::GameStats {
                    if accepted && let Some(gs) = identity_context { identity_rounds.observe_gs(e.clone(), gs); }
                    if accepted && valid_gs { latest_gs = Some(completed.id); }
                    else { calibration_window.clear(); new_calibration = None; }
                }
                if endpoint == Endpoint::ActivePlayer {
                    if accepted { identity_rounds.observe_ap(e.clone(), active_id); }
                    else { identity_rounds.missed_attempt(); }
                }
                if let Some(value) = new_calibration && accepted {
                    calibration = Some((completed.id, value));
                }
                slots[completed.index].next = completed.ended + cadence(endpoint, calibration.is_some(), failed);
            }
            _ = async { match next { Some(next) => tokio::time::sleep_until(next).await, None => std::future::pending().await } } => {}
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    let now = Instant::now();
    if let Some(row) = pending_reset
        && sink.try_records(vec![row]).is_err()
    {
        summary.unreported_source_resets += 1;
    }
    if let Some(row) = pending_identity
        && sink.try_records(vec![row]).is_err()
    {
        summary.unreported_identity_rounds += 1;
    }
    for (index, slot) in slots.iter_mut().enumerate() {
        let endpoint = ENDPOINTS[index];
        if let Some(flight) = slot.in_flight.take() {
            let e = evidence(
                &flight.source,
                flight.id,
                endpoint,
                (flight.started, now),
                origin,
                wall_time(),
                flight.refs,
            );
            record(
                sink,
                slot,
                &e,
                Record::Observation {
                    observation: Box::new(Observation::Gap {
                        evidence: e.clone(),
                        reason: GapReason::Cancelled,
                    }),
                },
            );
        }
        if !slot.stopped {
            let e = evidence(
                &scope,
                ObservationId(sequence),
                endpoint,
                (now, now),
                origin,
                wall_time(),
                (latest_gs, calibration.as_ref().map(|c| c.0)),
            );
            sequence += 1;
            record(
                sink,
                slot,
                &e,
                Record::Boundary {
                    evidence: e.clone(),
                    active: false,
                },
            );
        }
        summary.unreported_gaps += slot.pending_gap.as_ref().map_or(0, |(_, _, n)| *n);
    }
    summary
}

fn cadence(endpoint: Endpoint, calibrated: bool, failed: bool) -> Duration {
    match endpoint {
        Endpoint::GameStats if !calibrated && failed => super::CALIBRATION_RETRY_INTERVAL,
        Endpoint::GameStats if !calibrated => super::CALIBRATION_PROBE_INTERVAL,
        Endpoint::GameStats | Endpoint::Events => Duration::from_secs(1),
        Endpoint::PlayerList => Duration::from_secs(2),
        Endpoint::ActivePlayer => Duration::from_secs(10),
        _ => Duration::from_secs(10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chronobreak_replay_time::MediaId;
    use serde_json::Value;
    use std::collections::BTreeMap;
    #[cfg(windows)]
    use std::io::BufReader;
    use std::sync::Mutex;
    #[cfg(windows)]
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    #[cfg(windows)]
    use super::super::persistence::{
        CHECKPOINT_FILE, JOURNAL_FILE, QUEUE_BYTES, QueueSink, WriterSession,
    };
    #[cfg(windows)]
    use crate::league_client::sidecar::DirectoryGuard;

    struct FixtureSource {
        origin: Instant,
        failures: BTreeMap<Endpoint, usize>,
        delays: BTreeMap<Endpoint, Duration>,
        clock: Vec<f64>,
        rosters: Vec<Value>,
        starts: Mutex<BTreeMap<Endpoint, Vec<Instant>>>,
    }
    impl FixtureSource {
        fn new() -> Self {
            Self {
                origin: Instant::now(),
                failures: BTreeMap::new(),
                delays: BTreeMap::new(),
                clock: Vec::new(),
                rosters: Vec::new(),
                starts: Mutex::new(BTreeMap::new()),
            }
        }
    }
    impl Source for FixtureSource {
        async fn fetch(&self, endpoint: Endpoint) -> Result<Vec<u8>, GapReason> {
            let count = {
                let mut starts = self.starts.lock().unwrap();
                let row = starts.entry(endpoint).or_default();
                row.push(Instant::now());
                row.len()
            };
            if let Some(delay) = self.delays.get(&endpoint) {
                tokio::time::sleep(*delay).await;
            }
            if count <= *self.failures.get(&endpoint).unwrap_or(&0) {
                return Err(GapReason::Transport);
            }
            let mut fixture: Value = serde_json::from_str(include_str!(
                "../../../league-data/tests/fixtures/live-endpoints.json"
            ))
            .unwrap();
            fixture["gs"]["gameTime"] = serde_json::json!(
                self.clock
                    .get(count - 1)
                    .copied()
                    .unwrap_or(100.0 + Instant::now().duration_since(self.origin).as_secs_f64())
            );
            if endpoint == Endpoint::PlayerList
                && let Some(roster) = self.rosters.get(count - 1)
            {
                fixture["pl"] = roster.clone();
            }
            let key = match endpoint {
                Endpoint::GameStats => "gs",
                Endpoint::ActivePlayer => "ap",
                Endpoint::PlayerList => "pl",
                Endpoint::Events => "ev",
                _ => unreachable!(),
            };
            Ok(serde_json::to_vec(&fixture[key]).unwrap())
        }
    }
    #[derive(Default)]
    struct FixtureSink {
        rows: Vec<Record>,
        closed_until: Option<Instant>,
        reject_rounds: bool,
        reject_calibration_once: bool,
        reject_first: Option<Endpoint>,
    }
    impl Sink for FixtureSink {
        fn try_records(&mut self, records: Vec<Record>) -> Result<(), Vec<Record>> {
            if self
                .closed_until
                .is_some_and(|until| Instant::now() < until)
            {
                return Err(records);
            }
            if self.reject_rounds
                && records
                    .iter()
                    .any(|r| matches!(r, Record::IdentityRound { .. }))
            {
                return Err(records);
            }
            if self.reject_calibration_once
                && records
                    .iter()
                    .any(|r| matches!(r, Record::Calibration { .. }))
            {
                self.reject_calibration_once = false;
                return Err(records);
            }
            if self.reject_first.is_some_and(|endpoint| records.iter().any(|r| matches!(r, Record::Observation { observation } if matches!(observation.as_ref(), Observation::Success { evidence, .. } if evidence.endpoint == endpoint)))) {
                self.reject_first = None;
                return Err(records);
            }
            self.rows.extend(records);
            Ok(())
        }
    }
    fn scope() -> SourceEpoch {
        SourceEpoch {
            media_id: MediaId::parse("10000000-0000-4000-8000-000000000001").unwrap(),
            run_id: MediaId::parse("10000000-0000-4000-8000-000000000002").unwrap(),
            epoch: 0,
        }
    }
    async fn exercise<T: Sink + Send + 'static>(
        source: Arc<FixtureSource>,
        mut sink: T,
        duration: Duration,
        close_identity: Option<Duration>,
    ) -> (T, Summary) {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (open_tx, open_rx) = watch::channel(true);
        let video_started = Instant::now();
        let task = tokio::spawn(async move {
            let summary = run(
                source,
                &mut sink,
                scope(),
                video_started,
                cancel_rx,
                open_rx,
            )
            .await;
            (sink, summary)
        });
        if let Some(after) = close_identity {
            tokio::time::sleep(after).await;
            open_tx.send(false).unwrap();
            tokio::time::sleep(duration - after).await;
        } else {
            tokio::time::sleep(duration).await;
        }
        cancel_tx.send(true).unwrap();
        task.await.unwrap()
    }
    fn observations(sink: &FixtureSink, endpoint: Endpoint) -> Vec<&Observation> {
        sink.rows
            .iter()
            .filter_map(|r| match r {
                Record::Observation { observation } => Some(observation.as_ref()),
                _ => None,
            })
            .filter(|r| match r {
                Observation::Success { evidence, .. } | Observation::Gap { evidence, .. } => {
                    evidence.endpoint == endpoint
                }
            })
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn ap_and_clock_failure_never_suppress_roster_or_events() {
        let mut source = FixtureSource::new();
        source.failures.insert(Endpoint::ActivePlayer, usize::MAX);
        source.failures.insert(Endpoint::GameStats, usize::MAX);
        let (sink, summary) = exercise(
            Arc::new(source),
            FixtureSink::default(),
            Duration::from_secs(12),
            None,
        )
        .await;
        assert!(summary.endpoints[&Endpoint::PlayerList].requests >= 6);
        assert!(summary.endpoints[&Endpoint::Events].requests >= 11);
        assert!(observations(&sink, Endpoint::PlayerList).iter().all(
            |o| matches!(o, Observation::Success { evidence, .. } if evidence.calibration.is_none())
        ));
        assert!(summary.maximum_in_flight <= 4);
    }

    #[tokio::test(start_paused = true)]
    async fn five_sample_calibration_switches_the_single_gs_owner_to_normal_cadence() {
        let source = Arc::new(FixtureSource::new());
        let (sink, _) = exercise(
            Arc::clone(&source),
            FixtureSink::default(),
            Duration::from_secs(5),
            None,
        )
        .await;
        let calibrations: Vec<_> = sink
            .rows
            .iter()
            .filter_map(|r| match r {
                Record::Calibration { calibration, .. } => Some(calibration),
                _ => None,
            })
            .collect();
        assert_eq!(calibrations.len(), 1);
        assert_eq!(calibrations[0].sample_count, 5);
        assert_eq!(
            calibrations[0].last_game_tick.unwrap().get()
                - calibrations[0].first_game_tick.unwrap().get(),
            1_000_000
        );
        let starts = source.starts.lock().unwrap();
        let gs = &starts[&Endpoint::GameStats];
        for pair in gs[..5].windows(2) {
            assert_eq!(pair[1] - pair[0], Duration::from_millis(250));
        }
        for pair in gs[4..].windows(2) {
            assert_eq!(pair[1] - pair[0], Duration::from_secs(1));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn failure_recovery_continues_past_three_errors_and_keeps_equal_observations() {
        let mut source = FixtureSource::new();
        source.failures.insert(Endpoint::Events, 4);
        let (sink, summary) = exercise(
            Arc::new(source),
            FixtureSink::default(),
            Duration::from_secs(8),
            None,
        )
        .await;
        assert_eq!(summary.endpoints[&Endpoint::Events].failures, 4);
        let ev = observations(&sink, Endpoint::Events);
        assert_eq!(
            ev.iter()
                .filter(|o| matches!(o, Observation::Gap { .. }))
                .count(),
            4
        );
        assert!(
            ev.iter()
                .filter(|o| matches!(o, Observation::Success { .. }))
                .count()
                >= 3
        );
        let pl = observations(&sink, Endpoint::PlayerList);
        let [
            Observation::Success {
                evidence: a,
                payload: ap,
            },
            Observation::Success {
                evidence: b,
                payload: bp,
            },
            ..,
        ] = pl.as_slice()
        else {
            panic!()
        };
        assert_eq!(ap, bp);
        assert_ne!(a.id, b.id);
        assert_ne!(a.timing, b.timing);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_requests_have_independent_timing_no_overlap_and_no_catch_up_burst() {
        let mut source = FixtureSource::new();
        source
            .delays
            .insert(Endpoint::Events, Duration::from_millis(1500));
        let source = Arc::new(source);
        let (sink, summary) = exercise(
            Arc::clone(&source),
            FixtureSink::default(),
            Duration::from_secs(9),
            None,
        )
        .await;
        let starts = source.starts.lock().unwrap();
        for pair in starts[&Endpoint::Events].windows(2) {
            assert!(pair[1] - pair[0] >= Duration::from_millis(2500));
        }
        assert!(summary.endpoints[&Endpoint::PlayerList].requests >= 4);
        for o in observations(&sink, Endpoint::Events) {
            if let Observation::Success { evidence, .. } = o {
                assert_eq!(evidence.timing.end_us - evidence.timing.start_us, 1_500_000);
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn closing_identity_admission_stops_ap_without_stopping_other_streams() {
        let source = Arc::new(FixtureSource::new());
        let (_, summary) = exercise(
            source,
            FixtureSink::default(),
            Duration::from_secs(25),
            Some(Duration::from_secs(11)),
        )
        .await;
        assert_eq!(summary.endpoints[&Endpoint::ActivePlayer].requests, 2);
        assert!(summary.endpoints[&Endpoint::Events].requests >= 24);
    }

    #[tokio::test(start_paused = true)]
    async fn saturated_sink_skips_requests_and_retains_bounded_gap_evidence() {
        let source = Arc::new(FixtureSource::new());
        let sink = FixtureSink {
            closed_until: Some(Instant::now() + Duration::from_secs(5)),
            ..Default::default()
        };
        let (sink, summary) = exercise(source, sink, Duration::from_secs(12), None).await;
        assert!(summary.endpoints[&Endpoint::Events].skipped >= 5);
        assert!(sink.rows.iter().any(|r| matches!(r, Record::GapInterval { attempts, reason: GapReason::Backpressure, .. } if *attempts >= 5)));
        assert_eq!(summary.unreported_gaps, 0);
        assert!(
            observations(&sink, Endpoint::PlayerList)
                .iter()
                .any(|o| matches!(o, Observation::Success { .. }))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_and_cancellation_are_accounted_for_and_tasks_join() {
        let mut source = FixtureSource::new();
        source
            .delays
            .insert(Endpoint::Events, Duration::from_secs(60));
        let (sink, _) = exercise(
            Arc::new(source),
            FixtureSink::default(),
            Duration::from_millis(4500),
            None,
        )
        .await;
        let events = observations(&sink, Endpoint::Events);
        assert!(events.iter().any(|o| matches!(
            o,
            Observation::Gap {
                reason: GapReason::Timeout,
                ..
            }
        )));
        assert!(events.iter().any(|o| matches!(
            o,
            Observation::Gap {
                reason: GapReason::Cancelled,
                ..
            }
        )));
    }

    #[tokio::test(start_paused = true)]
    async fn identity_records_use_distinct_fresh_ap_and_independent_gs_observations() {
        let (sink, _) = exercise(
            Arc::new(FixtureSource::new()),
            FixtureSink::default(),
            Duration::from_secs(21),
            None,
        )
        .await;
        let rounds: Vec<_> = sink
            .rows
            .iter()
            .filter_map(|row| match row {
                Record::IdentityRound { round } => Some(round),
                _ => None,
            })
            .collect();
        assert_eq!(rounds.len(), 3);
        for pair in rounds.windows(2) {
            assert_eq!(pair[1].sequence, pair[0].sequence + 1);
            assert_ne!(pair[0].active_player.id, pair[1].active_player.id);
            assert!(
                pair[1].active_player.timing.start_us - pair[0].active_player.timing.start_us
                    >= 10_000_000
            );
        }
        assert!(
            rounds
                .iter()
                .all(|r| r.riot_id.as_deref() == Some("Fixture#TEST") && r.map_number == Some(11))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn rejected_identity_rounds_never_create_source_gaps_or_skip_polls() {
        let (sink, summary) = exercise(
            Arc::new(FixtureSource::new()),
            FixtureSink {
                reject_rounds: true,
                ..Default::default()
            },
            Duration::from_secs(21),
            None,
        )
        .await;
        assert_eq!(summary.endpoints[&Endpoint::ActivePlayer].requests, 3);
        assert!(summary.endpoints.values().all(|c| c.skipped == 0));
        assert_eq!(summary.unreported_gaps, 0);
        assert_eq!(summary.unreported_identity_rounds, 1);
        assert!(
            !sink
                .rows
                .iter()
                .any(|r| matches!(r, Record::GapInterval { .. }))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn rejected_calibration_is_atomic_with_its_source_observation() {
        let (sink, _) = exercise(
            Arc::new(FixtureSource::new()),
            FixtureSink {
                reject_calibration_once: true,
                ..Default::default()
            },
            Duration::from_secs(5),
            None,
        )
        .await;
        let gs = observations(&sink, Endpoint::GameStats);
        assert!(!gs.iter().any(|r| matches!(r, Observation::Success { evidence, .. } if evidence.timing.start_us == 1_000_000)));
        assert!(sink.rows.iter().any(|r| matches!(r, Record::GapInterval { first, last, attempts: 1, .. } if first.id == last.id && first.timing.start_us == 1_000_000)));
        for row in &sink.rows {
            if let Record::Calibration {
                evidence: calibrated,
                ..
            } = row
            {
                assert!(gs.iter().any(|r| matches!(r, Observation::Success { evidence, .. } if evidence.id == calibrated.id)));
            }
        }
        assert!(
            sink.rows
                .iter()
                .any(|r| matches!(r, Record::Calibration { .. }))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn frozen_clock_does_not_reset_epoch_and_advancing_clock_recalibrates() {
        let mut source = FixtureSource::new();
        source.clock = vec![100.0; 5];
        let (sink, _) = exercise(
            Arc::new(source),
            FixtureSink::default(),
            Duration::from_secs(6),
            None,
        )
        .await;
        assert!(
            !sink
                .rows
                .iter()
                .any(|r| matches!(r, Record::SourceReset { .. }))
        );
        assert!(observations(&sink, Endpoint::GameStats).iter().all(
            |r| matches!(r, Observation::Success { evidence, .. } if evidence.source.epoch == 0)
        ));
        assert!(sink.rows.iter().any(|r| matches!(r, Record::Calibration { calibration, .. } if calibration.sample_count == 5)));
    }

    #[tokio::test(start_paused = true)]
    async fn backward_clock_records_one_new_epoch_and_rejects_old_in_flight_data() {
        let mut source = FixtureSource::new();
        source.clock = vec![200.0, 200.25];
        source
            .delays
            .insert(Endpoint::Events, Duration::from_millis(1500));
        let (sink, _) = exercise(
            Arc::new(source),
            FixtureSink::default(),
            Duration::from_secs(5),
            None,
        )
        .await;
        let resets: Vec<_> = sink
            .rows
            .iter()
            .filter_map(|r| match r {
                Record::SourceReset {
                    evidence,
                    previous_epoch,
                } => Some((evidence, previous_epoch)),
                _ => None,
            })
            .collect();
        assert_eq!(resets.len(), 1);
        assert_eq!(*resets[0].1, 0);
        assert_eq!(resets[0].0.source.epoch, 1);
        let mut ids = std::collections::BTreeSet::new();
        for endpoint in ENDPOINTS {
            for row in observations(&sink, endpoint) {
                let e = match row {
                    Observation::Success { evidence, .. } | Observation::Gap { evidence, .. } => {
                        evidence
                    }
                };
                assert!(ids.insert(e.id));
                assert_ne!(e.id, resets[0].0.id);
            }
        }
        assert!(observations(&sink, Endpoint::Events).iter().any(|r| matches!(r, Observation::Gap { evidence, reason: GapReason::SourceReset } if evidence.source.epoch == 0)));
        assert!(observations(&sink, Endpoint::GameStats).iter().any(|r| matches!(r, Observation::Success { evidence, .. } if evidence.source.epoch == 1 && evidence.game_stats.is_none() && evidence.calibration.is_none())));
    }

    #[tokio::test(start_paused = true)]
    async fn saturated_reset_retains_each_epoch_boundary_before_more_acquisition() {
        struct ResetSink {
            until: Instant,
            rows: Vec<Record>,
        }
        impl Sink for ResetSink {
            fn try_records(&mut self, rows: Vec<Record>) -> Result<(), Vec<Record>> {
                if Instant::now() < self.until
                    && rows.iter().any(|r| matches!(r, Record::SourceReset { .. }))
                {
                    return Err(rows);
                }
                self.rows.extend(rows);
                Ok(())
            }
        }
        let mut source = FixtureSource::new();
        source.clock = vec![200.0, 199.0, 198.0, 197.0];
        let source = Arc::new(source);
        let until = Instant::now() + Duration::from_secs(2);
        let (sink, summary) = exercise(
            Arc::clone(&source),
            ResetSink {
                until,
                rows: Vec::new(),
            },
            Duration::from_secs(5),
            None,
        )
        .await;
        let starts = source.starts.lock().unwrap();
        assert!(starts[&Endpoint::GameStats][2] >= until);
        assert!(summary.endpoints[&Endpoint::GameStats].skipped > 0);
        assert_eq!(summary.unreported_source_resets, 0);
        let resets: Vec<_> = sink
            .rows
            .iter()
            .filter_map(|row| match row {
                Record::SourceReset {
                    evidence,
                    previous_epoch,
                } => Some((*previous_epoch, evidence.source.epoch)),
                _ => None,
            })
            .collect();
        assert!(resets.len() >= 3);
        for (index, (previous, next)) in resets.iter().enumerate() {
            assert_eq!(*previous, index as u64);
            assert_eq!(*next, index as u64 + 1);
        }
        let temp = tempfile::tempdir().unwrap();
        let writer = journal::Writer::create_new(
            &temp.path().join("journal.jsonl"),
            journal::Header::new(&scope()),
        )
        .unwrap();
        let mut writer = journal::RetainingWriter::new(writer);
        for row in sink.rows {
            writer.commit_acquired(&[row]).unwrap();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn rejected_ap_cannot_establish_a_local_participant() {
        let (sink, _) = exercise(
            Arc::new(FixtureSource::new()),
            FixtureSink {
                reject_first: Some(Endpoint::ActivePlayer),
                ..Default::default()
            },
            Duration::from_secs(6),
            None,
        )
        .await;
        assert!(
            !sink
                .rows
                .iter()
                .any(|r| matches!(r, Record::LocalParticipant { .. }))
        );
        assert!(!observations(&sink, Endpoint::PlayerList).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn rejected_roster_cannot_allocate_retained_participant_references() {
        let mut source = FixtureSource::new();
        source
            .rosters
            .push(serde_json::json!([{"riotId":"Rejected#TEST"}]));
        let (sink, _) = exercise(
            Arc::new(source),
            FixtureSink {
                reject_first: Some(Endpoint::PlayerList),
                ..Default::default()
            },
            Duration::from_secs(5),
            None,
        )
        .await;
        let mut fresh = identity::Participants::new(scope());
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../league-data/tests/fixtures/live-endpoints.json"
        ))
        .unwrap();
        let Payload::PlayerList(mut expected) = decode::live(
            Endpoint::PlayerList,
            &serde_json::to_vec(&fixture["pl"]).unwrap(),
        )
        .unwrap() else {
            panic!()
        };
        fresh.observe(&scope(), &mut expected, None).unwrap();
        for row in observations(&sink, Endpoint::PlayerList) {
            if let Observation::Success {
                payload: Payload::PlayerList(roster),
                ..
            } = row
            {
                assert_eq!(roster, &expected);
            }
        }
    }

    #[derive(Default)]
    struct FixtureDisk(Vec<u8>);
    impl std::io::Write for FixtureDisk {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl journal::DurableWrite for FixtureDisk {
        fn sync_data(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    struct JournalFixtureSink(journal::RetainingWriter<FixtureDisk>);
    impl Sink for JournalFixtureSink {
        fn try_records(&mut self, rows: Vec<Record>) -> Result<(), Vec<Record>> {
            self.0.commit_acquired(&rows).map(|_| ()).map_err(|_| rows)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn fixture_journal_connects_acquisition_to_committed_event_revisions_and_capacity_gaps() {
        for capacity in [0, MAX_EVENTS] {
            let writer =
                journal::Writer::new(FixtureDisk::default(), journal::Header::new(&scope()))
                    .unwrap();
            let sink = JournalFixtureSink(journal::RetainingWriter::with_event_capacity(
                writer, capacity,
            ));
            let (sink, summary) = exercise(
                Arc::new(FixtureSource::new()),
                sink,
                Duration::from_secs(8),
                None,
            )
            .await;
            assert_eq!(summary.unreported_gaps, 0);
            let disk = sink.0.into_inner();
            let mut rows = Vec::new();
            let recovery = journal::recover(std::io::Cursor::new(&disk.0), |batch, _| {
                rows.extend(batch.iter().map(|entry| entry.record.clone()));
                Ok(())
            })
            .unwrap();
            assert_eq!(recovery.tail_bytes, 0);
            let polls = summary.endpoints[&Endpoint::Events].requests;
            if capacity == 0 {
                assert_eq!(rows.iter().filter(|r| matches!(r, Record::Observation { observation } if matches!(observation.as_ref(), Observation::Gap { evidence, reason: GapReason::Capacity } if evidence.endpoint == Endpoint::Events))).count() as u64, polls);
            } else {
                let events: Vec<_> = rows
                    .iter()
                    .filter_map(|r| match r {
                        Record::EventObservation { revisions, .. } => Some(revisions),
                        _ => None,
                    })
                    .collect();
                assert_eq!(events.len() as u64, polls);
                assert!(events[0].iter().any(|r| r.event.source_id.is_some()));
                assert!(
                    events[1..]
                        .iter()
                        .all(|revisions| revisions.iter().all(|r| r.event.source_id.is_none()))
                );
            }
        }
    }

    fn ten_player_bodies() -> BTreeMap<Endpoint, Vec<u8>> {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../league-data/tests/fixtures/live-endpoints.json"
        ))
        .unwrap();
        let roster: Vec<_> = (0..10)
            .map(|index| {
                let mut player = fixture["pl"][0].clone();
                player["riotId"] = serde_json::json!(format!("Player{index}#TEST"));
                player["riotIdGameName"] = serde_json::json!(format!("Player{index}"));
                player["summonerName"] = serde_json::json!(format!("Player{index}"));
                player
            })
            .collect();
        BTreeMap::from([
            (
                Endpoint::GameStats,
                serde_json::to_vec(&fixture["gs"]).unwrap(),
            ),
            (
                Endpoint::ActivePlayer,
                serde_json::to_vec(&fixture["ap"]).unwrap(),
            ),
            (Endpoint::PlayerList, serde_json::to_vec(&roster).unwrap()),
            (
                Endpoint::Events,
                serde_json::to_vec(&fixture["ev"]).unwrap(),
            ),
        ])
    }

    #[test]
    #[ignore = "release-mode old/new decoder characterization; no network or League performance claim"]
    fn contract_decoder_schedule_profile() {
        let bodies = ten_player_bodies();
        for seconds in [600_u64, 3600, 7200] {
            for candidate in [false, true] {
                let mut results = Vec::new();
                for endpoint in ENDPOINTS {
                    let count = match (candidate, endpoint) {
                        (_, Endpoint::Events) | (true, Endpoint::GameStats) => seconds,
                        (true, Endpoint::PlayerList) => seconds / 2,
                        (true, Endpoint::ActivePlayer) => 0,
                        _ => seconds / 10,
                    };
                    let bytes = &bodies[&endpoint];
                    let start = std::time::Instant::now();
                    for _ in 0..count {
                        if candidate {
                            std::hint::black_box(decode::live(endpoint, bytes).unwrap());
                        } else {
                            match endpoint {
                                Endpoint::GameStats => {
                                    std::hint::black_box(
                                        serde_json::from_slice::<
                                            super::super::legacy_decode_baseline::RawGameData,
                                        >(bytes)
                                        .unwrap(),
                                    );
                                }
                                Endpoint::ActivePlayer => {
                                    std::hint::black_box(
                                        serde_json::from_slice::<
                                            super::super::legacy_decode_baseline::RawActivePlayer,
                                        >(bytes)
                                        .unwrap(),
                                    );
                                }
                                Endpoint::PlayerList => {
                                    std::hint::black_box(
                                        serde_json::from_slice::<
                                            Vec<super::super::legacy_decode_baseline::RawPlayer>,
                                        >(bytes)
                                        .unwrap(),
                                    );
                                }
                                Endpoint::Events => {
                                    std::hint::black_box(
                                        serde_json::from_slice::<
                                            super::super::legacy_decode_baseline::RawEventData,
                                        >(bytes)
                                        .unwrap(),
                                    );
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                    results.push(serde_json::json!({"endpoint":endpoint,"requests":count,"bytes":count * bytes.len() as u64,"decode_wall_us":start.elapsed().as_micros()}));
                }
                println!(
                    "contract_decoder_profile {}",
                    serde_json::json!({"virtual_seconds":seconds,"candidate":candidate,"endpoints":results})
                );
            }
        }
    }

    struct SteadySource {
        origin: Instant,
        bodies: BTreeMap<Endpoint, Vec<u8>>,
    }
    impl Source for SteadySource {
        async fn fetch(&self, endpoint: Endpoint) -> Result<Vec<u8>, GapReason> {
            if endpoint == Endpoint::GameStats {
                Ok(serde_json::to_vec(&serde_json::json!({"gameTime":Instant::now().duration_since(self.origin).as_secs_f64(),"gameMode":"CLASSIC","mapNumber":11})).unwrap())
            } else {
                Ok(self.bodies[&endpoint].clone())
            }
        }
    }
    #[derive(Default)]
    struct CountingSink {
        records: u64,
        maximum_record_bytes: usize,
    }
    impl Sink for CountingSink {
        fn try_records(&mut self, records: Vec<Record>) -> Result<(), Vec<Record>> {
            for record in &records {
                self.records += 1;
                self.maximum_record_bytes = self
                    .maximum_record_bytes
                    .max(serde_json::to_vec(record).unwrap().len());
            }
            Ok(())
        }
    }

    #[cfg(windows)]
    struct QueueSinkAdapter(QueueSink);
    #[cfg(windows)]
    impl Sink for QueueSinkAdapter {
        fn try_records(&mut self, records: Vec<Record>) -> Result<(), Vec<Record>> {
            self.0.try_records(records)
        }
    }

    #[cfg(windows)]
    fn soak_seconds() -> u64 {
        const VARIABLE: &str = "QUEUEBACK_TEST_LEAGUE_SOAK_SECONDS";
        let seconds = match std::env::var(VARIABLE) {
            Ok(value) => value
                .parse::<u64>()
                .unwrap_or_else(|_| panic!("{VARIABLE} must be an integer number of seconds")),
            Err(std::env::VarError::NotPresent) => 7_200,
            Err(error) => panic!("{VARIABLE} could not be read: {error}"),
        };
        assert!(
            (75..=86_400).contains(&seconds),
            "{VARIABLE} must be 75..=86400 seconds"
        );
        seconds
    }

    #[cfg(windows)]
    fn sample_process_resources(system: &mut System, pid: Pid) -> (u64, u64, u64) {
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            false,
            ProcessRefreshKind::nothing().with_memory().with_cpu(),
        );
        let process = system
            .process(pid)
            .expect("current process must be available");
        // On Windows sysinfo exposes PROCESS_MEMORY_COUNTERS_EX::PrivateUsage as
        // virtual_memory, and WorkingSetSize as memory.
        (
            process.virtual_memory(),
            process.memory(),
            process.accumulated_cpu_time(),
        )
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "release-mode real-time scheduler-to-journal resource profile; fake Live bodies, not League or capture performance"]
    async fn contract_realtime_acquisition_persistence_resource_profile() {
        // This intentionally uses wall time so WriterSession's production 2 s
        // flush and 60 s checkpoint schedules are exercised. A 75 s rehearsal
        // crosses the checkpoint interval but does not establish the 2 h target.
        let seconds = soak_seconds();
        let origin = Instant::now();
        let source = Arc::new(SteadySource {
            origin,
            bodies: ten_player_bodies(),
        });
        let mut resources = System::new();
        let pid = Pid::from_u32(std::process::id());
        // Warm process discovery and source fixture allocation before baseline.
        let _ = sample_process_resources(&mut resources, pid);
        let (baseline_private, baseline_resident, baseline_cpu_ms) =
            sample_process_resources(&mut resources, pid);

        // The window begins before WriterSession startup and ends after its
        // graceful drain; readback below remains outside it.
        let resource_started = Instant::now();
        let temp = tempfile::tempdir().unwrap();
        let directory = DirectoryGuard::capture(temp.path(), scope().media_id.clone()).unwrap();
        let (writer, queue) = WriterSession::start(directory, scope()).await.unwrap();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        // Resource measurement characterizes the candidate steady state after
        // association admission closes, so AP is intentionally already closed.
        let (_identity_tx, identity_rx) = watch::channel(false);
        let acquisition = tokio::spawn(async move {
            let mut sink = QueueSinkAdapter(queue);
            let summary = run(source, &mut sink, scope(), origin, cancel_rx, identity_rx).await;
            (sink, summary)
        });

        let mut samples = 0_u64;
        let mut peak_private = baseline_private;
        let mut peak_resident = baseline_resident;
        while resource_started.elapsed() < Duration::from_secs(seconds) {
            let (private, resident, _) = sample_process_resources(&mut resources, pid);
            samples = samples.saturating_add(1);
            peak_private = peak_private.max(private);
            peak_resident = peak_resident.max(resident);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        cancel_tx.send(true).unwrap();
        let (_, summary) = acquisition.await.unwrap();
        let report = writer.stop().await;
        let elapsed = resource_started.elapsed();
        let (final_private, final_resident, final_cpu_ms) =
            sample_process_resources(&mut resources, pid);
        samples = samples.saturating_add(1);
        peak_private = peak_private.max(final_private);
        peak_resident = peak_resident.max(final_resident);

        assert!(!report.failed, "{:?}", report.failure);
        assert_eq!(report.checkpoint_failures, 0);
        assert_eq!(report.manifest_failures, 0);
        assert_eq!(
            report.rejected_batches, 0,
            "fixture queue admission rejected"
        );
        assert!(report.queue_high_water_bytes <= QUEUE_BYTES);
        assert_eq!(summary.unreported_gaps, 0);
        assert_eq!(summary.unreported_source_resets, 0);
        assert_eq!(summary.unreported_identity_rounds, 0);
        assert!(summary.maximum_in_flight <= 3);
        for (endpoint, counters) in &summary.endpoints {
            assert_eq!(counters.failures, 0, "{endpoint:?} fixture request failed");
            assert_eq!(counters.skipped, 0, "{endpoint:?} fixture request skipped");
        }
        assert!(!summary.endpoints.contains_key(&Endpoint::ActivePlayer));
        assert!(
            summary.endpoints[&Endpoint::GameStats].requests >= seconds.saturating_mul(4) / 5,
            "host stall prevented enough GameStats cadence samples"
        );
        assert!(
            summary.endpoints[&Endpoint::Events].requests >= seconds.saturating_mul(4) / 5,
            "host stall prevented enough Events cadence samples"
        );
        assert!(
            summary.endpoints[&Endpoint::PlayerList].requests >= seconds.saturating_mul(2) / 5,
            "host stall prevented enough PlayerList cadence samples"
        );
        assert!(report.checkpoint_writes >= 2);

        // Readback is deliberately after the resource window. It verifies only
        // the durable prefix produced by the measured scheduler/worker run.
        let mut recovered_records = 0_u64;
        let recovery = journal::recover(
            BufReader::new(std::fs::File::open(temp.path().join(JOURNAL_FILE)).unwrap()),
            |entries, _| {
                recovered_records = recovered_records.saturating_add(entries.len() as u64);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(recovery.tail_bytes, 0);
        assert_eq!(recovered_records, report.committed_records);

        let journal_bytes = std::fs::metadata(temp.path().join(JOURNAL_FILE))
            .unwrap()
            .len();
        assert_eq!(journal_bytes, report.journal_bytes);
        let checkpoint_bytes = std::fs::metadata(temp.path().join(CHECKPOINT_FILE))
            .unwrap()
            .len();
        let manifest_bytes = std::fs::metadata(temp.path().join("game_log.json"))
            .unwrap()
            .len();
        let cpu_delta_ms = final_cpu_ms
            .checked_sub(baseline_cpu_ms)
            .expect("process CPU time moved backwards during resource profile");
        let one_core_percent = if elapsed.is_zero() {
            0.0
        } else {
            cpu_delta_ms as f64 * 100.0 / elapsed.as_millis() as f64
        };
        println!(
            "contract_realtime_acquisition_persistence_resource {}",
            serde_json::json!({
                "measurement": "real-time fixture scheduler through production WriterSession; current-process sampled resource usage",
                "limitations": [
                    "fake transport with static ten-player PlayerList and Events bodies; excludes HTTP, TLS, LCU, capture, and League frame impact",
                    "the static Events corpus does not exercise growing event-history cardinality",
                    "a duration below 7200 seconds is a cadence rehearsal, not two-hour private-memory acceptance",
                    "whole-test-process CPU includes fixture source generation and resource sampling overhead",
                    "one-second sampled private-memory peak is not an exact maximum"
                ],
                "configured_seconds": seconds,
                "elapsed_wall_ms": elapsed.as_millis(),
                "resource_samples": samples,
                "private_bytes": {"baseline":baseline_private,"peak":peak_private,"final":final_private,"incremental_peak":peak_private.saturating_sub(baseline_private)},
                "resident_bytes": {"baseline":baseline_resident,"peak":peak_resident,"final":final_resident,"incremental_peak":peak_resident.saturating_sub(baseline_resident)},
                "cpu_user_kernel_ms_delta": cpu_delta_ms,
                "cpu_one_logical_core_percent": one_core_percent,
                "acquisition": summary,
                "logical_write_bytes": {
                    "journal_append": report.journal_bytes,
                    "manifest_replacements": report.manifest_bytes,
                    "checkpoint_replacements": report.checkpoint_bytes,
                    "total": report.journal_bytes.saturating_add(report.manifest_bytes).saturating_add(report.checkpoint_bytes)
                },
                "writer": report,
                "final_file_bytes": {"journal":journal_bytes,"checkpoint":checkpoint_bytes,"manifest":manifest_bytes,"total":journal_bytes.saturating_add(checkpoint_bytes).saturating_add(manifest_bytes)},
                "recovered_records": recovered_records
            })
        );
    }

    #[tokio::test(start_paused = true)]
    #[ignore = "release-mode 24-hour virtual scheduler; no long-run RSS claim"]
    async fn contract_virtual_day_profile() {
        let origin = Instant::now();
        let source = Arc::new(SteadySource {
            origin,
            bodies: ten_player_bodies(),
        });
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (_identity_tx, identity_rx) = watch::channel(false);
        let start = std::time::Instant::now();
        let task = tokio::spawn(async move {
            let mut sink = CountingSink::default();
            let summary = run(source, &mut sink, scope(), origin, cancel_rx, identity_rx).await;
            (sink, summary)
        });
        tokio::time::sleep(Duration::from_secs(86400)).await;
        cancel_tx.send(true).unwrap();
        let (sink, summary) = task.await.unwrap();
        assert!(summary.maximum_in_flight <= 3);
        assert_eq!(summary.unreported_gaps, 0);
        assert_eq!(summary.endpoints[&Endpoint::Events].requests, 86400);
        assert_eq!(summary.endpoints[&Endpoint::PlayerList].requests, 43200);
        assert!(!summary.endpoints.contains_key(&Endpoint::ActivePlayer));
        println!(
            "contract_virtual_day {}",
            serde_json::json!({"summary":summary,"records":sink.records,"maximum_record_bytes":sink.maximum_record_bytes,"wall_us":start.elapsed().as_micros()})
        );
    }
}
