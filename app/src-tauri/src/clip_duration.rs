//! Optional display probes. The child only receives the existing protected HTTP route.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};
use serde::Serialize;

use crate::playback_file::{self, ApprovedRoot, FileFacts, FileScope};

const OUTPUT_LIMIT: u64 = 4096;
const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum DurationState {
    Available { duration_ms: u64 },
    Unavailable,
}

#[derive(Clone)]
pub(crate) struct ObservedDuration {
    pub facts: Option<FileFacts>,
    pub state: DurationState,
}

pub(crate) fn observe(path: &Path, root: &ApprovedRoot) -> Result<(std::fs::File, FileFacts)> {
    let file = playback_file::open_file(path, FileScope::Root(root))?;
    let facts = playback_file::file_facts(&file)?;
    Ok((file, facts))
}

pub(crate) fn resolve(
    path: &Path,
    root: &ApprovedRoot,
    origin: &str,
    id: &str,
    ffprobe: Option<&Path>,
    cancelled: impl Fn() -> bool,
    cached: Option<&ObservedDuration>,
) -> ObservedDuration {
    resolve_with(path, root, cached, || {
        let tool = ffprobe.ok_or_else(|| anyhow::anyhow!("media runtime unavailable"))?;
        let mut command = probe_command(tool, origin, id)?;
        let bytes = run_child(&mut command, DEADLINE, cancelled)?;
        let seconds = std::str::from_utf8(&bytes)?.trim().parse::<f64>()?;
        ensure!(
            seconds.is_finite() && seconds > 0.0 && seconds * 1000.0 < u64::MAX as f64,
            "invalid optional duration"
        );
        Ok((seconds * 1000.0).round() as u64)
    })
}

fn resolve_with(
    path: &Path,
    root: &ApprovedRoot,
    cached: Option<&ObservedDuration>,
    probe: impl FnOnce() -> Result<u64>,
) -> ObservedDuration {
    let Ok((_handle, before)) = observe(path, root) else {
        return ObservedDuration {
            facts: None,
            state: DurationState::Unavailable,
        };
    };
    // Keeping the first handle alive also prevents file-ID reuse during this probe.
    if let Some(cached) = cached.filter(|cached| cached.facts.as_ref() == Some(&before)) {
        return cached.clone();
    }
    if cached.is_some() {
        return ObservedDuration {
            facts: Some(before),
            state: DurationState::Unavailable,
        };
    }
    let duration = probe();
    let unchanged = observe(path, root).is_ok_and(|(_, after)| after == before);
    ObservedDuration {
        facts: unchanged.then_some(before),
        state: match duration {
            Ok(duration_ms) if unchanged => DurationState::Available { duration_ms },
            _ => DurationState::Unavailable,
        },
    }
}

fn probe_command(tool: &Path, origin: &str, id: &str) -> Result<Command> {
    ensure!(
        crate::library::valid_clip_asset(&format!("{id}.mp4")),
        "invalid clip ID"
    );
    let mut command = Command::new(tool);
    command
        .args([
            "-v",
            "error",
            "-f",
            "mov",
            "-enable_drefs",
            "0",
            "-use_absolute_path",
            "0",
            "-protocol_whitelist",
            "http,tcp",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(format!("{origin}/clips/{id}.mp4"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    Ok(command)
}

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // Reap on every exit, including unwinding. Never format the command or stderr:
        // either can contain the bearer capability.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_child(
    command: &mut Command,
    deadline: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<u8>> {
    ensure!(!cancelled(), "optional probe cancelled");
    let mut child = OwnedChild(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let stdout = child
        .0
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("missing probe output"))?;
    let started = Instant::now();
    // Exactly one bounded reader accompanies the one owned child. It is joined after
    // kill/reap, before the caller can release the duration worker permit.
    std::thread::scope(|scope| {
        let (send, receive) = mpsc::sync_channel(1);
        let reader = scope.spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout
                .take(OUTPUT_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = send.send(result);
        });
        let result = (|| {
            let mut output = None;
            loop {
                ensure!(!cancelled(), "optional probe cancelled");
                ensure!(started.elapsed() < deadline, "optional probe timed out");
                if output.is_none() {
                    match receive.try_recv() {
                        Ok(bytes) => {
                            let bytes = bytes?;
                            ensure!(
                                bytes.len() <= OUTPUT_LIMIT as usize,
                                "optional probe output exceeded limit"
                            );
                            output = Some(bytes);
                        }
                        Err(mpsc::TryRecvError::Empty) => {}
                        Err(mpsc::TryRecvError::Disconnected) => {
                            bail!("optional probe reader failed")
                        }
                    }
                }
                if let Some(status) = child.0.try_wait()? {
                    ensure!(status.success(), "optional probe failed");
                    if let Some(bytes) = output.take() {
                        return Ok(bytes);
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })();
        drop(child); // Kill/reap before joining a possibly blocked pipe read.
        if reader.join().is_err() {
            bail!("optional probe reader failed");
        }
        result
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn probe_arguments_only_allow_protected_http_mov_without_external_references() {
        let command = probe_command(
            Path::new("packaged-ffprobe.exe"),
            "http://127.0.0.1:123/cap/test",
            "100_200",
        )
        .unwrap();
        let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
        for pair in [
            ["-f", "mov"],
            ["-enable_drefs", "0"],
            ["-use_absolute_path", "0"],
            ["-protocol_whitelist", "http,tcp"],
        ] {
            assert!(args.windows(2).any(|a| a == pair));
        }
        assert_eq!(
            args.last(),
            Some(&"http://127.0.0.1:123/cap/test/clips/100_200.mp4")
        );
        assert!(probe_command(Path::new("unused"), "unused", "../secret").is_err());
        assert_eq!(OUTPUT_LIMIT, 4096);
        assert_eq!(DEADLINE, Duration::from_secs(10));
    }

    #[test]
    fn replacement_deletion_and_cached_identity_changes_become_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let root = ApprovedRoot::new(directory.path()).unwrap();
        let path = directory.path().join("100_200.mp4");
        fs::write(&path, b"original").unwrap();
        let cached = resolve_with(&path, &root, None, || Ok(1000));
        assert_eq!(cached.state, DurationState::Available { duration_ms: 1000 });
        assert_eq!(
            resolve_with(&path, &root, Some(&cached), || panic!(
                "cached success reprobed"
            ))
            .state,
            cached.state
        );
        let changed = resolve_with(&path, &root, None, || {
            fs::rename(&path, directory.path().join("previous.mp4"))?;
            fs::write(&path, b"replaced")?; // Same length: file identity must distinguish it.
            Ok(2000)
        });
        assert_eq!(changed.state, DurationState::Unavailable);
        assert_eq!(
            resolve_with(&path, &root, Some(&cached), || panic!(
                "changed cache must require retry"
            ))
            .state,
            DurationState::Unavailable
        );
        let deleted = resolve_with(&path, &root, None, || {
            fs::remove_file(&path)?;
            Ok(3000)
        });
        assert_eq!(deleted.state, DurationState::Unavailable);
        fs::write(&path, b"retry").unwrap();
        assert_eq!(
            resolve_with(&path, &root, Some(&deleted), || panic!(
                "unavailable requires retry"
            ))
            .state,
            DurationState::Unavailable
        );
        assert_eq!(
            resolve_with(&path, &root, None, || Ok(4000)).state,
            DurationState::Available { duration_ms: 4000 }
        );
    }

    fn powershell(script: &str) -> Command {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
        command
    }

    #[test]
    fn actual_child_output_cap_and_failure_are_local_errors() {
        assert_eq!(
            run_child(
                &mut powershell("[Console]::Out.Write('1.25')"),
                DEADLINE,
                || false
            )
            .unwrap(),
            b"1.25"
        );
        let error = run_child(
            &mut powershell("[Console]::Out.Write('x' * 5000); Start-Sleep -Seconds 60"),
            DEADLINE,
            || false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("output exceeded limit"));
        let error = run_child(
            &mut powershell("[Console]::Error.Write('secret URL must be discarded'); exit 1"),
            DEADLINE,
            || false,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "optional probe failed");
    }

    #[test]
    fn actual_child_ten_second_deadline_kills_and_reaps() {
        let started = Instant::now();
        let error = run_child(&mut powershell("Start-Sleep -Seconds 60"), DEADLINE, || {
            false
        })
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() >= DEADLINE);
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "child/reader ownership was not released after deadline"
        );
    }

    #[test]
    fn actual_child_cancellation_after_ready_kills_and_reaps() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("ready");
        let script = directory.path().join("child.ps1");
        fs::write(
            &script,
            "[System.IO.File]::WriteAllText($args[0], 'ready'); Start-Sleep -Seconds 60",
        )
        .unwrap();
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(script)
            .arg(&marker);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
        let error = run_child(&mut command, DEADLINE, || marker.exists()).unwrap_err();
        assert!(
            marker.exists(),
            "cancellation must follow the child's ready signal"
        );
        assert_eq!(error.to_string(), "optional probe cancelled");
    }

    #[tokio::test]
    async fn replacement_during_probe_admission_cannot_deliver_outside_root_bytes() {
        use crate::playback_policy::BoundServer;
        use crate::playback_server::{self, MediaRoots, PlaybackMetrics};
        use std::sync::Arc;
        let directory = tempfile::tempdir().unwrap();
        let library = directory.path().join("library");
        let clips = library.join("clips");
        let inside = library.join("real-clips");
        let outside = directory.path().join("outside");
        fs::create_dir_all(&inside).unwrap();
        fs::create_dir(&outside).unwrap();
        let status = Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(&clips)
            .arg(&inside)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        fs::write(clips.join("100_200.mp4"), b"approved").unwrap();
        fs::write(outside.join("100_200.mp4"), b"outside-secret").unwrap();
        let roots = Arc::new(MediaRoots::new(library.clone()).unwrap());
        let (_, approved) = roots.output_pair();
        let origin = playback_server::start(
            BoundServer::bind().unwrap(),
            roots,
            Arc::new(PlaybackMetrics::default()),
            directory.path().join("ddragon"),
            #[cfg(feature = "replay-benchmark")]
            None,
            false,
        )
        .await
        .unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let selected = library.join("clips/100_200.mp4");
        let task = tokio::task::spawn_blocking(move || {
            resolve_with(&selected, &approved, None, || {
                // This is after validated metadata admission, immediately before child spawn.
                started.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(5))?;
                // A real native HTTP client child stands in for ffprobe; it receives only
                // the protected route. Its stdout must contain no outside bytes.
                let directory = tempfile::tempdir()?;
                let script = directory.path().join("client.ps1");
                fs::write(
                    &script,
                    "try { $c = New-Object System.Net.WebClient; [Console]::Out.Write($c.DownloadString($args[0])) } catch { [Console]::Out.Write('denied') }",
                )?;
                let mut command = Command::new("powershell.exe");
                command
                    .args(["-NoProfile", "-NonInteractive", "-File"])
                    .arg(script)
                    .arg(format!("{origin}/clips/100_200.mp4"));
                let bytes = run_child(&mut command, DEADLINE, || false)?;
                assert_eq!(bytes, b"denied");
                Ok(1000)
            })
        });
        tokio::time::timeout(Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap();
        // Replace the junction itself; do not try to rename a directory whose
        // descendant is held open (Windows denies that operation).
        fs::remove_dir(&clips).unwrap();
        let status = Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(&clips)
            .arg(&outside)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        release.send(()).unwrap();
        assert_eq!(task.await.unwrap().state, DurationState::Unavailable);
        assert_eq!(
            fs::read(outside.join("100_200.mp4")).unwrap(),
            b"outside-secret"
        );
    }
    include!("clip_duration_packaged_tests.rs");
}
