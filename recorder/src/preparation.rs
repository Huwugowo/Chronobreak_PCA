//! Own the windowless preparation helper for the lifetime of the recorder.
use std::{path::Path, time::Duration};
pub(crate) fn start(root: &Path) -> tokio::task::JoinHandle<()> {
    let root = root.to_owned();
    tokio::spawn(async move {
        let Some(exe) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("league-replay-app.exe")))
        else {
            return;
        };
        if !exe.is_file() {
            tracing::info!("background preparation helper not installed beside recorder");
            return;
        }
        loop {
            let mut command = tokio::process::Command::new(&exe);
            command
                .arg("--prepare-library")
                .arg(&root)
                .kill_on_drop(true)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            // Hidden, below normal priority: preparation yields to gameplay.
            #[cfg(windows)]
            command.creation_flags(0x08000000 | 0x00004000);
            match command.spawn() {
                Ok(mut child) => {
                    let _ = child.wait().await;
                }
                Err(_) => tracing::warn!("background preparation helper could not start"),
            };
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    })
}
pub(crate) struct Owner(pub tokio::task::JoinHandle<()>);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.abort();
    }
}
