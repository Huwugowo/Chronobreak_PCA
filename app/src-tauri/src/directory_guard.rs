//! Retained Windows directory authority shared by staging and library mutations.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::playback_file::ApprovedRoot;

/// Denying delete sharing pins this directory's name until the handle closes.
pub(crate) struct DirectoryGuard {
    file: File,
    pub(crate) path: PathBuf,
}

impl DirectoryGuard {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let file = open_checked(path, false, false, false)?;
        ensure!(
            file.metadata()?.is_dir(),
            "guarded target is not a directory"
        );
        Ok(Self {
            file,
            path: path.to_owned(),
        })
    }

    pub(crate) fn child(&self, name: &str, create: bool) -> Result<Self> {
        let path = child_path(&self.path, OsStr::new(name))?;
        if create {
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("cannot create guarded directory"),
            }
        }
        Self::open(&path)
    }

    pub(crate) fn optional_child(&self, name: &str) -> Result<Option<Self>> {
        match self.child(name, false) {
            Ok(child) => Ok(Some(child)),
            Err(error) if is_missing(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn removal_target(&self, name: &OsStr) -> Result<RemovalTarget<'_>> {
        RemovalTarget::open(&self.path, name)
    }
}

pub(crate) fn guard_root(root: &ApprovedRoot) -> Result<DirectoryGuard> {
    let directory = DirectoryGuard::open(root.path())?;
    ensure!(
        root.matches_directory(&directory.file)?,
        "approved library root was replaced"
    );
    Ok(directory)
}

pub(crate) fn is_missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

fn child_path(parent: &Path, name: &OsStr) -> Result<PathBuf> {
    let mut components = Path::new(name).components();
    ensure!(
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none(),
        "guarded child must be one filename"
    );
    Ok(parent.join(name))
}

pub(crate) fn reject_reparse(metadata: &fs::Metadata) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        ensure!(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0,
            "guarded path contains a reparse point"
        );
    }
    #[cfg(not(windows))]
    ensure!(
        !metadata.file_type().is_symlink(),
        "guarded path contains a symlink"
    );
    Ok(())
}

pub(crate) fn open_regular(path: &Path, writable: bool, create: bool) -> Result<File> {
    let file = open_checked(path, writable, create, false)?;
    ensure!(
        file.metadata()?.is_file(),
        "guarded target is not a regular file"
    );
    Ok(file)
}

#[cfg(windows)]
fn open_checked(path: &Path, writable: bool, create: bool, delete: bool) -> Result<File> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(writable)
        .create(create)
        .truncate(false)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0);
    if delete {
        options.access_mode(FILE_GENERIC_READ.0 | DELETE.0);
    }
    let file = options
        .open(path)
        .with_context(|| format!("cannot retain filesystem authority for {}", path.display()))?;
    reject_reparse(&file.metadata()?)?;
    ensure!(
        crate::playback_file::final_path(&file)? == path,
        "guarded path identity changed"
    );
    Ok(file)
}

#[cfg(not(windows))]
fn open_checked(_path: &Path, _writable: bool, _create: bool, _delete: bool) -> Result<File> {
    anyhow::bail!("guarded filesystem mutation requires Windows")
}

/// The exact removable object, borrowing its still-pinned parent. DELETE access
/// permits removal through this handle while denying other owners name changes.
pub(crate) struct RemovalTarget<'a> {
    file: File,
    path: PathBuf,
    _parent: &'a Path,
}

impl<'a> RemovalTarget<'a> {
    fn open(parent: &'a Path, name: &OsStr) -> Result<Self> {
        Self::open_under(parent, name, parent)
    }

    fn open_under(parent: &Path, name: &OsStr, outer_parent: &'a Path) -> Result<Self> {
        let path = child_path(parent, name)?;
        let file = open_checked(&path, false, false, true)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() || metadata.is_dir(),
            "unsupported removal target"
        );
        Ok(Self {
            file,
            path,
            _parent: outer_parent,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn is_directory(&self) -> Result<bool> {
        Ok(self.file.metadata()?.is_dir())
    }

    pub(crate) fn remove_file(self) -> Result<()> {
        ensure!(
            self.file.metadata()?.is_file(),
            "removal target is not a regular file"
        );
        mark_deleted(&self.file)
    }

    pub(crate) fn remove_tree(self) -> Result<()> {
        let mut targets = Vec::new();
        let mut count = 1; // Include this root in the total handle budget.
        self.preflight(0, &mut count, &mut targets)?;
        for target in targets {
            mark_deleted(&target.file)?;
            drop(target);
        }
        mark_deleted(&self.file)
    }

    pub(crate) fn move_directory_into(self, destination: &DirectoryGuard) -> Result<()> {
        ensure!(self.is_directory()?, "move target is not a directory");
        let name = self.path.file_name().context("missing recording name")?;
        rename_no_replace(&self.file, &destination.file, name)
    }

    fn preflight(&self, depth: usize, count: &mut usize, targets: &mut Vec<Self>) -> Result<()> {
        if self.file.metadata()?.is_dir() {
            ensure!(
                depth < 64,
                "recording directory nesting exceeds the removal limit"
            );
            for entry in fs::read_dir(&self.path).context("cannot enumerate removal target")? {
                let entry = entry?;
                ensure!(
                    *count < 4096,
                    "recording exceeds the 4096-entry removal limit"
                );
                *count += 1;
                // Ancestors stay owned on this stack, then in the postorder vector.
                // All targets borrow the outer pinned collection; no reference
                // points into a movable target or vector element.
                let child = Self::open_under(&self.path, &entry.file_name(), self._parent)?;
                child.preflight(depth + 1, count, targets)?;
                targets.push(child);
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
fn rename_no_replace(source: &File, destination: &File, name: &OsStr) -> Result<()> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows::Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FileRenameInformation,
        NtSetInformationFile,
    };
    use windows::Win32::{Foundation::HANDLE, System::IO::IO_STATUS_BLOCK};

    let name: Vec<u16> = name.encode_wide().collect();
    let name_bytes = size_of_val(name.as_slice());
    let size = size_of::<FILE_RENAME_INFORMATION>() + name_bytes;
    let size_bytes = u32::try_from(size).context("recording filename is too long")?;
    let name_bytes = u32::try_from(name_bytes).context("recording filename is too long")?;
    const {
        assert!(std::mem::align_of::<FILE_RENAME_INFORMATION>() <= size_of::<usize>());
    }
    let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    let mut status = IO_STATUS_BLOCK::default();
    // SAFETY: the owned source has DELETE access; both directory handles stay
    // live. The aligned buffer includes the variable-length UTF-16 filename.
    unsafe {
        info.write(FILE_RENAME_INFORMATION {
            Anonymous: FILE_RENAME_INFORMATION_0 {
                ReplaceIfExists: false,
            },
            RootDirectory: HANDLE(destination.as_raw_handle()),
            FileNameLength: name_bytes,
            FileName: [0],
        });
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            (&raw mut (*info).FileName).cast(),
            name.len(),
        );
        NtSetInformationFile(
            HANDLE(source.as_raw_handle()),
            &mut status,
            info.cast(),
            size_bytes,
            FileRenameInformation,
        )
        .ok()
    }
    .context("cannot move recording into the unmatched folder")
}

#[cfg(not(windows))]
fn rename_no_replace(_source: &File, _destination: &File, _name: &OsStr) -> Result<()> {
    anyhow::bail!("handle-based directory moves require Windows")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn preflight_retains_every_descendant_until_effect_or_rejection() {
        let fixture = tempfile::tempdir().unwrap();
        fs::create_dir_all(fixture.path().join("game/nested")).unwrap();
        fs::write(fixture.path().join("game/nested/video"), b"fixture").unwrap();
        let approved = ApprovedRoot::new(fixture.path()).unwrap();
        let parent = guard_root(&approved).unwrap();
        let game = parent.removal_target(OsStr::new("game")).unwrap();
        let mut targets = Vec::new();
        game.preflight(0, &mut 1, &mut targets).unwrap();
        assert_eq!(targets.len(), 2);
        for target in &targets {
            assert!(
                fs::rename(target.path(), target.path().with_extension("replacement")).is_err()
            );
        }
        drop(targets);
        drop(game);
        let game = parent.removal_target(OsStr::new("game")).unwrap();
        game.remove_tree().unwrap();
        assert!(!parent.path.join("game").exists());
    }
}

#[cfg(windows)]
fn mark_deleted(file: &File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
        },
    };
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: this live, owned handle was opened with DELETE access. The correctly
    // sized disposition is valid for the call; no pathname is resolved or removed.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .context("cannot remove owned filesystem target")
}

#[cfg(not(windows))]
fn mark_deleted(_file: &File) -> Result<()> {
    anyhow::bail!("handle-based removal requires Windows")
}
