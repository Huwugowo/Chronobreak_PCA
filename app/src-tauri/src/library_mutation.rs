//! Filesystem authority and cross-process arbitration for app library mutations.

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::directory_guard::{DirectoryGuard, guard_root, open_regular};
use crate::playback_file::ApprovedRoot;

pub(crate) struct LibraryMutation {
    // Drop the lock before releasing the directory which pins its name.
    _lease: File,
    root: DirectoryGuard,
}

impl LibraryMutation {
    pub(crate) fn acquire(logical: &Path, approved: &ApprovedRoot) -> Result<Self> {
        let root = guard_root(approved)?;
        ensure!(
            &ApprovedRoot::new(logical)? == approved,
            "library alias changed since selection"
        );
        let lease = open_regular(&root.path.join(".chronobreak-mutation.lease"), true, true)?;
        match lease.try_lock() {
            Ok(()) => Ok(Self {
                _lease: lease,
                root,
            }),
            Err(std::fs::TryLockError::WouldBlock) => anyhow::bail!("library mutation busy"),
            Err(error) => Err(error).context("cannot acquire library mutation ownership"),
        }
    }

    pub(crate) fn games(&self) -> Result<DirectoryGuard> {
        self.root.child("games", false)
    }
    pub(crate) fn optional_games(&self) -> Result<Option<DirectoryGuard>> {
        self.root.optional_child("games")
    }
    pub(crate) fn unmatched(&self) -> Result<DirectoryGuard> {
        self.root.child(crate::library::UNMATCHED_RECORDINGS, true)
    }
    pub(crate) fn optional_unmatched(&self) -> Result<Option<DirectoryGuard>> {
        self.root
            .optional_child(crate::library::UNMATCHED_RECORDINGS)
    }
    pub(crate) fn clips(&self) -> Result<DirectoryGuard> {
        self.root.child("clips", false)
    }

    /// Existing root-leaf stores keep their own validation/atomic-write contract.
    pub(crate) fn root_path(&self) -> &Path {
        &self.root.path
    }
}

#[cfg(all(test, windows))]
mod process_tests;
#[cfg(all(test, windows))]
mod tests;
