mod benchmark;
mod clip_export;
mod config;
mod ddragon;
mod library;
mod music;
mod playback_server;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use anyhow::{Context, Result};
use benchmark::{
    BenchmarkEventInput, BenchmarkLaunch, BenchmarkSession, BenchmarkSessionInfo,
    BenchmarkTerminalInput, QueueStats, RecordEventsResult,
};
use clip_export::{ClipExportProgress, ClipExportRequest, ClipExportResult, ClipMusicSource};
use config::{Config, Settings, SettingsUpdate};
use ddragon::DdragonStatus;
use library::{AutoDeleteResult, ClipSummary, GameSummary, PlaybackProbe, StorageUsage};
use music::BuiltInMusicTrack;
use playback_server::{
    BenchmarkRequestTelemetry, MediaRoots, PlaybackMetrics, RequestTelemetrySnapshot, ServerMetrics,
};
use queueback_media_runtime::{MediaTools, RuntimeError};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{Manager, State};

const HEVC_PROBE_VERSION: u32 = 1;

struct AppState {
    config_path: PathBuf,
    config: RwLock<Config>,
    roots: Arc<MediaRoots>,
    playback_origin: String,
    playback_metrics: Arc<PlaybackMetrics>,
    hevc_probe_path: PathBuf,
    ddragon_cache: PathBuf,
    ddragon_status: Arc<RwLock<DdragonStatus>>,
    music_directory: PathBuf,
    media_runtime: MediaRuntimeState,
    benchmark: Option<Arc<BenchmarkSession>>,
    benchmark_requests: Option<Arc<BenchmarkRequestTelemetry>>,
}

#[derive(Clone)]
enum MediaRuntimeState {
    Available(Arc<MediaTools>),
    Unavailable(Arc<RuntimeError>),
}

impl MediaRuntimeState {
    fn available(&self) -> Option<&MediaTools> {
        match self {
            Self::Available(tools) => Some(tools),
            Self::Unavailable(_) => None,
        }
    }

    fn require(&self) -> Result<Arc<MediaTools>, String> {
        match self {
            Self::Available(tools) => Ok(Arc::clone(tools)),
            Self::Unavailable(error) => Err(error.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HevcProbeStatus {
    tested: bool,
    supported: bool,
    probe_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct HevcProbeMarker {
    version: u32,
    supported: bool,
}

#[tauri::command(async)]
fn list_games(state: State<'_, AppState>) -> Result<Vec<GameSummary>, String> {
    let started = Instant::now();
    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event("library_games_requested", serde_json::json!({}))
            .map_err(error_string)?;
    }
    let result = library::list_games(&state.roots.output_directory());
    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event(
                "library_games_ready",
                serde_json::json!({
                    "elapsed_ms": started.elapsed().as_secs_f64() * 1_000.0,
                    "count": result.as_ref().map(Vec::len).unwrap_or(0),
                    "successful": result.is_ok(),
                }),
            )
            .map_err(error_string)?;
    }
    result.map_err(error_string)
}

#[tauri::command(async)]
fn list_clips(state: State<'_, AppState>) -> Result<Vec<ClipSummary>, String> {
    let started = Instant::now();
    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event("library_clips_requested", serde_json::json!({}))
            .map_err(error_string)?;
    }
    let result = library::list_clips(
        &state.roots.output_directory(),
        &state.playback_origin,
        state.media_runtime.available().map(MediaTools::ffprobe),
    );
    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event(
                "library_clips_ready",
                serde_json::json!({
                    "elapsed_ms": started.elapsed().as_secs_f64() * 1_000.0,
                    "count": result.as_ref().map(Vec::len).unwrap_or(0),
                    "successful": result.is_ok(),
                }),
            )
            .map_err(error_string)?;
    }
    result.map_err(error_string)
}

#[tauri::command(async)]
fn get_playback_probe(
    state: State<'_, AppState>,
    game_timestamp: String,
) -> Result<PlaybackProbe, String> {
    let started = Instant::now();
    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event(
                "playback_payload_requested",
                serde_json::json!({ "game_timestamp": game_timestamp }),
            )
            .map_err(error_string)?;
    }
    let result = library::playback_probe(
        &state.roots.output_directory(),
        &state.playback_origin,
        &game_timestamp,
    );
    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event(
                if result.is_ok() {
                    "playback_payload_backend_ready"
                } else {
                    "playback_payload_backend_failed"
                },
                serde_json::json!({
                    "game_timestamp": game_timestamp,
                    "elapsed_ms": started.elapsed().as_secs_f64() * 1_000.0,
                }),
            )
            .map_err(error_string)?;
    }
    result.map_err(error_string)
}

#[tauri::command(async)]
fn save_game(
    state: State<'_, AppState>,
    game_timestamp: String,
    saved: bool,
) -> Result<(), String> {
    ensure_user_mutation_allowed(&state)?;
    library::save_game(&state.roots.output_directory(), &game_timestamp, saved)
        .map_err(error_string)
}

#[tauri::command(async)]
fn delete_game(state: State<'_, AppState>, game_timestamp: String) -> Result<(), String> {
    ensure_user_mutation_allowed(&state)?;
    library::delete_game(&state.roots.output_directory(), &game_timestamp).map_err(error_string)
}

#[tauri::command(async)]
fn delete_clip(state: State<'_, AppState>, clip_filename: String) -> Result<(), String> {
    ensure_user_mutation_allowed(&state)?;
    library::delete_clip(&state.roots.output_directory(), &clip_filename).map_err(error_string)
}

#[tauri::command]
fn list_built_in_music(state: State<'_, AppState>) -> Result<Vec<BuiltInMusicTrack>, String> {
    music::tracks(&state.playback_origin).map_err(error_string)
}

#[tauri::command]
fn prepare_imported_music_preview(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    ensure_user_mutation_allowed(&state)?;
    let path = music::resolve_imported(&path).map_err(error_string)?;
    let token = state.roots.register_imported_music_preview(path);
    Ok(format!("{}/music-preview/{token}", state.playback_origin))
}

#[tauri::command]
async fn export_clip(
    state: State<'_, AppState>,
    request: ClipExportRequest,
    progress: Channel<ClipExportProgress>,
) -> Result<ClipExportResult, String> {
    if state.benchmark.is_some() && matches!(&request.music, ClipMusicSource::File { .. }) {
        return Err("file-based music is disabled in replay benchmark mode".to_owned());
    }
    let output_directory = state.roots.output_directory();
    let music_directory = state.music_directory.clone();
    let media_tools = state.media_runtime.require()?;
    let started = Instant::now();
    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event(
                "export_started",
                serde_json::json!({
                    "game_timestamp": request.game_timestamp,
                    "media_id": request.media_id,
                    "start_frame": request.start_frame,
                    "end_frame_exclusive": request.end_frame_exclusive,
                    "presets": request.presets,
                }),
            )
            .map_err(error_string)?;
    }
    let result = clip_export::export(
        &output_directory,
        &music_directory,
        media_tools.ffmpeg(),
        media_tools.ffprobe(),
        request,
        progress,
    )
    .await;
    if let Some(benchmark) = &state.benchmark {
        let (kind, payload) = match &result {
            Ok(result) => (
                "export_backend_completed",
                serde_json::json!({
                    "elapsed_ms": result.elapsed_ms,
                    "setup_elapsed_ms": result.setup_elapsed_ms,
                    "source_probe_elapsed_ms": result.source_probe_elapsed_ms,
                    "source_probe_strategy": result.source_probe_strategy,
                    "finalize_elapsed_ms": result.finalize_elapsed_ms,
                    "observed_command_ms": started.elapsed().as_secs_f64() * 1_000.0,
                    "total_file_size_bytes": result.total_file_size_bytes,
                    "outputs": result.outputs,
                }),
            ),
            Err(error) => (
                "export_backend_failed",
                serde_json::json!({
                    "observed_command_ms": started.elapsed().as_secs_f64() * 1_000.0,
                    "error": error.to_string(),
                }),
            ),
        };
        benchmark
            .record_app_event(kind, payload)
            .map_err(error_string)?;
    }
    result.map_err(error_string)
}

#[tauri::command(async)]
fn get_storage_usage(state: State<'_, AppState>) -> Result<StorageUsage, String> {
    let started = Instant::now();
    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event("library_storage_requested", serde_json::json!({}))
            .map_err(error_string)?;
    }
    let result = library::storage_usage(&state.roots.output_directory());
    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event(
                "library_storage_ready",
                serde_json::json!({
                    "elapsed_ms": started.elapsed().as_secs_f64() * 1_000.0,
                    "successful": result.is_ok(),
                }),
            )
            .map_err(error_string)?;
    }
    result.map_err(error_string)
}

#[tauri::command(async)]
fn run_auto_delete(state: State<'_, AppState>) -> Result<AutoDeleteResult, String> {
    ensure_user_mutation_allowed(&state)?;
    let days = state
        .config
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .storage
        .auto_delete_days;
    library::run_auto_delete(&state.roots.output_directory(), days).map_err(error_string)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state
        .config
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .settings()
}

#[tauri::command(async)]
fn save_settings(state: State<'_, AppState>, settings: SettingsUpdate) -> Result<Settings, String> {
    ensure_user_mutation_allowed(&state)?;
    let mut next = state
        .config
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    next.apply_settings(settings).map_err(error_string)?;
    let output_directory = next.resolved_output_path().map_err(error_string)?;
    ensure_output_directories(&output_directory).map_err(error_string)?;
    config::save(&state.config_path, &next).map_err(error_string)?;
    state.roots.set_output_directory(output_directory);
    *state
        .config
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = next.clone();
    Ok(next.settings())
}

#[tauri::command(async)]
fn open_output_folder(state: State<'_, AppState>) -> Result<(), String> {
    ensure_user_mutation_allowed(&state)?;
    open_folder(&state.roots.output_directory()).map_err(error_string)
}

#[tauri::command(async)]
fn open_clips_folder(state: State<'_, AppState>) -> Result<(), String> {
    ensure_user_mutation_allowed(&state)?;
    open_folder(&state.roots.output_directory().join("clips")).map_err(error_string)
}

#[tauri::command]
fn get_hevc_probe_status(state: State<'_, AppState>) -> HevcProbeStatus {
    let marker = read_hevc_marker(&state.hevc_probe_path);
    let tested = marker.is_some();
    HevcProbeStatus {
        tested,
        supported: marker.is_some_and(|marker| marker.supported),
        probe_url: format!("{}/probe/hevc.mp4", state.playback_origin),
    }
}

#[tauri::command(async)]
fn record_hevc_probe_result(
    state: State<'_, AppState>,
    supported: bool,
) -> Result<HevcProbeStatus, String> {
    if state.benchmark.is_none() {
        ensure_user_mutation_allowed(&state)?;
    }
    let marker = HevcProbeMarker {
        version: HEVC_PROBE_VERSION,
        supported,
    };
    let mut bytes = serde_json::to_vec_pretty(&marker).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    if let Some(parent) = state.hevc_probe_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    config::write_atomic(&state.hevc_probe_path, &bytes).map_err(error_string)?;

    if state.benchmark.is_none() {
        let mut next = state
            .config
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        next.app.hevc_playback_supported = supported;
        config::save(&state.config_path, &next).map_err(error_string)?;
        *state
            .config
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = next;
    }

    if let Some(benchmark) = &state.benchmark {
        benchmark
            .record_app_event(
                "hevc_probe_completed",
                serde_json::json!({ "supported": supported }),
            )
            .map_err(error_string)?;
    }

    Ok(HevcProbeStatus {
        tested: true,
        supported,
        probe_url: format!("{}/probe/hevc.mp4", state.playback_origin),
    })
}

#[tauri::command]
fn get_ddragon_status(state: State<'_, AppState>) -> DdragonStatus {
    let mut status = state
        .ddragon_status
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(version) = &status.version {
        status.asset_base_url = Some(format!("{}/ddragon/{version}", state.playback_origin));
    }
    status
}

#[tauri::command(async)]
fn resolve_item_name(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<Option<String>, String> {
    let version = state
        .ddragon_status
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .version
        .clone();
    let Some(version) = version else {
        return Ok(None);
    };
    ddragon::resolve_item_name(&state.ddragon_cache, &version, &item_id).map_err(error_string)
}

#[tauri::command]
fn get_playback_server_metrics(state: State<'_, AppState>) -> ServerMetrics {
    state.playback_metrics.snapshot()
}

#[tauri::command]
fn get_replay_benchmark_session(
    state: State<'_, AppState>,
) -> Result<Option<BenchmarkSessionInfo>, String> {
    let Some(session) = &state.benchmark else {
        return Ok(None);
    };
    session
        .record_app_event(
            "frontend_session_requested",
            serde_json::json!({ "observer_profile": session.launch().manifest().observer_profile }),
        )
        .map_err(error_string)?;
    Ok(Some(session.info()))
}

#[tauri::command]
fn record_replay_benchmark_events(
    state: State<'_, AppState>,
    events: Vec<BenchmarkEventInput>,
) -> Result<RecordEventsResult, String> {
    benchmark_session(&state)?
        .record_events(events)
        .map_err(error_string)
}

#[tauri::command]
fn get_replay_benchmark_queue_stats(state: State<'_, AppState>) -> Result<QueueStats, String> {
    Ok(benchmark_session(&state)?.queue_stats())
}

#[tauri::command]
fn flush_replay_benchmark_server_requests(
    state: State<'_, AppState>,
) -> Result<RequestTelemetrySnapshot, String> {
    let session = benchmark_session(&state)?;
    let requests = state
        .benchmark_requests
        .as_ref()
        .ok_or_else(|| "replay benchmark request telemetry is unavailable".to_owned())?;
    let snapshot = requests.take(128);
    session
        .record_server_requests(&snapshot.requests)
        .map_err(error_string)?;
    Ok(snapshot)
}

#[tauri::command]
async fn complete_replay_benchmark(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    terminal: BenchmarkTerminalInput,
) -> Result<(), String> {
    let session = Arc::clone(benchmark_session(&state)?);
    if let Some(requests) = &state.benchmark_requests {
        loop {
            let snapshot = requests.take(128);
            session
                .record_server_requests(&snapshot.requests)
                .map_err(error_string)?;
            if snapshot.pending_records == 0 {
                session
                    .record_app_event(
                        "server_request_reconciliation",
                        serde_json::json!({
                            "capacity": snapshot.capacity,
                            "high_water_mark": snapshot.high_water_mark,
                            "overwritten_records": snapshot.overwritten_records,
                            "active_streams": snapshot.active_streams,
                            "peak_active_streams": snapshot.peak_active_streams,
                        }),
                    )
                    .map_err(error_string)?;
                break;
            }
        }
    }
    session.flush().await?;
    session.finish(terminal).await?;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        app.exit(0);
    });
    Ok(())
}

fn benchmark_session(state: &AppState) -> Result<&Arc<BenchmarkSession>, String> {
    state
        .benchmark
        .as_ref()
        .ok_or_else(|| "replay benchmark mode is not active".to_owned())
}

fn ensure_user_mutation_allowed(state: &AppState) -> Result<(), String> {
    if state.benchmark.is_some() {
        return Err("user-directed mutations are disabled in replay benchmark mode".to_owned());
    }
    Ok(())
}

fn ensure_output_directories(output_directory: &Path) -> Result<()> {
    fs::create_dir_all(output_directory.join("games")).with_context(|| {
        format!(
            "failed to create games directory under {}",
            output_directory.display()
        )
    })?;
    fs::create_dir_all(output_directory.join("clips")).with_context(|| {
        format!(
            "failed to create clips directory under {}",
            output_directory.display()
        )
    })?;
    Ok(())
}

fn read_hevc_marker(path: &Path) -> Option<HevcProbeMarker> {
    let bytes = fs::read(path).ok()?;
    let marker = serde_json::from_slice::<HevcProbeMarker>(&bytes).ok()?;
    (marker.version == HEVC_PROBE_VERSION).then_some(marker)
}

fn error_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(windows)]
fn open_folder(path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("explorer.exe")
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .with_context(|| format!("failed to open {}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_folder(path: &Path) -> Result<()> {
    Command::new("open")
        .arg(path)
        .spawn()
        .with_context(|| format!("failed to open {}", path.display()))?;
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn open_folder(_path: &Path) -> Result<()> {
    anyhow::bail!("opening folders is supported on Windows and macOS")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let process_started_at = Instant::now();
    let harness_started = Instant::now();
    let benchmark_launch = BenchmarkLaunch::from_process_args()
        .unwrap_or_else(|error| panic!("invalid replay benchmark launch: {error:#}"));
    let harness_initialization_ms = harness_started.elapsed().as_secs_f64() * 1_000.0;
    let mut tauri_context = tauri::generate_context!();
    let benchmark_webview = benchmark_launch.as_ref().map(|launch| {
        let window_config = tauri_context
            .config()
            .app
            .windows
            .first()
            .cloned()
            .expect("tauri.conf.json must declare the benchmark window");
        for configured_window in &mut tauri_context.config_mut().app.windows {
            configured_window.create = false;
        }
        (
            window_config,
            launch.app_data_root().join("webview2-user-data"),
        )
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let launch = benchmark_launch.clone();
            let (config_path, config, output_directory, app_data) = match &launch {
                Some(launch) => {
                    let config_path = launch.config_path().to_path_buf();
                    let config = Config::load_or_create(&config_path)?;
                    let output_directory = launch.library_root().to_path_buf();
                    let configured_output = PathBuf::from(&config.storage.output_path)
                        .canonicalize()
                        .context("benchmark config output_path must already exist")?;
                    let expected_output = output_directory
                        .canonicalize()
                        .context("benchmark library root must already exist")?;
                    if configured_output != expected_output || config.storage.auto_delete_days != 0 {
                        return Err(
                            anyhow::anyhow!(
                                "benchmark config must target the manifest library with retention disabled"
                            )
                            .into(),
                        );
                    }
                    (
                        config_path,
                        config,
                        output_directory,
                        launch.app_data_root().to_path_buf(),
                    )
                }
                None => {
                    let config_path = config::default_config_path()?;
                    let config = Config::load_or_create(&config_path)?;
                    let output_directory = config.resolved_output_path()?;
                    let app_data = app
                        .path()
                        .app_local_data_dir()
                        .context("could not resolve the app data directory")?;
                    (config_path, config, output_directory, app_data)
                }
            };
            ensure_output_directories(&output_directory)?;
            if let Err(error) =
                library::run_auto_delete(&output_directory, config.storage.auto_delete_days)
            {
                eprintln!("League Replay auto-delete skipped: {error}");
            }

            let resource_directory = app
                .path()
                .resource_dir()
                .context("could not resolve the app resource directory")?;
            let packaged_runtime = resource_directory
                .join("resources")
                .join(queueback_media_runtime::RUNTIME_DIRECTORY_NAME);
            let media_runtime = match tauri::async_runtime::block_on(
                queueback_media_runtime::resolve(&packaged_runtime),
            ) {
                Ok(tools) => {
                    eprintln!(
                        "League Replay: media runtime {} at {}",
                        tools.runtime_id(),
                        tools.root().display()
                    );
                    MediaRuntimeState::Available(Arc::new(tools))
                }
                Err(error) => {
                    eprintln!("League Replay: {error}");
                    MediaRuntimeState::Unavailable(Arc::new(error))
                }
            };
            let music_directory = music::install(&app_data)?;
            let ddragon_cache = app_data.join("ddragon");
            let roots = Arc::new(MediaRoots::new(output_directory.clone()));
            let playback_metrics = Arc::new(PlaybackMetrics::default());
            let benchmark = launch
                .map(|launch| {
                    BenchmarkSession::start(launch, harness_initialization_ms, process_started_at)
                })
                .transpose()?;
            let benchmark_requests = benchmark
                .as_ref()
                .map(|session| -> Result<_> {
                    let (scenario_id, trial_id) = session.launch().scenario_identity()?;
                    Ok(Arc::new(BenchmarkRequestTelemetry::new(
                        session.launch().manifest().run_id.clone(),
                        scenario_id.to_owned(),
                        trial_id.to_owned(),
                        session.elapsed_ms(),
                    )))
                })
                .transpose()?;
            let playback_origin = tauri::async_runtime::block_on(playback_server::start(
                Arc::clone(&roots),
                Arc::clone(&playback_metrics),
                ddragon_cache.clone(),
                benchmark_requests.as_ref().map(Arc::clone),
                benchmark.is_none(),
            ))?;

            let hevc_probe_path = app_data.join("hevc-probe-v1.json");
            let ddragon_status = Arc::new(RwLock::new(if benchmark.is_some() {
                DdragonStatus::offline(&ddragon_cache)
            } else {
                DdragonStatus::loading(&ddragon_cache)
            }));
            if benchmark.is_none() {
                tauri::async_runtime::spawn(ddragon::initialize(
                    ddragon_cache.clone(),
                    Arc::clone(&ddragon_status),
                ));
            }

            #[cfg(debug_assertions)]
            eprintln!(
                "League Replay: serving {} from {}",
                playback_origin,
                output_directory.display()
            );

            app.manage(AppState {
                config_path,
                config: RwLock::new(config),
                roots,
                playback_origin,
                playback_metrics,
                hevc_probe_path,
                ddragon_cache,
                ddragon_status,
                music_directory,
                media_runtime,
                benchmark: benchmark.clone(),
                benchmark_requests,
            });
            if let Some(benchmark) = &benchmark {
                benchmark.record_app_event(
                    "app_state_ready",
                    serde_json::json!({
                        "playback_origin_ready": true,
                        "ddragon_mode": "offline",
                    }),
                )?;
            }
            if let Some((window_config, data_directory)) = &benchmark_webview {
                fs::create_dir_all(data_directory).with_context(|| {
                    format!(
                        "failed to create benchmark WebView data directory {}",
                        data_directory.display()
                    )
                })?;
                let page_load_benchmark = benchmark.clone();
                tauri::WebviewWindowBuilder::from_config(app.handle(), window_config)?
                    .data_directory(data_directory.clone())
                    .always_on_top(true)
                    .on_page_load(move |_window, payload| {
                        let Some(benchmark) = &page_load_benchmark else {
                            return;
                        };
                        if let Err(error) = benchmark.record_app_event(
                            "benchmark_page_load",
                            serde_json::json!({
                                "event": format!("{:?}", payload.event()).to_ascii_lowercase(),
                                "url": payload.url().as_str(),
                            }),
                        ) {
                            eprintln!("League Replay: could not record benchmark page load: {error}");
                        }
                    })
                    .build()?;
                if let Some(benchmark) = &benchmark {
                    benchmark.record_app_event(
                        "benchmark_webview_created",
                        serde_json::json!({
                            "data_directory": data_directory,
                            "window_label": window_config.label,
                        }),
                    )?;
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_games,
            list_clips,
            get_playback_probe,
            save_game,
            delete_game,
            delete_clip,
            list_built_in_music,
            prepare_imported_music_preview,
            export_clip,
            get_storage_usage,
            run_auto_delete,
            get_settings,
            save_settings,
            open_output_folder,
            open_clips_folder,
            get_hevc_probe_status,
            record_hevc_probe_result,
            get_ddragon_status,
            resolve_item_name,
            get_playback_server_metrics,
            get_replay_benchmark_session,
            record_replay_benchmark_events,
            get_replay_benchmark_queue_stats,
            flush_replay_benchmark_server_requests,
            complete_replay_benchmark
        ])
        .run(tauri_context)
        .expect("error while running League Replay");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_probe_markers_from_an_unknown_version() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("probe.json");
        fs::write(&path, br#"{"version":99,"supported":true}"#).unwrap();
        assert_eq!(read_hevc_marker(&path), None);
    }

    #[tokio::test]
    async fn unavailable_media_runtime_is_operation_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let error = queueback_media_runtime::resolve_root(&directory.path().join("missing"))
            .await
            .unwrap_err();
        let state = MediaRuntimeState::Unavailable(Arc::new(error));
        assert!(state.available().is_none());
        assert!(state.require().unwrap_err().contains("Repair or reinstall"));
    }
}
