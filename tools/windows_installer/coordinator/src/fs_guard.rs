use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::os::windows::{
    ffi::OsStrExt,
    fs::OpenOptionsExt,
    io::{AsRawHandle, FromRawHandle},
};
use std::path::{Component, Path, Prefix};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows::Wdk::Storage::FileSystem::{
    FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FileRenameInformation, NtSetInformationFile,
};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, HANDLE,
};
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::{Diagnostics::ToolHelp::*, IO::IO_STATUS_BLOCK, Threading::*};
use windows::core::{PCWSTR, PWSTR};

use crate::Member;

const READ_ATTRIBUTES: u32 = 0x80;
const DELETE_ACCESS: u32 = 0x10000;
const GENERIC_READ_ACCESS: u32 = 0x80000000;
const NO_FOLLOW: u32 = FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_BACKUP_SEMANTICS.0;
const SHARE_ALL: u32 = FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Identity {
    pub volume: u32,
    pub index: u64,
}

pub struct Node {
    file: File,
    pub id: Identity,
    pub directory: bool,
}

fn handle(file: &File) -> HANDLE {
    HANDLE(file.as_raw_handle())
}

pub fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}

pub fn image_names(path: &Path, id: Identity) -> Result<Vec<(OsString, Identity)>> {
    use std::os::windows::ffi::OsStringExt;

    let long = path
        .file_name()
        .context("missing executable name")?
        .to_owned();
    let input = wide(path.as_os_str());
    let mut short = vec![0; 32768];
    // SAFETY: the input is NUL-terminated and the output buffer is writable.
    let length = unsafe { GetShortPathNameW(PCWSTR(input.as_ptr()), Some(&mut short)) } as usize;
    ensure!(
        length > 0 && length < short.len(),
        "cannot verify the legacy executable's Windows names"
    );
    let short = OsString::from_wide(&short[..length]);
    let short = Path::new(&short)
        .file_name()
        .context("missing short executable name")?;
    let mut names = vec![(long, id)];
    if !short.eq_ignore_ascii_case(&names[0].0) {
        names.push((short.to_owned(), id));
    }
    Ok(names)
}

pub fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

impl Node {
    pub fn raw_handle(&self) -> HANDLE {
        handle(&self.file)
    }

    fn open(path: &Path, access: u32, share: u32) -> Result<Self> {
        let file = OpenOptions::new()
            .access_mode(access)
            .share_mode(share)
            .custom_flags(NO_FOLLOW)
            .open(path)
            .with_context(|| format!("cannot pin {}", path.display()))?;
        Self::inspect(file)
    }

    fn inspect(file: File) -> Result<Self> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the file and writable output are valid for this call.
        unsafe { GetFileInformationByHandle(handle(&file), &mut info) }?;
        ensure!(
            info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0,
            "redirected setup path"
        );
        let directory = info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
        ensure!(
            directory || info.nNumberOfLinks == 1,
            "setup file has multiple names"
        );
        Ok(Self {
            file,
            id: Identity {
                volume: info.dwVolumeSerialNumber,
                index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            },
            directory,
        })
    }

    pub fn file_identity(path: &Path) -> Result<Self> {
        let node = Self::open(path, READ_ATTRIBUTES, SHARE_ALL)?;
        ensure!(!node.directory, "expected an owned executable");
        Ok(node)
    }

    pub fn deny_execution_by_id(volume: &Self, id: Identity) -> Result<Self> {
        ensure!(
            volume.id.volume == id.volume,
            "executable is on a different volume"
        );
        let descriptor = FILE_ID_DESCRIPTOR {
            dwSize: size_of::<FILE_ID_DESCRIPTOR>() as u32,
            Type: FileIdType,
            Anonymous: FILE_ID_DESCRIPTOR_0 {
                FileId: id.index.cast_signed(),
            },
        };
        // SAFETY: volume is a live file handle on the verified NTFS volume and
        // descriptor identifies the previously inspected executable.
        let raw = unsafe {
            OpenFileById(
                handle(&volume.file),
                &descriptor,
                GENERIC_READ_ACCESS,
                FILE_SHARE_DELETE,
                None,
                FILE_FLAG_OPEN_REPARSE_POINT,
            )
        }?;
        // SAFETY: Windows returned a newly owned file handle.
        let file = unsafe { File::from_raw_handle(raw.0) };
        let node = Self::inspect(file)?;
        ensure!(
            !node.directory && node.id == id,
            "legacy executable identity changed"
        );
        Ok(node)
    }

    pub fn boundaries(path: &Path) -> Result<Vec<Self>> {
        let mut parts = path.components();
        let Some(Component::Prefix(prefix)) = parts.next() else {
            bail!("setup path must be on a local drive");
        };
        ensure!(
            matches!(prefix.kind(), Prefix::Disk(_)),
            "unsupported setup path"
        );
        ensure!(
            parts.next() == Some(Component::RootDir),
            "setup path is not absolute"
        );
        ensure!(
            parts.all(|part| matches!(part, Component::Normal(_))),
            "unsupported setup path components"
        );
        let mut ancestors: Vec<_> = path.ancestors().filter(|p| p.has_root()).collect();
        ancestors.reverse();
        let mut pins = Vec::new();
        for ancestor in ancestors {
            let pin = Self::open(
                ancestor,
                READ_ATTRIBUTES | 1,
                FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0,
            )?;
            ensure!(pin.directory, "setup parent is not a directory");
            if pins.is_empty() {
                let root = wide(ancestor.as_os_str());
                let mut filesystem = [0; 32];
                // SAFETY: NUL-terminated drive root and live volume handle.
                let drive_type = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
                ensure!(drive_type == 3, "setup requires a fixed local drive");
                // SAFETY: the handle and filesystem output buffer are live.
                unsafe {
                    GetVolumeInformationByHandleW(
                        handle(&pin.file),
                        None,
                        None,
                        None,
                        None,
                        Some(&mut filesystem),
                    )
                }?;
                ensure!(
                    filesystem.starts_with(&[78, 84, 70, 83, 0]),
                    "setup requires NTFS"
                );
            }
            pins.push(pin);
        }
        Ok(pins)
    }

    pub fn directory(path: &Path, movable: bool) -> Result<Self> {
        let access = READ_ATTRIBUTES | if movable { DELETE_ACCESS } else { 0 };
        let share = if movable {
            FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0
        } else {
            SHARE_ALL
        };
        let node = Self::open(path, access, share)?;
        ensure!(node.directory, "expected an owned setup directory");
        Ok(node)
    }

    pub fn file(path: &Path, movable: bool, deny_launch: bool) -> Result<Self> {
        let access = GENERIC_READ_ACCESS | if movable { DELETE_ACCESS } else { 0 };
        let share = if deny_launch { 0 } else { FILE_SHARE_READ.0 };
        let node = Self::open(path, access, share)?;
        ensure!(!node.directory, "expected an owned setup file");
        Ok(node)
    }

    pub fn executable(path: &Path) -> Result<Self> {
        let node = Self::open(path, GENERIC_READ_ACCESS, FILE_SHARE_READ.0)?;
        ensure!(!node.directory, "expected a setup executable");
        Ok(node)
    }

    pub fn read(&self, limit: u64) -> Result<Vec<u8>> {
        ensure!(
            self.file.metadata()?.len() <= limit,
            "setup state is too large"
        );
        let mut bytes = Vec::new();
        (&self.file).rewind()?;
        (&self.file).take(limit + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() as u64 <= limit, "setup state is too large");
        Ok(bytes)
    }

    pub fn matches(&self, member: &Member) -> Result<()> {
        ensure!(
            self.file.metadata()?.len() == member.bytes,
            "setup file length changed"
        );
        (&self.file).rewind()?;
        let mut hasher = Sha256::new();
        let bytes = io::copy(&mut (&self.file).take(member.bytes + 1), &mut hasher)?;
        ensure!(
            bytes == member.bytes && format!("{:x}", hasher.finalize()) == member.sha256,
            "setup file hash changed"
        );
        Ok(())
    }

    pub fn rename(&self, destination: &Self, name: &OsStr, replace: bool) -> Result<()> {
        ensure!(
            self.id.volume == destination.id.volume,
            "setup move crosses volumes"
        );
        ensure!(
            Path::new(name).components().count() == 1,
            "invalid setup destination"
        );
        let encoded: Vec<_> = name.encode_wide().collect();
        let length = size_of_val(encoded.as_slice());
        let size = size_of::<FILE_RENAME_INFORMATION>() + length;
        let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: the source has DELETE access, the parent handle remains live, and
        // the aligned buffer holds the structure and its variable-length name.
        unsafe {
            info.write(FILE_RENAME_INFORMATION {
                Anonymous: FILE_RENAME_INFORMATION_0 {
                    ReplaceIfExists: replace,
                },
                RootDirectory: handle(&destination.file),
                FileNameLength: u32::try_from(length)?,
                FileName: [0],
            });
            std::ptr::copy_nonoverlapping(
                encoded.as_ptr(),
                (&raw mut (*info).FileName).cast(),
                encoded.len(),
            );
            NtSetInformationFile(
                handle(&self.file),
                &mut status,
                info.cast(),
                u32::try_from(size)?,
                FileRenameInformation,
            )
            .ok()
        }
        .with_context(|| {
            format!(
                "cannot move verified setup item to {}",
                name.to_string_lossy()
            )
        })
    }

    pub fn delete(self) -> Result<()> {
        let info = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: the owned handle was opened with DELETE access.
        unsafe {
            SetFileInformationByHandle(
                handle(&self.file),
                FileDispositionInfo,
                (&raw const info).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        }
        .context("cannot remove verified setup item")
    }
}

pub fn fixed_write(parent: &Path, parent_pin: &Node, name: &str, bytes: &[u8]) -> Result<()> {
    let temporary = parent.join(format!("{name}.next"));
    if present(&temporary)? {
        Node::file(&temporary, true, false)?.delete()?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    let written = Node::file(&temporary, true, false)?;
    ensure!(
        written.read(bytes.len() as u64)? == bytes,
        "setup state changed while writing"
    );
    if present(&parent.join(name))? {
        let previous = Node::file(&parent.join(name), true, false)?;
        ensure!(
            previous.id.volume == parent_pin.id.volume,
            "invalid setup state location"
        );
        drop(previous);
        written.rename(parent_pin, OsStr::new(name), true)
    } else {
        written.rename(parent_pin, OsStr::new(name), false)
    }
}

struct SystemHandle(HANDLE);

impl Drop for SystemHandle {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the returned handle.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn reject_running_images(images: &[(OsString, Identity)]) -> Result<()> {
    // SAFETY: this requests an owned process snapshot.
    let snapshot = SystemHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: the initialized entry has the size required by Toolhelp.
    unsafe { Process32FirstW(snapshot.0, &mut entry) }?;
    loop {
        let name = String::from_utf16_lossy(
            &entry.szExeFile[..entry
                .szExeFile
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(entry.szExeFile.len())],
        );
        if images
            .iter()
            .any(|(expected, _)| expected.eq_ignore_ascii_case(&name))
        {
            // SAFETY: this asks for an owned, query-only process handle.
            let process = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION,
                    false,
                    entry.th32ProcessID,
                )
            };
            match process {
                Ok(process) => {
                    let process = SystemHandle(process);
                    let mut image = vec![0u16; 32768];
                    let mut count = image.len() as u32;
                    // SAFETY: the handle is live and the output buffer has count code units.
                    unsafe {
                        QueryFullProcessImageNameW(
                            process.0,
                            PROCESS_NAME_WIN32,
                            PWSTR(image.as_mut_ptr()),
                            &mut count,
                        )
                    }
                    .context("cannot establish identity of a possible installed process")?;
                    use std::os::windows::ffi::OsStringExt;
                    let image = OsString::from_wide(&image[..count as usize]);
                    let pin = Node::open(Path::new(&image), READ_ATTRIBUTES, SHARE_ALL).context(
                        "cannot establish file identity of a possible installed process",
                    )?;
                    ensure!(
                        !images.iter().any(|(_, identity)| *identity == pin.id),
                        "Close the installed app and tray recorder, then retry after they exit."
                    );
                }
                Err(error) if error.code() == ERROR_INVALID_PARAMETER.to_hresult() => {}
                Err(error) => {
                    return Err(error).context("cannot inspect a possible installed process");
                }
            }
        }
        // SAFETY: the snapshot and process entry remain valid.
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => return Ok(()),
            Err(error) => return Err(error).context("cannot inspect running processes"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn file_id_barrier_blocks_launch_while_the_whole_directory_moves() {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source");
        fs::create_dir(&source).unwrap();
        let image = "chronobreak-legacy-image-probe.exe";
        let path = source.join(image);
        fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
        let parent = Node::directory(fixture.path(), false).unwrap();
        let id = Node::file_identity(&path).unwrap().id;
        let mut child = Command::new(&path)
            .args([
                "--exact",
                "fs_guard::tests::legacy_fixture_worker",
                "--ignored",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let barrier = Node::deny_execution_by_id(&parent, id).unwrap();
        let images = [(OsString::from(image), id)];
        assert!(reject_running_images(&images).is_err());
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
        reject_running_images(&images).unwrap();
        assert!(Command::new(&path).arg("--help").spawn().is_err());

        let root = Node::directory(&source, true).unwrap();
        root.rename(&parent, OsStr::new("previous"), false).unwrap();
        drop(root);
        let moved = fixture.path().join("previous").join(image);
        assert_eq!(Node::file_identity(&moved).unwrap().id, id);
        assert!(Command::new(&moved).arg("--help").spawn().is_err());
        drop(barrier);
        assert!(
            Command::new(&moved)
                .args(["--list"])
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    #[ignore = "spawned by the file identity test"]
    fn legacy_fixture_worker() {
        assert_eq!(io::stdin().read(&mut [0]).unwrap(), 0);
    }
}
