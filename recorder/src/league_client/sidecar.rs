//! Immutable optional context installation, independent of canonical media writes.

use std::io::{self, Write};
use std::path::Path;

use chronobreak_replay_time::MediaId;
use serde::Serialize;
use tokio::task::JoinHandle;

use super::association::ProvisionalCandidate;

pub(crate) const MATCH_FILE: &str = "league_match.json";
const MAX_BYTES: usize = 64 * 1024;

struct CappedBuffer(Vec<u8>, usize);
impl Write for CappedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.1 - self.0.len() {
            return Err(io::Error::other("context size"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(value: &impl Serialize) -> Result<Vec<u8>, &'static str> {
    let mut buffer = CappedBuffer(Vec::new(), MAX_BYTES);
    serde_json::to_writer(&mut buffer, value).map_err(|_| "context_serialization")?;
    Ok(buffer.0)
}

pub(crate) struct DirectoryGuard {
    #[cfg(windows)]
    inner: platform::Directory,
}

impl DirectoryGuard {
    pub(crate) fn try_clone(&self) -> Result<Self, &'static str> {
        #[cfg(windows)]
        {
            Ok(Self {
                inner: self.inner.try_clone()?,
            })
        }
        #[cfg(not(windows))]
        {
            Err("context_non_windows")
        }
    }

    pub(super) fn available(&self) -> bool {
        #[cfg(windows)]
        {
            self.inner.available()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub(super) fn install_result(
        self,
        result: &chronobreak_league_data::ResultFile,
    ) -> Result<usize, &'static str> {
        let mut buffer = CappedBuffer(Vec::new(), chronobreak_league_data::MAX_RESULT_BYTES);
        serde_json::to_writer(&mut buffer, result).map_err(|_| "result_serialization")?;
        #[cfg(windows)]
        {
            self.inner.install_result(result, &buffer.0)?;
            Ok(buffer.0.len())
        }
        #[cfg(not(windows))]
        {
            let _ = result;
            Err("context_non_windows")
        }
    }
    pub fn capture(path: &Path, media_id: MediaId) -> Result<Self, &'static str> {
        #[cfg(windows)]
        {
            Ok(Self {
                inner: platform::Directory::capture(path, media_id)?,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (path, media_id);
            Err("context_non_windows")
        }
    }

    fn install(self, candidate: &ProvisionalCandidate, bytes: &[u8]) -> Result<(), &'static str> {
        #[cfg(windows)]
        {
            self.inner.install(candidate, bytes)
        }
        #[cfg(not(windows))]
        {
            let _ = (candidate, bytes);
            Err("context_non_windows")
        }
    }
}

pub(crate) struct ContextWriter {
    task: Option<JoinHandle<Result<(), &'static str>>>,
    runtime: tokio::runtime::Handle,
}
impl ContextWriter {
    pub fn start(
        directory: DirectoryGuard,
        candidate: ProvisionalCandidate,
    ) -> Result<Self, &'static str> {
        let runtime = tokio::runtime::Handle::current();
        if runtime.runtime_flavor() != tokio::runtime::RuntimeFlavor::MultiThread {
            return Err("context_writer_runtime");
        }
        let bytes = encode(&candidate)?;
        let task = tokio::task::spawn_blocking(move || directory.install(&candidate, &bytes));
        Ok(Self {
            task: Some(task),
            runtime,
        })
    }
    pub async fn finish(mut self) -> Result<(), &'static str> {
        let joined = self.task.as_mut().ok_or("context_writer")?.await;
        self.task.take();
        joined.map_err(|_| "context_writer")?
    }
}
impl Drop for ContextWriter {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            // Preserve ownership of an uninterruptible filesystem worker even
            // when its awaiting future is abandoned.
            tokio::task::block_in_place(|| {
                if self.runtime.block_on(task).is_err() {
                    tracing::warn!(
                        code = "context_writer",
                        "optional context writer join failed"
                    );
                }
            });
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::fs::{File, OpenOptions};
    use std::mem::size_of;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::path::{Component, PathBuf};
    use windows::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows::Wdk::Storage::FileSystem::{
        FILE_CREATE, FILE_NON_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT, FILE_RENAME_INFORMATION,
        FILE_RENAME_INFORMATION_0, FILE_SYNCHRONOUS_IO_NONALERT, FileRenameInformation,
        NtCreateFile, NtSetInformationFile,
    };
    use windows::Win32::Foundation::{
        HANDLE, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, UNICODE_STRING,
    };
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE, FILE_ID_INFO,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
        FileDispositionInfo, FileIdInfo, GetFileInformationByHandleEx, SYNCHRONIZE,
        SetFileInformationByHandle,
    };
    use windows::Win32::System::IO::IO_STATUS_BLOCK;
    use windows::core::PWSTR;

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Identity {
        volume: u64,
        file: [u8; 16],
    }

    fn identity(file: &File) -> Result<Identity, &'static str> {
        let mut info = FILE_ID_INFO::default();
        // SAFETY: live owned file handle, correctly sized/aligned writable structure.
        unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileIdInfo,
                (&mut info as *mut FILE_ID_INFO).cast(),
                size_of::<FILE_ID_INFO>() as u32,
            )
            .map_err(|_| "context_directory_identity")?;
        }
        Ok(Identity {
            volume: info.VolumeSerialNumber,
            file: info.FileId.Identifier,
        })
    }

    fn plain_directory(file: &File) -> Result<(), &'static str> {
        let metadata = file.metadata().map_err(|_| "context_directory_metadata")?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            return Err("context_directory_reparse");
        }
        Ok(())
    }

    fn open_directory(path: &Path, allow_delete: bool) -> Result<File, &'static str> {
        let share = FILE_SHARE_READ
            | FILE_SHARE_WRITE
            | if allow_delete {
                FILE_SHARE_DELETE
            } else {
                Default::default()
            };
        let file = OpenOptions::new()
            .access_mode((FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE).0)
            .share_mode(share.0)
            .custom_flags((FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).0)
            .open(path)
            .map_err(|_| "context_directory_open")?;
        plain_directory(&file)?;
        Ok(file)
    }

    pub(super) struct Directory {
        path: PathBuf,
        // Retain allocation handle with share-delete: user deletion remains possible,
        // and its file ID cannot be reused while this handle is retained.
        original: File,
        expected: Identity,
        media_id: MediaId,
    }

    impl Directory {
        pub fn try_clone(&self) -> Result<Self, &'static str> {
            Ok(Self {
                path: self.path.clone(),
                original: self
                    .original
                    .try_clone()
                    .map_err(|_| "context_directory_clone")?,
                expected: self.expected,
                media_id: self.media_id.clone(),
            })
        }
        pub fn available(&self) -> bool {
            open_directory(&self.path, true)
                .is_ok_and(|f| identity(&f).is_ok_and(|id| id == self.expected))
        }

        pub fn install_result(
            self,
            result: &chronobreak_league_data::ResultFile,
            bytes: &[u8],
        ) -> Result<(), &'static str> {
            self.install_result_with(result, bytes, || {})
        }
        pub(super) fn install_result_with(
            self,
            result: &chronobreak_league_data::ResultFile,
            bytes: &[u8],
            before_validate: impl FnOnce(),
        ) -> Result<(), &'static str> {
            if !result.valid_for(&self.media_id, &result.game_id)
                || bytes.len() > chronobreak_league_data::MAX_RESULT_BYTES
            {
                return Err("result_identity");
            }
            plain_directory(&self.original)?;
            let pinned = open_directory(&self.path, true)?;
            if identity(&pinned)? != self.expected {
                return Err("result_directory_replaced");
            }
            let mut temporary = OwnedTemporary::create_shared(&pinned, true)?;
            temporary
                .file
                .write_all(bytes)
                .map_err(|_| "result_write")?;
            temporary.file.sync_all().map_err(|_| "result_flush")?;
            before_validate();
            // Reads are bounded and delete-shared; a race only abandons optional facts.
            let read = |leaf: &str, cap: u64| -> Result<Vec<u8>, &'static str> {
                use std::io::Read;
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
                    .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                    .open(self.path.join(leaf))
                    .map_err(|_| "result_core_absent")?;
                let meta = file.metadata().map_err(|_| "result_core_metadata")?;
                if !meta.is_file()
                    || meta.len() > cap
                    || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
                {
                    return Err("result_core_invalid");
                }
                let mut bytes = Vec::new();
                file.take(cap + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "result_core_read")?;
                if bytes.len() as u64 > cap {
                    return Err("result_core_size");
                }
                Ok(bytes)
            };
            #[derive(serde::Deserialize)]
            struct Core {
                schema_version: u32,
                media_id: MediaId,
                media_timeline: chronobreak_replay_time::MediaTimelineV2,
            }
            #[derive(serde::Deserialize)]
            struct Log {
                schema_version: u32,
                media_id: MediaId,
            }
            #[derive(serde::Deserialize)]
            struct Match {
                schema_version: u32,
                status: String,
                media_id: MediaId,
                game_id: String,
            }
            let core: Core = serde_json::from_slice(&read("metadata.json", 256 * 1024)?)
                .map_err(|_| "result_core_json")?;
            let log: Log = serde_json::from_slice(&read("game_log.json", 16 * 1024 * 1024)?)
                .map_err(|_| "result_core_json")?;
            let provisional: Match = serde_json::from_slice(&read(MATCH_FILE, MAX_BYTES as u64)?)
                .map_err(|_| "result_match_json")?;
            if core.schema_version != 2
                || core.media_id != self.media_id
                || core.media_timeline.media_id != self.media_id
                || core.media_timeline.validate().is_err()
                || log.schema_version != 2
                || log.media_id != self.media_id
                || provisional.schema_version != 1
                || provisional.status != "provisional"
                || provisional.media_id != self.media_id
                || provisional.game_id != result.game_id
            {
                return Err("result_core_identity");
            }
            let video = std::fs::symlink_metadata(self.path.join("video.mp4"))
                .map_err(|_| "result_video_absent")?;
            if !video.is_file()
                || video.len() == 0
                || video.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
            {
                return Err("result_video_invalid");
            }
            let current = open_directory(&self.path, true)?;
            if identity(&current)? != self.expected || identity(&pinned)? != self.expected {
                return Err("result_directory_replaced");
            }
            temporary.install_leaf(&pinned, chronobreak_league_data::RESULT_FILE)?;
            Ok(())
        }
        pub fn capture(path: &Path, media_id: MediaId) -> Result<Self, &'static str> {
            if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
                return Err("context_directory_path");
            }
            let original = open_directory(path, true)?;
            let expected = identity(&original)?;
            Ok(Self {
                path: path.to_owned(),
                original,
                expected,
                media_id,
            })
        }

        pub fn install(
            self,
            candidate: &ProvisionalCandidate,
            bytes: &[u8],
        ) -> Result<(), &'static str> {
            if !candidate.matches_media(&self.media_id) || bytes.len() > MAX_BYTES {
                return Err("context_candidate_identity");
            }
            plain_directory(&self.original)?;
            let pinned = open_directory(&self.path, false)?;
            if identity(&pinned)? != self.expected {
                return Err("context_directory_replaced");
            }
            // All child creation and rename below are relative to this retained
            // handle, so mutable parent paths are never retraversed for writes.
            let mut temporary = OwnedTemporary::create(&pinned)?;
            temporary
                .file
                .write_all(bytes)
                .map_err(|_| "context_write")?;
            temporary.file.sync_all().map_err(|_| "context_flush")?;
            if temporary
                .file
                .metadata()
                .map_err(|_| "context_temp_metadata")?
                .file_attributes()
                & FILE_ATTRIBUTE_REPARSE_POINT.0
                != 0
            {
                return Err("context_temp_reparse");
            }
            plain_directory(&pinned)?;
            temporary.install(&pinned)?;
            plain_directory(&pinned)?;
            Ok(())
        }
    }

    struct OwnedTemporary {
        file: File,
        installed: bool,
    }
    impl OwnedTemporary {
        fn create(directory: &File) -> Result<Self, &'static str> {
            Self::create_shared(directory, false)
        }
        fn create_shared(directory: &File, allow_delete: bool) -> Result<Self, &'static str> {
            let leaf = format!(".league_match-{}.tmp", MediaId::new_v4());
            let mut wide: Vec<u16> = leaf.encode_utf16().chain(Some(0)).collect();
            let name = UNICODE_STRING {
                Length: ((wide.len() - 1) * 2) as u16,
                MaximumLength: (wide.len() * 2) as u16,
                Buffer: PWSTR(wide.as_mut_ptr()),
            };
            let attributes = OBJECT_ATTRIBUTES {
                Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
                RootDirectory: HANDLE(directory.as_raw_handle()),
                ObjectName: &name,
                Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
                ..Default::default()
            };
            let mut handle = HANDLE::default();
            let mut status = IO_STATUS_BLOCK::default();
            // SAFETY: all inputs live through this synchronous native filesystem
            // call; root is an owned directory handle, leaf is bounded UTF-16 and
            // FILE_CREATE never opens/replaces an existing child.
            unsafe {
                NtCreateFile(
                    &mut handle,
                    FILE_GENERIC_WRITE | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
                    &attributes,
                    &mut status,
                    None,
                    FILE_ATTRIBUTE_NORMAL,
                    if allow_delete {
                        FILE_SHARE_READ | FILE_SHARE_DELETE
                    } else {
                        FILE_SHARE_READ
                    },
                    FILE_CREATE,
                    FILE_NON_DIRECTORY_FILE
                        | FILE_SYNCHRONOUS_IO_NONALERT
                        | FILE_OPEN_REPARSE_POINT,
                    None,
                    0,
                )
                .ok()
                .map_err(|_| "context_temp_create")?;
            }
            // SAFETY: successful NtCreateFile returned a uniquely owned handle;
            // transfer closing responsibility to File exactly once.
            let file = unsafe { File::from_raw_handle(handle.0) };
            Ok(Self {
                file,
                installed: false,
            })
        }

        fn install(&mut self, directory: &File) -> Result<(), &'static str> {
            self.install_leaf(directory, MATCH_FILE)
        }
        fn install_leaf(&mut self, directory: &File, leaf: &str) -> Result<(), &'static str> {
            let wide: Vec<u16> = leaf.encode_utf16().collect();
            let bytes = size_of::<FILE_RENAME_INFORMATION>() + wide.len() * 2;
            const {
                assert!(
                    std::mem::align_of::<FILE_RENAME_INFORMATION>()
                        <= std::mem::align_of::<usize>()
                );
            }
            let mut buffer = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
            let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
            let mut status = IO_STATUS_BLOCK::default();
            // SAFETY: buffer is aligned and sized for the header and UTF-16 tail.
            // Root handle remains owned; false ReplaceIfExists makes installation
            // atomic/no-clobber. The name is one fixed leaf, never a pathname.
            unsafe {
                info.write(FILE_RENAME_INFORMATION {
                    Anonymous: FILE_RENAME_INFORMATION_0 {
                        ReplaceIfExists: false,
                    },
                    RootDirectory: HANDLE(directory.as_raw_handle()),
                    FileNameLength: (wide.len() * 2) as u32,
                    FileName: [0],
                });
                std::ptr::copy_nonoverlapping(
                    wide.as_ptr(),
                    std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
                    wide.len(),
                );
                NtSetInformationFile(
                    HANDLE(self.file.as_raw_handle()),
                    &mut status,
                    info.cast(),
                    bytes as u32,
                    FileRenameInformation,
                )
                .ok()
                .map_err(|_| "context_install")?;
            }
            self.installed = true;
            Ok(())
        }
    }
    impl Drop for OwnedTemporary {
        fn drop(&mut self) {
            if !self.installed {
                let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
                // SAFETY: only the exclusively created temporary's owned handle
                // is marked for deletion; no pathname or destination is removed.
                let cleanup = unsafe {
                    SetFileInformationByHandle(
                        HANDLE(self.file.as_raw_handle()),
                        FileDispositionInfo,
                        (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                        size_of::<FILE_DISPOSITION_INFO>() as u32,
                    )
                };
                if cleanup.is_err() {
                    tracing::warn!(
                        code = "context_temp_cleanup",
                        "optional context temporary cleanup failed"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn result_fixture(directory: &Path, media: &MediaId) -> chronobreak_league_data::ResultFile {
        use serde_json::json;
        let candidate = super::super::association::fixture_candidate(media.clone());
        std::fs::write(directory.join(MATCH_FILE), encode(&candidate).unwrap()).unwrap();
        let timeline = json!({"schema_version":2,"replay_ticks_per_second":"48000000","media_id":media,
            "video":{"codec":"h264","profile":"High","time_base":{"numerator":"1","denominator":"60"},"first_pts":"0","frame_rate":{"numerator":"60","denominator":"1"},"frame_count":"300","one_past_last_pts":"300","replay_end":"240000000","exact_cfr":true},
            "audio":{"present":false,"codec":null,"sample_rate":null,"time_base":null,"first_pts":null,"replay_start":null,"replay_end":null},
            "container":{"start_seconds":{"numerator":"0","denominator":"1"},"duration_seconds":{"numerator":"5","denominator":"1"}},
            "producer":{"backend":"synthetic","expected_frame_rate":{"numerator":"60","denominator":"1"},"expected_frame_count":"300","media_runtime_id":"synthetic"},"capture":null});
        std::fs::write(
            directory.join("metadata.json"),
            serde_json::to_vec(
                &json!({"schema_version":2,"media_id":media,"media_timeline":timeline}),
            )
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("game_log.json"),
            serde_json::to_vec(&json!({"schema_version":2,"media_id":media})).unwrap(),
        )
        .unwrap();
        std::fs::write(directory.join("video.mp4"), b"healthy publication fixture").unwrap();
        let b = candidate.result_binding();
        let eog:super::super::result::Eog=serde_json::from_value(json!({"gameId":b.game_id.parse::<u64>().unwrap(),"localPlayer":{"stats":{"WIN":1,"kills":4}}})).unwrap();
        eog.confirm(&b).unwrap()
    }
    #[test]
    #[cfg(windows)]
    fn result_writer_is_immutable_and_requires_core_and_generation_agreement() {
        for action in 0..5 {
            let (_root, path, media) = fixture();
            let guard = DirectoryGuard::capture(&path, media.clone()).unwrap();
            let mut result = result_fixture(&path, &media);
            if action == 1 {
                result.media_id = MediaId::new_v4();
            } else if action == 2 {
                result.game_id = "8".into();
            } else if action == 3 {
                std::fs::remove_file(path.join("video.mp4")).unwrap();
            } else if action == 4 {
                std::fs::rename(&path, path.with_extension("old")).unwrap();
                std::fs::create_dir(&path).unwrap();
            }
            let wrote = guard.install_result(&result);
            if action == 0 {
                assert!(wrote.is_ok(), "{wrote:?}");
                let before =
                    std::fs::read(path.join(chronobreak_league_data::RESULT_FILE)).unwrap();
                assert!(
                    DirectoryGuard::capture(&path, media)
                        .unwrap()
                        .install_result(&result)
                        .is_err()
                );
                assert_eq!(
                    before,
                    std::fs::read(path.join(chronobreak_league_data::RESULT_FILE)).unwrap()
                );
            } else {
                assert!(wrote.is_err());
                assert!(!path.join(chronobreak_league_data::RESULT_FILE).exists());
            }
            assert!(
                !std::fs::read_dir(&path).unwrap().any(|e| e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp"))
            );
        }
    }
    #[test]
    #[cfg(windows)]
    fn result_writer_never_blocks_delete_or_recreates_deleted_bundle() {
        for between in [false, true] {
            let (_root, path, media) = fixture();
            let guard = DirectoryGuard::capture(&path, media.clone()).unwrap();
            let result = result_fixture(&path, &media);
            let bytes = serde_json::to_vec(&result).unwrap();
            let write = if between {
                let deleting = path.clone();
                guard.inner.install_result_with(&result, &bytes, move || {
                    std::fs::remove_dir_all(deleting)
                        .expect("delete-shared writer must permit deletion")
                })
            } else {
                std::fs::remove_dir_all(&path).unwrap();
                guard.install_result(&result).map(|_| ())
            };
            assert!(write.is_err());
            assert!(!path.exists());
        }
    }

    #[test]
    fn serialization_is_capped_before_any_filesystem_write() {
        assert!(encode(&"x".repeat(MAX_BYTES)).is_err());
        assert_eq!(
            encode(&serde_json::json!({"status":"provisional"}))
                .unwrap()
                .len(),
            24
        );
    }

    #[cfg(windows)]
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, MediaId) {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("recording");
        std::fs::create_dir(&directory).unwrap();
        (root, directory, MediaId::new_v4())
    }

    #[cfg(windows)]
    #[test]
    fn writes_provisional_decimal_identity_once_and_preserves_other_files() {
        let (_root, directory, media) = fixture();
        std::fs::write(directory.join("video.fixture"), b"synthetic").unwrap();
        let candidate = super::super::association::fixture_candidate(media.clone());
        let first = DirectoryGuard::capture(&directory, media.clone()).unwrap();
        first
            .install(&candidate, &encode(&candidate).unwrap())
            .unwrap();
        let path = directory.join(MATCH_FILE);
        let original = std::fs::read(&path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        assert_eq!(value["status"], "provisional");
        assert_eq!(value["game_id"], "9007199254740993");
        let second = DirectoryGuard::capture(&directory, media).unwrap();
        assert!(
            second
                .install(&candidate, &encode(&candidate).unwrap())
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            std::fs::read(directory.join("video.fixture")).unwrap(),
            b"synthetic"
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn missing_replaced_and_wrong_media_directories_never_receive_context() {
        let (root, directory, media) = fixture();
        let candidate = super::super::association::fixture_candidate(media.clone());
        let guard = DirectoryGuard::capture(&directory, media.clone()).unwrap();
        std::fs::rename(&directory, root.path().join("old")).unwrap();
        std::fs::create_dir(&directory).unwrap();
        assert!(
            guard
                .install(&candidate, &encode(&candidate).unwrap())
                .is_err()
        );
        assert!(!directory.join(MATCH_FILE).exists());
        let guard = DirectoryGuard::capture(&directory, MediaId::new_v4()).unwrap();
        assert!(
            guard
                .install(&candidate, &encode(&candidate).unwrap())
                .is_err()
        );
        assert!(!directory.join(MATCH_FILE).exists());
        let guard = DirectoryGuard::capture(&directory, media).unwrap();
        std::fs::remove_dir(&directory).unwrap();
        assert!(
            guard
                .install(&candidate, &encode(&candidate).unwrap())
                .is_err()
        );
        assert!(!directory.exists());
    }

    #[cfg(windows)]
    #[test]
    fn reparse_replacement_cannot_redirect_temporary_creation() {
        let (root, directory, media) = fixture();
        let candidate = super::super::association::fixture_candidate(media.clone());
        let guard = DirectoryGuard::capture(&directory, media.clone()).unwrap();
        std::fs::rename(&directory, root.path().join("old")).unwrap();
        let target = root.path().join("synthetic-target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("sentinel"), b"synthetic").unwrap();
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&directory)
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            made.status.success(),
            "dedicated junction fixture must be available"
        );
        assert!(DirectoryGuard::capture(&directory, media).is_err());
        assert!(
            guard
                .install(&candidate, &encode(&candidate).unwrap())
                .is_err()
        );
        assert_eq!(std::fs::read_dir(target).unwrap().count(), 1);
        std::fs::remove_dir(directory).unwrap(); // unlink only the dedicated junction
    }

    #[cfg(windows)]
    #[test]
    fn concurrent_install_has_exactly_one_winner() {
        let (_root, directory, media) = fixture();
        let candidate = super::super::association::fixture_candidate(media.clone());
        let first = DirectoryGuard::capture(&directory, media.clone()).unwrap();
        let second = DirectoryGuard::capture(&directory, media).unwrap();
        let other = candidate.clone();
        let a = std::thread::spawn(move || first.install(&candidate, &encode(&candidate).unwrap()));
        let b = std::thread::spawn(move || second.install(&other, &encode(&other).unwrap()));
        assert_ne!(a.join().unwrap().is_ok(), b.join().unwrap().is_ok());
        assert_eq!(std::fs::read_dir(directory).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn normal_writer_finishes_installation_before_returning() {
        let (_root, directory, media) = fixture();
        let candidate = super::super::association::fixture_candidate(media.clone());
        let guard = DirectoryGuard::capture(&directory, media).unwrap();
        ContextWriter::start(guard, candidate)
            .unwrap()
            .finish()
            .await
            .unwrap();
        assert!(directory.join(MATCH_FILE).is_file());
        assert_eq!(std::fs::read_dir(directory).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn abandoned_writer_and_cancelled_finish_join_before_owner_returns() {
        for cancel_finish in [false, true] {
            let (_root, directory, media) = fixture();
            let candidate = super::super::association::fixture_candidate(media.clone());
            let guard = DirectoryGuard::capture(&directory, media).unwrap();
            let bytes = encode(&candidate).unwrap();
            let (started, began) = tokio::sync::oneshot::channel();
            let (release, blocked) = std::sync::mpsc::channel();
            let task = tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                blocked.recv().unwrap();
                guard.install(&candidate, &bytes)
            });
            let writer = ContextWriter {
                task: Some(task),
                runtime: tokio::runtime::Handle::current(),
            };
            began.await.unwrap();
            assert!(!directory.join(MATCH_FILE).exists());
            let releaser = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(20));
                release.send(()).unwrap();
            });
            if cancel_finish {
                let mut finish = Box::pin(writer.finish());
                tokio::select! {
                    biased;
                    _ = &mut finish => panic!("blocked worker must remain owned"),
                    _ = std::future::ready(()) => {}
                }
                drop(finish);
            } else {
                drop(writer);
            }
            // The publication caller can proceed only after Drop joined the worker.
            assert!(directory.join(MATCH_FILE).is_file());
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
            releaser.join().unwrap();
        }
    }
}
