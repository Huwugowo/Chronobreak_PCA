use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Serialize};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::core::PCWSTR;

use super::post_game::{Coordinator, Handle};
use super::sidecar::{DirectoryGuard, DirectoryLocator};

const MAX_ROOTS: usize = 16;
const MAX_BYTES: usize = 256 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    version: u32,
    roots: Vec<DirectoryLocator>,
}

struct Library {
    locator: DirectoryLocator,
    coordinator: Option<Coordinator>,
}

/// The recorder lease makes the process the sole registry writer.
pub(crate) struct Libraries {
    path: PathBuf,
    entries: Vec<Library>,
}

impl Libraries {
    pub(crate) fn open(config_path: &Path) -> Result<Self> {
        let mut path = config_path.as_os_str().to_owned();
        path.push(".recorder-libraries.json");
        let path = PathBuf::from(path);
        let registry = match OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&path)
        {
            Ok(file) => {
                let metadata = file.metadata()?;
                if !metadata.is_file()
                    || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
                {
                    bail!("recorder library registry is not a regular file");
                }
                let mut bytes = Vec::new();
                file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
                if bytes.len() > MAX_BYTES {
                    bail!("recorder library registry exceeds the size limit");
                }
                serde_json::from_slice::<Registry>(&bytes)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Registry {
                version: 1,
                roots: Vec::new(),
            },
            Err(error) => return Err(error.into()),
        };
        if registry.version != 1 || registry.roots.len() > MAX_ROOTS {
            bail!("invalid recorder library registry");
        }
        for (index, root) in registry.roots.iter().enumerate() {
            if !root.path.is_absolute()
                || root
                    .path
                    .components()
                    .any(|part| part == std::path::Component::ParentDir)
                || registry.roots[..index].iter().any(|other| {
                    other.path == root.path
                        || (other.volume, other.file_id) == (root.volume, root.file_id)
                })
            {
                bail!("invalid recorder library identity");
            }
        }
        Ok(Self {
            path,
            entries: registry
                .roots
                .into_iter()
                .map(|locator| Library {
                    locator,
                    coordinator: None,
                })
                .collect(),
        })
    }

    pub(crate) fn register(&mut self, root: &Path) -> Result<()> {
        let locator = DirectoryGuard::capture(root, MediaId::new_v4())
            .and_then(|guard| guard.locator())
            .map_err(anyhow::Error::msg)
            .context("recording library identity is unavailable")?;
        for entry in &self.entries {
            let previous = &entry.locator;
            let same_identity =
                (previous.volume, previous.file_id) == (locator.volume, locator.file_id);
            if previous.path == locator.path && same_identity {
                return Ok(());
            }
            if previous.path == locator.path || same_identity {
                bail!("recording library path or identity conflicts with an existing library");
            }
        }
        if self.entries.len() == MAX_ROOTS {
            bail!("recorder library registry is full while older libraries have pending work");
        }
        self.write(
            self.entries
                .iter()
                .map(|entry| entry.locator.clone())
                .chain(std::iter::once(locator.clone()))
                .collect(),
        )?;
        self.entries.push(Library {
            locator,
            coordinator: None,
        });
        Ok(())
    }

    pub(crate) fn resume(&mut self) {
        for entry in &mut self.entries {
            if entry.coordinator.is_none() && DirectoryGuard::recover(&entry.locator).is_ok() {
                entry.coordinator =
                    Coordinator::start_at(&entry.locator.path, Some(entry.locator.clone()))
                        .inspect_err(|code| {
                            tracing::warn!(code, "optional post-game enrichment unavailable")
                        })
                        .ok();
            }
        }
    }

    pub(crate) fn handle(&self, root: &Path) -> Option<Handle> {
        self.entries
            .iter()
            .find(|entry| entry.locator.path == root)?
            .coordinator
            .as_ref()
            .map(Coordinator::handle)
    }

    pub(crate) async fn retire_settled(&mut self, current: &Path) -> Result<()> {
        let mut index = 0;
        while index < self.entries.len() {
            let entry = &mut self.entries[index];
            if entry.locator.path != current
                && DirectoryGuard::recover(&entry.locator).is_ok()
                && entry.coordinator.as_ref().is_some_and(Coordinator::settled)
            {
                let worker = entry.coordinator.take().expect("settled coordinator");
                if worker.stop().await && DirectoryGuard::recover(&entry.locator).is_ok() {
                    let roots = self
                        .entries
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != index)
                        .map(|(_, entry)| entry.locator.clone())
                        .collect();
                    self.write(roots)?;
                    self.entries.remove(index);
                    continue;
                }
            }
            index += 1;
        }
        self.resume();
        Ok(())
    }

    pub(crate) async fn stop(self) {
        for entry in self.entries {
            if let Some(coordinator) = entry.coordinator {
                coordinator.stop().await;
            }
        }
    }

    fn write(&self, roots: Vec<DirectoryLocator>) -> Result<()> {
        let bytes = serde_json::to_vec(&Registry { version: 1, roots })?;
        if bytes.len() > MAX_BYTES {
            bail!("recorder library registry exceeds the size limit");
        }
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", MediaId::new_v4().as_str()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            let wide = |path: &Path| {
                path.as_os_str()
                    .encode_wide()
                    .chain(Some(0))
                    .collect::<Vec<_>>()
            };
            let from = wide(&temporary);
            let to = wide(&self.path);
            // SAFETY: both arguments are terminated UTF-16 paths that remain valid for the call.
            unsafe {
                MoveFileExW(
                    PCWSTR(from.as_ptr()),
                    PCWSTR(to.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::league_client::association;
    use crate::league_client::post_game_ledger::{self as ledger, SavedJob, Store, Terminal};
    use std::time::Duration;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reopens_original_enrichment_after_switching_libraries_and_restarting() {
        let fixture = tempfile::tempdir().unwrap();
        let config = fixture.path().join("config.toml");
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        let game = first.join("games").join("100");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir(&second).unwrap();
        let media = MediaId::new_v4();
        let binding = association::fixture_candidate(media.clone()).result_binding();
        let directory = DirectoryGuard::capture(&game, media.clone()).unwrap();
        let now = ledger::wall_ms().unwrap();
        let (store, mut saved) = Store::open(&first, now).unwrap();
        let mut job = SavedJob::new(binding, directory.locator().unwrap(), now).unwrap();
        job.attempts = 1;
        job.result = Some(super::super::sidecar::tests::result_fixture(&game, &media));
        let deadline = job.expires_ms;
        saved.jobs.push(job);
        store.save(&saved).unwrap();
        drop(store);

        let mut libraries = Libraries::open(&config).unwrap();
        libraries.register(&first).unwrap();
        libraries.register(&second).unwrap();
        libraries.stop().await;

        let mut libraries = Libraries::open(&config).unwrap();
        libraries.register(&second).unwrap();
        libraries.resume();
        assert!(libraries.handle(&second).is_some());
        let result = game.join(chronobreak_league_data::RESULT_FILE);
        tokio::time::timeout(Duration::from_secs(5), async {
            while !result.is_file() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        libraries.stop().await;
        let (_, saved) = Store::open(&first, ledger::wall_ms().unwrap()).unwrap();
        assert_eq!(saved.jobs[0].binding.media_id, media);
        assert_eq!(saved.jobs[0].attempts, 1);
        assert_eq!(saved.jobs[0].expires_ms, deadline);
        assert!(matches!(saved.jobs[0].terminal, Some(Terminal::Completed)));
        assert!(!second.join("games").exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_replaced_root_is_neither_reopened_nor_forgotten() {
        let fixture = tempfile::tempdir().unwrap();
        let config = fixture.path().join("config.toml");
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let mut libraries = Libraries::open(&config).unwrap();
        libraries.register(&first).unwrap();
        libraries.register(&second).unwrap();
        libraries.stop().await;
        std::fs::rename(&first, fixture.path().join("original")).unwrap();
        std::fs::create_dir(&first).unwrap();
        let mut libraries = Libraries::open(&config).unwrap();
        libraries.resume();
        libraries.retire_settled(&second).await.unwrap();
        assert!(libraries.handle(&first).is_none());
        assert!(libraries.register(&first).is_err());
        assert_eq!(libraries.entries.len(), 2);
        assert!(!first.join(ledger::FILE).exists());
        libraries.stop().await;
    }
}
