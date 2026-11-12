//! Disposable export staging. Final clips and recordings are never cleanup targets.

use std::collections::{BinaryHeap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::clip_bundle::{ClipId, EVENTS, HEADER, THUMBNAIL, title_from_leaf};
use crate::directory_guard::{DirectoryGuard, guard_root, open_regular};
use crate::playback_file::ApprovedRoot;

const STAGING: &str = ".chronobreak-export";
const OWNER: &str = "owner.json";
const MANIFEST_LIMIT: u64 = 8 * 1024;
const BATCH_LIMIT: usize = 64;
static CURSOR: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryMode {
    Normal,
    ReadOnly,
}

#[derive(Debug, Default)]
pub(crate) struct RecoveryReport {
    pub recovered: usize,
    pub retained: usize,
    pub deferred: usize,
    pub active: bool,
    diagnostics: Vec<String>,
}

impl RecoveryReport {
    fn retain(&mut self, error: impl std::fmt::Display) {
        self.retained = self.retained.saturating_add(1);
        if self.diagnostics.len() < 4 {
            self.diagnostics
                .push(error.to_string().chars().take(320).collect());
        }
    }

    pub(crate) fn log(&self) {
        if let Some(summary) = self.summary() {
            eprintln!("{summary}");
        }
    }

    fn summary(&self) -> Option<String> {
        (self.active || self.recovered > 0 || self.retained > 0 || self.deferred > 0).then(|| {
            format!(
                "Chronobreak export recovery (active={}): {self:?}",
                self.active
            )
        })
    }
}

pub(crate) struct Lease {
    _root: DirectoryGuard,
    clips: DirectoryGuard,
    staging: DirectoryGuard,
    _lock: File,
}

impl Lease {
    pub(crate) fn acquire(root: &ApprovedRoot, create: bool) -> Result<Option<Self>> {
        let root = guard_root(root)?;
        if !create && !root.path.join("clips").join(STAGING).try_exists()? {
            return Ok(None);
        }
        let clips = root.child("clips", create)?;
        let staging = clips.child(STAGING, create)?;
        let lock = open_regular(&staging.path.join(".lease"), true, true)?;
        match lock.try_lock() {
            Ok(()) => Ok(Some(Self {
                _root: root,
                clips,
                staging,
                _lock: lock,
            })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(error) => Err(error).context("cannot acquire export recovery lease"),
        }
    }

    pub(crate) fn clips(&self) -> &Path {
        &self.clips.path
    }

    pub(crate) fn begin(&self, destinations: Vec<Destination>) -> Result<Batch<'_>> {
        let mut id = crate::playback_policy::random_capability()?;
        id.truncate(32);
        ensure!(valid_id(&id), "invalid random export batch identity");
        let manifest = Manifest {
            version: 2,
            batch_id: id,
            destinations,
        };
        manifest.validate(&manifest.batch_id)?;
        let bytes = serde_json::to_vec(&manifest)?;
        ensure!(
            bytes.len() as u64 <= MANIFEST_LIMIT,
            "export manifest exceeds limit"
        );
        let path = self.staging.path.join(&manifest.batch_id);
        fs::create_dir(&path).context("cannot allocate export batch")?;
        let directory = DirectoryGuard::open_movable(&path)?;
        let mut owner = File::create_new(path.join(OWNER))?;
        owner
            .write_all(&bytes)
            .context("cannot write export ownership")?;
        owner
            .sync_all()
            .context("cannot persist export ownership")?;
        drop(owner);
        let mut batch = Batch {
            lease: self,
            directory: Some(directory),
            members: Vec::with_capacity(manifest.destinations.len()),
            manifest,
        };
        for destination in &batch.manifest.destinations {
            let member = path.join(destination.clip_id.as_str());
            fs::create_dir(&member).context("cannot allocate staged clip bundle")?;
            batch
                .members
                .push(Some(DirectoryGuard::open_movable(&member)?));
        }
        Ok(batch)
    }

    pub(crate) fn recover(&self, check_current: impl Fn() -> Result<()>) -> Result<RecoveryReport> {
        check_current()?;
        let cursor = CURSOR
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .filter(|(root, _)| root == &self.staging.path)
            .map(|(_, last)| last.clone());
        let mut report = RecoveryReport::default();
        let (mut names, total, foreign) = self.select(cursor.as_deref(), &check_current)?;
        if foreign > 0 {
            report.retain(format!("{foreign} unrecognized staging entries retained"));
            report.retained = foreign;
        }
        if names.is_empty() && cursor.is_some() {
            (names, _, _) = self.select(None, &check_current)?;
        }
        report.deferred = total.saturating_sub(names.len());
        for id in &names {
            check_current()?;
            match self.recover_batch(id) {
                Ok(()) => report.recovered += 1,
                Err(error) => report.retain(format!("{id}: {error:#}")),
            }
        }
        if let Some(last) = names.last() {
            *CURSOR.lock().unwrap_or_else(|p| p.into_inner()) =
                Some((self.staging.path.clone(), last.clone()));
        }
        Ok(report)
    }

    fn select(
        &self,
        after: Option<&str>,
        check: &impl Fn() -> Result<()>,
    ) -> Result<(Vec<String>, usize, usize)> {
        let mut selected = BinaryHeap::with_capacity(BATCH_LIMIT + 1);
        let mut total = 0_usize;
        let mut foreign = 0_usize;
        for entry in fs::read_dir(&self.staging.path)? {
            check()?;
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|name| valid_id(name)) else {
                if name != ".lease" {
                    foreign = foreign.saturating_add(1);
                }
                continue;
            };
            total = total.saturating_add(1);
            if after.is_none_or(|after| name > after) {
                selected.push(name.to_owned());
                if selected.len() > BATCH_LIMIT {
                    selected.pop();
                }
            }
        }
        Ok((selected.into_sorted_vec(), total, foreign))
    }

    fn recover_batch(&self, id: &str) -> Result<()> {
        let directory = DirectoryGuard::open_movable(&self.staging.path.join(id))?;
        let owner = open_regular(&directory.path.join(OWNER), false, false)?;
        ensure!(
            owner.metadata()?.len() <= MANIFEST_LIMIT,
            "oversized export ownership"
        );
        let mut bytes = Vec::new();
        owner.take(MANIFEST_LIMIT + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MANIFEST_LIMIT,
            "oversized export ownership"
        );
        let manifest: Manifest =
            serde_json::from_slice(&bytes).context("invalid export ownership")?;
        manifest.validate(id)?;
        cleanup(directory, &manifest)
    }
}

pub(crate) fn recover(
    root: &ApprovedRoot,
    mode: RecoveryMode,
    check: impl Fn() -> Result<()>,
) -> Result<RecoveryReport> {
    if mode == RecoveryMode::ReadOnly {
        return Ok(RecoveryReport::default());
    }
    let Some(lease) = Lease::acquire(root, false)? else {
        return Ok(RecoveryReport {
            active: root.path().join("clips").join(STAGING).try_exists()?,
            ..RecoveryReport::default()
        });
    };
    lease.recover(check)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    batch_id: String,
    destinations: Vec<Destination>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Destination {
    pub clip_id: ClipId,
    pub video_leaf: String,
}

impl Destination {
    fn leaves(&self) -> [&str; 4] {
        [&self.video_leaf, THUMBNAIL, HEADER, EVENTS]
    }
}

impl Manifest {
    fn validate(&self, id: &str) -> Result<()> {
        ensure!(
            self.version == 2 && self.batch_id == id && valid_id(id),
            "unsupported export ownership"
        );
        ensure!(
            (1..=3).contains(&self.destinations.len()),
            "invalid export member count"
        );
        let mut seen = HashSet::new();
        for destination in &self.destinations {
            ensure!(
                title_from_leaf(&destination.video_leaf).is_some()
                    && seen.insert(&destination.clip_id),
                "invalid export destination"
            );
        }
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) struct Batch<'a> {
    lease: &'a Lease,
    directory: Option<DirectoryGuard>,
    members: Vec<Option<DirectoryGuard>>,
    manifest: Manifest,
}

impl Batch<'_> {
    pub(crate) fn staged(&self, member: usize, leaf: &str) -> PathBuf {
        let leaf = match leaf {
            "mp4" => &self.manifest.destinations[member].video_leaf,
            "jpg" => THUMBNAIL,
            HEADER | EVENTS => leaf,
            _ => unreachable!("invalid clip staging leaf"),
        };
        self.members[member]
            .as_ref()
            .expect("unpublished clip member")
            .path
            .join(leaf)
    }

    pub(crate) fn validate_members(&self) -> Result<()> {
        for (guard, destination) in self.members.iter().zip(&self.manifest.destinations) {
            let directory = guard.as_ref().context("missing staged clip bundle")?;
            let leaves = destination.leaves();
            let mut count = 0;
            for entry in fs::read_dir(&directory.path)? {
                let entry = entry?;
                ensure!(
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|leaf| leaves.contains(&leaf)),
                    "foreign staged clip asset"
                );
                let asset = open_regular(&entry.path(), false, false)?;
                ensure!(asset.metadata()?.len() > 0, "empty staged clip asset");
                count += 1;
            }
            ensure!(count == leaves.len(), "incomplete staged clip bundle");
        }
        Ok(())
    }

    pub(crate) fn publish(&mut self, member: usize) -> Result<()> {
        let member = self
            .members
            .get_mut(member)
            .context("invalid staged clip member")?;
        let source = member
            .as_ref()
            .context("clip bundle was already published")?;
        source
            .move_into(&self.lease.clips)
            .context("cannot publish clip without replacing an existing bundle")?;
        drop(member.take());
        Ok(())
    }
}

impl Drop for Batch<'_> {
    fn drop(&mut self) {
        self.members.clear();
        if let Some(directory) = self.directory.take()
            && let Err(error) = cleanup(directory, &self.manifest)
        {
            eprintln!(
                "Chronobreak export staging retained: {}",
                error.to_string().chars().take(320).collect::<String>()
            );
        }
    }
}

fn cleanup(directory: DirectoryGuard, manifest: &Manifest) -> Result<()> {
    let path = directory.path.clone();
    directory.remove_owned_tree(|entry, is_dir| {
        let Ok(relative) = entry.strip_prefix(&path) else {
            return false;
        };
        let mut parts = relative.iter();
        let Some(first) = parts.next().and_then(|part| part.to_str()) else {
            return false;
        };
        let second = parts.next().and_then(|part| part.to_str());
        if parts.next().is_some() {
            return false;
        }
        if first == OWNER {
            return !is_dir && second.is_none();
        }
        manifest.destinations.iter().any(|destination| {
            destination.clip_id.as_str() == first
                && if is_dir {
                    second.is_none()
                } else {
                    second.is_some_and(|leaf| destination.leaves().contains(&leaf))
                }
        })
    })
}

#[cfg(all(test, windows))]
#[path = "export_staging_tests.rs"]
mod tests;
