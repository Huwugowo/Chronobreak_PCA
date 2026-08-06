mod library;
mod playback_server;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use library::{GameSummary, PlaybackProbe};
use playback_server::{PlaybackMetrics, ServerMetrics};
use tauri::{Manager, State};

struct AppState {
    games_directory: PathBuf,
    playback_origin: String,
    playback_metrics: Arc<PlaybackMetrics>,
}

#[tauri::command]
fn list_games(state: State<'_, AppState>) -> Result<Vec<GameSummary>, String> {
    library::list_games(&state.games_directory).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_playback_probe(state: State<'_, AppState>) -> Result<Option<PlaybackProbe>, String> {
    library::playback_probe(&state.games_directory, &state.playback_origin)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_playback_server_metrics(state: State<'_, AppState>) -> ServerMetrics {
    state.playback_metrics.snapshot()
}

fn output_directory(app: &tauri::App) -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("LEAGUE_REPLAY_OUTPUT_PATH") {
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty() {
            anyhow::bail!("LEAGUE_REPLAY_OUTPUT_PATH cannot be empty");
        }
        return Ok(path);
    }

    Ok(app
        .path()
        .home_dir()
        .context("could not resolve the user home directory")?
        .join("LeagueReplays"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let games_directory = output_directory(app)?.join("games");
            let playback_metrics = Arc::new(PlaybackMetrics::default());
            let playback_origin = tauri::async_runtime::block_on(playback_server::start(
                games_directory.clone(),
                Arc::clone(&playback_metrics),
            ))?;

            #[cfg(debug_assertions)]
            eprintln!(
                "League Replay: serving {} from {}",
                playback_origin,
                games_directory.display()
            );

            app.manage(AppState {
                games_directory,
                playback_origin,
                playback_metrics,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_games,
            get_playback_probe,
            get_playback_server_metrics
        ])
        .run(tauri::generate_context!())
        .expect("error while running League Replay");
}
