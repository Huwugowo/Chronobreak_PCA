use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail, ensure};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::storage::{
    GAME_LOG_JSON, JsonWriteStats, write_json_atomic, write_json_atomic_with_stats,
};

const LIVE_CLIENT_BASE_URL: &str = "https://127.0.0.1:2999/liveclientdata";
const API_TIMEOUT: Duration = Duration::from_secs(2);
const CALIBRATION_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const CALIBRATION_PROBE_INTERVAL: Duration = Duration::from_millis(250);
const CALIBRATION_MIN_WINDOW_SPAN: Duration = Duration::from_millis(750);
const CALIBRATION_CLOCK_TOLERANCE: Duration = Duration::from_millis(250);
const API_FAILURE_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const EVENT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(10);
const CALIBRATION_SAMPLE_COUNT: usize = 5;
const MAX_CONSECUTIVE_API_FAILURES: u8 = 3;
const POLLER_STARTUP_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const POLLER_TASK_STOP_TIMEOUT: Duration = Duration::from_secs(5);
const POLLER_ABORT_REAP_TIMEOUT: Duration = Duration::from_secs(1);
const POLLER_FINALIZATION_TIMEOUT: Duration = Duration::from_secs(5);
const GAME_LOG_WRITE_COALESCE_WINDOW: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GameLog {
    pub game_start_video_offset_ms: Option<i64>,
    pub snapshots: Vec<Snapshot>,
    pub events: Vec<GameEvent>,
    pub snapshot_derived_changes: Vec<SnapshotChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub game_time_ms: i64,
    pub players: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerSnapshot {
    pub summoner_name: String,
    pub team: String,
    pub champion: String,
    pub gold: Option<i64>,
    pub hp: Option<i64>,
    pub hp_max: Option<i64>,
    pub cs: u32,
    pub level: u32,
    pub items: Vec<ItemSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summoner_spells: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keystone_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rune_ids: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemSnapshot {
    pub item_id: u32,
    pub slot: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub game_time_ms: i64,
    pub video_time_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub killer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub victim: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assisters: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inhibitor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dragon_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stolen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kill_streak: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acing_team: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotChange {
    pub game_time_ms: i64,
    pub player: String,
    pub change_type: SnapshotChangeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_level: Option<u32>,
    pub video_time_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SnapshotChangeType {
    ItemPurchased,
    ItemSold,
    LevelUp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollerSummary {
    pub game_start_video_offset_ms: Option<i64>,
    pub game_mode: Option<String>,
    pub local_player_summoner_name: Option<String>,
    pub local_player_champion: Option<String>,
    pub local_player_team: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingMetadata {
    pub recorded_at: String,
    pub duration_ms: u64,
    pub game_mode: Option<String>,
    pub local_player_summoner_name: Option<String>,
    pub local_player_champion: Option<String>,
    pub local_player_team: Option<String>,
    pub video_offset_ms: Option<i64>,
    pub encoder_used: String,
    pub recording_codec: String,
    pub recording_profile: String,
    pub recording_resolution: String,
    pub recording_fps: u32,
    pub capture_backend: String,
    pub capture_adapter_luid: Option<String>,
    pub capture_adapter_name: Option<String>,
    pub capture_output: Option<String>,
    pub encoder_interop: Option<String>,
    pub media_runtime_id: String,
    pub capture_support_label: String,
    pub source_frames_surfaced: u64,
    pub source_frames_superseded: u64,
    pub cfr_duplicates: u64,
    pub cfr_discards: u64,
    pub pool_recreations: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture: Option<CaptureMetadata>,
    pub saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureMetadata {
    pub schema_version: u32,
    pub backend: String,
    pub diagnostics_abi: u32,
    pub support_label: String,
    pub capture_adapter_luid: String,
    pub encoder_adapter_luid: String,
    pub capture_adapter_name: String,
    pub capture_output: String,
    pub encoder_backend: String,
    pub encoder_interop: String,
    pub media_runtime_id: String,
    pub source_format: String,
    pub converted_format: String,
    pub host_readback: bool,
    pub gpu_stages: Vec<String>,
    pub frame_pool_capacity: u32,
    pub capture_output_pool_capacity: u32,
    pub filter_buffered_frame_limit: u32,
    pub encoder_depth: u32,
    pub progress_stall_timeout_seconds: u32,
    pub maximum_texture_bytes: u64,
    pub source_frames_surfaced: u64,
    pub source_frames_superseded: u64,
    pub encoded_frames: u64,
    pub muxed_bytes: u64,
    pub cfr_duplicates: u64,
    pub cfr_discards: u64,
    pub pool_recreations: u64,
    pub first_qpc_100ns: i64,
    pub latest_qpc_100ns: i64,
    pub terminal_progress: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordingDetails {
    pub encoder_used: String,
    pub codec: String,
    pub profile: String,
    pub resolution: String,
    pub fps: u32,
    pub capture_backend: String,
    pub capture_adapter_luid: Option<String>,
    pub capture_adapter_name: Option<String>,
    pub capture_output: Option<String>,
    pub encoder_interop: Option<String>,
    pub media_runtime_id: String,
    pub capture_support_label: String,
    pub source_frames_surfaced: u64,
    pub source_frames_superseded: u64,
    pub cfr_duplicates: u64,
    pub cfr_discards: u64,
    pub pool_recreations: u64,
    pub capture: Option<CaptureMetadata>,
}

impl RecordingMetadata {
    pub fn new(
        recorded_at: SystemTime,
        duration: Duration,
        summary: PollerSummary,
        recording: RecordingDetails,
    ) -> Result<Self> {
        let recorded_at = OffsetDateTime::from(recorded_at)
            .format(&Rfc3339)
            .context("failed to format recording start time")?;
        let duration_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);

        Ok(Self {
            recorded_at,
            duration_ms,
            game_mode: summary.game_mode,
            local_player_summoner_name: summary.local_player_summoner_name,
            local_player_champion: summary.local_player_champion,
            local_player_team: summary.local_player_team,
            video_offset_ms: summary.game_start_video_offset_ms,
            encoder_used: recording.encoder_used,
            recording_codec: recording.codec,
            recording_profile: recording.profile,
            recording_resolution: recording.resolution,
            recording_fps: recording.fps,
            capture_backend: recording.capture_backend,
            capture_adapter_luid: recording.capture_adapter_luid,
            capture_adapter_name: recording.capture_adapter_name,
            capture_output: recording.capture_output,
            encoder_interop: recording.encoder_interop,
            media_runtime_id: recording.media_runtime_id,
            capture_support_label: recording.capture_support_label,
            source_frames_surfaced: recording.source_frames_surfaced,
            source_frames_superseded: recording.source_frames_superseded,
            cfr_duplicates: recording.cfr_duplicates,
            cfr_discards: recording.cfr_discards,
            pool_recreations: recording.pool_recreations,
            capture: recording.capture,
            saved: false,
        })
    }
}

pub struct PollerSession {
    cancellation: watch::Sender<bool>,
    task: JoinHandle<Result<()>>,
    state: Arc<Mutex<PollerState>>,
    writer: GameLogWriter,
    output: PathBuf,
}

struct GameLogWriter {
    revisions: watch::Sender<u64>,
    commands: mpsc::Sender<GameLogWriterCommand>,
    task: Option<JoinHandle<Result<()>>>,
}

enum GameLogWriterCommand {
    FlushAndStop {
        required_revision: u64,
        completed: oneshot::Sender<()>,
    },
}

impl GameLogWriter {
    fn start(state: Arc<Mutex<PollerState>>, output: PathBuf) -> Self {
        Self::start_with_window(state, output, GAME_LOG_WRITE_COALESCE_WINDOW)
    }

    fn start_with_window(
        state: Arc<Mutex<PollerState>>,
        output: PathBuf,
        coalesce_window: Duration,
    ) -> Self {
        let (revisions, revision_receiver) = watch::channel(0_u64);
        let (commands, command_receiver) = mpsc::channel(1);
        let task = tokio::spawn(run_game_log_writer(
            state,
            output,
            coalesce_window,
            revision_receiver,
            command_receiver,
        ));
        Self {
            revisions,
            commands,
            task: Some(task),
        }
    }

    fn notifier(&self) -> watch::Sender<u64> {
        self.revisions.clone()
    }

    async fn flush_and_stop(&mut self, required_revision: u64) -> Result<()> {
        let (completed, completion) = oneshot::channel();
        let send_error = self
            .commands
            .send(GameLogWriterCommand::FlushAndStop {
                required_revision,
                completed,
            })
            .await
            .err();
        let completion_error = if send_error.is_none() {
            completion.await.err()
        } else {
            None
        };
        let task_result = self
            .task
            .as_mut()
            .context("game-log writer task was already reaped")?
            .await
            .context("game-log writer task panicked");
        self.task = None;
        task_result??;
        if let Some(error) = send_error {
            return Err(error).context("game-log writer stopped before its final flush request");
        }
        if let Some(error) = completion_error {
            return Err(error)
                .context("game-log writer stopped before acknowledging its final flush");
        }
        Ok(())
    }

    async fn abort_and_reap(&mut self) {
        let Some(task) = self.task.as_mut() else {
            return;
        };
        task.abort();
        let _ = tokio::time::timeout(POLLER_ABORT_REAP_TIMEOUT, task).await;
        self.task = None;
    }
}

impl Drop for GameLogWriter {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl PollerSession {
    pub async fn start(directory: &Path, video_started_at: Instant) -> Result<Self> {
        let output = directory.join(GAME_LOG_JSON);
        let state = Arc::new(Mutex::new(PollerState::default()));
        tokio::time::timeout(
            POLLER_STARTUP_WRITE_TIMEOUT,
            write_json_atomic(&output, &GameLog::default()),
        )
        .await
        .context("initial game log write timed out")??;

        let client = LiveClient::new(LIVE_CLIENT_BASE_URL)?;
        let writer = GameLogWriter::start(Arc::clone(&state), output.clone());
        let writer_notifier = writer.notifier();
        let (cancellation, receiver) = watch::channel(false);
        let task_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            run_poller(
                client,
                video_started_at,
                receiver,
                task_state,
                writer_notifier,
            )
            .await
        });

        Ok(Self {
            cancellation,
            task,
            state,
            writer,
            output,
        })
    }

    pub async fn stop(mut self) -> PollerSummary {
        let _ = self.cancellation.send(true);
        match tokio::time::timeout(POLLER_TASK_STOP_TIMEOUT, &mut self.task).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(error))) => error!(%error, "Live Client poller stopped with an error"),
            Ok(Err(error)) => error!(%error, "Live Client poller task panicked"),
            Err(_) => {
                error!(
                    timeout_ms = POLLER_TASK_STOP_TIMEOUT.as_millis(),
                    "Live Client poller stopped with an error: shutdown timed out"
                );
                self.task.abort();
                match tokio::time::timeout(POLLER_ABORT_REAP_TIMEOUT, &mut self.task).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) if error.is_cancelled() => {}
                    Ok(Err(error)) => {
                        error!(%error, "Live Client poller task panicked while being reaped")
                    }
                    Err(_) => error!(
                        timeout_ms = POLLER_ABORT_REAP_TIMEOUT.as_millis(),
                        "Live Client poller task could not be reaped within its deadline"
                    ),
                }
            }
        }

        let required_revision = self.state.lock().await.revision;
        let finalization = async {
            if let Err(error) = self.writer.flush_and_stop(required_revision).await {
                error!(%error, "could not perform final game log flush");
            }
            let game_log_bytes = tokio::fs::metadata(&self.output)
                .await
                .map(|metadata| metadata.len())
                .unwrap_or_default();
            let state = self.state.lock().await;
            log_poller_diagnostics(&state, game_log_bytes);
            state.summary.clone()
        };

        match tokio::time::timeout(POLLER_FINALIZATION_TIMEOUT, finalization).await {
            Ok(summary) => summary,
            Err(_) => {
                self.writer.abort_and_reap().await;
                error!(
                    timeout_ms = POLLER_FINALIZATION_TIMEOUT.as_millis(),
                    "could not perform final game log flush: finalization timed out"
                );
                log_poller_diagnostics(&PollerState::default(), 0);
                PollerSummary::default()
            }
        }
    }
}

impl Drop for PollerSession {
    fn drop(&mut self) {
        let _ = self.cancellation.send(true);
        self.task.abort();
    }
}

fn log_poller_diagnostics(state: &PollerState, game_log_bytes: u64) {
    let diagnostics = &state.diagnostics;
    info!(
        calibration_requests = diagnostics.calibration_requests,
        event_requests = diagnostics.event_requests,
        snapshot_requests = diagnostics.snapshot_requests,
        successful_responses = diagnostics.successful_responses,
        average_response_latency_ms = diagnostics.average_response_latency_ms(),
        maximum_response_latency_ms = duration_ms_f64(diagnostics.maximum_response_latency),
        captured_events = state.game_log.events.len(),
        captured_snapshots = state.game_log.snapshots.len(),
        game_log_bytes,
        game_log_revision = state.revision,
        durable_game_log_revision = state.durable_revision,
        json_write_requests = diagnostics.json_write_requests,
        json_writes = diagnostics.json_writes,
        json_coalesced_writes = diagnostics.json_coalesced_writes,
        json_write_failures = diagnostics.json_write_failures,
        json_serialized_bytes = diagnostics.json_serialized_bytes,
        total_json_clone_ms = duration_ms_f64(diagnostics.total_json_clone),
        total_json_serialize_ms = duration_ms_f64(diagnostics.total_json_serialize),
        total_json_file_write_ms = duration_ms_f64(diagnostics.total_json_file_write),
        total_json_sync_ms = duration_ms_f64(diagnostics.total_json_sync),
        total_json_rename_ms = duration_ms_f64(diagnostics.total_json_rename),
        total_json_atomic_ms = duration_ms_f64(diagnostics.total_json_atomic),
        total_json_write_ms = duration_ms_f64(diagnostics.total_json_write),
        slowest_json_write_ms = duration_ms_f64(diagnostics.slowest_json_write),
        event_failure_reason = diagnostics
            .event_failure_reason
            .as_deref()
            .unwrap_or("none"),
        snapshot_failure_reason = diagnostics
            .snapshot_failure_reason
            .as_deref()
            .unwrap_or("none"),
        "Live Client poller diagnostics"
    );
}

#[derive(Debug, Default)]
struct PollerState {
    game_log: GameLog,
    summary: PollerSummary,
    seen_event_ids: HashSet<i64>,
    revision: u64,
    durable_revision: u64,
    diagnostics: PollerDiagnostics,
}

impl PollerState {
    fn mark_dirty(&mut self) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.diagnostics.json_write_requests =
            self.diagnostics.json_write_requests.saturating_add(1);
        self.revision
    }
}

#[derive(Debug, Default)]
struct PollerDiagnostics {
    calibration_requests: u64,
    event_requests: u64,
    snapshot_requests: u64,
    successful_responses: u64,
    total_response_latency: Duration,
    maximum_response_latency: Duration,
    json_write_requests: u64,
    json_writes: u64,
    json_coalesced_writes: u64,
    json_write_failures: u64,
    json_serialized_bytes: u64,
    total_json_clone: Duration,
    total_json_serialize: Duration,
    total_json_file_write: Duration,
    total_json_sync: Duration,
    total_json_rename: Duration,
    total_json_atomic: Duration,
    total_json_write: Duration,
    slowest_json_write: Duration,
    event_failure_reason: Option<String>,
    snapshot_failure_reason: Option<String>,
}

impl PollerDiagnostics {
    fn observe_response(&mut self, latency: Duration) {
        self.successful_responses = self.successful_responses.saturating_add(1);
        self.total_response_latency = self.total_response_latency.saturating_add(latency);
        self.maximum_response_latency = self.maximum_response_latency.max(latency);
    }

    fn observe_json_write(
        &mut self,
        clone_elapsed: Duration,
        elapsed: Duration,
        stats: Option<JsonWriteStats>,
    ) {
        self.total_json_clone = self.total_json_clone.saturating_add(clone_elapsed);
        self.total_json_write = self.total_json_write.saturating_add(elapsed);
        self.slowest_json_write = self.slowest_json_write.max(elapsed);
        match stats {
            Some(stats) => {
                self.json_writes = self.json_writes.saturating_add(1);
                self.json_serialized_bytes = self
                    .json_serialized_bytes
                    .saturating_add(stats.serialized_bytes);
                self.total_json_serialize = self
                    .total_json_serialize
                    .saturating_add(stats.serialization);
                self.total_json_file_write = self.total_json_file_write.saturating_add(stats.write);
                self.total_json_sync = self.total_json_sync.saturating_add(stats.sync);
                self.total_json_rename = self.total_json_rename.saturating_add(stats.rename);
                self.total_json_atomic = self.total_json_atomic.saturating_add(stats.total);
            }
            None => {
                self.json_write_failures = self.json_write_failures.saturating_add(1);
            }
        }
    }

    fn average_response_latency_ms(&self) -> f64 {
        if self.successful_responses == 0 {
            return 0.0;
        }
        duration_ms_f64(self.total_response_latency) / self.successful_responses as f64
    }
}

#[derive(Debug, Clone, Copy)]
enum RequestKind {
    Calibration,
    Event,
    Snapshot,
}

async fn record_request(state: &Arc<Mutex<PollerState>>, request_kind: RequestKind) {
    let mut state = state.lock().await;
    let counter = match request_kind {
        RequestKind::Calibration => &mut state.diagnostics.calibration_requests,
        RequestKind::Event => &mut state.diagnostics.event_requests,
        RequestKind::Snapshot => &mut state.diagnostics.snapshot_requests,
    };
    *counter = counter.saturating_add(1);
}

#[derive(Clone)]
struct LiveClient {
    http: Client,
    base_url: String,
}

impl LiveClient {
    fn new(base_url: &str) -> Result<Self> {
        let http = Client::builder()
            .danger_accept_invalid_certs(true)
            .connect_timeout(API_TIMEOUT)
            .timeout(API_TIMEOUT)
            .no_proxy()
            .build()
            .context("failed to create Live Client HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }

    async fn game_stats(&self) -> Result<Received<RawGameData>> {
        self.get("gamestats").await
    }

    async fn active_player(&self) -> Result<Received<RawActivePlayer>> {
        self.get("activeplayer").await
    }

    async fn player_list(&self) -> Result<Received<Vec<RawPlayer>>> {
        self.get("playerlist").await
    }

    async fn event_data(&self) -> Result<Received<RawEventData>> {
        self.get("eventdata").await
    }

    async fn snapshot_data(&self) -> Result<Received<RawSnapshotData>> {
        let (game_data, active_player, all_players, events) = tokio::join!(
            self.game_stats(),
            self.active_player(),
            self.player_list(),
            self.event_data(),
        );
        let game_data = game_data.context("snapshot game stats unavailable")?;
        let active_player = active_player.context("snapshot active player unavailable")?;
        let all_players = all_players.context("snapshot player list unavailable")?;
        let (events, event_latency) = match events {
            Ok(received) => (received.value, Some(received.latency)),
            Err(_) => (RawEventData::default(), None),
        };
        let latency = game_data
            .latency
            .max(active_player.latency)
            .max(all_players.latency)
            .max(event_latency.unwrap_or_default());

        Ok(Received {
            value: RawSnapshotData {
                active_player: active_player.value,
                all_players: all_players.value,
                game_data: game_data.value,
                events,
            },
            received_at: game_data.received_at,
            latency,
        })
    }

    async fn get<T>(&self, endpoint: &str) -> Result<Received<T>>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}/{endpoint}", self.base_url);
        let request_started_at = Instant::now();
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("request to {url} failed"))?
            .error_for_status()
            .with_context(|| format!("request to {url} returned an error status"))?;
        let body = response
            .bytes()
            .await
            .with_context(|| format!("failed to receive response body from {url}"))?;
        let received_at = Instant::now();
        let latency = received_at.saturating_duration_since(request_started_at);
        let value = serde_json::from_slice(&body)
            .with_context(|| format!("invalid JSON response from {url}"))?;
        Ok(Received {
            value,
            received_at,
            latency,
        })
    }
}

struct Received<T> {
    value: T,
    received_at: Instant,
    latency: Duration,
}

async fn run_poller(
    client: LiveClient,
    video_started_at: Instant,
    mut cancellation: watch::Receiver<bool>,
    state: Arc<Mutex<PollerState>>,
    writer: watch::Sender<u64>,
) -> Result<()> {
    info!("waiting for the Live Client API");
    let Some(calibration) = calibrate(&client, video_started_at, &mut cancellation, &state).await?
    else {
        return Ok(());
    };
    let mut initial_data = fetch_initial_snapshot(&client, &mut cancellation, &state).await;
    if *cancellation.borrow() {
        return Ok(());
    }
    let take_first_snapshot_immediately = initial_data.is_none();

    let calibration_revision = {
        let mut state = state.lock().await;
        state.game_log.game_start_video_offset_ms = Some(calibration.video_offset_ms);
        state.summary.game_start_video_offset_ms = Some(calibration.video_offset_ms);
        if let Some(initial_data) = &mut initial_data {
            update_summary(&mut state.summary, initial_data);
            reconcile_events(
                &mut state,
                std::mem::take(&mut initial_data.events.events),
                calibration.video_offset_ms,
            );
            append_snapshot(
                &mut state.game_log,
                initial_data,
                calibration.video_offset_ms,
            )?;
        }
        state.mark_dirty()
    };
    notify_game_log_writer(&writer, calibration_revision)?;
    info!(
        video_offset_ms = calibration.video_offset_ms,
        "Live Client clock calibrated"
    );

    let event_loop = event_loop(
        client.clone(),
        calibration.video_offset_ms,
        cancellation.clone(),
        Arc::clone(&state),
        writer.clone(),
    );
    let snapshot_loop = snapshot_loop(
        client,
        calibration.video_offset_ms,
        cancellation,
        state,
        writer,
        take_first_snapshot_immediately,
    );
    tokio::try_join!(event_loop, snapshot_loop)?;
    Ok(())
}

struct Calibration {
    video_offset_ms: i64,
}

#[derive(Debug, Clone, Copy)]
struct ClockSample {
    received_at: Instant,
    game_time_seconds: f64,
}

#[derive(Debug, Default)]
struct CalibrationWindow {
    samples: VecDeque<ClockSample>,
}

impl CalibrationWindow {
    fn push(&mut self, sample: ClockSample, video_started_at: Instant) -> Option<i64> {
        if self
            .samples
            .back()
            .is_some_and(|previous| sample.game_time_seconds <= previous.game_time_seconds)
        {
            self.samples.clear();
        }
        self.samples.push_back(sample);
        while self.samples.len() > CALIBRATION_SAMPLE_COUNT {
            self.samples.pop_front();
        }
        if self.samples.len() < CALIBRATION_SAMPLE_COUNT {
            return None;
        }

        let offset = calibration_offset(&self.samples, video_started_at);
        if offset.is_none() {
            self.samples.clear();
        }
        offset
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

async fn calibrate(
    client: &LiveClient,
    video_started_at: Instant,
    cancellation: &mut watch::Receiver<bool>,
    state: &Arc<Mutex<PollerState>>,
) -> Result<Option<Calibration>> {
    let mut window = CalibrationWindow::default();

    loop {
        record_request(state, RequestKind::Calibration).await;
        let response = tokio::select! {
            _ = cancelled(cancellation) => return Ok(None),
            response = client.game_stats() => response,
        };

        match response {
            Ok(received) => {
                state
                    .lock()
                    .await
                    .diagnostics
                    .observe_response(received.latency);
                let Some(game_time_seconds) = valid_game_time(&received.value) else {
                    window.clear();
                    sleep_or_cancel(CALIBRATION_PROBE_INTERVAL, cancellation).await;
                    continue;
                };
                if let Some(video_offset_ms) = window.push(
                    ClockSample {
                        received_at: received.received_at,
                        game_time_seconds,
                    },
                    video_started_at,
                ) {
                    return Ok(Some(Calibration { video_offset_ms }));
                }
                sleep_or_cancel(CALIBRATION_PROBE_INTERVAL, cancellation).await;
            }
            Err(_) => {
                window.clear();
                sleep_or_cancel(CALIBRATION_RETRY_INTERVAL, cancellation).await;
            }
        }
    }
}

fn calibration_offset(samples: &VecDeque<ClockSample>, video_started_at: Instant) -> Option<i64> {
    if samples.len() != CALIBRATION_SAMPLE_COUNT {
        return None;
    }
    let first = samples.front()?;
    let last = samples.back()?;
    let window_span = last.received_at.checked_duration_since(first.received_at)?;
    if window_span < CALIBRATION_MIN_WINDOW_SPAN {
        return None;
    }

    let tolerance_ms = CALIBRATION_CLOCK_TOLERANCE.as_secs_f64() * 1000.0;
    for (previous, current) in samples.iter().zip(samples.iter().skip(1)) {
        let monotonic_delta_ms = current
            .received_at
            .checked_duration_since(previous.received_at)?
            .as_secs_f64()
            * 1000.0;
        let game_delta_ms = (current.game_time_seconds - previous.game_time_seconds) * 1000.0;
        if game_delta_ms <= 0.0 || (game_delta_ms - monotonic_delta_ms).abs() > tolerance_ms {
            return None;
        }
    }

    let mut candidates = samples
        .iter()
        .map(|sample| {
            let elapsed_ms = sample
                .received_at
                .checked_duration_since(video_started_at)?
                .as_secs_f64()
                * 1000.0;
            Some((elapsed_ms - sample.game_time_seconds * 1000.0).round() as i64)
        })
        .collect::<Option<Vec<_>>>()?;
    let minimum = *candidates.iter().min()?;
    let maximum = *candidates.iter().max()?;
    if maximum.saturating_sub(minimum) > duration_ms(CALIBRATION_CLOCK_TOLERANCE) as i64 {
        return None;
    }
    median(&mut candidates).ok()
}

async fn fetch_initial_snapshot(
    client: &LiveClient,
    cancellation: &mut watch::Receiver<bool>,
    state: &Arc<Mutex<PollerState>>,
) -> Option<RawSnapshotData> {
    let mut consecutive_failures = 0;
    loop {
        record_request(state, RequestKind::Snapshot).await;
        let response = tokio::select! {
            _ = cancelled(cancellation) => return None,
            response = client.snapshot_data() => response,
        };
        if let Ok(received) = response {
            state
                .lock()
                .await
                .diagnostics
                .observe_response(received.latency);
            if valid_game_time(&received.value.game_data).is_some() {
                return Some(received.value);
            }
        }

        if record_failure(&mut consecutive_failures) {
            return None;
        }
        sleep_or_cancel(API_FAILURE_RETRY_INTERVAL, cancellation).await;
        if *cancellation.borrow() {
            return None;
        }
    }
}

async fn event_loop(
    client: LiveClient,
    video_offset_ms: i64,
    mut cancellation: watch::Receiver<bool>,
    state: Arc<Mutex<PollerState>>,
    writer: watch::Sender<u64>,
) -> Result<()> {
    let mut interval = tokio::time::interval(EVENT_POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut consecutive_failures = 0;

    loop {
        tokio::select! {
            _ = cancelled(&mut cancellation) => return Ok(()),
            _ = interval.tick() => {}
        }
        record_request(&state, RequestKind::Event).await;
        let response = tokio::select! {
            _ = cancelled(&mut cancellation) => return Ok(()),
            response = client.event_data() => response,
        };
        let received = match response {
            Ok(received) => {
                consecutive_failures = 0;
                received
            }
            Err(error) => {
                if record_failure(&mut consecutive_failures) {
                    state.lock().await.diagnostics.event_failure_reason = Some(error.to_string());
                    return Ok(());
                }
                continue;
            }
        };

        let (count, revision) = {
            let mut state = state.lock().await;
            state.diagnostics.observe_response(received.latency);
            let count = reconcile_events(&mut state, received.value.events, video_offset_ms);
            let revision = (count > 0).then(|| state.mark_dirty());
            (count, revision)
        };
        if let Some(revision) = revision {
            notify_game_log_writer(&writer, revision)?;
            debug!(count, "stored Live Client event batch");
        }
    }
}

async fn snapshot_loop(
    client: LiveClient,
    video_offset_ms: i64,
    mut cancellation: watch::Receiver<bool>,
    state: Arc<Mutex<PollerState>>,
    writer: watch::Sender<u64>,
    take_first_snapshot_immediately: bool,
) -> Result<()> {
    let start = if take_first_snapshot_immediately {
        tokio::time::Instant::now()
    } else {
        tokio::time::Instant::now() + SNAPSHOT_INTERVAL
    };
    let mut interval = tokio::time::interval_at(start, SNAPSHOT_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut consecutive_failures = 0;

    loop {
        tokio::select! {
            _ = cancelled(&mut cancellation) => return Ok(()),
            _ = interval.tick() => {}
        }

        loop {
            record_request(&state, RequestKind::Snapshot).await;
            let response = tokio::select! {
                _ = cancelled(&mut cancellation) => return Ok(()),
                response = client.snapshot_data() => response,
            };
            let received = match response {
                Ok(received) => received,
                Err(error) => {
                    if record_failure(&mut consecutive_failures) {
                        state.lock().await.diagnostics.snapshot_failure_reason =
                            Some(error.to_string());
                        return Ok(());
                    }
                    sleep_or_cancel(API_FAILURE_RETRY_INTERVAL, &mut cancellation).await;
                    if *cancellation.borrow() {
                        return Ok(());
                    }
                    continue;
                }
            };
            {
                let mut state = state.lock().await;
                state.diagnostics.observe_response(received.latency);
            }
            let mut data = received.value;
            if valid_game_time(&data.game_data).is_none() {
                if record_failure(&mut consecutive_failures) {
                    state.lock().await.diagnostics.snapshot_failure_reason =
                        Some("snapshot game stats returned no valid gameTime".to_owned());
                    return Ok(());
                }
                sleep_or_cancel(API_FAILURE_RETRY_INTERVAL, &mut cancellation).await;
                if *cancellation.borrow() {
                    return Ok(());
                }
                continue;
            }

            consecutive_failures = 0;
            let revision = {
                let mut state = state.lock().await;
                update_summary(&mut state.summary, &data);
                reconcile_events(
                    &mut state,
                    std::mem::take(&mut data.events.events),
                    video_offset_ms,
                );
                append_snapshot(&mut state.game_log, &data, video_offset_ms)?;
                state.mark_dirty()
            };
            notify_game_log_writer(&writer, revision)?;
            break;
        }
    }
}

fn notify_game_log_writer(writer: &watch::Sender<u64>, revision: u64) -> Result<()> {
    writer
        .send(revision)
        .map_err(|_| anyhow::anyhow!("game-log writer stopped before revision {revision}"))
}

async fn run_game_log_writer(
    state: Arc<Mutex<PollerState>>,
    output: PathBuf,
    coalesce_window: Duration,
    mut revisions: watch::Receiver<u64>,
    mut commands: mpsc::Receiver<GameLogWriterCommand>,
) -> Result<()> {
    loop {
        let mut pending_revision = tokio::select! {
            command = commands.recv() => {
                let Some(GameLogWriterCommand::FlushAndStop {
                    required_revision,
                    completed,
                }) = command else {
                    bail!("game-log writer command channel closed before final flush");
                };
                let result =
                    persist_latest_game_log(&state, &output, required_revision, true).await;
                if result.is_ok() {
                    let _ = completed.send(());
                }
                return result;
            }
            changed = revisions.changed() => {
                changed.context("game-log revision channel closed before final flush")?;
                *revisions.borrow_and_update()
            }
        };

        let coalesce_deadline = tokio::time::Instant::now() + coalesce_window;
        let coalesce = tokio::time::sleep_until(coalesce_deadline);
        tokio::pin!(coalesce);
        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(GameLogWriterCommand::FlushAndStop {
                        required_revision,
                        completed,
                    }) = command else {
                        bail!("game-log writer command channel closed before final flush");
                    };
                    let required_revision = required_revision.max(pending_revision);
                    let result =
                        persist_latest_game_log(&state, &output, required_revision, true).await;
                    if result.is_ok() {
                        let _ = completed.send(());
                    }
                    return result;
                }
                changed = revisions.changed() => {
                    changed.context("game-log revision channel closed before final flush")?;
                    pending_revision = pending_revision.max(*revisions.borrow_and_update());
                }
                () = &mut coalesce => break,
            }
        }
        persist_latest_game_log(&state, &output, pending_revision, false).await?;
    }
}

async fn persist_latest_game_log(
    state: &Arc<Mutex<PollerState>>,
    output: &Path,
    required_revision: u64,
    force: bool,
) -> Result<()> {
    let requested_at = Instant::now();
    let clone_started = Instant::now();
    let Some((game_log, revision)) = ({
        let state = state.lock().await;
        ensure!(
            state.revision >= required_revision,
            "game-log writer observed revision {} before required revision {required_revision}",
            state.revision
        );
        (force || state.durable_revision < required_revision)
            .then(|| (state.game_log.clone(), state.revision))
    }) else {
        return Ok(());
    };
    let clone_elapsed = clone_started.elapsed();
    let result = write_json_atomic_with_stats(output, &game_log).await;
    let elapsed = requested_at.elapsed();
    let mut state = state.lock().await;
    match &result {
        Ok(stats) => {
            let newly_durable = revision.saturating_sub(state.durable_revision);
            state.diagnostics.json_coalesced_writes = state
                .diagnostics
                .json_coalesced_writes
                .saturating_add(newly_durable.saturating_sub(1));
            state.durable_revision = state.durable_revision.max(revision);
            state
                .diagnostics
                .observe_json_write(clone_elapsed, elapsed, Some(*stats));
        }
        Err(_) => state
            .diagnostics
            .observe_json_write(clone_elapsed, elapsed, None),
    }
    result.map(|_| ())
}

fn append_snapshot(
    game_log: &mut GameLog,
    raw: &RawSnapshotData,
    video_offset_ms: i64,
) -> Result<()> {
    if raw.all_players.is_empty() {
        return Ok(());
    }
    let first_snapshot = game_log.snapshots.is_empty();
    let snapshot = make_snapshot(raw, first_snapshot)?;
    if let Some(previous) = game_log.snapshots.last() {
        game_log.snapshot_derived_changes.extend(diff_snapshots(
            previous,
            &snapshot,
            video_offset_ms,
        ));
    }
    game_log.snapshots.push(snapshot);
    Ok(())
}

fn make_snapshot(raw: &RawSnapshotData, include_stable_fields: bool) -> Result<Snapshot> {
    let game_time_ms = seconds_to_ms(
        valid_game_time(&raw.game_data).context("snapshot does not contain a valid game time")?,
    )?;
    let active_name = raw.active_player.summoner_name.as_deref();
    let active_riot_id = raw.active_player.riot_id.as_deref();
    let mut players = Vec::with_capacity(raw.all_players.len());

    for player in &raw.all_players {
        let is_active = active_name.is_some_and(|name| name == player.summoner_name)
            || active_riot_id
                .zip(player.riot_id.as_deref())
                .is_some_and(|(active, candidate)| active == candidate);
        let champion_stats = is_active
            .then_some(raw.active_player.champion_stats.as_ref())
            .flatten();
        let summoner_spells = include_stable_fields
            .then(|| {
                [
                    player.summoner_spells.spell_one.as_ref(),
                    player.summoner_spells.spell_two.as_ref(),
                ]
                .into_iter()
                .flatten()
                .filter_map(canonical_spell_id)
                .collect::<Vec<_>>()
            })
            .filter(|spells| !spells.is_empty());
        let keystone_id = include_stable_fields
            .then_some(player.runes.keystone.as_ref().map(|rune| rune.id))
            .flatten();
        let rune_ids = (include_stable_fields && is_active)
            .then(|| {
                raw.active_player
                    .full_runes
                    .general_runes
                    .iter()
                    .chain(&raw.active_player.full_runes.stat_runes)
                    .map(|rune| rune.id)
                    .collect::<Vec<_>>()
            })
            .filter(|runes| !runes.is_empty());

        players.push(PlayerSnapshot {
            summoner_name: player.summoner_name.clone(),
            team: player.team.clone(),
            champion: player.champion_name.clone(),
            gold: is_active
                .then_some(raw.active_player.current_gold.map(round_number))
                .flatten(),
            hp: champion_stats
                .and_then(|stats| stats.current_health)
                .map(round_number),
            hp_max: champion_stats
                .and_then(|stats| stats.max_health)
                .map(round_number),
            cs: player.scores.creep_score,
            level: player.level,
            items: player
                .items
                .iter()
                .map(|item| ItemSnapshot {
                    item_id: item.item_id,
                    slot: item.slot,
                    count: item.count,
                })
                .collect(),
            summoner_spells,
            keystone_id,
            rune_ids,
        });
    }

    Ok(Snapshot {
        game_time_ms,
        players,
    })
}

fn diff_snapshots(
    previous: &Snapshot,
    current: &Snapshot,
    video_offset_ms: i64,
) -> Vec<SnapshotChange> {
    let previous_players: HashMap<&str, &PlayerSnapshot> = previous
        .players
        .iter()
        .map(|player| (player.summoner_name.as_str(), player))
        .collect();
    let video_time_ms = video_offset_ms.saturating_add(current.game_time_ms);
    let mut changes = Vec::new();

    for player in &current.players {
        let Some(previous) = previous_players.get(player.summoner_name.as_str()) else {
            continue;
        };
        let old_items = item_quantities(&previous.items);
        let new_items = item_quantities(&player.items);

        for (item_id, count) in &new_items {
            if *count > old_items.get(item_id).copied().unwrap_or_default() {
                changes.push(SnapshotChange {
                    game_time_ms: current.game_time_ms,
                    player: player.summoner_name.clone(),
                    change_type: SnapshotChangeType::ItemPurchased,
                    item_id: Some(*item_id),
                    new_level: None,
                    video_time_ms,
                });
            }
        }
        for (item_id, count) in &old_items {
            if *count > new_items.get(item_id).copied().unwrap_or_default() {
                changes.push(SnapshotChange {
                    game_time_ms: current.game_time_ms,
                    player: player.summoner_name.clone(),
                    change_type: SnapshotChangeType::ItemSold,
                    item_id: Some(*item_id),
                    new_level: None,
                    video_time_ms,
                });
            }
        }
        if player.level > previous.level {
            changes.push(SnapshotChange {
                game_time_ms: current.game_time_ms,
                player: player.summoner_name.clone(),
                change_type: SnapshotChangeType::LevelUp,
                item_id: None,
                new_level: Some(player.level),
                video_time_ms,
            });
        }
    }
    changes
}

fn item_quantities(items: &[ItemSnapshot]) -> HashMap<u32, u32> {
    let mut quantities = HashMap::new();
    for item in items {
        *quantities.entry(item.item_id).or_default() += item.count.max(1);
    }
    quantities
}

fn normalize_event(raw: RawEvent, video_offset_ms: i64) -> Result<GameEvent> {
    let game_time_ms = seconds_to_ms(raw.event_time)?;
    Ok(GameEvent {
        event_type: raw.event_name,
        game_time_ms,
        video_time_ms: video_offset_ms.saturating_add(game_time_ms),
        killer: string_field(&raw.fields, "KillerName"),
        victim: string_field(&raw.fields, "VictimName"),
        assisters: string_list_field(&raw.fields, "Assisters"),
        turret: string_field(&raw.fields, "TurretKilled"),
        inhibitor: string_field(&raw.fields, "InhibKilled")
            .or_else(|| string_field(&raw.fields, "InhibRespawningSoon"))
            .or_else(|| string_field(&raw.fields, "InhibRespawned")),
        dragon_type: string_field(&raw.fields, "DragonType"),
        stolen: bool_field(&raw.fields, "Stolen"),
        kill_streak: u32_field(&raw.fields, "KillStreak"),
        acer: string_field(&raw.fields, "Acer"),
        acing_team: string_field(&raw.fields, "AcingTeam"),
        result: string_field(&raw.fields, "Result"),
    })
}

fn reconcile_events(
    state: &mut PollerState,
    raw_events: Vec<RawEvent>,
    video_offset_ms: i64,
) -> usize {
    let mut new_events = Vec::new();
    for raw_event in raw_events {
        if !state.seen_event_ids.insert(raw_event.event_id) {
            continue;
        }
        match normalize_event(raw_event, video_offset_ms) {
            Ok(event) => new_events.push(event),
            Err(error) => warn!(%error, "ignoring invalid Live Client event"),
        }
    }
    let count = new_events.len();
    if count > 0 {
        state.game_log.events.extend(new_events);
        state
            .game_log
            .events
            .sort_by_key(|event| event.game_time_ms);
    }
    count
}

fn update_summary(summary: &mut PollerSummary, data: &RawSnapshotData) {
    if summary.game_mode.is_none() {
        summary.game_mode = data.game_data.game_mode.clone();
    }
    if summary.local_player_summoner_name.is_none() {
        summary.local_player_summoner_name = data.active_player.summoner_name.clone();
    }
    let local_player = data.all_players.iter().find(|player| {
        data.active_player
            .summoner_name
            .as_deref()
            .is_some_and(|name| name == player.summoner_name)
            || data
                .active_player
                .riot_id
                .as_deref()
                .zip(player.riot_id.as_deref())
                .is_some_and(|(active, candidate)| active == candidate)
    });
    if let Some(player) = local_player {
        if summary.local_player_champion.is_none() {
            summary.local_player_champion = Some(player.champion_name.clone());
        }
        if summary.local_player_team.is_none() {
            summary.local_player_team = Some(player.team.clone());
        }
    }
}

fn canonical_spell_id(spell: &RawSpell) -> Option<String> {
    let marker = "SummonerSpell_";
    spell
        .raw_display_name
        .split_once(marker)
        .and_then(|(_, suffix)| suffix.split_once("_DisplayName"))
        .map(|(identifier, _)| identifier.to_owned())
        .or_else(|| (!spell.display_name.is_empty()).then(|| spell.display_name.clone()))
}

fn string_field(fields: &Map<String, Value>, name: &str) -> Option<String> {
    fields
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn string_list_field(fields: &Map<String, Value>, name: &str) -> Option<Vec<String>> {
    let values = fields.get(name)?.as_array()?;
    Some(
        values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    )
}

fn bool_field(fields: &Map<String, Value>, name: &str) -> Option<bool> {
    let value = fields.get(name)?;
    value
        .as_bool()
        .or_else(|| value.as_str().and_then(|text| text.parse::<bool>().ok()))
}

fn u32_field(fields: &Map<String, Value>, name: &str) -> Option<u32> {
    fields
        .get(name)?
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
}

fn valid_game_time(data: &RawGameData) -> Option<f64> {
    data.game_time
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn record_failure(consecutive_failures: &mut u8) -> bool {
    *consecutive_failures = consecutive_failures.saturating_add(1);
    *consecutive_failures >= MAX_CONSECUTIVE_API_FAILURES
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn duration_ms_f64(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn seconds_to_ms(seconds: f64) -> Result<i64> {
    if !seconds.is_finite() || seconds < 0.0 || seconds > i64::MAX as f64 / 1000.0 {
        bail!("invalid game time {seconds}");
    }
    Ok((seconds * 1000.0).round() as i64)
}

fn median(values: &mut [i64]) -> Result<i64> {
    if values.is_empty() {
        bail!("cannot calculate a median from zero samples");
    }
    values.sort_unstable();
    Ok(values[values.len() / 2])
}

fn round_number(value: f64) -> i64 {
    value.round() as i64
}

async fn cancelled(receiver: &mut watch::Receiver<bool>) {
    if *receiver.borrow() {
        return;
    }
    let _ = receiver.changed().await;
}

async fn sleep_or_cancel(duration: Duration, receiver: &mut watch::Receiver<bool>) {
    tokio::select! {
        _ = tokio::time::sleep(duration) => {}
        _ = cancelled(receiver) => {}
    }
}

#[derive(Debug, Clone, Default)]
struct RawSnapshotData {
    active_player: RawActivePlayer,
    all_players: Vec<RawPlayer>,
    game_data: RawGameData,
    events: RawEventData,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawActivePlayer {
    #[serde(rename = "currentGold")]
    current_gold: Option<f64>,
    #[serde(rename = "summonerName")]
    summoner_name: Option<String>,
    #[serde(rename = "riotId")]
    riot_id: Option<String>,
    #[serde(rename = "championStats")]
    champion_stats: Option<RawChampionStats>,
    #[serde(rename = "fullRunes", default)]
    full_runes: RawFullRunes,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawChampionStats {
    #[serde(rename = "currentHealth")]
    current_health: Option<f64>,
    #[serde(rename = "maxHealth")]
    max_health: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawFullRunes {
    #[serde(rename = "generalRunes", default)]
    general_runes: Vec<RawRune>,
    #[serde(rename = "statRunes", default)]
    stat_runes: Vec<RawRune>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawPlayer {
    #[serde(rename = "summonerName", default)]
    summoner_name: String,
    #[serde(rename = "riotId")]
    riot_id: Option<String>,
    #[serde(rename = "championName", default)]
    champion_name: String,
    #[serde(default)]
    team: String,
    #[serde(default)]
    level: u32,
    #[serde(default, deserialize_with = "deserialize_items")]
    items: Vec<RawItem>,
    #[serde(default)]
    scores: RawScores,
    #[serde(default)]
    runes: RawRunes,
    #[serde(rename = "summonerSpells", default)]
    summoner_spells: RawSummonerSpells,
}

#[derive(Debug, Clone, Deserialize)]
struct RawItem {
    #[serde(rename = "itemID", default)]
    item_id: u32,
    #[serde(default)]
    slot: u32,
    #[serde(default)]
    count: u32,
}

fn deserialize_items<'de, D>(deserializer: D) -> std::result::Result<Vec<RawItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Array(items) => {
            serde_json::from_value(Value::Array(items)).map_err(serde::de::Error::custom)
        }
        Value::Object(error) if error.contains_key("error") => Ok(Vec::new()),
        other => Err(serde::de::Error::custom(format!(
            "expected an item array or API error object, got {other}"
        ))),
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawScores {
    #[serde(rename = "creepScore", default)]
    creep_score: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawRunes {
    keystone: Option<RawRune>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRune {
    id: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawSummonerSpells {
    #[serde(rename = "summonerSpellOne")]
    spell_one: Option<RawSpell>,
    #[serde(rename = "summonerSpellTwo")]
    spell_two: Option<RawSpell>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSpell {
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(rename = "rawDisplayName", default)]
    raw_display_name: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawGameData {
    #[serde(rename = "gameTime")]
    game_time: Option<f64>,
    #[serde(rename = "gameMode")]
    game_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawEventData {
    #[serde(rename = "Events", default)]
    events: Vec<RawEvent>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawEvent {
    #[serde(rename = "EventID")]
    event_id: i64,
    #[serde(rename = "EventName")]
    event_name: String,
    #[serde(rename = "EventTime")]
    event_time: f64,
    #[serde(flatten)]
    fields: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    {
      "activePlayer": {
        "currentGold": 578.24,
        "summonerName": "Local#EUW",
        "riotId": "Local#EUW",
        "championStats": { "currentHealth": 901.6, "maxHealth": 1089.6 },
        "fullRunes": {
          "generalRunes": [{"id": 8369}, {"id": 8321}],
          "statRunes": [{"id": 5005}, {"id": 5008}, {"id": 5011}]
        }
      },
      "allPlayers": [
        {
          "summonerName": "Local#EUW",
          "riotId": "Local#EUW",
          "championName": "LeBlanc",
          "team": "ORDER",
          "level": 6,
          "items": [{"itemID": 3340, "slot": 6, "count": 1}],
          "scores": {"creepScore": 42},
          "runes": {"keystone": {"id": 8369}},
          "summonerSpells": {
            "summonerSpellOne": {
              "displayName": "Flash",
              "rawDisplayName": "GeneratedTip_SummonerSpell_SummonerFlash_DisplayName"
            },
            "summonerSpellTwo": {
              "displayName": "Ignite",
              "rawDisplayName": "GeneratedTip_SummonerSpell_SummonerDot_DisplayName"
            }
          }
        },
        {
          "summonerName": "Other#EUW",
          "riotId": "Other#EUW",
          "championName": "Jinx",
          "team": "CHAOS",
          "level": 5,
          "items": [],
          "scores": {"creepScore": 38},
          "runes": {"keystone": {"id": 8005}},
          "summonerSpells": {}
        }
      ],
      "gameData": {"gameMode": "CLASSIC", "gameTime": 220.125}
    }
    "#;

    #[tokio::test]
    async fn game_log_write_records_revision_and_atomic_io_costs() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join(GAME_LOG_JSON);
        let state = Arc::new(Mutex::new(PollerState::default()));
        let mut writer = GameLogWriter::start(Arc::clone(&state), output.clone());
        let revision = {
            let mut state = state.lock().await;
            state.game_log.game_start_video_offset_ms = Some(42);
            state.mark_dirty()
        };

        notify_game_log_writer(&writer.notifier(), revision).unwrap();
        writer.flush_and_stop(revision).await.unwrap();

        let state = state.lock().await;
        assert_eq!(state.revision, 1);
        assert_eq!(state.durable_revision, 1);
        assert_eq!(state.diagnostics.json_write_requests, 1);
        assert_eq!(state.diagnostics.json_writes, 1);
        assert_eq!(state.diagnostics.json_write_failures, 0);
        assert!(state.diagnostics.json_serialized_bytes > 0);
        assert!(output.is_file());
    }

    #[tokio::test]
    async fn game_log_writer_coalesces_concurrent_revisions_to_the_newest_state() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join(GAME_LOG_JSON);
        let state = Arc::new(Mutex::new(PollerState::default()));
        let mut writer = GameLogWriter::start_with_window(
            Arc::clone(&state),
            output.clone(),
            Duration::from_millis(10),
        );
        let notifier = writer.notifier();

        let first_revision = {
            let mut state = state.lock().await;
            state.game_log.game_start_video_offset_ms = Some(1);
            state.mark_dirty()
        };
        notify_game_log_writer(&notifier, first_revision).unwrap();
        let second_revision = {
            let mut state = state.lock().await;
            state.game_log.game_start_video_offset_ms = Some(2);
            state.mark_dirty()
        };
        notify_game_log_writer(&notifier, second_revision).unwrap();

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if state.lock().await.durable_revision >= second_revision {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("coalesced game-log revision did not become durable");

        {
            let state = state.lock().await;
            assert_eq!(state.diagnostics.json_write_requests, 2);
            assert_eq!(state.diagnostics.json_writes, 1);
            assert_eq!(state.diagnostics.json_coalesced_writes, 1);
        }
        let stored: GameLog =
            serde_json::from_slice(&tokio::fs::read(&output).await.unwrap()).unwrap();
        assert_eq!(stored.game_start_video_offset_ms, Some(2));
        writer.flush_and_stop(second_revision).await.unwrap();
    }

    #[tokio::test]
    async fn game_log_writer_final_flush_bypasses_the_coalescing_delay() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join(GAME_LOG_JSON);
        let state = Arc::new(Mutex::new(PollerState::default()));
        let mut writer = GameLogWriter::start_with_window(
            Arc::clone(&state),
            output.clone(),
            Duration::from_secs(60),
        );
        let revision = {
            let mut state = state.lock().await;
            state.game_log.game_start_video_offset_ms = Some(42);
            state.mark_dirty()
        };
        notify_game_log_writer(&writer.notifier(), revision).unwrap();

        tokio::time::timeout(Duration::from_secs(1), writer.flush_and_stop(revision))
            .await
            .expect("final game-log flush waited for the coalescing window")
            .unwrap();

        let state = state.lock().await;
        assert_eq!(state.durable_revision, revision);
        assert_eq!(state.diagnostics.json_writes, 1);
        drop(state);
        let stored: GameLog =
            serde_json::from_slice(&tokio::fs::read(output).await.unwrap()).unwrap();
        assert_eq!(stored.game_start_video_offset_ms, Some(42));
    }

    #[tokio::test]
    async fn game_log_writer_failure_is_latched_and_returned() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("missing-parent").join(GAME_LOG_JSON);
        let state = Arc::new(Mutex::new(PollerState::default()));
        let mut writer =
            GameLogWriter::start_with_window(Arc::clone(&state), output, Duration::from_millis(1));
        let revision = state.lock().await.mark_dirty();
        notify_game_log_writer(&writer.notifier(), revision).unwrap();

        let error = writer.flush_and_stop(revision).await.unwrap_err();

        assert!(format!("{error:#}").contains("failed to create temporary file"));
        let state = state.lock().await;
        assert_eq!(state.durable_revision, 0);
        assert_eq!(state.diagnostics.json_write_failures, 1);
    }

    #[tokio::test]
    #[ignore = "explicit Package 6 disk profile; rewrites a synthetic 50-minute game log"]
    async fn profile_pre_coalescing_representative_long_synthetic_game_log() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join(GAME_LOG_JSON);
        let state = Arc::new(Mutex::new(PollerState::default()));
        let mut write_latencies = Vec::new();

        for snapshot_index in 0..300_u32 {
            if let Some(revision) = append_synthetic_profile_event(&state, snapshot_index).await {
                let started = Instant::now();
                persist_latest_game_log(&state, &output, revision, false)
                    .await
                    .unwrap();
                write_latencies.push(started.elapsed());
            }

            let revision = append_synthetic_profile_snapshot(&state, snapshot_index).await;
            let started = Instant::now();
            persist_latest_game_log(&state, &output, revision, false)
                .await
                .unwrap();
            write_latencies.push(started.elapsed());
        }

        write_latencies.sort_unstable();
        let state = state.lock().await;
        let final_bytes = std::fs::metadata(&output).unwrap().len();
        let p95_index = write_latencies.len().saturating_mul(95).div_ceil(100) - 1;
        let p95 = write_latencies[p95_index];
        let rewrite_ratio = state.diagnostics.json_serialized_bytes as f64 / final_bytes as f64;
        println!(
            "PACKAGE6_POLLER_PROFILE writes={} final_bytes={} cumulative_bytes={} rewrite_ratio={rewrite_ratio:.3} p95_ms={:.3} slowest_ms={:.3} clone_ms={:.3} serialize_ms={:.3} file_write_ms={:.3} sync_ms={:.3} rename_ms={:.3} total_atomic_ms={:.3} total_requested_ms={:.3}",
            state.diagnostics.json_writes,
            final_bytes,
            state.diagnostics.json_serialized_bytes,
            duration_ms_f64(p95),
            duration_ms_f64(state.diagnostics.slowest_json_write),
            duration_ms_f64(state.diagnostics.total_json_clone),
            duration_ms_f64(state.diagnostics.total_json_serialize),
            duration_ms_f64(state.diagnostics.total_json_file_write),
            duration_ms_f64(state.diagnostics.total_json_sync),
            duration_ms_f64(state.diagnostics.total_json_rename),
            duration_ms_f64(state.diagnostics.total_json_atomic),
            duration_ms_f64(state.diagnostics.total_json_write),
        );
        assert_eq!(state.durable_revision, state.revision);
        assert_eq!(state.diagnostics.json_write_failures, 0);
    }

    #[tokio::test]
    #[ignore = "explicit Package 6 disk profile; writes a coalesced synthetic 50-minute game log"]
    async fn profile_coalesced_representative_long_synthetic_game_log() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join(GAME_LOG_JSON);
        let state = Arc::new(Mutex::new(PollerState::default()));
        let mut writer = GameLogWriter::start_with_window(
            Arc::clone(&state),
            output.clone(),
            Duration::from_millis(25),
        );
        let notifier = writer.notifier();
        let mut cycle_latencies = Vec::new();
        let mut final_revision = 0_u64;

        for snapshot_index in 0..300_u32 {
            let cycle_started = Instant::now();
            if let Some(revision) = append_synthetic_profile_event(&state, snapshot_index).await {
                notify_game_log_writer(&notifier, revision).unwrap();
            }
            final_revision = append_synthetic_profile_snapshot(&state, snapshot_index).await;
            notify_game_log_writer(&notifier, final_revision).unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if state.lock().await.durable_revision >= final_revision {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("synthetic coalesced revision did not become durable");
            cycle_latencies.push(cycle_started.elapsed());
        }
        writer.flush_and_stop(final_revision).await.unwrap();

        cycle_latencies.sort_unstable();
        let state = state.lock().await;
        let final_bytes = std::fs::metadata(&output).unwrap().len();
        let p95_index = cycle_latencies.len().saturating_mul(95).div_ceil(100) - 1;
        let p95 = cycle_latencies[p95_index];
        let rewrite_ratio = state.diagnostics.json_serialized_bytes as f64 / final_bytes as f64;
        println!(
            "PACKAGE6_POLLER_COALESCED_PROFILE requests={} writes={} coalesced={} final_bytes={} cumulative_bytes={} rewrite_ratio={rewrite_ratio:.3} cycle_p95_ms={:.3} slowest_write_ms={:.3} clone_ms={:.3} serialize_ms={:.3} file_write_ms={:.3} sync_ms={:.3} rename_ms={:.3} total_atomic_ms={:.3} total_requested_ms={:.3}",
            state.diagnostics.json_write_requests,
            state.diagnostics.json_writes,
            state.diagnostics.json_coalesced_writes,
            final_bytes,
            state.diagnostics.json_serialized_bytes,
            duration_ms_f64(p95),
            duration_ms_f64(state.diagnostics.slowest_json_write),
            duration_ms_f64(state.diagnostics.total_json_clone),
            duration_ms_f64(state.diagnostics.total_json_serialize),
            duration_ms_f64(state.diagnostics.total_json_file_write),
            duration_ms_f64(state.diagnostics.total_json_sync),
            duration_ms_f64(state.diagnostics.total_json_rename),
            duration_ms_f64(state.diagnostics.total_json_atomic),
            duration_ms_f64(state.diagnostics.total_json_write),
        );
        assert_eq!(state.durable_revision, state.revision);
        assert_eq!(state.diagnostics.json_write_requests, 400);
        assert_eq!(state.diagnostics.json_writes, 301);
        assert_eq!(state.diagnostics.json_coalesced_writes, 100);
        assert_eq!(state.diagnostics.json_write_failures, 0);
    }

    async fn append_synthetic_profile_event(
        state: &Arc<Mutex<PollerState>>,
        snapshot_index: u32,
    ) -> Option<u64> {
        if !snapshot_index.is_multiple_of(3) {
            return None;
        }
        let mut state = state.lock().await;
        state.game_log.events.push(GameEvent {
            event_type: "ChampionKill".to_owned(),
            game_time_ms: i64::from(snapshot_index) * 10_000,
            video_time_ms: i64::from(snapshot_index) * 10_000 + 750,
            killer: Some(format!("Player{}", snapshot_index % 10)),
            victim: Some(format!("Player{}", (snapshot_index + 1) % 10)),
            assisters: Some(vec![format!("Player{}", (snapshot_index + 2) % 10)]),
            turret: None,
            inhibitor: None,
            dragon_type: None,
            stolen: None,
            kill_streak: Some(snapshot_index % 5),
            acer: None,
            acing_team: None,
            result: None,
        });
        Some(state.mark_dirty())
    }

    async fn append_synthetic_profile_snapshot(
        state: &Arc<Mutex<PollerState>>,
        snapshot_index: u32,
    ) -> u64 {
        let mut state = state.lock().await;
        state.game_log.snapshots.push(Snapshot {
            game_time_ms: i64::from(snapshot_index) * 10_000,
            players: (0..10_u32)
                .map(|player_index| PlayerSnapshot {
                    summoner_name: format!("Player{player_index}#TEST"),
                    team: if player_index < 5 { "ORDER" } else { "CHAOS" }.to_owned(),
                    champion: format!("Champion{player_index}"),
                    gold: (player_index == 0).then_some(500 + i64::from(snapshot_index) * 7),
                    hp: (player_index == 0).then_some(900),
                    hp_max: (player_index == 0).then_some(1_200),
                    cs: snapshot_index.saturating_add(player_index),
                    level: 1 + snapshot_index / 30,
                    items: vec![ItemSnapshot {
                        item_id: 3_000 + player_index,
                        slot: player_index % 6,
                        count: 1,
                    }],
                    summoner_spells: (snapshot_index == 0)
                        .then(|| vec!["SummonerFlash".to_owned(), "SummonerDot".to_owned()]),
                    keystone_id: (snapshot_index == 0).then_some(8_005 + player_index),
                    rune_ids: (snapshot_index == 0)
                        .then(|| vec![8_005 + player_index, 8_100 + player_index]),
                })
                .collect(),
        });
        state
            .game_log
            .snapshot_derived_changes
            .push(SnapshotChange {
                game_time_ms: i64::from(snapshot_index) * 10_000,
                player: format!("Player{}#TEST", snapshot_index % 10),
                change_type: SnapshotChangeType::LevelUp,
                item_id: None,
                new_level: Some(1 + snapshot_index / 30),
                video_time_ms: i64::from(snapshot_index) * 10_000 + 750,
            });
        state.mark_dirty()
    }

    fn snapshot_data_from_aggregate_sample(data: &Value) -> RawSnapshotData {
        RawSnapshotData {
            active_player: serde_json::from_value(data["activePlayer"].clone()).unwrap(),
            all_players: serde_json::from_value(data["allPlayers"].clone()).unwrap(),
            game_data: serde_json::from_value(data["gameData"].clone()).unwrap(),
            events: data
                .get("events")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .unwrap()
                .unwrap_or_default(),
        }
    }

    fn sample_snapshot_data() -> RawSnapshotData {
        let data: Value = serde_json::from_str(SAMPLE).unwrap();
        snapshot_data_from_aggregate_sample(&data)
    }

    #[tokio::test(start_paused = true)]
    async fn poller_stop_aborts_an_unresponsive_task_within_its_deadline() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join(GAME_LOG_JSON);
        let state = Arc::new(Mutex::new(PollerState::default()));
        let (cancellation, _receiver) = watch::channel(false);
        let task = tokio::spawn(std::future::pending::<Result<()>>());
        let session = PollerSession {
            cancellation,
            task,
            writer: GameLogWriter::start(Arc::clone(&state), output.clone()),
            state,
            output: output.clone(),
        };

        let maximum_duration = POLLER_TASK_STOP_TIMEOUT
            + POLLER_ABORT_REAP_TIMEOUT
            + POLLER_FINALIZATION_TIMEOUT
            + Duration::from_secs(1);
        let summary = tokio::time::timeout(maximum_duration, session.stop())
            .await
            .expect("poller shutdown exceeded its complete deadline");

        assert_eq!(summary, PollerSummary::default());
        assert!(output.is_file());
    }

    #[tokio::test]
    async fn dropping_a_poller_session_cannot_orphan_its_task() {
        struct TaskDrop(Option<tokio::sync::oneshot::Sender<()>>);

        impl Drop for TaskDrop {
            fn drop(&mut self) {
                if let Some(sender) = self.0.take() {
                    let _ = sender.send(());
                }
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let (task_dropped, task_dropped_receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _drop_notification = TaskDrop(Some(task_dropped));
            std::future::pending::<Result<()>>().await
        });
        tokio::task::yield_now().await;
        let (cancellation, _receiver) = watch::channel(false);
        let state = Arc::new(Mutex::new(PollerState::default()));
        let output = directory.path().join(GAME_LOG_JSON);
        let session = PollerSession {
            cancellation,
            task,
            writer: GameLogWriter::start(Arc::clone(&state), output.clone()),
            state,
            output,
        };

        drop(session);

        tokio::time::timeout(Duration::from_secs(1), task_dropped_receiver)
            .await
            .expect("aborted poller task was not reaped")
            .expect("poller task drop notification was lost");
    }

    #[test]
    fn snapshot_uses_private_stats_only_for_the_active_player() {
        let raw = sample_snapshot_data();
        let snapshot = make_snapshot(&raw, true).unwrap();

        assert_eq!(snapshot.game_time_ms, 220_125);
        assert_eq!(snapshot.players.len(), 2);
        assert_eq!(snapshot.players[0].gold, Some(578));
        assert_eq!(snapshot.players[0].hp, Some(902));
        assert_eq!(snapshot.players[0].hp_max, Some(1_090));
        assert_eq!(
            snapshot.players[0].rune_ids,
            Some(vec![8369, 8321, 5005, 5008, 5011])
        );
        assert_eq!(
            snapshot.players[0].summoner_spells,
            Some(vec!["SummonerFlash".to_owned(), "SummonerDot".to_owned()])
        );
        assert_eq!(snapshot.players[1].gold, None);
        assert_eq!(snapshot.players[1].hp, None);
        assert_eq!(snapshot.players[1].hp_max, None);
        assert_eq!(snapshot.players[1].rune_ids, None);
    }

    #[test]
    fn stable_fields_are_omitted_after_the_first_snapshot() {
        let raw = sample_snapshot_data();
        let snapshot = make_snapshot(&raw, false).unwrap();
        assert!(snapshot.players.iter().all(|player| {
            player.summoner_spells.is_none()
                && player.keystone_id.is_none()
                && player.rune_ids.is_none()
        }));
    }

    #[test]
    fn snapshot_diff_detects_item_and_level_changes() {
        let raw = sample_snapshot_data();
        let previous = make_snapshot(&raw, true).unwrap();
        let mut current = previous.clone();
        current.game_time_ms = 230_000;
        current.players[0].level = 7;
        current.players[0].items = vec![ItemSnapshot {
            item_id: 3006,
            slot: 0,
            count: 1,
        }];

        let changes = diff_snapshots(&previous, &current, 142_300);
        assert!(changes.iter().any(|change| {
            change.change_type == SnapshotChangeType::ItemPurchased && change.item_id == Some(3006)
        }));
        assert!(changes.iter().any(|change| {
            change.change_type == SnapshotChangeType::ItemSold && change.item_id == Some(3340)
        }));
        assert!(changes.iter().any(|change| {
            change.change_type == SnapshotChangeType::LevelUp && change.new_level == Some(7)
        }));
        assert!(changes.iter().all(|change| change.video_time_ms == 372_300));
    }

    #[test]
    fn event_is_normalized_and_time_synced() {
        let raw: RawEvent = serde_json::from_str(
            r#"{
              "EventID": 17,
              "EventName": "ChampionKill",
              "EventTime": 214.5,
              "KillerName": "Player3",
              "VictimName": "Player7",
              "Assisters": ["Player1"]
            }"#,
        )
        .unwrap();

        let event = normalize_event(raw, 142_300).unwrap();
        assert_eq!(event.event_type, "ChampionKill");
        assert_eq!(event.game_time_ms, 214_500);
        assert_eq!(event.video_time_ms, 356_800);
        assert_eq!(event.killer.as_deref(), Some("Player3"));
        assert_eq!(event.victim.as_deref(), Some("Player7"));
    }

    #[test]
    fn calibration_uses_the_middle_sample() {
        let mut candidates = [142_303, 142_301, 180_000, 142_302, 142_300];
        assert_eq!(median(&mut candidates).unwrap(), 142_302);
    }

    #[test]
    fn frozen_loading_clock_is_rejected_then_advancing_clock_is_calibrated() {
        let video_started_at = Instant::now();
        let frozen = (0..CALIBRATION_SAMPLE_COUNT)
            .map(|index| ClockSample {
                received_at: video_started_at
                    + Duration::from_millis(u64::try_from(index).unwrap() * 250),
                game_time_seconds: 0.018,
            })
            .collect();
        assert_eq!(calibration_offset(&frozen, video_started_at), None);

        let moving = (0..CALIBRATION_SAMPLE_COUNT)
            .map(|index| ClockSample {
                received_at: video_started_at
                    + Duration::from_secs(40)
                    + Duration::from_millis(u64::try_from(index).unwrap() * 250),
                game_time_seconds: 0.050 + index as f64 * 0.250,
            })
            .collect();
        assert_eq!(calibration_offset(&moving, video_started_at), Some(39_950));
    }

    #[test]
    fn calibration_window_resets_on_frozen_backward_and_outlier_samples() {
        let video_started_at = Instant::now();
        let mut window = CalibrationWindow::default();
        let sample = |elapsed_ms, game_time_seconds| ClockSample {
            received_at: video_started_at + Duration::from_millis(elapsed_ms),
            game_time_seconds,
        };

        assert_eq!(window.push(sample(10_000, 0.018), video_started_at), None);
        assert_eq!(window.push(sample(10_250, 0.018), video_started_at), None);
        assert_eq!(window.samples.len(), 1);
        assert_eq!(window.push(sample(10_500, 0.010), video_started_at), None);
        assert_eq!(window.samples.len(), 1);

        window.clear();
        for index in 0..4 {
            assert_eq!(
                window.push(
                    sample(20_000 + index * 250, index as f64 * 0.250),
                    video_started_at,
                ),
                None
            );
        }
        assert_eq!(window.push(sample(21_000, 2.0), video_started_at), None);
        assert!(window.samples.is_empty());

        let mut offset = None;
        for index in 0..CALIBRATION_SAMPLE_COUNT as u64 {
            offset = window.push(
                sample(22_000 + index * 250, 3.0 + index as f64 * 0.250),
                video_started_at,
            );
        }
        assert_eq!(offset, Some(19_000));
    }

    #[test]
    fn api_failure_threshold_requires_three_consecutive_failures() {
        let mut failures = 0;
        assert!(!record_failure(&mut failures));
        assert!(!record_failure(&mut failures));
        assert!(record_failure(&mut failures));

        failures = 0;
        assert!(!record_failure(&mut failures));
    }

    #[test]
    fn event_reconciliation_deduplicates_and_restores_chronological_order() {
        let raw_event = |event_id, event_time| RawEvent {
            event_id,
            event_name: "ChampionKill".to_owned(),
            event_time,
            fields: Map::new(),
        };
        let mut state = PollerState::default();

        assert_eq!(
            reconcile_events(&mut state, vec![raw_event(2, 20.0)], 100),
            1
        );
        assert_eq!(
            reconcile_events(
                &mut state,
                vec![raw_event(1, 10.0), raw_event(2, 20.0)],
                100,
            ),
            1
        );
        assert_eq!(state.game_log.events.len(), 2);
        assert_eq!(state.game_log.events[0].game_time_ms, 10_000);
        assert_eq!(state.game_log.events[1].game_time_ms, 20_000);
        assert_eq!(
            reconcile_events(&mut state, vec![raw_event(1, 10.0)], 100),
            0
        );
    }

    #[tokio::test(start_paused = true)]
    async fn event_poll_interval_is_one_second_without_burst_catch_up() {
        let mut interval = tokio::time::interval(EVENT_POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await;

        tokio::time::advance(Duration::from_millis(999)).await;
        assert!(
            tokio::time::timeout(Duration::ZERO, interval.tick())
                .await
                .is_err()
        );
        tokio::time::advance(Duration::from_millis(1)).await;
        interval.tick().await;

        tokio::time::advance(Duration::from_secs(5)).await;
        interval.tick().await;
        assert!(
            tokio::time::timeout(Duration::ZERO, interval.tick())
                .await
                .is_err()
        );
        tokio::time::advance(EVENT_POLL_INTERVAL).await;
        interval.tick().await;
    }

    #[test]
    fn metadata_keeps_unavailable_live_client_fields_null() {
        let metadata = RecordingMetadata::new(
            SystemTime::UNIX_EPOCH,
            Duration::from_millis(12_345),
            PollerSummary::default(),
            RecordingDetails {
                encoder_used: "nvenc".to_owned(),
                codec: "hevc".to_owned(),
                profile: "high".to_owned(),
                resolution: "1920x1080".to_owned(),
                fps: 60,
                ..RecordingDetails::default()
            },
        )
        .unwrap();
        let json = serde_json::to_value(metadata).unwrap();

        assert_eq!(json["recorded_at"], "1970-01-01T00:00:00Z");
        assert_eq!(json["duration_ms"], 12_345);
        assert!(json["game_mode"].is_null());
        assert!(json["video_offset_ms"].is_null());
        assert_eq!(json["recording_codec"], "hevc");
        assert_eq!(json["recording_profile"], "high");
    }

    #[test]
    fn focused_snapshot_parts_remain_parseable_from_empirical_capture() {
        #[derive(Deserialize)]
        struct Capture {
            samples: Vec<CaptureSample>,
        }

        #[derive(Deserialize)]
        struct CaptureSample {
            data: Value,
        }

        let capture: Capture = serde_json::from_str(include_str!(
            "../../live-client-capture-20260730-110603.json"
        ))
        .unwrap();
        assert_eq!(capture.samples.len(), 25);
        let mut event_count = 0;
        for sample in capture.samples {
            let snapshot_data = snapshot_data_from_aggregate_sample(&sample.data);
            make_snapshot(&snapshot_data, true).unwrap();

            for event in snapshot_data.events.events {
                normalize_event(event, 0).unwrap();
                event_count += 1;
            }
        }
        assert!(event_count > 0);
    }
}
