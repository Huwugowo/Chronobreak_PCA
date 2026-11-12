use super::{AppState, LibraryRefreshError, command_error};
use crate::clip_bundle::{
    self, ClipDescriptor, ClipEvents, ClipGame, ClipHeader, ClipId, EventAvailability, Highlight,
};
use crate::directory_guard::DirectoryGuard;
use crate::league_index::{
    self, Index,
    clips::{ClipPage, ClipQuery},
};
use crate::library;
use crate::library_coordinator::Selection;
use anyhow::{Context, Result, ensure};
use chronobreak_replay_time::{
    ClipRange, FrameBoundary, MediaId, RoundingMode, frame_boundary_to_replay_tick,
};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::io::Write;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tauri::State;

#[derive(Default)]
pub(crate) struct DraftSlot {
    revision: AtomicU64,
    accepted: Mutex<Option<Arc<PreparedClip>>>,
}

impl DraftSlot {
    pub(crate) fn resolve(&self, snapshot: &str, token: &str) -> Option<Arc<PreparedClip>> {
        self.accepted
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .filter(|draft| draft.snapshot == snapshot && draft.token == token)
            .cloned()
    }
}

pub(crate) struct PreparedClip {
    snapshot: String,
    token: String,
    pub bundle: String,
    pub range: ClipRange,
    pub stamp: Option<String>,
    pub game: ClipGame,
    pub events: ClipEvents,
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftRange {
    game_timestamp: String,
    media_id: MediaId,
    start_frame: FrameBoundary,
    end_frame_exclusive: FrameBoundary,
}

#[derive(Debug, Serialize)]
pub(crate) struct DraftSummary {
    token: String,
    title: String,
    availability: EventAvailability,
    candidates: Vec<Highlight>,
}

#[tauri::command]
pub(crate) async fn prepare_clip_draft(
    state: State<'_, AppState>,
    snapshot_token: String,
    request: DraftRange,
) -> std::result::Result<DraftSummary, LibraryRefreshError> {
    let revision = state.clip_draft.revision.fetch_add(1, Ordering::AcqRel) + 1;
    let cache = state.league_index_cache.clone();
    let bundle = request.game_timestamp.clone();
    let draft = state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            Some(&bundle),
            false,
            move |root, snapshot, check| {
                let root_guard = DirectoryGuard::open(root)?;
                let games = root_guard.child("games", false)?;
                let game_directory = games.child(&request.game_timestamp, false)?;
                let directory = &game_directory.path;
                let authority = library::recording_export_authority(directory, None)?;
                let timeline = authority.media_timeline;
                let range = ClipRange::new(
                    request.media_id,
                    request.start_frame,
                    request.end_frame_exclusive,
                    timeline.video.frame_count,
                )?;
                range.validate_for(&timeline)?;
                let start = frame_boundary_to_replay_tick(
                    range.start_frame,
                    timeline.video.frame_rate,
                    RoundingMode::Exact,
                )?;
                let end = frame_boundary_to_replay_tick(
                    range.end_frame_exclusive,
                    timeline.video.frame_rate,
                    RoundingMode::Exact,
                )?;
                let stamp = league_index::source_stamp(directory).ok();
                let events = (|| -> Result<_> {
                    let index = Index::reader(&cache, root)?;
                    index.require_source_stamp(
                        &request.game_timestamp,
                        &timeline.media_id,
                        stamp.as_deref().context("source revision unavailable")?,
                    )?;
                    index.clip_events(&timeline, start, end, check)
                })()
                .unwrap_or_else(|_| ClipEvents::unavailable());
                let mut game = authority.game;
                if let Some(group) = &game.match_group {
                    let earliest = snapshot
                        .games
                        .iter()
                        .filter(|part| part.match_group.as_ref() == Some(group))
                        .filter_map(|part| {
                            let at = time::OffsetDateTime::parse(
                                &part.recorded_at,
                                &time::format_description::well_known::Rfc3339,
                            )
                            .ok()?;
                            Some((at, &part.recorded_at))
                        })
                        .min_by_key(|(at, _)| *at);
                    if let Some((_, date)) = earliest {
                        game.played_at = Some(date.clone());
                    }
                }
                let title = crate::clip_export::default_title(&game, start);
                Ok(PreparedClip {
                    snapshot: snapshot.token.clone(),
                    token: crate::playback_policy::random_capability()?,
                    bundle: request.game_timestamp,
                    range,
                    stamp,
                    game,
                    events,
                    title,
                })
            },
        )
        .await?;
    let mut accepted = state
        .clip_draft
        .accepted
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if state.clip_draft.revision.load(Ordering::Acquire) != revision {
        return Err(LibraryRefreshError::Superseded);
    }
    let summary = DraftSummary {
        token: draft.token.clone(),
        title: draft.title.clone(),
        availability: draft.events.availability,
        candidates: draft.events.candidates(),
    };
    *accepted = Some(Arc::new(draft));
    Ok(summary)
}

#[tauri::command]
pub(crate) async fn reconcile_clips(
    state: State<'_, AppState>,
    snapshot_token: String,
) -> std::result::Result<bool, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            None,
            true,
            move |root, snapshot, check| {
                Index::open(&cache, root)?.reconcile_clips(snapshot, check)
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn clip_page(
    state: State<'_, AppState>,
    snapshot_token: String,
    query: ClipQuery,
) -> std::result::Result<ClipPage, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    let origin = state.playback_origin.clone();
    let capability = state.roots.clip_capability();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            None,
            false,
            move |root, snapshot, _| {
                if snapshot.managed_clips.is_empty() {
                    return Ok(ClipPage {
                        groups: Vec::new(),
                        next: None,
                    });
                }
                Index::reader(&cache, root)?.clips(snapshot, &query, &origin, &capability)
            },
        )
        .await
}

#[tauri::command]
pub(crate) async fn clip_tag_suggestions(
    state: State<'_, AppState>,
    snapshot_token: String,
    prefix: String,
) -> std::result::Result<Vec<String>, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            None,
            false,
            move |root, snapshot, _| {
                ensure!(prefix.chars().count() <= 40, "tag prefix is too long");
                if snapshot.managed_clips.is_empty() {
                    return Ok(Vec::new());
                }
                Index::reader(&cache, root)?.clip_tags(&snapshot.token, &prefix)
            },
        )
        .await
}

#[derive(Debug, Serialize)]
pub(crate) struct ClipDetail {
    header: ClipHeader,
    events: ClipEvents,
    candidates: Vec<Highlight>,
}

#[tauri::command]
pub(crate) async fn clip_detail(
    state: State<'_, AppState>,
    snapshot_token: String,
    clip_id: ClipId,
) -> std::result::Result<ClipDetail, LibraryRefreshError> {
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            None,
            false,
            move |_, snapshot, _| {
                let clip = snapshot
                    .managed_clips
                    .get(&clip_id)
                    .context("clip unavailable")?;
                let _directory = checked_directory(clip, false)?;
                let header = clip.read_header()?;
                let events = clip
                    .read_events()
                    .unwrap_or_else(|_| ClipEvents::unavailable());
                let candidates = events.candidates();
                Ok(ClipDetail {
                    header,
                    events,
                    candidates,
                })
            },
        )
        .await
}

fn checked_directory(clip: &ClipDescriptor, movable: bool) -> Result<DirectoryGuard> {
    let directory = if movable {
        DirectoryGuard::open_movable(&clip.directory)?
    } else {
        DirectoryGuard::open(&clip.directory)?
    };
    ensure!(
        directory.generation()? == clip.generation,
        "clip folder changed"
    );
    Ok(directory)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ClipEdit {
    Rename {
        title: String,
    },
    Tags {
        custom_tags: Vec<String>,
        highlights: Vec<u32>,
    },
    Delete,
    Reveal,
}

#[tauri::command]
pub(crate) async fn edit_managed_clip(
    state: State<'_, AppState>,
    snapshot_token: String,
    clip_id: ClipId,
    edit: ClipEdit,
) -> std::result::Result<(), String> {
    let clip = state
        .library
        .clip_descriptor(&state.roots, &snapshot_token, &clip_id)
        .map_err(command_error)?;
    if matches!(edit, ClipEdit::Reveal) {
        return state
            .library
            .index_job(
                Arc::clone(&state.roots),
                &snapshot_token,
                None,
                false,
                move |_, _, _| {
                    let directory = checked_directory(&clip, false)?;
                    super::open_folder(&directory.path)
                },
            )
            .await
            .map_err(command_error);
    }
    state
        .library
        .mutate(
            &state.roots,
            &snapshot_token,
            Selection::ManagedClip(&clip_id),
            move |owner| {
                let collection = owner.clips()?;
                ensure!(
                    clip.directory.parent() == Some(collection.path.as_path()),
                    "clip folder moved"
                );
                let directory = checked_directory(&clip, matches!(edit, ClipEdit::Delete))?;
                match edit {
                    ClipEdit::Rename { title } => {
                        let leaf = clip_bundle::media_leaf(&title)?;
                        if leaf != clip.video_leaf {
                            let target = directory.removal_target(OsStr::new(&clip.video_leaf))?;
                            ensure!(target.facts()? == clip.video, "clip video changed");
                            target.rename_file(&directory, OsStr::new(&leaf))?;
                        }
                        Ok(())
                    }
                    ClipEdit::Tags {
                        custom_tags,
                        highlights,
                    } => {
                        ensure!(
                            highlights.len() <= clip_bundle::MAX_HIGHLIGHTS,
                            "too many clip highlights"
                        );
                        let mut header = clip.read_header()?;
                        header.custom_tags = clip_bundle::normalize_tags(&custom_tags)?;
                        header.highlights = match clip.read_events() {
                            Ok(events) => events.select(&highlights)?,
                            Err(_)
                                if highlights
                                    .iter()
                                    .copied()
                                    .eq(header.highlights.iter().map(|tag| tag.event_index)) =>
                            {
                                header.highlights
                            }
                            Err(error) => return Err(error),
                        };
                        header.validate(&clip.id)?;
                        let json = serde_json::to_vec(&header)?;
                        ensure!(
                            json.len() as u64 <= clip_bundle::HEADER_LIMIT,
                            "clip metadata exceeds its limit"
                        );
                        atomicwrites::AtomicFile::new(
                            directory.path.join(clip_bundle::HEADER),
                            atomicwrites::AllowOverwrite,
                        )
                        .write(|file| {
                            file.write_all(&json)?;
                            file.sync_all()
                        })
                        .map_err(anyhow::Error::from)
                    }
                    ClipEdit::Delete => directory.remove_owned_files(&[
                        (&clip.video_leaf, &clip.video),
                        (clip_bundle::THUMBNAIL, &clip.thumbnail),
                        (clip_bundle::HEADER, &clip.header),
                        (clip_bundle::EVENTS, &clip.events),
                    ]),
                    ClipEdit::Reveal => unreachable!(),
                }
            },
        )
        .await
}
