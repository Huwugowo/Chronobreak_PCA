use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::Serialize;
use tokio::io::AsyncWriteExt;

pub const VIDEO_MP4: &str = "video.mp4";
pub const VIDEO_PARTIAL_MP4: &str = "video.partial.mp4";
pub const GAME_LOG_JSON: &str = "game_log.json";
pub const METADATA_JSON: &str = "metadata.json";

// One hour at native H.264's 18 Mb/s maximum plus 192 kb/s AAC is
// 8,186,400,000 bytes. 8 GiB leaves modest mux/metadata headroom. This is a
// critical-start floor, not a reservation or a guarantee that any match fits.
#[cfg(target_os = "windows")]
const MIN_RECORDING_FREE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[cfg(target_os = "windows")]
pub(crate) fn preflight_recording_storage(games_path: &Path) -> Result<()> {
    preflight_recording_storage_with(games_path, available_recording_bytes)
}

#[cfg(target_os = "windows")]
fn preflight_recording_storage_with(
    games_path: &Path,
    available_bytes: impl FnOnce(&Path) -> Result<u64>,
) -> Result<()> {
    // Resolve the games directory itself: it may be a junction/mount on a
    // different volume from the configured library root or system drive.
    let destination = fs::canonicalize(games_path).with_context(|| {
        format!(
            "Recording folder unavailable; check access or change folder, then restart recorder: {}",
            games_path.display()
        )
    })?;
    let available = available_bytes(&destination).with_context(|| {
        format!(
            "Cannot check recording space; check folder access, then restart recorder: {}",
            destination.display()
        )
    })?;
    anyhow::ensure!(
        available >= MIN_RECORDING_FREE_BYTES,
        "Low recording space: free 8 GiB or change folder; restart recorder. {} ({:.2} GiB free).",
        games_path.display(),
        available as f64 / (1024.0 * 1024.0 * 1024.0),
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn available_recording_bytes(destination: &Path) -> Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;

    let path: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut available = 0;
    // SAFETY: path is a live NUL-terminated UTF-16 directory path; available is
    // writable for the call. The other optional outputs are deliberately null.
    unsafe {
        GetDiskFreeSpaceExW(PCWSTR(path.as_ptr()), Some(&mut available), None, None)?;
    }
    // Use bytes available to this caller, respecting quotas rather than total
    // free bytes on the volume.
    Ok(available)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JsonWriteStats {
    pub serialized_bytes: u64,
    pub serialization: Duration,
    pub write: Duration,
    pub sync: Duration,
    pub rename: Duration,
    pub total: Duration,
}

pub fn unix_timestamp_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs())
}

pub fn create_game_directory(output_path: &Path, unix_timestamp: u64) -> Result<PathBuf> {
    let games = output_path.join("games");
    fs::create_dir_all(&games)
        .with_context(|| format!("failed to create games directory {}", games.display()))?;

    for suffix in 0..1000_u32 {
        let name = if suffix == 0 {
            unix_timestamp.to_string()
        } else {
            format!("{unix_timestamp}-{suffix}")
        };
        let path = games.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to create game directory {}", path.display())
                });
            }
        }
    }

    anyhow::bail!("could not allocate a unique game directory for timestamp {unix_timestamp}")
}

pub async fn write_json_atomic<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize + ?Sized,
{
    write_json_atomic_with_stats(path, value).await.map(|_| ())
}

pub(crate) async fn write_json_atomic_with_stats<T>(
    path: &Path,
    value: &T,
) -> Result<JsonWriteStats>
where
    T: Serialize + ?Sized,
{
    let total_started = Instant::now();
    let file_name = path
        .file_name()
        .context("JSON output path has no file name")?
        .to_string_lossy();
    let temporary = path.with_file_name(format!("{file_name}.tmp"));
    let serialization_started = Instant::now();
    let mut bytes = serde_json::to_vec_pretty(value).context("failed to serialize JSON")?;
    bytes.push(b'\n');
    let serialization = serialization_started.elapsed();
    let serialized_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);

    let write_started = Instant::now();
    let mut file = tokio::fs::File::create(&temporary)
        .await
        .with_context(|| format!("failed to create temporary file {}", temporary.display()))?;
    file.write_all(&bytes)
        .await
        .with_context(|| format!("failed to write temporary file {}", temporary.display()))?;
    let write = write_started.elapsed();
    let sync_started = Instant::now();
    file.sync_all()
        .await
        .with_context(|| format!("failed to flush temporary file {}", temporary.display()))?;
    let sync = sync_started.elapsed();
    drop(file);

    let rename_started = Instant::now();
    tokio::fs::rename(&temporary, path).await.with_context(|| {
        format!(
            "failed to atomically replace {} with {}",
            path.display(),
            temporary.display()
        )
    })?;
    let rename = rename_started.elapsed();
    Ok(JsonWriteStats {
        serialized_bytes,
        serialization,
        write,
        sync,
        rename,
        total: total_started.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn storage_preflight_checks_resolved_destination_and_preserves_recordings() {
        let directory = tempfile::tempdir().unwrap();
        let games = directory.path().join("games");
        let existing = create_game_directory(directory.path(), 42).unwrap();
        let recording = existing.join(VIDEO_MP4);
        fs::write(&recording, b"existing recording").unwrap();

        for available in [
            0,
            MIN_RECORDING_FREE_BYTES - 1,
            MIN_RECORDING_FREE_BYTES,
            u64::MAX,
        ] {
            let result = preflight_recording_storage_with(&games, |destination| {
                assert_eq!(destination, fs::canonicalize(&games).unwrap());
                Ok(available)
            });
            if available < MIN_RECORDING_FREE_BYTES {
                let message = result.unwrap_err().to_string();
                assert!(message.contains("Low recording space"));
                assert!(message.contains("free 8 GiB or change folder"));
                assert!(message.contains("restart recorder"));
                assert!(message.contains(&games.display().to_string()));
            } else {
                result.unwrap();
            }
            assert_eq!(fs::read(&recording).unwrap(), b"existing recording");
            assert_eq!(fs::read_dir(&games).unwrap().count(), 1);
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn storage_preflight_fails_actionably_when_destination_or_query_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");
        let message = preflight_recording_storage_with(&missing, |_| {
            panic!("must not query a missing destination")
        })
        .unwrap_err()
        .to_string();
        assert!(message.contains("Recording folder unavailable"));
        assert!(message.contains("check access or change folder"));
        assert!(!missing.exists());

        let error = preflight_recording_storage_with(directory.path(), |_| {
            anyhow::bail!("controlled query failure")
        })
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("Cannot check recording space"));
        assert!(message.contains("check folder access, then restart recorder"));
        assert!(message.contains("controlled query failure"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn storage_preflight_queries_real_windows_destination_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let destination = fs::canonicalize(directory.path()).unwrap();
        // No minimum-space assumption about the machine running the test.
        let _available = available_recording_bytes(&destination).unwrap();
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn current_timestamp_is_after_2025() {
        assert!(unix_timestamp_now().unwrap() > 1_735_689_600);
    }

    #[test]
    fn game_directories_do_not_collide() {
        let directory = tempfile::tempdir().unwrap();
        let first = create_game_directory(directory.path(), 42).unwrap();
        let second = create_game_directory(directory.path(), 42).unwrap();
        assert_ne!(first, second);
        assert_eq!(first.file_name().unwrap(), "42");
        assert_eq!(second.file_name().unwrap(), "42-1");
    }

    #[tokio::test]
    async fn atomic_json_write_replaces_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join(GAME_LOG_JSON);

        write_json_atomic(&output, &serde_json::json!({ "version": 1 }))
            .await
            .unwrap();
        write_json_atomic(&output, &serde_json::json!({ "version": 2 }))
            .await
            .unwrap();

        let stored: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(output).await.unwrap()).unwrap();
        assert_eq!(stored["version"], 2);
        assert!(!directory.path().join("game_log.json.tmp").exists());
    }

    #[tokio::test]
    async fn observed_atomic_write_reports_the_installed_byte_count() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join(GAME_LOG_JSON);

        let stats = write_json_atomic_with_stats(&output, &serde_json::json!({ "version": 3 }))
            .await
            .unwrap();

        assert_eq!(
            stats.serialized_bytes,
            tokio::fs::metadata(output).await.unwrap().len()
        );
        assert!(stats.total >= stats.serialization);
        assert!(stats.total >= stats.write);
        assert!(stats.total >= stats.sync);
        assert!(stats.total >= stats.rename);
    }
}
