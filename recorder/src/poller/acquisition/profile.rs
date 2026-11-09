use super::*;
use crate::league_client::sidecar::DirectoryGuard;
use crate::poller::persistence::{
    CHECKPOINT_INTERVAL, FLUSH_BYTES, FLUSH_INTERVAL, JOURNAL_FILE, QUEUE_BYTES, queue_charge,
};
use serde_json::{Value, json};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant as WallInstant;
use windows::Win32::{
    Foundation::FILETIME,
    System::Threading::{GetCurrentProcess, GetCurrentThread, GetProcessTimes},
};

pub(super) use chronobreak_league_data::observation::profile::enter;

type Stage = (&'static str, Option<Endpoint>);
#[derive(Clone, Copy, Default)]
struct Allocations {
    calls: u64,
    bytes: u64,
}

thread_local! {
    static ALLOC: Cell<Option<Allocations>> = const { Cell::new(None) };
    static ACTIVE: Cell<Option<(Stage, SampleStart)>> = const { Cell::new(None) };
    static SAMPLES: RefCell<Vec<(Stage, Sample)>> = const { RefCell::new(Vec::new()) };
}

struct CountingAllocator;
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocated(bytes: usize) {
    let _ = ALLOC.try_with(|counter| {
        if let Some(mut total) = counter.get() {
            total.calls += 1;
            total.bytes += bytes as u64;
            counter.set(Some(total));
        }
    });
}

// SAFETY: The system allocator owns every allocation. Accounting only observes
// successful allocations on the benchmark thread and never allocates itself.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forward the caller's valid allocation layout to System.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            allocated(layout.size());
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forward the caller's valid allocation layout to System.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            allocated(layout.size());
        }
        ptr
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: The caller supplies a System allocation and its original layout.
        let ptr = unsafe { System.realloc(ptr, layout, size) };
        if !ptr.is_null() {
            allocated(size);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies a System allocation and its original layout.
        unsafe { System.dealloc(ptr, layout) };
    }
}

fn process_cpu() -> u64 {
    let [mut created, mut exited, mut kernel, mut user] = [FILETIME::default(); 4];
    // SAFETY: The process pseudohandle and all four output pointers are valid.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .unwrap()
    };
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    ticks(kernel) + ticks(user)
}

fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn QueryThreadCycleTime(
            thread: windows::Win32::Foundation::HANDLE,
            cycles: *mut u64,
        ) -> i32;
    }
    let mut cycles = 0;
    // SAFETY: The thread pseudohandle and output pointer are valid.
    let success = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    assert_ne!(success, 0);
    cycles
}

#[derive(Clone, Copy)]
struct SampleStart {
    wall: WallInstant,
    cycles: u64,
    alloc: Allocations,
}
struct Sample {
    ns: u64,
    cycles: u64,
    calls: u64,
    bytes: u64,
}

fn mark(stage: Option<Stage>) {
    if let Some(stage) = stage {
        let start = SampleStart {
            wall: WallInstant::now(),
            cycles: cycles(),
            alloc: ALLOC.get().unwrap(),
        };
        assert!(ACTIVE.replace(Some((stage, start))).is_none());
    } else {
        let (stage, start) = ACTIVE.take().unwrap();
        let total = ALLOC.get().unwrap();
        let sample = Sample {
            cycles: cycles() - start.cycles,
            ns: start.wall.elapsed().as_nanos() as u64,
            calls: total.calls - start.alloc.calls,
            bytes: total.bytes - start.alloc.bytes,
        };
        SAMPLES.with_borrow_mut(|samples| samples.push((stage, sample)));
    }
}

fn by_stage(samples: &[(Stage, Sample)]) -> BTreeMap<String, Value> {
    let mut groups = BTreeMap::<String, Vec<&Sample>>::new();
    for ((stage, endpoint), sample) in samples {
        let label = endpoint.map_or_else(|| (*stage).into(), |e| format!("{e:?}/{stage}"));
        groups.entry(label).or_default().push(sample);
    }
    groups.into_iter().map(|(label, samples)| {
        let dist = |get: fn(&Sample) -> u64| {
            let mut values = samples.iter().map(|s| get(s)).collect::<Vec<_>>();
            values.sort_unstable();
            let percentile = |p: usize| values[(values.len() * p).div_ceil(100) - 1];
            json!({"sum":values.iter().sum::<u64>(),"p95":percentile(95),"p99":percentile(99)})
        };
        (label, json!({"samples":samples.len(), "wall_ns":dist(|s|s.ns), "thread_cycles":dist(|s|s.cycles),
            "allocation_calls":samples.iter().map(|s|s.calls).sum::<u64>(),
            "allocation_bytes":samples.iter().map(|s|s.bytes).sum::<u64>()}))
    }).collect()
}

struct MatchFixture {
    origin: Instant,
    events: Vec<Vec<u8>>,
    roster: Vec<Vec<u8>>,
    stats: Vec<Vec<u8>>,
}

impl MatchFixture {
    fn new(origin: Instant) -> Self {
        let mut roster: Value = serde_json::from_slice(include_bytes!(
            "../../../../league-data/benches/fixtures/live-player-list-midgame.json"
        ))
        .unwrap();
        let mut events = vec![json!({"EventID":0,"EventName":"GameStart","EventTime":0.016})];
        let mut event_bodies = Vec::new();
        let mut roster_bodies = Vec::new();
        for second in 0..3600 {
            if second == 30 {
                events.push(json!({"EventID":1,"EventName":"MinionsSpawning","EventTime":30.029}));
            }
            if second >= 90 && second % 11 == 2 {
                let id = events.len();
                let time = second as f64 - 0.137;
                let player = |index: usize| format!("Player{:02}#TEST", index % 10 + 1);
                let actor = player(id);
                let assisters = [player(id + 2), player(id + 4)];
                let mut event = match id % 16 {
                    0 => {
                        json!({"EventName":"BaronKill","KillerName":actor,"Assisters":assisters,"Stolen":false})
                    }
                    4 => {
                        json!({"EventName":"DragonKill","KillerName":actor,"Assisters":assisters,"DragonType":"Fire","Stolen":false})
                    }
                    8 | 12 => {
                        json!({"EventName":"TurretKilled","KillerName":actor,"Assisters":assisters,"TurretKilled":"Turret_T2_C_03_A"})
                    }
                    _ => {
                        json!({"EventName":"ChampionKill","KillerName":actor,"VictimName":player(id + 5),"Assisters":assisters})
                    }
                };
                event["EventID"] = json!(id);
                event["EventTime"] = json!(time);
                events.push(event);
                if id == 2 {
                    events.push(json!({"EventID":3,"EventName":"FirstBlood","EventTime":time,"Recipient":actor}));
                }
            }
            if second == 1800 || second == 1830 {
                events[2]["EventTime"] = json!(if second == 1800 { 89.875 } else { 89.863 });
            }
            if second == 3599 {
                events.push(json!({"EventID":events.len(),"EventName":"GameEnd","EventTime":3598.95,"Result":"Win"}));
            }
            event_bodies.push(serde_json::to_vec(&json!({"Events":&events})).unwrap());
            if second % 2 == 0 {
                let player = second / 2 % 10;
                roster[player]["scores"]["creepScore"] = json!(second / 12 + player);
                roster_bodies.push(serde_json::to_vec(&roster).unwrap());
            }
        }
        let stats = (0..3600 * 4)
            .map(|tick| {
                serde_json::to_vec(&json!({
                    "gameTime":tick as f64 / 4.0 + 1.0, "gameMode":"CLASSIC", "mapNumber":11,
                    "mapName":"Map11", "mapTerrain":"Default"
                }))
                .unwrap()
            })
            .collect();
        Self {
            origin,
            events: event_bodies,
            roster: roster_bodies,
            stats,
        }
    }
}

impl Source for MatchFixture {
    async fn fetch(&self, endpoint: Endpoint) -> Result<Vec<u8>, GapReason> {
        let age = Instant::now().duration_since(self.origin);
        let second = age.as_secs() as usize;
        let _profile = enter("fixture_body_copy", Some(endpoint));
        Ok(match endpoint {
            Endpoint::Events => self.events[second].clone(),
            Endpoint::PlayerList => self.roster[second / 2].clone(),
            Endpoint::GameStats => self.stats[(age.as_millis() / 250) as usize].clone(),
            Endpoint::ActivePlayer => br#"{"riotId":"Player01#TEST","currentGold":500}"#.to_vec(),
            _ => unreachable!(),
        })
    }
}

// Virtual time drives batching on this thread. Production journal and
// checkpoint code still validates, encodes, syncs and publishes real files.
struct SimulatedWriter {
    writer: journal::RetainingWriter<std::fs::File>,
    state: checkpoint::State,
    directory: DirectoryGuard,
    pending: Vec<Record>,
    charged: usize,
    first: Instant,
    last_checkpoint: Instant,
    commits: usize,
}

impl SimulatedWriter {
    fn start(directory: DirectoryGuard, scope: &SourceEpoch, origin: Instant) -> Self {
        let _profile = enter("initialize", None);
        let header = journal::Header::new(scope);
        let writer = journal::Writer::new(
            directory.create_journal(&scope.media_id).unwrap(),
            header.clone(),
        )
        .unwrap();
        let state = checkpoint::State::new(header, writer.position().clone());
        let manifest = manifest::Manifest::from_checkpoint(state.snapshot())
            .encode()
            .unwrap();
        directory.replace_telemetry(true, &manifest).unwrap();
        Self {
            writer: journal::RetainingWriter::new(writer),
            state,
            directory,
            pending: Vec::new(),
            charged: 0,
            first: origin,
            last_checkpoint: origin,
            commits: 0,
        }
    }
    fn checkpoint(&mut self, now: Instant) {
        let _profile = enter("checkpoint_and_manifest", None);
        let checkpoint = self.state.snapshot().encode().unwrap();
        self.directory
            .replace_telemetry(false, &checkpoint)
            .unwrap();
        let manifest = manifest::Manifest::from_checkpoint(self.state.snapshot())
            .encode()
            .unwrap();
        self.directory.replace_telemetry(true, &manifest).unwrap();
        self.last_checkpoint = now;
    }
    fn flush(&mut self, now: Instant) {
        if self.pending.is_empty() {
            return;
        }
        self.writer
            .commit_owned_with(std::mem::take(&mut self.pending), |records, position| {
                let _profile = enter("checkpoint_projection", None);
                self.state.apply(records, position).unwrap();
            })
            .unwrap();
        self.charged = 0;
        self.commits += 1;
        if now - self.last_checkpoint >= CHECKPOINT_INTERVAL {
            self.checkpoint(now);
        }
    }
}

impl Sink for SimulatedWriter {
    fn try_records(&mut self, mut records: Vec<Record>) -> Result<(), Vec<Record>> {
        let now = Instant::now();
        let profile = enter(
            "queue_charge",
            records.iter().find_map(|row| match row {
                Record::Observation { observation } => match observation.as_ref() {
                    Observation::Success { evidence, .. } | Observation::Gap { evidence, .. } => {
                        Some(evidence.endpoint)
                    }
                },
                _ => None,
            }),
        );
        let charge = queue_charge(&records);
        drop(profile);
        assert!(charge < QUEUE_BYTES - 64 * 1024);
        if now - self.first >= FLUSH_INTERVAL
            || self.pending.len() + records.len() > journal::MAX_BATCH_RECORDS
        {
            self.flush(now);
        }
        if self.pending.is_empty() {
            self.first = now;
        }
        self.pending.append(&mut records);
        self.charged += charge;
        if self.charged >= FLUSH_BYTES {
            self.flush(now);
        }
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
#[ignore = "accelerated one-hour acquisition profile with generated API bodies and durable files"]
async fn contract_growing_match_profile() {
    let origin = Instant::now();
    let scope = super::tests::scope();
    let fixture = Arc::new(MatchFixture::new(origin));
    let temp = tempfile::tempdir().unwrap();
    let directory = DirectoryGuard::capture(temp.path(), scope.media_id.clone()).unwrap();
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (identity_tx, identity_rx) = watch::channel(true);
    let mut minutes = Vec::with_capacity(60);
    SAMPLES.with_borrow_mut(|samples| samples.reserve(120_000));
    ALLOC.set(Some(Allocations::default()));
    chronobreak_league_data::observation::profile::install(Some(mark));
    let cpu_start = process_cpu();
    let mut sink = SimulatedWriter::start(directory, &scope, origin);
    let acquisition = tokio::spawn(async move {
        let summary = run(fixture, &mut sink, scope, origin, cancel_rx, identity_rx).await;
        sink.flush(Instant::now());
        sink.checkpoint(Instant::now());
        (sink, summary)
    });
    tokio::time::sleep_until(origin + Duration::from_millis(30_500)).await;
    identity_tx.send(false).unwrap();
    let mut previous_cpu = cpu_start;
    let mut previous_sample = 0;
    for minute in 1..=60 {
        tokio::time::sleep_until(origin + Duration::from_millis(minute * 60_000 - 125)).await;
        let cpu = process_cpu();
        let end = SAMPLES.with_borrow(Vec::len);
        minutes.push((minute, cpu - previous_cpu, previous_sample..end));
        previous_cpu = cpu;
        previous_sample = end;
    }
    cancel_tx.send(true).unwrap();
    let (sink, summary) = acquisition.await.unwrap();
    let cpu_total = process_cpu() - cpu_start;
    let alloc = ALLOC.take().unwrap();
    chronobreak_league_data::observation::profile::install(None);
    let samples = SAMPLES.take();
    for (minute, cpu, range) in minutes {
        if [5, 15, 30, 45, 60].contains(&minute) {
            println!(
                "match_minute {}",
                json!({"minute":minute,"cpu_ms":cpu as f64/10000.0,"stages":by_stage(&samples[range])})
            );
        }
    }
    for counts in summary.endpoints.values() {
        assert_eq!(counts.skipped + counts.failures, 0);
    }
    assert_eq!(summary.endpoints[&Endpoint::Events].requests, 3600);
    assert_eq!(summary.endpoints[&Endpoint::PlayerList].requests, 1800);
    assert_eq!(summary.endpoints[&Endpoint::ActivePlayer].requests, 4);
    assert_eq!(
        summary.unreported_gaps
            + summary.unreported_identity_rounds
            + summary.unreported_source_resets,
        0
    );
    let mut event_observations = 0;
    let mut event_revisions = 0;
    let mut hash = DefaultHasher::new();
    let recovery = journal::recover(
        std::io::BufReader::new(std::fs::File::open(temp.path().join(JOURNAL_FILE)).unwrap()),
        |entries, _| {
            for entry in entries {
                if let Record::EventObservation { revisions, .. } = &entry.record {
                    event_observations += 1;
                    event_revisions += revisions.len();
                    let mut row = serde_json::to_value(&entry.record).unwrap();
                    row.pointer_mut("/evidence/timing")
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .remove("received_at");
                    row.hash(&mut hash);
                }
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(recovery.tail_bytes, 0);
    assert_eq!(&recovery.position, sink.writer.position());
    assert_eq!(event_observations, 3600);
    assert_eq!(event_revisions, 326);
    println!(
        "match_total {}",
        json!({
            "cpu_ms":cpu_total as f64/10000.0,"allocation_calls":alloc.calls,"allocation_bytes":alloc.bytes,
            "commits":sink.commits,"records":recovery.position.last_sequence,
            "journal_bytes":recovery.position.end_offset,"event_observations":event_observations,
            "event_revisions":event_revisions,"event_digest":format!("{:016x}",hash.finish()),
            "acquisition":summary,"stages":by_stage(&samples)
        })
    );
}
