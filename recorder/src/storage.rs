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
