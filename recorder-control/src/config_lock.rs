//! The same adjacent lock name is used by every configuration writer.
use anyhow::Result;
use std::path::Path;
use std::time::Duration;

pub struct ConfigLock {
    #[cfg(windows)]
    _lease: crate::windows::FileLease,
    #[cfg(not(windows))]
    _file: std::fs::File,
}

impl ConfigLock {
    pub fn acquire(config: &Path) -> Result<Self> {
        let config = std::path::absolute(config)?;
        let mut name = config.as_os_str().to_os_string();
        name.push(".lock");
        let path = std::path::PathBuf::from(name);
        #[cfg(windows)]
        {
            Ok(Self {
                _lease: crate::windows::FileLease::acquire(&path, Duration::from_secs(2))?,
            })
        }
        #[cfg(not(windows))]
        {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?;
            let started = std::time::Instant::now();
            loop {
                match file.try_lock() {
                    Ok(()) => break,
                    Err(std::fs::TryLockError::WouldBlock)
                        if started.elapsed() < Duration::from_secs(2) =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) => {
                        return Err(anyhow::anyhow!("configuration lock failed: {error}"));
                    }
                }
            }
            Ok(Self { _file: file })
        }
    }
}
