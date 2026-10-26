//! Disposable export staging. Final clips and recordings are never cleanup targets.

use std::collections::{BinaryHeap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

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

/// Keeping each directory open without delete sharing freezes this path chain.
pub(crate) struct DirectoryGuard {
    _file: File,
    path: PathBuf,
}

impl DirectoryGuard {
    #[cfg(windows)]
    fn open(path: &Path) -> Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
            .with_context(|| format!("cannot guard export directory {}", path.display()))?;
        ensure!(
            file.metadata()?.is_dir(),
            "export directory is not a directory"
        );
        reject_reparse(&file.metadata()?)?;
        ensure!(
            crate::playback_file::final_path(&file)? == path,
            "export directory identity changed"
        );
        Ok(Self {
            _file: file,
            path: path.to_owned(),
        })
    }

    #[cfg(not(windows))]
    fn open(_path: &Path) -> Result<Self> {
        anyhow::bail!("guarded export staging requires Windows")
    }

    fn child(&self, name: &str, create: bool) -> Result<Self> {
        let path = self.path.join(name);
        if create {
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("cannot create export directory"),
            }
        }
        Self::open(&path)
    }
}

fn reject_reparse(metadata: &fs::Metadata) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        ensure!(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0,
            "export staging contains a reparse point"
        );
    }
    #[cfg(not(windows))]
    ensure!(
        !metadata.file_type().is_symlink(),
        "export staging contains a symlink"
    );
    Ok(())
}

fn open_regular(path: &Path, writable: bool, create: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(writable).create(create);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        options
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    let file = options
        .open(path)
        .context("cannot open export ownership file")?;
    ensure!(
        file.metadata()?.is_file(),
        "export ownership target is not a file"
    );
    reject_reparse(&file.metadata()?)?;
    #[cfg(windows)]
    ensure!(
        crate::playback_file::final_path(&file)? == path,
        "export file identity changed"
    );
    Ok(file)
}

pub(crate) struct Lease {
    _root: DirectoryGuard,
    clips: DirectoryGuard,
    staging: DirectoryGuard,
    _lock: File,
}

pub(crate) fn guard_root(root: &ApprovedRoot) -> Result<DirectoryGuard> {
    let directory = DirectoryGuard::open(root.path())?;
    ensure!(
        root.matches_directory(&directory._file)?,
        "approved export root was replaced"
    );
    Ok(directory)
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

    pub(crate) fn begin(&self, destinations: Vec<String>) -> Result<Batch<'_>> {
        let mut id = crate::playback_policy::random_capability()?;
        id.truncate(32);
        ensure!(valid_id(&id), "invalid random export batch identity");
        let path = self.staging.path.join(&id);
        fs::create_dir(&path).context("cannot allocate export batch")?;
        let directory = DirectoryGuard::open(&path)?;
        let manifest = Manifest {
            version: 1,
            batch_id: id,
            destinations,
        };
        manifest.validate(&manifest.batch_id)?;
        let bytes = serde_json::to_vec(&manifest)?;
        ensure!(
            bytes.len() as u64 <= MANIFEST_LIMIT,
            "export manifest exceeds limit"
        );
        let mut owner = File::create_new(path.join(OWNER))?;
        owner
            .write_all(&bytes)
            .context("cannot write export ownership")?;
        owner
            .sync_all()
            .context("cannot persist export ownership")?;
        Ok(Batch {
            lease: self,
            directory: Some(directory),
            manifest,
        })
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
        let directory = self.staging.child(id, false)?;
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
    destinations: Vec<String>,
}

impl Manifest {
    fn validate(&self, id: &str) -> Result<()> {
        ensure!(
            self.version == 1 && self.batch_id == id && valid_id(id),
            "unsupported export ownership"
        );
        ensure!(
            (1..=3).contains(&self.destinations.len()),
            "invalid export member count"
        );
        let mut seen = HashSet::new();
        for destination in &self.destinations {
            let (game, clip) = destination
                .rsplit_once('_')
                .context("invalid export destination")?;
            ensure!(
                crate::library::valid_game_id(game)
                    && !clip.is_empty()
                    && clip.len() <= 20
                    && clip.bytes().all(|b| b.is_ascii_digit())
                    && destination.len() <= 80
                    && seen.insert(destination),
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
    manifest: Manifest,
}

impl Batch<'_> {
    pub(crate) fn staged(&self, member: usize, extension: &str) -> PathBuf {
        self.directory
            .as_ref()
            .expect("live export batch")
            .path
            .join(format!("{member}.{extension}"))
    }

    pub(crate) fn publish(&self, member: usize, extension: &str) -> Result<()> {
        ensure!(
            member < self.manifest.destinations.len() && matches!(extension, "mp4" | "jpg"),
            "invalid export publication"
        );
        let source = self.staged(member, extension);
        // Validate a regular, unreplaced source before moving it. The directory
        // remains guarded; no manifest-supplied path is used for cleanup.
        drop(open_regular(&source, false, false)?);
        let destination = self.lease.clips.path.join(format!(
            "{}.{}",
            self.manifest.destinations[member], extension
        ));
        move_no_replace(&source, &destination)
            .context("cannot publish clip without replacing an existing file")
    }
}

impl Drop for Batch<'_> {
    fn drop(&mut self) {
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
    let mut names = HashSet::from([OWNER.to_owned()]);
    for member in 0..manifest.destinations.len() {
        for extension in ["mp4", "jpg"] {
            names.insert(format!("{member}.{extension}"));
        }
    }
    // Validate the entire flat directory before removing even the first member.
    for entry in fs::read_dir(&directory.path)? {
        let entry = entry?;
        ensure!(
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| names.contains(name)),
            "foreign export staging entry retained"
        );
        let metadata = fs::symlink_metadata(entry.path())?;
        reject_reparse(&metadata)?;
        ensure!(
            metadata.is_file(),
            "nonregular export staging entry retained"
        );
    }
    for name in names.iter().filter(|name| name.as_str() != OWNER) {
        match fs::remove_file(directory.path.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("cannot remove abandoned export member"),
        }
    }
    fs::remove_file(directory.path.join(OWNER)).context("cannot remove export ownership")?;
    let path = directory.path.clone();
    drop(directory);
    fs::remove_dir(path).context("cannot remove empty export batch")
}

#[cfg(windows)]
fn move_no_replace(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{MOVE_FILE_FLAGS, MoveFileExW};
    use windows::core::PCWSTR;
    let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<_> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both strings are NUL-terminated and live through this call. Flags
    // prohibit replacement and cross-volume copy; errors preserve existing files.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVE_FILE_FLAGS(0),
        )
    }
    .context("atomic no-replace move failed")
}

#[cfg(not(windows))]
fn move_no_replace(_source: &Path, _destination: &Path) -> Result<()> {
    anyhow::bail!("atomic export publication requires Windows")
}

#[cfg(all(test, windows))]
#[path = "export_staging_tests.rs"]
mod tests;
