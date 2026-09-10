//! Validate the opened file, then hand that same handle to the streaming reader.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct ApprovedRoot(PathBuf);

impl ApprovedRoot {
    pub fn new(path: &Path) -> Result<Self> {
        let file = open_directory(path)?;
        if !file.metadata()?.is_dir() {
            bail!("media root is not a directory")
        }
        Ok(Self(final_path(&file)?))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn contains(&self, candidate: &Path) -> bool {
        let mut candidate = candidate.components();
        self.0.components().all(|part| {
            candidate
                .next()
                .is_some_and(|other| component_eq(part.as_os_str(), other.as_os_str()))
        })
    }
}

pub(crate) enum FileScope<'a> {
    Root(&'a ApprovedRoot),
    Exact(&'a Path),
}

pub(crate) fn open_file(path: &Path, scope: FileScope<'_>) -> Result<std::fs::File> {
    let file = std::fs::File::open(path).context("could not open media file")?;
    if !file.metadata()?.is_file() {
        bail!("media target is not a regular file")
    }
    let resolved = final_path(&file)?;
    let allowed = match scope {
        FileScope::Root(root) => root.contains(&resolved),
        FileScope::Exact(expected) => path_eq(expected, &resolved),
    };
    if !allowed {
        bail!("media target is outside its approved location")
    }
    Ok(file)
}

pub(crate) fn approve_import(path: &Path) -> Result<PathBuf> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp3") || ext.eq_ignore_ascii_case("wav"))
    {
        bail!("only MP3 and WAV previews are supported")
    }
    let file = std::fs::File::open(path).context("could not open imported preview")?;
    if !file.metadata()?.is_file() {
        bail!("imported preview is not a regular file")
    }
    final_path(&file)
}

fn path_eq(a: &Path, b: &Path) -> bool {
    a.components().count() == b.components().count()
        && a.components()
            .zip(b.components())
            .all(|(a, b)| component_eq(a.as_os_str(), b.as_os_str()))
}

#[cfg(windows)]
fn component_eq(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
    let a: Vec<u16> = a.encode_wide().collect();
    let b: Vec<u16> = b.encode_wide().collect();
    // SAFETY: both slices remain alive and the generated binding supplies their lengths.
    unsafe { CompareStringOrdinal(&a, &b, true) == CSTR_EQUAL }
}

#[cfg(windows)]
fn open_directory(path: &Path) -> Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
    Ok(std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)?)
}

#[cfg(windows)]
fn final_path(file: &std::fs::File) -> Result<PathBuf> {
    use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW},
    };
    // Windows normalized DOS names are extended paths for both directory and file handles.
    let mut buffer = vec![0_u16; 1024];
    // SAFETY: file owns a valid handle for the call; buffer is writable and length-bound.
    let mut count = unsafe {
        GetFinalPathNameByHandleW(
            HANDLE(file.as_raw_handle()),
            &mut buffer,
            FILE_NAME_NORMALIZED,
        )
    } as usize;
    if count >= buffer.len() && count <= 32_768 {
        buffer.resize(count, 0);
        // SAFETY: the same owned handle and the resized, bounded writable buffer.
        count = unsafe {
            GetFinalPathNameByHandleW(
                HANDLE(file.as_raw_handle()),
                &mut buffer,
                FILE_NAME_NORMALIZED,
            )
        } as usize;
    }
    if count == 0 {
        return Err(std::io::Error::last_os_error())
            .context("could not resolve opened media handle");
    }
    if count >= buffer.len() {
        bail!("opened media path exceeds Windows path limit")
    }
    buffer.truncate(count);
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer)))
}

// Windows is the supported desktop delivery target. Never fall back to a racy
// path-canonicalize/reopen implementation on an unimplemented platform.
#[cfg(not(windows))]
fn component_eq(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    a == b
}
#[cfg(not(windows))]
fn open_directory(_path: &Path) -> Result<std::fs::File> {
    bail!("opened-handle media containment requires Windows")
}
#[cfg(not(windows))]
fn final_path(_file: &std::fs::File) -> Result<PathBuf> {
    bail!("opened-handle media containment requires Windows")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::io::Read;

    fn junction(link: &Path, target: &Path) {
        // Both paths are explicit children of the test's dedicated temporary tree.
        let output = std::process::Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "could not create temporary Windows junction"
        );
    }

    #[test]
    fn junctions_and_root_replacement_cannot_redirect_delivery_outside_snapshot() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("library");
        let outside = fixture.path().join("outside");
        std::fs::create_dir_all(root.join("inside")).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(root.join("inside/video.mp4"), b"approved").unwrap();
        std::fs::write(outside.join("video.mp4"), b"outside").unwrap();
        let approved = ApprovedRoot::new(&root).unwrap();
        junction(&root.join("external"), &outside);
        junction(&root.join("internal"), &root.join("inside"));
        assert!(open_file(&root.join("external/video.mp4"), FileScope::Root(&approved)).is_err());
        assert!(open_file(&root.join("internal/video.mp4"), FileScope::Root(&approved)).is_ok());
        assert!(open_file(&outside.join("video.mp4"), FileScope::Root(&approved)).is_err());
        // Path selection precedes replacement: final handle validation must still deny it.
        let selected = root.join("video.mp4");
        std::fs::rename(&root, fixture.path().join("previous-library")).unwrap();
        junction(&root, &outside);
        assert!(open_file(&selected, FileScope::Root(&approved)).is_err());
        let changed_root = ApprovedRoot::new(&root).unwrap();
        assert!(open_file(&selected, FileScope::Root(&changed_root)).is_ok());
    }

    #[test]
    fn checked_handle_survives_path_replacement_without_reopening() {
        let fixture = tempfile::tempdir().unwrap();
        let root = ApprovedRoot::new(fixture.path()).unwrap();
        let path = fixture.path().join("video.mp4");
        std::fs::write(&path, b"approved-handle").unwrap();
        let mut opened = open_file(&path, FileScope::Root(&root)).unwrap();
        std::fs::rename(&path, fixture.path().join("old.mp4")).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        let mut bytes = Vec::new();
        opened.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"approved-handle");
    }

    #[test]
    fn import_approves_one_exact_file_and_rejects_other_types_and_directories() {
        let fixture = tempfile::tempdir().unwrap();
        let first = fixture.path().join("one.WAV");
        let second = fixture.path().join("two.mp3");
        let unsupported = fixture.path().join("secret.json");
        for path in [&first, &second, &unsupported] {
            std::fs::write(path, b"fixture").unwrap();
        }
        let approved = approve_import(&first).unwrap();
        assert!(open_file(&first, FileScope::Exact(&approved)).is_ok());
        assert!(open_file(&second, FileScope::Exact(&approved)).is_err());
        assert!(approve_import(&unsupported).is_err());
        std::fs::create_dir(fixture.path().join("directory.wav")).unwrap();
        assert!(approve_import(&fixture.path().join("directory.wav")).is_err());
    }

    #[test]
    fn containment_is_component_aware_and_uses_windows_ordinal_case() {
        let fixture = tempfile::tempdir().unwrap();
        let root = ApprovedRoot::new(fixture.path()).unwrap();
        assert!(root.contains(&root.path().join("games/video.mp4")));
        assert!(!root.contains(&PathBuf::from(format!(
            "{}-sibling/video.mp4",
            root.path().display()
        ))));
        assert!(component_eq(
            std::ffi::OsStr::new("Library"),
            std::ffi::OsStr::new("LIBRARY")
        ));
    }
}
