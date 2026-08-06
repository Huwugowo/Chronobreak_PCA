mod config;
mod ddragon;
mod library;
mod playback_server;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use config::{Config, Settings, SettingsUpdate};
use ddragon::DdragonStatus;
use library::{AutoDeleteResult, ClipSummary, GameSummary, PlaybackProbe, StorageUsage};
use playback_server::{MediaRoots, PlaybackMetrics, ServerMetrics};
use serde::{Deserialize, Serialize};
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
    library::list_games(&state.roots.output_directory()).map_err(error_string)
}

#[tauri::command(async)]
fn list_clips(state: State<'_, AppState>) -> Result<Vec<ClipSummary>, String> {
    library::list_clips(&state.roots.output_directory(), &state.playback_origin)
        .map_err(error_string)
}

#[tauri::command(async)]
fn get_playback_probe(
    state: State<'_, AppState>,
    game_timestamp: String,
) -> Result<PlaybackProbe, String> {
    library::playback_probe(
        &state.roots.output_directory(),
        &state.playback_origin,
        &game_timestamp,
    )
    .map_err(error_string)
}

#[tauri::command(async)]
fn save_game(
    state: State<'_, AppState>,
    game_timestamp: String,
    saved: bool,
) -> Result<(), String> {
    library::save_game(&state.roots.output_directory(), &game_timestamp, saved)
        .map_err(error_string)
}

#[tauri::command(async)]
fn delete_game(state: State<'_, AppState>, game_timestamp: String) -> Result<(), String> {
    library::delete_game(&state.roots.output_directory(), &game_timestamp).map_err(error_string)
}

#[tauri::command(async)]
fn delete_clip(state: State<'_, AppState>, clip_filename: String) -> Result<(), String> {
    library::delete_clip(&state.roots.output_directory(), &clip_filename).map_err(error_string)
}

#[tauri::command(async)]
fn get_storage_usage(state: State<'_, AppState>) -> Result<StorageUsage, String> {
    library::storage_usage(&state.roots.output_directory()).map_err(error_string)
}

#[tauri::command(async)]
fn run_auto_delete(state: State<'_, AppState>) -> Result<AutoDeleteResult, String> {
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
    open_folder(&state.roots.output_directory()).map_err(error_string)
}

#[tauri::command(async)]
fn open_clips_folder(state: State<'_, AppState>) -> Result<(), String> {
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

    Ok(HevcProbeStatus {
        tested: true,
        supported,
        probe_url: format!("{}/probe/hevc.mp4", state.playback_origin),
    })
}

#[tauri::command]
fn get_ddragon_status(state: State<'_, AppState>) -> DdragonStatus {
    state
        .ddragon_status
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_path = config::default_config_path()?;
            let config = Config::load_or_create(&config_path)?;
            let output_directory = config.resolved_output_path()?;
            ensure_output_directories(&output_directory)?;
            if let Err(error) =
                library::run_auto_delete(&output_directory, config.storage.auto_delete_days)
            {
                eprintln!("League Replay auto-delete skipped: {error}");
            }

            let roots = Arc::new(MediaRoots::new(output_directory.clone()));
            let playback_metrics = Arc::new(PlaybackMetrics::default());
            let playback_origin = tauri::async_runtime::block_on(playback_server::start(
                Arc::clone(&roots),
                Arc::clone(&playback_metrics),
            ))?;

            let app_data = app
                .path()
                .app_local_data_dir()
                .context("could not resolve the app data directory")?;
            let hevc_probe_path = app_data.join("hevc-probe-v1.json");
            let ddragon_cache = app_data.join("ddragon");
            let ddragon_status = Arc::new(RwLock::new(DdragonStatus::loading(&ddragon_cache)));
            tauri::async_runtime::spawn(ddragon::initialize(
                ddragon_cache.clone(),
                Arc::clone(&ddragon_status),
            ));

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
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_games,
            list_clips,
            get_playback_probe,
            save_game,
            delete_game,
            delete_clip,
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
            get_playback_server_metrics
        ])
        .run(tauri::generate_context!())
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
}
