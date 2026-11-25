//! Shared process use and exclusive maintenance of the fixed per-user installation.
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};

use crate::windows::{FileLease, Namespace, regular_executable};

pub struct Installation {
    root: PathBuf,
    state: PathBuf,
}

impl Installation {
    pub fn production() -> Result<Self> {
        let dirs = directories::BaseDirs::new().context("user profile unavailable")?;
        Ok(Self::at(dirs.data_local_dir()))
    }

    fn at(local_data: &Path) -> Self {
        Self {
            root: local_data.join("League Replay"),
            state: local_data.join(".Chronobreak-setup"),
        }
    }

    #[cfg(feature = "setup-fixture")]
    pub fn fixture(local_data: &Path) -> Self {
        Self::at(local_data)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn state(&self) -> &Path {
        &self.state
    }

    fn lease(&self) -> PathBuf {
        self.state.join("maintenance.lock")
    }

    pub fn protect_current_process() -> Result<()> {
        // Keep the handle until Windows tears down the process, including after
        // Rust main returns and while any image mapping still exists.
        static HELD: OnceLock<FileLease> = OnceLock::new();
        if HELD.get().is_none()
            && let Some(lease) = Self::production()?.begin_use(&std::env::current_exe()?)?
        {
            let _ = HELD.set(lease);
        }
        Ok(())
    }

    fn begin_use(&self, executable: &Path) -> Result<Option<FileLease>> {
        let parent = executable
            .parent()
            .context("application has no containing directory")?;
        let matches = |candidate: &Path, root: &Path| {
            candidate.as_os_str().eq_ignore_ascii_case(root.as_os_str())
        };
        let installed = matches(parent, &self.root)
            || match (parent.canonicalize(), self.root.canonicalize()) {
                (Ok(parent), Ok(root)) => matches(&parent, &root),
                _ => false,
            };
        if !installed {
            return Ok(None);
        }
        let guard = FileLease::try_acquire_shared(&self.lease())?
            .context("Chronobreak setup is working. Launch the app after setup exits.")?;
        regular_executable(executable).context("the installation path is unavailable")?;
        match std::fs::symlink_metadata(self.state.join("launch.blocked")) {
            Ok(_) => bail!("Chronobreak installation needs repair before it can start."),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("Chronobreak setup status is unavailable"),
        }
        Ok(Some(guard))
    }

    pub fn begin_maintenance(&self) -> Result<Maintenance> {
        self.begin_maintenance_with(&Namespace::production()?.lease)
    }

    #[cfg(feature = "setup-fixture")]
    pub fn begin_fixture_maintenance(&self) -> Result<Maintenance> {
        self.begin_maintenance_with(&self.state.join("fixture-recorder.lock"))
    }

    fn begin_maintenance_with(&self, recorder_lease: &Path) -> Result<Maintenance> {
        let busy =
            "Close Chronobreak and use Quit in the recorder tray, then retry after they exit.";
        let install = FileLease::try_acquire(&self.lease())?.context(busy)?;
        let recorder = FileLease::try_acquire(recorder_lease)?.context(busy)?;
        Ok(Maintenance {
            _recorder: recorder,
            _installation: install,
        })
    }
}

pub struct Maintenance {
    _recorder: FileLease,
    _installation: FileLease,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn installed_processes_share_use_and_both_leases_exclude_setup() {
        let fixture = tempfile::tempdir().unwrap();
        let install = Installation::at(fixture.path());
        let recorder_lease = fixture.path().join("recorder-owner.lock");
        let desktop = install.root().join("league-replay-app.exe");
        let recorder = install.root().join("recorder.exe");
        std::fs::create_dir(install.root()).unwrap();
        std::fs::write(&desktop, b"fixture desktop").unwrap();
        std::fs::write(&recorder, b"fixture recorder").unwrap();
        let desktop_guard = install.begin_use(&desktop).unwrap().unwrap();
        let recorder_guard = install.begin_use(&recorder).unwrap().unwrap();
        assert!(install.begin_maintenance_with(&recorder_lease).is_err());
        drop(desktop_guard);
        drop(recorder_guard);

        let leased_recorder = FileLease::try_acquire(&recorder_lease).unwrap().unwrap();
        assert!(install.begin_maintenance_with(&recorder_lease).is_err());
        drop(leased_recorder);

        let setup = install.begin_maintenance_with(&recorder_lease).unwrap();
        assert!(install.begin_use(&desktop).is_err());
        assert!(FileLease::try_acquire(&recorder_lease).unwrap().is_none());
        assert!(
            install
                .begin_use(&fixture.path().join("portable.exe"))
                .unwrap()
                .is_none()
        );
        drop(setup);

        std::fs::write(install.state().join("launch.blocked"), b"fixture").unwrap();
        assert!(install.begin_use(&desktop).is_err());
        assert!(install.begin_maintenance_with(&recorder_lease).is_ok());
    }

    #[test]
    fn installed_process_exit_releases_exclusive_maintenance() {
        let fixture = tempfile::tempdir().unwrap();
        let install = Installation::at(fixture.path());
        let recorder_lease = fixture.path().join("recorder-owner.lock");
        std::fs::create_dir(install.root()).unwrap();
        std::fs::write(install.root().join("recorder.exe"), b"fixture recorder").unwrap();
        let ready = fixture.path().join("process-held");
        let mut process = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "install_guard::tests::installed_process_fixture",
                "--ignored",
            ])
            .env("CHRONOBREAK_INSTALL_GUARD_FIXTURE", fixture.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let close = process.stdin.take().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready.exists() && Instant::now() < deadline && process.try_wait().unwrap().is_none()
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let held = ready.exists() && install.begin_maintenance_with(&recorder_lease).is_err();
        drop(close);
        assert!(process.wait().unwrap().success());
        assert!(held);
        assert!(install.begin_maintenance_with(&recorder_lease).is_ok());
    }

    #[test]
    #[ignore = "started by the installed process ownership test"]
    fn installed_process_fixture() {
        let root = PathBuf::from(std::env::var_os("CHRONOBREAK_INSTALL_GUARD_FIXTURE").unwrap());
        let installation = Installation::at(&root);
        let _guard = installation
            .begin_use(&installation.root().join("recorder.exe"))
            .unwrap()
            .unwrap();
        std::fs::write(root.join("process-held"), []).unwrap();
        // The parent closes the pipe when it's finished checking that setup is blocked.
        assert_eq!(std::io::stdin().read(&mut [0]).unwrap(), 0);
    }
}
