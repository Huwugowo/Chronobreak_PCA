//! Windows file-generation binding and no-replace recording publication.
// Uses the same pinned-handle/relative-rename contract as League sidecar writes.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::mem::size_of;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Serialize};
use windows::Wdk::Storage::FileSystem::{
    FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FileRenameInformation, NtSetInformationFile,
};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_TRAVERSE, FileIdInfo, GetFileInformationByHandleEx, SYNCHRONIZE,
};
use windows::Win32::System::IO::IO_STATUS_BLOCK;

pub(super) const MAX_METADATA_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    volume: u64,
    file_id: [u8; 16],
}

fn identity(file: &File) -> Result<Identity> {
    let mut info = FILE_ID_INFO::default();
    // SAFETY: owned live handle and a correctly sized/aligned output structure.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    Ok(Identity {
        volume: info.VolumeSerialNumber,
        file_id: info.FileId.Identifier,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    version: u32,
    media_id: MediaId,
    directory: Identity,
    media: Identity,
    length: u64,
    last_write: u64,
}

impl Receipt {
    pub(super) fn capture(
        directory: &DirectoryPins,
        media: &File,
        media_id: &MediaId,
    ) -> Result<Self> {
        let facts = media.metadata()?;
        if !facts.is_file() || facts.len() == 0 {
            bail!("publication media is not a nonempty regular file");
        }
        Ok(Self {
            version: 1,
            media_id: media_id.clone(),
            directory: identity(directory.leaf())?,
            media: identity(media)?,
            length: facts.len(),
            last_write: facts.last_write_time(),
        })
    }

    pub(super) fn verify(
        &self,
        directory: &DirectoryPins,
        media: &File,
        media_id: &MediaId,
    ) -> Result<()> {
        if *self != Self::capture(directory, media, media_id)? {
            bail!("publication receipt does not match media/directory generation or media ID");
        }
        Ok(())
    }
}

/// Ancestors are opened root-first without following reparses, and cannot be
/// renamed/deleted while path-based readers or the relative publication run.
pub(super) struct DirectoryPins(Vec<File>);

impl DirectoryPins {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let absolute = std::path::absolute(path)?;
        let mut paths: Vec<_> = absolute.ancestors().collect();
        paths.reverse();
        let mut pins = Vec::with_capacity(paths.len());
        for path in paths {
            let file = OpenOptions::new()
                .access_mode((FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE).0)
                .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
                .custom_flags((FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).0)
                .open(path)
                .with_context(|| format!("cannot pin publication directory {}", path.display()))?;
            let metadata = file.metadata()?;
            if !metadata.is_dir()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
            {
                bail!("publication directory is not ordinary (reparse points are preserved)");
            }
            pins.push(file);
        }
        Ok(Self(pins))
    }

    fn leaf(&self) -> &File {
        self.0.last().expect("absolute path has a directory")
    }

    pub(super) fn identity(&self) -> Result<Identity> {
        identity(self.leaf())
    }

    pub(super) fn rename(&self, source: &File, leaf: &str) -> Result<()> {
        // All callers use fixed recorder artifact names, never paths from metadata.
        if !matches!(leaf, "video.mp4" | "video.partial.mp4" | "metadata.json") {
            bail!("invalid publication leaf");
        }
        let wide: Vec<u16> = leaf.encode_utf16().collect();
        let bytes = size_of::<FILE_RENAME_INFORMATION>() + wide.len() * 2;
        const {
            assert!(std::mem::align_of::<FILE_RENAME_INFORMATION>() <= size_of::<usize>());
        }
        let mut buffer = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: aligned/sized buffer including UTF-16 tail, live source/root
        // handles, fixed leaf. ReplaceIfExists=false atomically rejects collisions.
        unsafe {
            info.write(FILE_RENAME_INFORMATION {
                Anonymous: FILE_RENAME_INFORMATION_0 {
                    ReplaceIfExists: false,
                },
                RootDirectory: HANDLE(self.leaf().as_raw_handle()),
                FileNameLength: (wide.len() * 2) as u32,
                FileName: [0],
            });
            std::ptr::copy_nonoverlapping(
                wide.as_ptr(),
                std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
                wide.len(),
            );
            NtSetInformationFile(
                HANDLE(source.as_raw_handle()),
                &mut status,
                info.cast(),
                bytes as u32,
                FileRenameInformation,
            )
            .ok()
            .with_context(|| format!("no-replace publication of {leaf} failed"))?;
        }
        Ok(())
    }
}

/// Deny concurrent writes/deletion. Read-only probe handles omit DELETE because
/// ffprobe need not share deletion; rename handles are acquired after it joins.
pub(super) fn open_file(path: &Path, rename: bool) -> Result<File> {
    let file = OpenOptions::new()
        .access_mode((FILE_GENERIC_READ | if rename { DELETE } else { Default::default() }).0)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .with_context(|| format!("cannot open publication file {}", path.display()))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        bail!("publication artifact is not an ordinary file");
    }
    Ok(file)
}

pub(super) fn read_metadata(file: &mut File) -> Result<Vec<u8>> {
    if file.metadata()?.len() > MAX_METADATA_BYTES as u64 {
        bail!("pending metadata exceeds 64 KiB");
    }
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_METADATA_BYTES {
        bail!("pending metadata exceeds 64 KiB");
    }
    Ok(bytes)
}

pub(super) fn create_pending(path: &Path, bytes: &[u8]) -> Result<File> {
    if bytes.len() > MAX_METADATA_BYTES {
        bail!("pending metadata exceeds 64 KiB");
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode((FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE).0)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(file)
}

pub(super) fn exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true), // Includes broken links and directories: never overwrite.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
