//! Accepted filesystem membership gates every cache operation.
use super::{AppState, LibraryRefreshError};
use crate::{
    league_index::{self, Index},
    library::{self, GameSummary, LibrarySnapshot},
};
use anyhow::{Context, Result, ensure};
use chronobreak_replay_time::{GameTick, MediaId};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

fn accepted_game<'a>(snapshot: &'a LibrarySnapshot, bundle: &str) -> Result<&'a GameSummary> {
    let game = snapshot
        .games
        .iter()
        .find(|g| g.timestamp == bundle)
        .context("recording is outside accepted library")?;
    let media = game
        .media_id
        .as_ref()
        .context("recording media identity unavailable")?;
    ensure!(
        snapshot
            .games
            .iter()
            .filter(|g| g.media_id.as_ref() == Some(media))
            .count()
            == 1,
        "duplicate media identity in accepted library"
    );
    Ok(game)
}

fn selected_media(
    root: &std::path::Path,
    snapshot: &LibrarySnapshot,
    bundle: &str,
) -> Result<library::ReplayDescriptor> {
    let game = accepted_game(snapshot, bundle)?;
    let descriptor = library::replay_descriptor(root, "", bundle, snapshot.token.clone())?;
    ensure!(
        game.media_id.as_ref() == Some(&descriptor.media_timeline.media_id),
        "recording changed since accepted scan"
    );
    Ok(descriptor)
}

#[tauri::command]
pub(crate) async fn index_league_recording(
    state: State<'_, AppState>,
    snapshot_token: String,
    game_timestamp: String,
    rebuild: bool,
) -> Result<league_index::ImportProgress, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    let bundle = game_timestamp.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            Some(&game_timestamp),
            true,
            move |root, snapshot, check| {
                let game = accepted_game(snapshot, &bundle)?;
                let mut index = Index::open(&cache, root)?;
                index.reconcile(&snapshot.games, &snapshot.token)?;
                index.recording(root, game, rebuild, check)
            },
        )
        .await
}

#[derive(Debug, Serialize)]
pub(crate) struct CatalogItem {
    game_timestamp: String,
    media_id: Option<MediaId>,
    progress: Option<league_index::ImportProgress>,
    error: Option<String>,
}
#[derive(Debug, Serialize)]
pub(crate) struct CatalogPage {
    snapshot_token: String,
    items: Vec<CatalogItem>,
    next: Option<String>,
    catalog_recordings: usize,
}

#[tauri::command]
pub(crate) async fn index_league_catalog(
    state: State<'_, AppState>,
    snapshot_token: String,
    after: Option<String>,
    limit: u8,
) -> Result<CatalogPage, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            None,
            true,
            move |root, snapshot, check| {
                ensure!((1..=4).contains(&limit), "catalog import limit");
                let start = match after {
                    Some(bundle) => {
                        snapshot
                            .games
                            .iter()
                            .position(|g| g.timestamp == bundle)
                            .context("catalog cursor is outside accepted snapshot")?
                            + 1
                    }
                    None => 0,
                };
                let mut index = Index::open(&cache, root)?;
                index.reconcile(&snapshot.games, &snapshot.token)?;
                let mut items = Vec::new();
                for game in snapshot.games.iter().skip(start).take(usize::from(limit)) {
                    check()?;
                    let result = accepted_game(snapshot, &game.timestamp)
                        .and_then(|game| index.recording(root, game, false, check));
                    check()?;
                    let (progress, error) = match result {
                        Ok(progress) => (Some(progress), None),
                        Err(error) => (None, Some(error.to_string())),
                    };
                    items.push(CatalogItem {
                        game_timestamp: game.timestamp.clone(),
                        media_id: game.media_id.clone(),
                        progress,
                        error,
                    });
                }
                let next = (start + items.len() < snapshot.games.len())
                    .then(|| items.last().map(|i| i.game_timestamp.clone()))
                    .flatten();
                Ok(CatalogPage {
                    snapshot_token: snapshot.token.clone(),
                    items,
                    next,
                    catalog_recordings: snapshot.games.len(),
                })
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn reset_league_index(
    state: State<'_, AppState>,
    snapshot_token: String,
) -> Result<league_index::ResetResult, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    state
        .library
        .index_admin_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            move |root, snapshot, check| {
                league_index::reset::reset(&cache, root, &snapshot.games, &snapshot.token, check)
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn query_league_history(
    state: State<'_, AppState>,
    snapshot_token: String,
    query: league_index::HistoryQuery,
) -> Result<league_index::HistoryPage, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            None,
            false,
            move |root, snapshot, _| {
                let index = Index::reader(&cache, root)?;
                index.require_catalog(&snapshot.token)?;
                index.history(&query, snapshot.games.len())
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn league_source_scopes(
    state: State<'_, AppState>,
    snapshot_token: String,
    game_timestamp: String,
) -> Result<Vec<league_index::SourceScope>, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    let bundle = game_timestamp.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            Some(&game_timestamp),
            false,
            move |root, snapshot, _| {
                let descriptor = selected_media(root, snapshot, &bundle)?;
                let index = Index::reader(&cache, root)?;
                index.require_catalog(&snapshot.token)?;
                index.require_source(root, &bundle, &descriptor.media_timeline.media_id)?;
                index.scopes(&descriptor.media_timeline.media_id)
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn query_league_events(
    state: State<'_, AppState>,
    snapshot_token: String,
    game_timestamp: String,
    query: league_index::WindowQuery,
) -> Result<league_index::EventWindow, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    let bundle = game_timestamp.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            Some(&game_timestamp),
            false,
            move |root, snapshot, _| {
                let descriptor = selected_media(root, snapshot, &bundle)?;
                let index = Index::reader(&cache, root)?;
                index.require_catalog(&snapshot.token)?;
                index.require_source(root, &bundle, &descriptor.media_timeline.media_id)?;
                index.events(&query, &descriptor.media_timeline)
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn query_league_roster(
    state: State<'_, AppState>,
    snapshot_token: String,
    game_timestamp: String,
    scope: league_index::ScopeQuery,
    tick: GameTick,
) -> Result<league_index::RosterAt, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    let bundle = game_timestamp.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            Some(&game_timestamp),
            false,
            move |root, snapshot, _| {
                let descriptor = selected_media(root, snapshot, &bundle)?;
                ensure!(
                    scope.media_id == descriptor.media_timeline.media_id,
                    "roster query media mismatch"
                );
                let index = Index::reader(&cache, root)?;
                index.require_catalog(&snapshot.token)?;
                index.require_source(root, &bundle, &descriptor.media_timeline.media_id)?;
                index.roster_at(&scope, tick)
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn query_league_authored(
    state: State<'_, AppState>,
    snapshot_token: String,
    after: Option<MediaId>,
    limit: u8,
) -> Result<league_index::AuthoredPage, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            None,
            true,
            move |root, snapshot, _| {
                let mut index = Index::open(&cache, root)?;
                index.reconcile(&snapshot.games, &snapshot.token)?;
                index.authored(after.as_ref(), limit)
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn query_league_overlay_page(
    state: State<'_, AppState>,
    snapshot_token: String,
    game_timestamp: String,
    query: league_index::OverlayQuery,
) -> Result<league_index::OverlayPage, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    let bundle = game_timestamp.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            Some(&game_timestamp),
            false,
            move |root, snapshot, _| {
                let descriptor = selected_media(root, snapshot, &bundle)?;
                let index = Index::reader(&cache, root)?;
                index.require_catalog(&snapshot.token)?;
                index.require_source(root, &bundle, &descriptor.media_timeline.media_id)?;
                index.overlay_page(&query, &descriptor.media_timeline)
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn query_league_replay_window(
    state: State<'_, AppState>,
    snapshot_token: String,
    game_timestamp: String,
    query: league_index::ReplayWindowQuery,
) -> Result<league_index::ReplayWindow, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    let bundle = game_timestamp.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            Some(&game_timestamp),
            false,
            move |root, snapshot, _| {
                let descriptor = selected_media(root, snapshot, &bundle)?;
                let index = Index::reader(&cache, root)?;
                index.require_catalog(&snapshot.token)?;
                index.require_source(root, &bundle, &descriptor.media_timeline.media_id)?;
                index.replay_window(&query, &descriptor.media_timeline)
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn league_recording_facts(
    state: State<'_, AppState>,
    snapshot_token: String,
    game_timestamp: String,
) -> Result<library::LeagueFacts, LibraryRefreshError> {
    let bundle = game_timestamp.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            Some(&game_timestamp),
            false,
            move |root, snapshot, _| {
                let descriptor = selected_media(root, snapshot, &bundle)?;
                library::league_facts(root, &bundle, &descriptor.media_timeline.media_id)
            },
        )
        .await
}

#[derive(Debug, Serialize)]
pub(crate) struct AuthoredRevision {
    revision: u64,
}
#[tauri::command]
pub(crate) async fn save_league_authored_record(
    state: State<'_, AppState>,
    snapshot_token: String,
    expected_revision: u64,
    record: crate::league_authored::AuthoredRecord,
) -> Result<AuthoredRevision, String> {
    state
        .library
        .mutate(
            &state.roots,
            &snapshot_token,
            crate::library_coordinator::Selection::Library,
            move |root| {
                let store =
                    crate::league_authored::put(root, expected_revision, record, &|| Ok(()))?;
                Ok(AuthoredRevision {
                    revision: store.revision,
                })
            },
        )
        .await
}
