//! Explicit one-time conversion of development clip headers.
use std::{ffi::OsString, fs, path::Path};

use anyhow::{Context, Result, ensure};
use serde_json::Value;

use crate::{
    clip_bundle::{ClipDescriptor, ClipHeader, ClipId, HEADER, HEADER_LIMIT, VERSION, read_json},
    clip_commands::checked_directory,
    directory_guard::guard_root,
    library_mutation::LibraryMutation,
    playback_file::ApprovedRoot,
    tag_commands::write_clip_header,
    tag_store::{self, Store},
    tags::{self, TagDefinition, TagId},
};

pub fn migrate_clip_tags_headless(args: &[OsString]) -> Result<()> {
    ensure!(
        args.len() == 2,
        "usage: --migrate-clip-tags <absolute library path>"
    );
    let migrated = migrate(Path::new(&args[1]))?;
    println!("Migrated {migrated} clip headers.");
    Ok(())
}

fn migrate(root: &Path) -> Result<usize> {
    ensure!(root.is_absolute(), "an absolute library path is required");
    let approved = ApprovedRoot::new(root)?;
    let owner = LibraryMutation::acquire(root, &approved)?;
    let Some(collection) = guard_root(&approved)?.optional_child("clips")? else {
        return Ok(0);
    };
    let mut source = tag_store::load(owner.root_path())?.store;
    let mut count = 0;
    for entry in fs::read_dir(&collection.path)? {
        let entry = entry?;
        let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|leaf| ClipId::parse(leaf).ok())
        else {
            continue;
        };
        let mut clip = ClipDescriptor::inspect(&collection, id)?;
        let _directory = checked_directory(&clip, false)?;
        let value: Value = read_json(
            &clip.directory.join(HEADER),
            HEADER_LIMIT,
            Some(&clip.header),
        )?;
        if value["version"].as_u64() == Some(u64::from(VERSION)) {
            continue;
        }
        let definitions = source.definitions.len();
        let header =
            convert(value, &mut source).with_context(|| format!("clip {}", clip.id.as_str()))?;
        header.validate(&clip.id)?;
        // Publish new IDs before the clip, so an interrupted run reuses them.
        if source.definitions.len() != definitions {
            source.publish(&owner)?;
        }
        write_clip_header(&mut clip, &header)?;
        count += 1;
    }
    Ok(count)
}

fn convert(mut value: Value, store: &mut Store) -> Result<ClipHeader> {
    ensure!(
        value["version"].as_u64() == Some(2),
        "expected a version 2 development clip"
    );
    let fields = value.as_object_mut().context("invalid clip metadata")?;
    let predefined: Vec<String> = serde_json::from_value(
        fields
            .remove("predefined_tags")
            .context("missing predefined tags")?,
    )?;
    let custom: Vec<String> = serde_json::from_value(
        fields
            .remove("custom_tags")
            .context("missing custom tags")?,
    )?;
    let mut assignments = Vec::new();
    for kind in predefined {
        let id = TagId::parse(&format!("builtin:{kind}"))?;
        assignments.push(tags::builtin_definition(&id).context("unknown predefined tag")?);
    }
    for label in custom {
        let label = tags::clean_label(&label)?;
        let key = tags::label_key(label);
        let existing = tags::builtin_definitions()
            .chain(store.definitions.values().cloned())
            .find(|tag| tags::label_key(&tag.label) == key);
        let tag = match existing {
            Some(tag) => tag,
            None => {
                let tag = TagDefinition::custom(label)?;
                store.insert_definition(tag.clone())?;
                tag
            }
        };
        assignments.push(tag);
    }
    fields.insert("version".into(), VERSION.into());
    // V2 allowed separate limits. Apply the unified limit to unique assignments.
    let mut seen = std::collections::HashSet::new();
    assignments.retain(|tag| seen.insert(tag.id.clone()));
    fields.insert(
        "tags".into(),
        serde_json::to_value(tags::normalize_assignments(assignments)?)?,
    );
    Ok(serde_json::from_value(value)?)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::clip_bundle::{ClipGame, EVENTS, EventAvailability, THUMBNAIL};
    use crate::clip_export::ClipExportPreset;
    use crate::league_index::{Index, clips::ClipGridQuery};
    use chronobreak_replay_time::{ClipRange, FrameBoundary, MediaId, ReplayTick};

    #[test]
    fn migrates_once_and_resolves_a_custom_tag_without_its_vocabulary() {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let end = FrameBoundary::new(60).unwrap();
        let mut headers = Vec::new();
        for label in ["Final fight", "FINAL FIGHT"] {
            let id = ClipId::new();
            let directory = root.path().join("clips").join(id.as_str());
            fs::create_dir_all(&directory).unwrap();
            let mut old = serde_json::to_value(ClipHeader {
                version: 2,
                id: id.clone(),
                range: ClipRange::new(MediaId::new_v4(), FrameBoundary::new(0).unwrap(), end, end)
                    .unwrap(),
                source_timestamp: "100".into(),
                game: ClipGame {
                    champion: Some("Ahri".into()),
                    ..Default::default()
                },
                exported_at: 42,
                preset: ClipExportPreset::Horizontal,
                duration: ReplayTick::new(96_000_000).unwrap(),
                events: EventAvailability::Unavailable,
                tags: Vec::new(),
            })
            .unwrap();
            old.as_object_mut().unwrap().remove("tags");
            old["custom_tags"] = serde_json::json!([label]);
            old["predefined_tags"] = serde_json::json!(["double_kill", "pentakill"]);
            fs::write(directory.join(HEADER), old.to_string()).unwrap();
            for leaf in ["Fixture.mp4", THUMBNAIL, EVENTS] {
                fs::write(directory.join(leaf), b"fixture").unwrap();
            }
            headers.push((id, directory));
        }
        assert_eq!(migrate(root.path()).unwrap(), 2);
        assert_eq!(migrate(root.path()).unwrap(), 0);
        let approved = ApprovedRoot::new(root.path()).unwrap();
        let source = tag_store::load(approved.path()).unwrap();
        assert_eq!(source.store.definitions.len(), 1);
        let tag = source.store.definitions.values().next().unwrap();
        let detached = tempfile::tempdir().unwrap();
        for (id, dir) in &headers {
            let target = detached.path().join("clips").join(id.as_str());
            fs::create_dir_all(&target).unwrap();
            for asset in fs::read_dir(dir).unwrap() {
                let asset = asset.unwrap();
                fs::copy(asset.path(), target.join(asset.file_name())).unwrap();
            }
        }
        let snapshot =
            crate::library::build_library_snapshot(detached.path(), "", "detached".into(), || {
                Ok(())
            })
            .unwrap();
        let mut index = Index::open(cache.path(), detached.path()).unwrap();
        while !index.reconcile_clips(&snapshot, &|| Ok(())).unwrap() {}
        let recovered = index.tag_choices("final").unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id, tag.id);
        let query = ClipGridQuery {
            tags: vec![tag.id.clone()],
            ..Default::default()
        };
        let page = index
            .clip_grid_page(&snapshot, &query, None, "http://fixture", "fixture")
            .unwrap();
        assert_eq!(page.clips.len(), 2);
        assert!(
            page.clips
                .iter()
                .all(|clip| clip.tags.len() == 2 && clip.tags[0].tier == Some(5))
        );

        let approved = ApprovedRoot::new(detached.path()).unwrap();
        let owner = LibraryMutation::acquire(detached.path(), &approved).unwrap();
        let collection = owner.clips().unwrap();
        let mut clip = ClipDescriptor::inspect(&collection, headers[0].0.clone()).unwrap();
        let _directory = checked_directory(&clip, false).unwrap();
        let mut header = clip.read_header().unwrap();
        header.tags.retain(|assigned| assigned.id != tag.id);
        write_clip_header(&mut clip, &header).unwrap();
        index.reproject_clip(&clip).unwrap();
        assert!(index.clip_ready(&snapshot.managed_clip_stamp));
        let page = index
            .clip_grid_page(&snapshot, &query, None, "http://fixture", "fixture")
            .unwrap();
        assert_eq!(page.clips.len(), 1);
        assert_ne!(page.clips[0].id, clip.id);
    }
}
