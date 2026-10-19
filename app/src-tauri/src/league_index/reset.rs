//! Replacement of the disposable SQLite projection after a damaged cache.

use super::{GameSummary, Index, cache_path};
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Serialize)]
pub(crate) struct ResetResult {
    pub catalog_recordings: usize,
}

pub(crate) fn reset(
    cache: &Path,
    root: &Path,
    games: &[GameSummary],
    token: &str,
    check: &(impl Fn() -> Result<()> + ?Sized),
) -> Result<ResetResult> {
    check()?;
    ensure_cache_directory(cache)?;
    let database = cache_path(cache, root);
    let nonce = crate::playback_policy::random_capability().context("cache reset nonce")?;
    let components = cache_components(&database);
    let quarantined = quarantine_components(&components, &nonce)?;

    let rebuilt = (|| {
        check()?;
        let mut index = Index::open(cache, root)?;
        check()?;
        index.reconcile(games, token)?;
        check()?;
        Ok(ResetResult {
            catalog_recordings: games.len(),
        })
    })();

    match rebuilt {
        Ok(result) => {
            // The old files are only a rollback aid. Cleanup is deliberately
            // best effort: a locked quarantine cannot affect the new cache.
            for (_, old) in quarantined {
                let _ = remove_regular_file(&old);
            }
            Ok(result)
        }
        Err(error) => {
            remove_new_components(&components)?;
            restore_components(&quarantined)?;
            Err(error)
        }
    }
}

fn ensure_cache_directory(cache: &Path) -> Result<()> {
    fs::create_dir_all(cache)?;
    let metadata = fs::symlink_metadata(cache)?;
    ensure!(
        metadata.is_dir() && !metadata.is_symlink(),
        "invalid index cache directory"
    );
    Ok(())
}

/// These are the only app-cache paths reset may move, remove, or restore.
fn cache_components(database: &Path) -> [PathBuf; 3] {
    [
        database.to_path_buf(),
        append_suffix(database, "-wal"),
        append_suffix(database, "-shm"),
    ]
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn quarantine_components(
    components: &[PathBuf; 3],
    nonce: &str,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut planned = Vec::new();
    for component in components {
        match fs::symlink_metadata(component) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file() && !metadata.is_symlink(),
                    "invalid index cache file"
                );
                let quarantine = append_suffix(component, &format!(".reset-{nonce}"));
                match fs::symlink_metadata(&quarantine) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Ok(_) => anyhow::bail!("index cache quarantine exists"),
                    Err(error) => return Err(error.into()),
                }
                planned.push((component.clone(), quarantine));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut moved = Vec::with_capacity(planned.len());
    for (source, quarantine) in planned {
        if let Err(error) = fs::rename(&source, &quarantine) {
            restore_components(&moved)?;
            return Err(error).context("quarantine index cache");
        }
        moved.push((source, quarantine));
    }
    Ok(moved)
}

fn remove_new_components(components: &[PathBuf; 3]) -> Result<()> {
    for component in components {
        remove_regular_file(component)?;
    }
    Ok(())
}

fn remove_regular_file(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.is_symlink(),
                "invalid index cache file"
            );
            fs::remove_file(path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn restore_components(quarantined: &[(PathBuf, PathBuf)]) -> Result<()> {
    for (source, quarantine) in quarantined.iter().rev() {
        if let Err(error) = fs::rename(quarantine, source) {
            return Err(error).with_context(|| format!("restore index cache {}", source.display()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_a_corrupt_cache_and_preserves_authored_source() {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        crate::library::tests::write_game(root.path(), "100", "2026-08-05T10:00:00Z", false);
        let games = crate::library::list_games(root.path()).unwrap();
        let authored = crate::league_authored::Store::default().encode().unwrap();
        let authored_path = root.path().join(crate::league_authored::FILE);
        fs::write(&authored_path, &authored).unwrap();

        let database = cache_path(cache.path(), root.path());
        fs::write(&database, b"not sqlite").unwrap();
        fs::write(append_suffix(&database, "-wal"), b"broken wal").unwrap();
        let result = reset(cache.path(), root.path(), &games, "accepted", &|| Ok(())).unwrap();

        assert_eq!(result.catalog_recordings, 1);
        assert_eq!(fs::read(authored_path).unwrap(), authored);
        let reader = Index::reader(cache.path(), root.path()).unwrap();
        reader.require_catalog("accepted").unwrap();
        let recordings: i64 = reader
            .db
            .query_row("SELECT COUNT(*) FROM recordings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(recordings, 1);
    }
}
