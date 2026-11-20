//! Durable library vocabulary and manual match assignments.
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Read, Write},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    directory_guard::open_regular,
    library_mutation::LibraryMutation,
    playback_file::file_facts,
    tags::{self, ManualMatchTags, TagDefinition, TagId},
};

const FILE: &str = "league_tags.json";
const VERSION: u8 = 1;
const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Store {
    version: u8,
    pub revision: u64,
    pub definitions: BTreeMap<TagId, TagDefinition>,
    pub matches: BTreeMap<String, ManualMatchTags>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: VERSION,
            revision: 0,
            definitions: BTreeMap::new(),
            matches: BTreeMap::new(),
        }
    }
}

impl Store {
    fn validate(&self) -> Result<()> {
        ensure!(self.version == VERSION, "invalid tag store version");
        let mut names: HashSet<_> = tags::builtin_definitions()
            .map(|tag| tags::label_key(&tag.label))
            .collect();
        for (id, tag) in &self.definitions {
            tag.validate()?;
            ensure!(id == &tag.id && !tag.builtin, "invalid stored tag identity");
            ensure!(
                names.insert(tags::label_key(&tag.label)),
                "duplicate tag name"
            );
        }
        for (match_group, entry) in &self.matches {
            validate_match_group(match_group)?;
            entry.validate()?;
            ensure!(
                entry
                    .ids
                    .iter()
                    .all(|id| id.is_builtin() || self.definitions.contains_key(id)),
                "unknown match tag definition"
            );
        }
        Ok(())
    }

    pub(crate) fn insert_definition(&mut self, tag: TagDefinition) -> Result<()> {
        tag.validate()?;
        ensure!(!tag.builtin, "built-in tags cannot be changed");
        let key = tags::label_key(&tag.label);
        ensure!(
            !tags::builtin_definitions()
                .chain(self.definitions.values().cloned())
                .any(|other| other.id != tag.id && tags::label_key(&other.label) == key),
            "a tag already uses that name"
        );
        self.definitions.insert(tag.id.clone(), tag);
        Ok(())
    }

    /// The caller holds the same root lease used for clip and recording mutations.
    pub(crate) fn publish(&mut self, owner: &LibraryMutation) -> Result<String> {
        self.revision = self
            .revision
            .checked_add(1)
            .context("tag revision exhausted")?;
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(bytes.len() as u64 <= MAX_BYTES, "tag store capacity");
        atomicwrites::AtomicFile::new(owner.root_path().join(FILE), atomicwrites::AllowOverwrite)
            .write(|file| {
                file.write_all(&bytes)?;
                file.sync_all()
            })
            .map_err(|error| anyhow::anyhow!("tag store publication: {error}"))?;
        current_stamp(owner.root_path())
    }
}

pub(crate) fn validate_match_group(key: &str) -> Result<()> {
    ensure!(
        key.len() == 64
            && key
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "invalid match identity"
    );
    Ok(())
}

pub(crate) struct LoadedStore {
    pub store: Store,
    pub stamp: String,
}

fn open(root: &Path) -> Result<Option<fs::File>> {
    let path = root.join(FILE);
    match fs::symlink_metadata(&path) {
        Ok(_) => Ok(Some(open_regular(&path, false, false)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn current_stamp(root: &Path) -> Result<String> {
    match open(root)? {
        Some(file) => Ok(file_facts(&file)?.stamp()),
        None => Ok("absent".into()),
    }
}

pub(crate) fn load(root: &Path) -> Result<LoadedStore> {
    let Some(file) = open(root)? else {
        return Ok(LoadedStore {
            store: Store::default(),
            stamp: "absent".into(),
        });
    };
    let facts = file_facts(&file)?;
    ensure!(
        (1..=MAX_BYTES).contains(&facts.length()),
        "invalid tag store size"
    );
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_BYTES, "tag store capacity");
    let store: Store = serde_json::from_slice(&bytes).context("invalid tag store")?;
    store.validate()?;
    Ok(LoadedStore {
        store,
        stamp: facts.stamp(),
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::playback_file::ApprovedRoot;

    #[test]
    fn revisions_preserve_definitions_and_manual_assignments_across_reopen() {
        let root = tempfile::tempdir().unwrap();
        let approved = ApprovedRoot::new(root.path()).unwrap();
        let owner = LibraryMutation::acquire(root.path(), &approved).unwrap();
        let mut loaded = load(owner.root_path()).unwrap();
        let tag = TagDefinition::custom("Clutch").unwrap();
        loaded.store.insert_definition(tag.clone()).unwrap();
        let match_group = "a".repeat(64);
        let mut manual = ManualMatchTags::default();
        manual.replace(vec![tag.id.clone()]).unwrap();
        loaded.store.matches.insert(match_group.clone(), manual);
        let stamp = loaded.store.publish(&owner).unwrap();
        let reopened = load(owner.root_path()).unwrap();
        assert_eq!(reopened.stamp, stamp);
        assert_eq!(reopened.store.revision, 1);
        assert_eq!(reopened.store.definitions.get(&tag.id), Some(&tag));
        assert_eq!(
            tags::effective_assignments(
                [TagId::parse("builtin:pentakill").unwrap()],
                &reopened.store.matches[&match_group]
            ),
            vec![
                (
                    TagId::parse("builtin:pentakill").unwrap(),
                    tags::TagAssignmentSource::Detected
                ),
                (tag.id, tags::TagAssignmentSource::Manual),
            ]
        );
    }
}
