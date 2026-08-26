use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};

const SCHEMA_VERSION: u32 = 1;
const SENTINEL_DIRECTORY: &str = ".chronobreak-replay-benchmark";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const EVENT_QUEUE_CAPACITY: usize = 256;
const MAX_EVENT_BATCH: usize = 128;
const MAX_EVENT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObserverProfile {
    Minimal,
    Full,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DdragonCondition {
    pub mode: String,
    pub cache_root: PathBuf,
    pub cache_fingerprint: String,
    #[serde(flatten)]
    pub _extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BenchmarkFixture {
    pub id: String,
    pub alias: String,
    pub game_timestamp: String,
    #[serde(flatten)]
    pub _extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BenchmarkManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub sentinel_root: PathBuf,
    pub library_root: PathBuf,
    pub config_path: PathBuf,
    pub app_data_root: PathBuf,
    pub result_root: PathBuf,
    pub scratch_root: PathBuf,
    pub observer_profile: ObserverProfile,
    pub ddragon: DdragonCondition,
    #[serde(default)]
    pub fixtures: Vec<BenchmarkFixture>,
    #[serde(default)]
    pub scenarios: Vec<Value>,
    #[serde(flatten)]
    pub _extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct BenchmarkLaunch {
    manifest_path: PathBuf,
    manifest: BenchmarkManifest,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkSessionInfo {
    pub schema_version: u32,
    pub run_id: String,
    pub observer_profile: ObserverProfile,
    pub harness_initialization_ms: f64,
    pub session_elapsed_ms: f64,
    pub fixtures: Vec<BenchmarkFixture>,
    pub scenarios: Vec<Value>,
    pub ddragon_mode: String,
    pub ddragon_cache_fingerprint: String,
    pub event_queue: QueueStats,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BenchmarkEventInput {
    pub scenario_id: String,
    pub trial_id: String,
    pub monotonic_ms: f64,
    pub source: String,
    pub kind: String,
    #[serde(default)]
    pub generation: Option<u64>,
    #[serde(default)]
    pub action_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkEvent {
    schema_version: u32,
    run_id: String,
    scenario_id: String,
    trial_id: String,
    monotonic_ms: f64,
    source: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_id: Option<String>,
    payload: Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct QueueStats {
    pub capacity: u64,
    pub high_water_mark: u64,
    pub accepted_records: u64,
    pub dropped_records: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordEventsResult {
    pub accepted: bool,
    pub queue: QueueStats,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BenchmarkTerminalInput {
    pub scenario_id: String,
    pub trial_id: String,
    pub status: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkTerminal {
    schema_version: u32,
    run_id: String,
    scenario_id: String,
    trial_id: String,
    status: String,
    reason: Option<String>,
    elapsed_ms: f64,
    event_queue: QueueStats,
    payload: Value,
}

#[derive(Debug, Clone, Serialize)]
struct ObserverContext<'a> {
    schema_version: u32,
    run_id: &'a str,
    scenario_id: &'a str,
    trial_id: &'a str,
    active: bool,
}

#[derive(Debug, Clone, Copy)]
enum Artifact {
    Events,
    ServerRequests,
}

enum WriterMessage {
    Records {
        artifact: Artifact,
        lines: Vec<Vec<u8>>,
    },
    Flush(oneshot::Sender<Result<(), String>>),
    Terminal {
        bytes: Vec<u8>,
        reply: oneshot::Sender<Result<(), String>>,
    },
}

pub struct BenchmarkSession {
    launch: BenchmarkLaunch,
    started_at: Instant,
    harness_initialization_ms: f64,
    sender: mpsc::Sender<WriterMessage>,
    accepted_records: AtomicU64,
    dropped_records: AtomicU64,
    high_water_mark: AtomicU64,
    terminal_written: AtomicBool,
}

impl BenchmarkLaunch {
    pub fn from_args<I>(args: I) -> Result<Option<Self>>
    where
        I: IntoIterator<Item = OsString>,
    {
        let args = args.into_iter().collect::<Vec<_>>();
        let flag = OsString::from("--replay-benchmark-manifest");
        let flag_positions = args
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| (argument == &flag).then_some(index))
            .collect::<Vec<_>>();
        if flag_positions.is_empty() {
            return Ok(None);
        }
        if flag_positions != [1] || args.len() != 3 {
            bail!(
                "benchmark mode requires the exact form: <executable> --replay-benchmark-manifest <absolute-manifest-path>"
            );
        }
        let manifest_path = PathBuf::from(&args[2]);
        Self::load(&manifest_path).map(Some)
    }

    pub fn from_process_args() -> Result<Option<Self>> {
        Self::from_args(std::env::args_os())
    }

    fn load(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            bail!("benchmark manifest path must be absolute");
        }
        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to inspect benchmark manifest {}", path.display()))?;
        if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
            bail!("benchmark manifest must be a file no larger than {MAX_MANIFEST_BYTES} bytes");
        }
        let manifest_path = path.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize benchmark manifest {}",
                path.display()
            )
        })?;
        let bytes = fs::read(&manifest_path).with_context(|| {
            format!(
                "failed to read benchmark manifest {}",
                manifest_path.display()
            )
        })?;
        let manifest: BenchmarkManifest = serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "failed to parse benchmark manifest {}",
                manifest_path.display()
            )
        })?;
        validate_manifest(&manifest_path, &manifest)?;
        Ok(Self {
            manifest_path,
            manifest,
        })
    }

    pub fn manifest(&self) -> &BenchmarkManifest {
        &self.manifest
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn library_root(&self) -> &Path {
        &self.manifest.library_root
    }

    pub fn config_path(&self) -> &Path {
        &self.manifest.config_path
    }

    pub fn app_data_root(&self) -> &Path {
        &self.manifest.app_data_root
    }

    pub fn result_root(&self) -> &Path {
        &self.manifest.result_root
    }

    pub fn scenario_identity(&self) -> Result<(&str, &str)> {
        scenario_identity(&self.manifest)
    }
}

impl BenchmarkSession {
    pub fn start(
        launch: BenchmarkLaunch,
        harness_initialization_ms: f64,
        process_started_at: Instant,
    ) -> Result<Arc<Self>> {
        let result_root = launch.result_root().to_path_buf();
        for artifact in [
            "observer-context.json",
            "events.jsonl",
            "server_requests.jsonl",
            "terminal.json",
        ] {
            if result_root.join(artifact).exists() {
                bail!("benchmark result artifact already exists: {artifact}");
            }
        }
        write_observer_context(&launch, true)?;
        let (sender, receiver) = mpsc::channel(EVENT_QUEUE_CAPACITY);
        tauri::async_runtime::spawn(writer_task(result_root, receiver));
        let session = Arc::new(Self {
            launch,
            started_at: process_started_at,
            harness_initialization_ms,
            sender,
            accepted_records: AtomicU64::new(0),
            dropped_records: AtomicU64::new(0),
            high_water_mark: AtomicU64::new(0),
            terminal_written: AtomicBool::new(false),
        });
        session.record_app_event(
            "benchmark_session_started",
            serde_json::json!({
                "manifest_path": session.launch.manifest_path().file_name().and_then(|name| name.to_str()),
                "harness_initialization_ms": harness_initialization_ms,
            }),
        )?;
        Ok(session)
    }

    pub fn launch(&self) -> &BenchmarkLaunch {
        &self.launch
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64() * 1_000.0
    }

    pub fn info(&self) -> BenchmarkSessionInfo {
        BenchmarkSessionInfo {
            schema_version: SCHEMA_VERSION,
            run_id: self.launch.manifest.run_id.clone(),
            observer_profile: self.launch.manifest.observer_profile,
            harness_initialization_ms: self.harness_initialization_ms,
            session_elapsed_ms: self.elapsed_ms(),
            fixtures: self.launch.manifest.fixtures.clone(),
            scenarios: self.launch.manifest.scenarios.clone(),
            ddragon_mode: self.launch.manifest.ddragon.mode.clone(),
            ddragon_cache_fingerprint: self.launch.manifest.ddragon.cache_fingerprint.clone(),
            event_queue: self.queue_stats(),
        }
    }

    pub fn queue_stats(&self) -> QueueStats {
        QueueStats {
            capacity: EVENT_QUEUE_CAPACITY as u64,
            high_water_mark: self.high_water_mark.load(Ordering::Relaxed),
            accepted_records: self.accepted_records.load(Ordering::Relaxed),
            dropped_records: self.dropped_records.load(Ordering::Relaxed),
        }
    }

    pub fn record_events(&self, inputs: Vec<BenchmarkEventInput>) -> Result<RecordEventsResult> {
        if inputs.is_empty() || inputs.len() > MAX_EVENT_BATCH {
            bail!("benchmark event batches must contain 1..={MAX_EVENT_BATCH} records");
        }
        let (expected_scenario_id, expected_trial_id) = scenario_identity(&self.launch.manifest)?;
        let mut lines = Vec::with_capacity(inputs.len());
        for input in inputs {
            validate_event_input(&input)?;
            if input.scenario_id != expected_scenario_id || input.trial_id != expected_trial_id {
                bail!("benchmark event identity does not match the active scenario and trial");
            }
            let event = BenchmarkEvent {
                schema_version: SCHEMA_VERSION,
                run_id: self.launch.manifest.run_id.clone(),
                scenario_id: input.scenario_id,
                trial_id: input.trial_id,
                monotonic_ms: input.monotonic_ms,
                source: input.source,
                kind: input.kind,
                generation: input.generation,
                action_id: input.action_id,
                payload: input.payload,
            };
            let mut line = serde_json::to_vec(&event)?;
            if line.len() > MAX_EVENT_BYTES {
                bail!("one benchmark event exceeds the {MAX_EVENT_BYTES}-byte limit");
            }
            line.push(b'\n');
            lines.push(line);
        }
        let accepted = self.enqueue(Artifact::Events, lines);
        Ok(RecordEventsResult {
            accepted,
            queue: self.queue_stats(),
        })
    }

    pub fn record_server_requests<T: Serialize>(
        &self,
        requests: &[T],
    ) -> Result<RecordEventsResult> {
        if requests.is_empty() {
            return Ok(RecordEventsResult {
                accepted: true,
                queue: self.queue_stats(),
            });
        }
        if requests.len() > MAX_EVENT_BATCH {
            bail!("server request batches must contain at most {MAX_EVENT_BATCH} records");
        }
        let mut lines = Vec::with_capacity(requests.len());
        for request in requests {
            let mut line = serde_json::to_vec(request)?;
            if line.len() > MAX_EVENT_BYTES {
                bail!("one server request record exceeds the {MAX_EVENT_BYTES}-byte limit");
            }
            line.push(b'\n');
            lines.push(line);
        }
        let accepted = self.enqueue(Artifact::ServerRequests, lines);
        Ok(RecordEventsResult {
            accepted,
            queue: self.queue_stats(),
        })
    }

    pub fn record_app_event(&self, kind: &str, payload: Value) -> Result<()> {
        let (scenario_id, trial_id) = scenario_identity(&self.launch.manifest)?;
        self.record_events(vec![BenchmarkEventInput {
            scenario_id: scenario_id.to_owned(),
            trial_id: trial_id.to_owned(),
            monotonic_ms: self.elapsed_ms(),
            source: "app".to_owned(),
            kind: kind.to_owned(),
            generation: None,
            action_id: None,
            payload,
        }])?;
        Ok(())
    }

    fn enqueue(&self, artifact: Artifact, lines: Vec<Vec<u8>>) -> bool {
        let record_count = lines.len() as u64;
        match self
            .sender
            .try_send(WriterMessage::Records { artifact, lines })
        {
            Ok(()) => {
                self.accepted_records
                    .fetch_add(record_count, Ordering::Relaxed);
                let depth = EVENT_QUEUE_CAPACITY.saturating_sub(self.sender.capacity()) as u64;
                self.high_water_mark.fetch_max(depth, Ordering::Relaxed);
                true
            }
            Err(_) => {
                self.dropped_records
                    .fetch_add(record_count, Ordering::Relaxed);
                false
            }
        }
    }

    pub async fn flush(&self) -> Result<(), String> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send(WriterMessage::Flush(reply))
            .await
            .map_err(|_| "benchmark writer stopped before flush".to_owned())?;
        response
            .await
            .map_err(|_| "benchmark writer dropped the flush response".to_owned())?
    }

    pub async fn finish(&self, input: BenchmarkTerminalInput) -> Result<(), String> {
        validate_identifier("scenario_id", &input.scenario_id)
            .map_err(|error| error.to_string())?;
        validate_identifier("trial_id", &input.trial_id).map_err(|error| error.to_string())?;
        if !matches!(input.status.as_str(), "complete" | "failed" | "invalid") {
            return Err("terminal status must be complete, failed, or invalid".to_owned());
        }
        let (expected_scenario_id, expected_trial_id) =
            scenario_identity(&self.launch.manifest).map_err(|error| error.to_string())?;
        if input.scenario_id != expected_scenario_id || input.trial_id != expected_trial_id {
            return Err(
                "benchmark terminal identity does not match the active scenario and trial"
                    .to_owned(),
            );
        }
        if self.terminal_written.swap(true, Ordering::AcqRel) {
            return Err("benchmark terminal result was already written".to_owned());
        }
        let terminal = BenchmarkTerminal {
            schema_version: SCHEMA_VERSION,
            run_id: self.launch.manifest.run_id.clone(),
            scenario_id: input.scenario_id,
            trial_id: input.trial_id,
            status: input.status,
            reason: input.reason,
            elapsed_ms: self.elapsed_ms(),
            event_queue: self.queue_stats(),
            payload: input.payload,
        };
        let mut bytes = serde_json::to_vec_pretty(&terminal).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        let (reply, response) = oneshot::channel();
        self.sender
            .send(WriterMessage::Terminal { bytes, reply })
            .await
            .map_err(|_| "benchmark writer stopped before terminal result".to_owned())?;
        response
            .await
            .map_err(|_| "benchmark writer dropped the terminal response".to_owned())??;
        write_observer_context(&self.launch, false).map_err(|error| error.to_string())
    }
}

async fn writer_task(result_root: PathBuf, mut receiver: mpsc::Receiver<WriterMessage>) {
    let events_path = result_root.join("events.jsonl");
    let requests_path = result_root.join("server_requests.jsonl");
    let terminal_path = result_root.join("terminal.json");
    let mut events = match create_artifact(&events_path).await {
        Ok(file) => file,
        Err(error) => {
            eprintln!("replay benchmark writer failed: {error}");
            return;
        }
    };
    let mut requests = match create_artifact(&requests_path).await {
        Ok(file) => file,
        Err(error) => {
            eprintln!("replay benchmark writer failed: {error}");
            return;
        }
    };
    while let Some(message) = receiver.recv().await {
        match message {
            WriterMessage::Records { artifact, lines } => {
                let file = match artifact {
                    Artifact::Events => &mut events,
                    Artifact::ServerRequests => &mut requests,
                };
                for line in lines {
                    if let Err(error) = file.write_all(&line).await {
                        eprintln!("replay benchmark writer failed: {error}");
                        return;
                    }
                }
            }
            WriterMessage::Flush(reply) => {
                let result = flush_artifacts(&mut events, &mut requests).await;
                let _ = reply.send(result);
            }
            WriterMessage::Terminal { bytes, reply } => {
                let result = async {
                    flush_artifacts(&mut events, &mut requests).await?;
                    let mut terminal = tokio::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(&terminal_path)
                        .await
                        .map_err(|error| {
                            format!("failed to create {}: {error}", terminal_path.display())
                        })?;
                    terminal.write_all(&bytes).await.map_err(|error| {
                        format!("failed to write {}: {error}", terminal_path.display())
                    })?;
                    terminal.sync_all().await.map_err(|error| {
                        format!("failed to sync {}: {error}", terminal_path.display())
                    })
                }
                .await;
                let _ = reply.send(result);
            }
        }
    }
}

async fn create_artifact(path: &Path) -> Result<tokio::fs::File, String> {
    tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .await
        .map_err(|error| format!("failed to create {}: {error}", path.display()))
}

async fn flush_artifacts(
    events: &mut tokio::fs::File,
    requests: &mut tokio::fs::File,
) -> Result<(), String> {
    events
        .flush()
        .await
        .map_err(|error| format!("failed to flush benchmark events: {error}"))?;
    requests
        .flush()
        .await
        .map_err(|error| format!("failed to flush benchmark requests: {error}"))?;
    Ok(())
}

fn validate_manifest(manifest_path: &Path, manifest: &BenchmarkManifest) -> Result<()> {
    if manifest.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported replay benchmark schema version {}",
            manifest.schema_version
        );
    }
    validate_identifier("run_id", &manifest.run_id)?;
    if manifest.ddragon.mode != "offline" {
        bail!("benchmark Data Dragon mode must be offline");
    }
    let cache_fingerprint = manifest.ddragon.cache_fingerprint.as_str();
    if cache_fingerprint.len() != "sha256:".len() + 64
        || !cache_fingerprint.starts_with("sha256:")
        || !cache_fingerprint["sha256:".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("benchmark Data Dragon cache fingerprint must be a lowercase sha256 identity");
    }
    if manifest.fixtures.is_empty() {
        bail!("benchmark manifest must declare at least one fixture");
    }
    if manifest.scenarios.len() != 1 {
        bail!("benchmark manifest must declare exactly one scenario per process");
    }
    scenario_identity(manifest)?;
    let fixture_ids = manifest
        .fixtures
        .iter()
        .map(|fixture| fixture.id.as_str())
        .collect::<BTreeSet<_>>();
    if fixture_ids.len() != manifest.fixtures.len() {
        bail!("benchmark fixture ids must be unique");
    }
    let fixture_aliases = manifest
        .fixtures
        .iter()
        .map(|fixture| fixture.alias.as_str())
        .collect::<BTreeSet<_>>();
    if fixture_aliases.len() != manifest.fixtures.len() {
        bail!("benchmark fixture aliases must be unique");
    }
    let game_timestamps = manifest
        .fixtures
        .iter()
        .map(|fixture| fixture.game_timestamp.as_str())
        .collect::<BTreeSet<_>>();
    if game_timestamps.len() != manifest.fixtures.len() {
        bail!("benchmark fixture game timestamps must be unique");
    }
    for fixture in &manifest.fixtures {
        validate_identifier("fixture id", &fixture.id)?;
        validate_identifier("fixture alias", &fixture.alias)?;
        validate_identifier("fixture game_timestamp", &fixture.game_timestamp)?;
    }
    validate_scenario(manifest, &fixture_ids)?;

    for path in [
        &manifest.sentinel_root,
        &manifest.library_root,
        &manifest.config_path,
        &manifest.app_data_root,
        &manifest.result_root,
        &manifest.scratch_root,
        &manifest.ddragon.cache_root,
    ] {
        validate_absolute_normal_path(path)?;
    }
    let sentinel = canonical_directory(&manifest.sentinel_root, "sentinel root")?;
    if sentinel.file_name().and_then(|name| name.to_str()) != Some(SENTINEL_DIRECTORY) {
        bail!("benchmark sentinel root must end in {SENTINEL_DIRECTORY}");
    }
    ensure_canonical_below(manifest_path, &sentinel, "manifest")?;
    let ddragon_cache = canonical_directory(&manifest.ddragon.cache_root, "Data Dragon cache")?;
    let expected_ddragon_cache = canonical_directory(
        &manifest.app_data_root.join("ddragon"),
        "expected Data Dragon cache",
    )?;
    if ddragon_cache != expected_ddragon_cache {
        bail!("benchmark Data Dragon cache must equal app_data_root/ddragon");
    }
    ensure_canonical_below(&ddragon_cache, &sentinel, "Data Dragon cache")?;
    let mut isolated_roots = Vec::new();
    for (label, path) in [
        ("library root", &manifest.library_root),
        ("app data root", &manifest.app_data_root),
        ("result root", &manifest.result_root),
        ("scratch root", &manifest.scratch_root),
    ] {
        let canonical = canonical_directory(path, label)?;
        ensure_canonical_below(&canonical, &sentinel, label)?;
        isolated_roots.push((label, canonical));
    }
    let config_parent = manifest
        .config_path
        .parent()
        .context("benchmark config path has no parent")?;
    let config_parent = canonical_directory(config_parent, "config parent")?;
    ensure_canonical_below(&config_parent, &sentinel, "config path")?;
    isolated_roots.push(("config parent", config_parent));
    for left_index in 0..isolated_roots.len() {
        for right_index in (left_index + 1)..isolated_roots.len() {
            let (left_label, left) = &isolated_roots[left_index];
            let (right_label, right) = &isolated_roots[right_index];
            if left == right || left.starts_with(right) || right.starts_with(left) {
                bail!("benchmark {left_label} and {right_label} must be disjoint");
            }
        }
    }
    if !manifest.config_path.is_file() {
        bail!("benchmark config path must name an existing file");
    }
    let config = manifest.config_path.canonicalize()?;
    ensure_canonical_below(&config, &sentinel, "config path")?;
    Ok(())
}

fn scenario_identity(manifest: &BenchmarkManifest) -> Result<(&str, &str)> {
    let scenario = manifest
        .scenarios
        .first()
        .and_then(Value::as_object)
        .context("benchmark scenario must be an object")?;
    let scenario_id = scenario
        .get("id")
        .and_then(Value::as_str)
        .context("benchmark scenario id is missing")?;
    let trial_id = scenario
        .get("trial_id")
        .and_then(Value::as_str)
        .context("benchmark scenario trial_id is missing")?;
    let kind = scenario
        .get("kind")
        .and_then(Value::as_str)
        .context("benchmark scenario kind is missing")?;
    validate_identifier("scenario id", scenario_id)?;
    validate_identifier("scenario trial_id", trial_id)?;
    validate_identifier("scenario kind", kind)?;
    Ok((scenario_id, trial_id))
}

fn validate_scenario(manifest: &BenchmarkManifest, fixture_ids: &BTreeSet<&str>) -> Result<()> {
    let scenario = manifest
        .scenarios
        .first()
        .and_then(Value::as_object)
        .context("benchmark scenario must be an object")?;
    let kind = scenario
        .get("kind")
        .and_then(Value::as_str)
        .context("benchmark scenario kind is missing")?;
    if !matches!(
        kind,
        "app_idle"
            | "cold_open"
            | "warm_open"
            | "play_pause"
            | "rate"
            | "seek"
            | "scrub"
            | "layout"
            | "lifecycle"
            | "export"
    ) {
        bail!("unsupported benchmark scenario kind: {kind}");
    }
    let declared = scenario
        .get("fixture_ids")
        .and_then(Value::as_array)
        .context("benchmark scenario fixture_ids are missing")?;
    if declared.is_empty() {
        bail!("benchmark scenario must declare at least one fixture id");
    }
    if kind != "lifecycle" && declared.len() != 1 {
        bail!("only lifecycle scenarios may declare multiple fixture ids");
    }
    let mut unique = BTreeSet::new();
    for value in declared {
        let fixture_id = value
            .as_str()
            .context("benchmark scenario fixture ids must be strings")?;
        validate_identifier("scenario fixture id", fixture_id)?;
        if !fixture_ids.contains(fixture_id) {
            bail!("benchmark scenario references an unknown fixture id");
        }
        if !unique.insert(fixture_id) {
            bail!("benchmark scenario fixture ids must be unique");
        }
    }
    Ok(())
}

fn write_observer_context(launch: &BenchmarkLaunch, active: bool) -> Result<()> {
    let (scenario_id, trial_id) = scenario_identity(launch.manifest())?;
    let context = ObserverContext {
        schema_version: SCHEMA_VERSION,
        run_id: &launch.manifest.run_id,
        scenario_id,
        trial_id,
        active,
    };
    let mut bytes = serde_json::to_vec_pretty(&context)?;
    bytes.push(b'\n');
    crate::config::write_atomic(&launch.result_root().join("observer-context.json"), &bytes)
}

fn validate_absolute_normal_path(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        bail!("benchmark paths must be absolute: {}", path.display());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        bail!(
            "benchmark paths must not contain dot segments: {}",
            path.display()
        );
    }
    Ok(())
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {label} {}", path.display()))?;
    if !canonical.is_dir() {
        bail!("{label} is not a directory: {}", canonical.display());
    }
    Ok(canonical)
}

fn ensure_canonical_below(path: &Path, root: &Path, label: &str) -> Result<()> {
    if path == root || !path.starts_with(root) {
        bail!("benchmark {label} must resolve below the sentinel root");
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        bail!("invalid benchmark {label}");
    }
    Ok(())
}

fn validate_event_input(event: &BenchmarkEventInput) -> Result<()> {
    validate_identifier("scenario_id", &event.scenario_id)?;
    validate_identifier("trial_id", &event.trial_id)?;
    validate_identifier("source", &event.source)?;
    validate_identifier("event kind", &event.kind)?;
    if let Some(action_id) = &event.action_id {
        validate_identifier("action_id", action_id)?;
    }
    if !event.monotonic_ms.is_finite() || event.monotonic_ms < 0.0 {
        bail!("benchmark monotonic_ms must be finite and non-negative");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_fixture() -> (tempfile::TempDir, PathBuf) {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join(SENTINEL_DIRECTORY);
        for directory in ["library", "config", "app-data", "run", "scratch"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::create_dir_all(root.join("app-data").join("ddragon")).unwrap();
        fs::write(
            root.join("config").join("config.toml"),
            b"# benchmark test\n",
        )
        .unwrap();
        let manifest_path = root.join("run").join("manifest.json");
        let manifest = serde_json::json!({
            "schema_version": 1,
            "run_id": "run-001",
            "sentinel_root": root,
            "library_root": root.join("library"),
            "config_path": root.join("config").join("config.toml"),
            "app_data_root": root.join("app-data"),
            "result_root": root.join("run"),
            "scratch_root": root.join("scratch"),
            "observer_profile": "full",
            "ddragon": {
                "mode": "offline",
                "cache_root": root.join("app-data").join("ddragon"),
                "cache_fingerprint": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            },
            "fixtures": [{ "id": "short-h264", "alias": "short-h264", "game_timestamp": "20260824-120000" }],
            "scenarios": [{ "id": "cold-open", "trial_id": "trial-1", "kind": "cold_open", "fixture_ids": ["short-h264"] }]
        });
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        (temporary, manifest_path)
    }

    #[test]
    fn normal_arguments_do_not_activate_benchmark_mode() {
        assert!(
            BenchmarkLaunch::from_args([OsString::from("app.exe")])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn activation_requires_the_exact_argument_form() {
        let error = BenchmarkLaunch::from_args([
            OsString::from("app.exe"),
            OsString::from("--other"),
            OsString::from("--replay-benchmark-manifest"),
            OsString::from("manifest.json"),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("exact form"));
    }

    #[test]
    fn accepts_a_manifest_whose_roots_resolve_below_the_sentinel() {
        let (_temporary, manifest_path) = manifest_fixture();
        let launch = BenchmarkLaunch::from_args([
            OsString::from("app.exe"),
            OsString::from("--replay-benchmark-manifest"),
            manifest_path.into_os_string(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(launch.manifest().run_id, "run-001");
    }

    #[test]
    fn rejects_a_library_outside_the_sentinel() {
        let (temporary, manifest_path) = manifest_fixture();
        let mut value: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        value["library_root"] = Value::String(temporary.path().display().to_string());
        fs::write(&manifest_path, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = BenchmarkLaunch::load(&manifest_path).unwrap_err();
        assert!(error.to_string().contains("library root"));
    }

    #[test]
    fn rejects_overlapping_mutable_roots() {
        let (_temporary, manifest_path) = manifest_fixture();
        let mut value: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        value["scratch_root"] = value["library_root"].clone();
        fs::write(&manifest_path, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = BenchmarkLaunch::load(&manifest_path).unwrap_err();
        assert!(error.to_string().contains("must be disjoint"));
    }

    #[test]
    fn rejects_a_scenario_that_references_an_unknown_fixture() {
        let (_temporary, manifest_path) = manifest_fixture();
        let mut value: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        value["scenarios"][0]["fixture_ids"] = serde_json::json!(["not-declared"]);
        fs::write(&manifest_path, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = BenchmarkLaunch::load(&manifest_path).unwrap_err();
        assert!(error.to_string().contains("unknown fixture"));
    }

    #[test]
    fn rejects_an_unpinned_data_dragon_cache() {
        let (_temporary, manifest_path) = manifest_fixture();
        let mut value: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        value["ddragon"]["cache_fingerprint"] = Value::String("sha256:test".to_owned());
        fs::write(&manifest_path, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = BenchmarkLaunch::load(&manifest_path).unwrap_err();
        assert!(error.to_string().contains("lowercase sha256 identity"));
    }

    #[test]
    fn rejects_a_data_dragon_cache_outside_app_data() {
        let (_temporary, manifest_path) = manifest_fixture();
        let mut value: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        value["ddragon"]["cache_root"] = value["scratch_root"].clone();
        fs::write(&manifest_path, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = BenchmarkLaunch::load(&manifest_path).unwrap_err();
        assert!(error.to_string().contains("app_data_root/ddragon"));
    }

    #[test]
    fn validates_bounded_event_envelopes() {
        let event = BenchmarkEventInput {
            scenario_id: "seek-near".to_owned(),
            trial_id: "trial-1".to_owned(),
            monotonic_ms: 42.0,
            source: "frontend".to_owned(),
            kind: "seek_requested".to_owned(),
            generation: Some(1),
            action_id: Some("seek-1".to_owned()),
            payload: serde_json::json!({ "target_ms": 1000 }),
        };
        assert!(validate_event_input(&event).is_ok());
        let invalid = BenchmarkEventInput {
            monotonic_ms: f64::NAN,
            ..event
        };
        assert!(validate_event_input(&invalid).is_err());
    }
}
