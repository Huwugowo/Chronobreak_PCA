use std::{io::Write, sync::Arc};

use anyhow::{Context, Result, ensure};
use tauri::State;

use crate::{
    AppState,
    clip_bundle::{ClipDescriptor, ClipHeader, ClipId, HEADER, HEADER_LIMIT},
    clip_commands::checked_directory,
    directory_guard::open_regular,
    league_index::Index,
    library::LibrarySnapshot,
    library_coordinator::{LibraryRefreshError, command_error},
    library_mutation::LibraryMutation,
    playback_file::{ApprovedRoot, file_facts},
    tag_store, tags,
    tags::{TagDefinition, TagId},
};

#[tauri::command]
pub(crate) async fn tag_choices(
    state: State<'_, AppState>,
    snapshot_token: String,
    prefix: String,
) -> Result<Vec<TagDefinition>, LibraryRefreshError> {
    let cache = state.league_index_cache.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            &snapshot_token,
            None,
            false,
            move |root, _, _| {
                ensure!(
                    prefix.encode_utf16().count() <= tags::MAX_LABEL,
                    "tag search is too long"
                );
                match Index::reader(&cache, root) {
                    Ok(index) => index.tag_choices(&prefix),
                    Err(error)
                        if error
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
                    {
                        let prefix = tags::label_key(&prefix);
                        let source = tag_store::load(root)?;
                        Ok(tags::builtin_definitions()
                            .chain(source.store.definitions.into_values())
                            .filter(|tag| tags::label_key(&tag.label).starts_with(&prefix))
                            .take(24)
                            .collect())
                    }
                    Err(error) => Err(error),
                }
            },
        )
        .await
}

async fn write<T: Send + 'static>(
    state: &AppState,
    token: &str,
    edit: impl FnOnce(
        &mut Index,
        &LibraryMutation,
        &LibrarySnapshot,
        &dyn Fn() -> Result<()>,
    ) -> Result<T>
    + Send
    + 'static,
) -> Result<T, String> {
    let permit = state.library.mutation_permit().map_err(command_error)?;
    let cache = state.league_index_cache.clone();
    state
        .library
        .index_job(
            Arc::clone(&state.roots),
            token,
            None,
            true,
            move |root, snapshot, check| {
                let _permit = permit;
                let owner = LibraryMutation::acquire(root, &ApprovedRoot::new(root)?)?;
                check()?;
                let mut index = Index::open(&cache, root)?;
                edit(&mut index, &owner, snapshot, check)
            },
        )
        .await
        .map_err(command_error)
}

#[tauri::command]
pub(crate) async fn create_tag(
    state: State<'_, AppState>,
    snapshot_token: String,
    label: String,
) -> Result<TagDefinition, String> {
    write(&state, &snapshot_token, move |index, owner, _, check| {
        let label = tags::clean_label(&label)?;
        let mut loaded = tag_store::load(owner.root_path())?;
        index.refresh_tag_store(&loaded)?;
        if let Some(existing) = index.exact_tag_label(label)? {
            return Ok(existing);
        }
        let tag = TagDefinition::custom(label)?;
        loaded.store.insert_definition(tag.clone())?;
        check()?;
        let stamp = loaded.store.publish(owner)?;
        index.update_tag_projection(&stamp, std::slice::from_ref(&tag), None)?;
        Ok(tag)
    })
    .await
}

#[tauri::command]
pub(crate) async fn rename_tag(
    state: State<'_, AppState>,
    snapshot_token: String,
    tag_id: TagId,
    label: String,
) -> Result<TagDefinition, String> {
    write(&state, &snapshot_token, move |index, owner, _, check| {
        let label = tags::clean_label(&label)?;
        let mut loaded = tag_store::load(owner.root_path())?;
        index.refresh_tag_store(&loaded)?;
        let mut tag = index
            .tag_definitions(vec![tag_id])?
            .pop()
            .context("tag unavailable")?;
        ensure!(!tag.builtin, "built-in tags cannot be renamed");
        if tag.label == label && loaded.store.definitions.contains_key(&tag.id) {
            return Ok(tag);
        }
        ensure!(
            index
                .exact_tag_label(label)?
                .is_none_or(|other| other.id == tag.id),
            "a tag already uses that name"
        );
        tag.label = label.to_owned();
        loaded.store.insert_definition(tag.clone())?;
        check()?;
        let stamp = loaded.store.publish(owner)?;
        index.update_tag_projection(&stamp, std::slice::from_ref(&tag), None)?;
        Ok(tag)
    })
    .await
}

#[tauri::command]
pub(crate) async fn edit_match_tags(
    state: State<'_, AppState>,
    snapshot_token: String,
    match_group: String,
    tags: Vec<TagId>,
) -> Result<Vec<crate::tags::TagAssignment>, String> {
    write(
        &state,
        &snapshot_token,
        move |index, owner, snapshot, check| {
            tag_store::validate_match_group(&match_group)?;
            index.require_catalog(&snapshot.catalog_key)?;
            ensure!(index.match_exists(&match_group)?, "match unavailable");
            let mut loaded = tag_store::load(owner.root_path())?;
            index.refresh_tag_store(&loaded)?;
            let selected = index.tag_definitions(tags.clone())?;
            ensure!(selected.len() == tags.len(), "tag unavailable");
            let original = loaded
                .store
                .matches
                .get(&match_group)
                .cloned()
                .unwrap_or_default();
            let mut manual = original.clone();
            manual.replace(tags)?;
            manual.validate()?;
            let mut definitions = Vec::new();
            for tag in selected {
                if manual.ids.contains(&tag.id)
                    && !tag.builtin
                    && !loaded.store.definitions.contains_key(&tag.id)
                {
                    loaded.store.insert_definition(tag.clone())?;
                    definitions.push(tag);
                }
            }
            if manual != original || !definitions.is_empty() {
                if manual == Default::default() {
                    loaded.store.matches.remove(&match_group);
                } else {
                    loaded
                        .store
                        .matches
                        .insert(match_group.clone(), manual.clone());
                }
                check()?;
                let stamp = loaded.store.publish(owner)?;
                index.update_tag_projection(&stamp, &definitions, Some((&match_group, &manual)))?;
            }
            index.effective_match_tags(&match_group)
        },
    )
    .await
}

#[tauri::command]
pub(crate) async fn edit_clip_tags(
    state: State<'_, AppState>,
    snapshot_token: String,
    clip_id: ClipId,
    tags: Vec<TagDefinition>,
) -> Result<Vec<crate::tags::TagAssignment>, String> {
    let library = state.library.clone();
    let roots = Arc::clone(&state.roots);
    let token = snapshot_token.clone();
    write(
        &state,
        &snapshot_token,
        move |index, owner, snapshot, check| {
            ensure!(
                index.clip_ready(&snapshot.managed_clip_stamp),
                "clip index is still preparing"
            );
            let mut clip = library
                .clip_descriptor(&roots, &token, &clip_id)
                .map_err(|error| anyhow::anyhow!(command_error(error)))?;
            let clips = owner.clips()?;
            ensure!(
                clips.path.join(clip_id.as_str()) == clip.directory,
                "clip folder changed"
            );
            let _folder = checked_directory(&clip, false)?;
            let mut header = clip.read_header()?;
            let tags = tags::normalize_assignments(
                tags.into_iter()
                    .map(crate::tags::TagAssignment::manual)
                    .collect(),
            )?;
            let selected = index
                .tag_definitions(tags.iter().map(|tag| tag.definition.id.clone()).collect())?;
            ensure!(selected.len() == tags.len(), "tag unavailable");
            // Keep portable labels current whenever this clip's header is edited.
            let selected = tags::normalize_assignments(
                selected
                    .into_iter()
                    .map(crate::tags::TagAssignment::manual)
                    .collect(),
            )?;
            let detected = header
                .tags
                .iter()
                .filter(|tag| tag.source == crate::tags::TagAssignmentSource::Detected)
                .cloned();
            let assignments =
                tags::normalize_assignments(detected.chain(selected.clone()).collect())?;
            if header.tags != assignments {
                header.tags = assignments.clone();
                check()?;
                write_clip_header(&mut clip, &header)?;
                index.reproject_clip(&clip)?;
                library.publish_clip_header(&token, &clip)?;
            }
            Ok(assignments)
        },
    )
    .await
}

/// The caller holds the root lease and the clip directory guard.
pub(crate) fn write_clip_header(clip: &mut ClipDescriptor, header: &ClipHeader) -> Result<()> {
    header.validate(&clip.id)?;
    let bytes = serde_json::to_vec(header)?;
    ensure!(
        bytes.len() as u64 <= HEADER_LIMIT,
        "clip metadata is too large"
    );
    let path = clip.directory.join(HEADER);
    ensure!(
        file_facts(&open_regular(&path, false, false)?)? == clip.header,
        "clip metadata changed"
    );
    atomicwrites::AtomicFile::new(&path, atomicwrites::AllowOverwrite)
        .write(|file| {
            file.write_all(&bytes)?;
            file.sync_all()
        })
        .map_err(|error| anyhow::anyhow!("clip metadata publication: {error}"))?;
    clip.header = file_facts(&open_regular(&path, false, false)?)?;
    Ok(())
}
