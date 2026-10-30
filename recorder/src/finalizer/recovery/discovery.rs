//! Bounded restart progress without replaying an ever-growing directory prefix.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use anyhow::{Result, ensure};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, HANDLE};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FindClose,
    FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW, WIN32_FIND_DATAW,
};
use windows::core::PCWSTR;

use super::super::publication::DirectoryPins;
use super::bundle_name;

pub(super) const CURSOR: &str = ".qb-cap-003.cursor";
const PREFIX_BYTES: usize = 24; // u64 timestamp, '-' and collision suffix <=999.
const ALPHABET: &[u8] = b"-0123456789";

fn valid_prefix(prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    let (timestamp, suffix) = prefix.split_once('-').unwrap_or((prefix, ""));
    bundle_name(timestamp)
        && !timestamp.contains('-')
        && suffix.len() <= 3
        && (suffix.is_empty()
            || (suffix.bytes().all(|c| c.is_ascii_digit()) && !suffix.starts_with('0')))
}

/// A single prefix encodes a depth-first frontier. Only saturated regions split;
/// an exhausted/empty region skips its entire subtree regardless of listing order.
pub(super) fn descend(prefix: &str) -> Option<String> {
    ALPHABET.iter().find_map(|c| {
        let next = format!("{prefix}{}", char::from(*c));
        (next.len() <= PREFIX_BYTES && valid_prefix(&next)).then_some(next)
    })
}

pub(super) fn advance(prefix: &str) -> Option<String> {
    let mut parent = prefix.to_owned();
    while let Some(last) = parent.pop() {
        for c in ALPHABET.iter().copied().filter(|c| char::from(*c) > last) {
            let next = format!("{parent}{}", char::from(c));
            if valid_prefix(&next) {
                return Some(next);
            }
        }
    }
    None
}

/// Recorder-private, directory-bound scheduling hint. Never used as media proof.
/// No truncation/replacement: only the recognized fixed prefix slot is writable.
pub(super) struct Cursor {
    file: File,
    offset: u64,
    pub prefix: String,
}

impl Cursor {
    pub fn open(games: &Path, pins: &DirectoryPins) -> Result<Self> {
        let header = format!(
            "QB-CAP-003 recovery cursor v1\n{}\n",
            serde_json::to_string(&pins.identity()?)?
        );
        let path = games.join(CURSOR);
        let open = |create| {
            OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(FILE_SHARE_READ.0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                .create_new(create)
                .open(&path)
        };
        let (mut file, created) = match open(true) {
            Ok(file) => (file, true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                (open(false)?, false)
            }
            Err(error) => return Err(error.into()),
        };
        let facts = file.metadata()?;
        ensure!(
            facts.is_file() && facts.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0,
            "recovery cursor is not an ordinary file; retained"
        );
        let offset = header.len() as u64;
        if created {
            file.write_all(header.as_bytes())?;
            file.write_all(&[b' '; PREFIX_BYTES])?;
            file.sync_all()?;
        }
        ensure!(
            file.metadata()?.len() == offset + PREFIX_BYTES as u64,
            "unrecognized recovery cursor size; retained"
        );
        file.rewind()?;
        let mut bytes = vec![0; header.len() + PREFIX_BYTES];
        file.read_exact(&mut bytes)?;
        ensure!(
            bytes.starts_with(header.as_bytes()),
            "foreign recovery cursor; retained"
        );
        let prefix = std::str::from_utf8(&bytes[header.len()..])?.trim_end_matches(' ');
        ensure!(
            valid_prefix(prefix),
            "invalid recovery cursor prefix; retained"
        );
        Ok(Self {
            file,
            offset,
            prefix: prefix.to_owned(),
        })
    }

    pub fn save(&mut self, prefix: &str) -> Result<()> {
        ensure!(
            prefix.len() <= PREFIX_BYTES && valid_prefix(prefix),
            "invalid next recovery prefix"
        );
        if prefix == self.prefix {
            return Ok(());
        }
        let mut bytes = [b' '; PREFIX_BYTES];
        bytes[..prefix.len()].copy_from_slice(prefix.as_bytes());
        self.file.seek(SeekFrom::Start(self.offset))?;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.prefix = prefix.to_owned();
        Ok(())
    }
}

/// Win32 does not sort results. Search only a selected prefix, not a skipped
/// whole-library enumeration. Filter long names too: short aliases also match.
pub(super) struct Search {
    handle: HANDLE,
    first: Option<WIN32_FIND_DATAW>,
}

impl Search {
    pub fn open(games: &Path, prefix: &str) -> Result<Option<Self>> {
        let pattern = std::path::absolute(games)?.join(format!("{prefix}*"));
        let path: Vec<_> = pattern.as_os_str().encode_wide().collect();
        // Unlike std::fs, raw Win32 calls do not add the extended path prefix.
        let extended = if path.starts_with(&[92, 92, 63, 92]) {
            path
        } else if path.starts_with(&[92, 92]) {
            r"\\?\UNC\"
                .encode_utf16()
                .chain(path.into_iter().skip(2))
                .collect()
        } else {
            r"\\?\".encode_utf16().chain(path).collect()
        };
        let wide: Vec<_> = extended.into_iter().chain(Some(0)).collect();
        let mut data = WIN32_FIND_DATAW::default();
        // SAFETY: terminated input, valid sized output, search handle uniquely owned.
        match unsafe {
            FindFirstFileExW(
                PCWSTR(wide.as_ptr()),
                FindExInfoBasic,
                (&mut data as *mut WIN32_FIND_DATAW).cast(),
                FindExSearchNameMatch,
                None,
                Default::default(),
            )
        } {
            Ok(handle) => Ok(Some(Self {
                handle,
                first: Some(data),
            })),
            Err(error) if error.code() == ERROR_FILE_NOT_FOUND.to_hresult() => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn next(&mut self) -> Result<Option<String>> {
        let data = match self.first.take() {
            Some(data) => data,
            None => {
                let mut data = WIN32_FIND_DATAW::default();
                // SAFETY: owned live search handle and valid output buffer.
                match unsafe { FindNextFileW(self.handle, &mut data) } {
                    Ok(()) => data,
                    Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => {
                        return Ok(None);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };
        let length = data
            .cFileName
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(data.cFileName.len());
        // Unpaired UTF-16 cannot be a canonical ASCII bundle; still count the entry.
        Ok(Some(String::from_utf16_lossy(&data.cFileName[..length])))
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        // SAFETY: this uniquely owned handle came from FindFirstFileExW.
        let _ = unsafe { FindClose(self.handle) };
    }
}
