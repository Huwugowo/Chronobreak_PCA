use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

pub const VIDEO_MP4: &str = "video.mp4";

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
}
