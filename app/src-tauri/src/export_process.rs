//! Export work stays owned after its caller disappears. All filesystem stages
//! execute on one admitted blocking worker; child pipes have bounded retention.

use std::fmt;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::time::{Instant, MissedTickBehavior};

const PROGRESS_RECORD_LIMIT: usize = 8 * 1024;
const DIAGNOSTIC_LIMIT: usize = 64 * 1024;
const CANCEL_POLL: Duration = Duration::from_millis(50);

#[derive(Clone, Default)]
pub(crate) struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub(crate) fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Acquire) {
            return Err(Interrupted("export cancelled").into());
        }
        Ok(())
    }
}

struct CancelOnDrop(Cancellation);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.0.store(true, Ordering::Release);
    }
}

/// Move the mutation guard into `work`; dropping this waiter only requests
/// cancellation, never releases the worker's admission or aborts its cleanup.
pub(crate) async fn run_owned<T: Send + 'static>(
    work: impl FnOnce(Cancellation) -> T + Send + 'static,
) -> Result<T> {
    let cancellation = Cancellation::default();
    let _cancel_on_drop = CancelOnDrop(cancellation.clone());
    tokio::task::spawn_blocking(move || work(cancellation))
        .await
        .context("export worker failed")
}

#[derive(Debug)]
pub(crate) struct Interrupted(&'static str);

impl fmt::Display for Interrupted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Interrupted {}

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub deadline: Duration,
    pub stall: Duration,
}

impl Limits {
    pub(crate) fn encoding(duration_seconds: u64) -> Self {
        Self {
            deadline: Duration::from_secs(
                duration_seconds
                    .saturating_mul(20)
                    .saturating_add(120)
                    .clamp(300, 21_600),
            ),
            stall: Duration::from_secs(120),
        }
    }

    pub(crate) fn thumbnail() -> Self {
        Self {
            deadline: Duration::from_secs(30),
            stall: Duration::from_secs(30),
        }
    }
}

struct ProgressRecords {
    record: Vec<u8>,
    latest: u64,
}

impl ProgressRecords {
    fn new() -> Self {
        Self {
            record: Vec::with_capacity(PROGRESS_RECORD_LIMIT),
            latest: 0,
        }
    }

    fn accept(&mut self, bytes: &[u8], progress: &mut impl FnMut(u64)) -> Result<bool> {
        let mut advanced = false;
        for byte in bytes {
            if *byte == b'\n' {
                advanced |= self.finish(progress)?;
            } else {
                if self.record.len() == PROGRESS_RECORD_LIMIT {
                    return Err(Interrupted("ffmpeg progress record exceeded 8 KiB").into());
                }
                self.record.push(*byte);
            }
        }
        Ok(advanced)
    }

    fn finish(&mut self, progress: &mut impl FnMut(u64)) -> Result<bool> {
        let line = std::str::from_utf8(&self.record)
            .map_err(|_| Interrupted("ffmpeg progress is not UTF-8"))?;
        let value = line
            .trim_end_matches('\r')
            .strip_prefix("out_time_us=")
            .and_then(|value| value.parse::<u64>().ok());
        self.record.clear();
        if let Some(value) = value.filter(|value| *value > self.latest) {
            self.latest = value;
            progress(value);
            return Ok(true);
        }
        Ok(false)
    }
}

fn append_tail(tail: &mut Vec<u8>, bytes: &[u8]) {
    let bytes = &bytes[bytes.len().saturating_sub(DIAGNOSTIC_LIMIT)..];
    let discard = (tail.len() + bytes.len()).saturating_sub(DIAGNOSTIC_LIMIT);
    tail.drain(..discard);
    tail.extend_from_slice(bytes);
}

async fn read_pipe(reader: &mut (impl AsyncRead + Unpin), buffer: &mut [u8]) -> Result<usize> {
    reader.read(buffer).await.map_err(|error| {
        anyhow::Error::new(Interrupted("failed to read ffmpeg pipe")).context(error)
    })
}

/// Owns both pipes and the child until reaped. No drain task can escape an error
/// path. A successful process must also finish its progress/diagnostic streams.
pub(crate) async fn run(
    mut command: Command,
    cancellation: &Cancellation,
    limits: Limits,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    cancellation.check()?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().context("failed to start export process")?;
    // These pipes are guaranteed by Command above; keep setup in the cleanup scope.
    let result: Result<()> = async {
        let mut stdout = child
            .stdout
            .take()
            .context("missing export progress pipe")?;
        let mut stderr = child
            .stderr
            .take()
            .context("missing export diagnostic pipe")?;
        let mut output_buffer = [0_u8; 4096];
        let mut error_buffer = [0_u8; 4096];
        let mut records = ProgressRecords::new();
        let mut diagnostics = Vec::with_capacity(DIAGNOSTIC_LIMIT);
        let started = Instant::now();
        let mut advanced_at = started;
        let mut timer = tokio::time::interval(CANCEL_POLL);
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut output_done = false;
        let mut error_done = false;
        let mut status = None;
        loop {
            cancellation.check()?;
            if started.elapsed() >= limits.deadline {
                return Err(Interrupted("export process exceeded its deadline").into());
            }
            if advanced_at.elapsed() >= limits.stall {
                return Err(Interrupted("export process stopped making progress").into());
            }
            if output_done && error_done && status.is_some() {
                break;
            }
            tokio::select! {
                biased;
                _ = timer.tick() => {},
                result = child.wait(), if status.is_none() => {
                    status = Some(result.context("failed to reap export process")?);
                },
                result = read_pipe(&mut stdout, &mut output_buffer), if !output_done => {
                    let count = result?;
                    if count == 0 {
                        output_done = true;
                        records.finish(&mut progress)?;
                    } else if records.accept(&output_buffer[..count], &mut progress)? {
                        advanced_at = Instant::now();
                    }
                },
                result = read_pipe(&mut stderr, &mut error_buffer), if !error_done => {
                    let count = result?;
                    error_done = count == 0;
                    append_tail(&mut diagnostics, &error_buffer[..count]);
                },
            }
        }
        if !status
            .context("missing export process exit status")?
            .success()
        {
            let message = String::from_utf8_lossy(&diagnostics);
            let message = message
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("ffmpeg exited without an error message");
            bail!("{}", message.trim().chars().take(320).collect::<String>());
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        // Always wait even if termination reports that the process already exited.
        let kill_error = child.start_kill().err();
        if let Err(error) = child.wait().await {
            return result.context(format!(
                "export child cleanup failed: {error}; kill: {kill_error:?}"
            ));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    // Spawn just this test in a fresh process: no shell, execution policy, real
    // encoder, recording, or external child process is involved.
    #[test]
    fn child_fixture() {
        let Ok(mode) = std::env::var("CHRONOBREAK_EXPORT_CHILD") else {
            return;
        };
        let marker = std::env::var_os("CHRONOBREAK_EXPORT_MARKER").unwrap();
        std::fs::write(&marker, "ready").unwrap();
        let mut stdout = std::io::stdout();
        match mode.as_str() {
            "invalid" => {
                stdout.write_all(&[0xff, b'\n']).unwrap();
            }
            "oversized" => {
                stdout
                    .write_all(&vec![b'x'; PROGRESS_RECORD_LIMIT + 1])
                    .unwrap();
            }
            "progress" => {
                stdout
                    .write_all(b"out_time_us=100\nout_time_us=200\nprogress=end\n")
                    .unwrap();
                return;
            }
            "flood" => {
                let mut stderr = std::io::stderr();
                for _ in 0..256 {
                    stderr.write_all(&[b'x'; 4096]).unwrap();
                }
                stderr.write_all(b"\nlast diagnostic\n").unwrap();
                std::process::exit(7);
            }
            "advancing" => {
                for time in 1..200 {
                    writeln!(stdout, "out_time_us={time}").unwrap();
                    stdout.flush().unwrap();
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            "silent" => {}
            _ => panic!("unknown child fixture mode"),
        }
        stdout.flush().unwrap();
        std::thread::sleep(Duration::from_millis(600));
        std::fs::write(marker, "wrote after return").unwrap();
    }

    fn fixture(directory: &TempDir, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "export_process::tests::child_fixture",
                "--nocapture",
            ])
            .env("CHRONOBREAK_EXPORT_CHILD", mode)
            .env("CHRONOBREAK_EXPORT_MARKER", directory.path().join("marker"));
        command
    }

    fn limits() -> Limits {
        Limits {
            deadline: Duration::from_secs(5),
            stall: Duration::from_secs(5),
        }
    }

    #[tokio::test]
    async fn progress_is_incremental_and_diagnostics_are_drained() {
        let directory = tempfile::tempdir().unwrap();
        let mut values = Vec::new();
        run(
            fixture(&directory, "progress"),
            &Cancellation::default(),
            limits(),
            |v| values.push(v),
        )
        .await
        .unwrap();
        assert_eq!(values, [100, 200]);
        let error = run(
            fixture(&directory, "flood"),
            &Cancellation::default(),
            limits(),
            |_| {},
        )
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "last diagnostic");
        assert!(
            !error.is::<Interrupted>(),
            "ordinary encoder exits may fall back"
        );
    }

    #[tokio::test]
    async fn malformed_progress_kills_and_reaps_before_return() {
        for mode in ["invalid", "oversized"] {
            let directory = tempfile::tempdir().unwrap();
            let error = run(
                fixture(&directory, mode),
                &Cancellation::default(),
                limits(),
                |_| {},
            )
            .await
            .unwrap_err();
            assert!(error.is::<Interrupted>(), "{error:#}");
            tokio::time::sleep(Duration::from_millis(750)).await;
            assert_eq!(
                std::fs::read_to_string(directory.path().join("marker")).unwrap(),
                "ready"
            );
        }
    }

    #[tokio::test]
    async fn stalls_and_absolute_deadlines_are_fatal_and_reaped() {
        for (mode, policy, expected) in [
            (
                "silent",
                Limits {
                    deadline: Duration::from_secs(5),
                    stall: Duration::from_millis(300),
                },
                "stopped making progress",
            ),
            (
                "advancing",
                Limits {
                    deadline: Duration::from_millis(300),
                    stall: Duration::from_secs(5),
                },
                "deadline",
            ),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let error = run(
                fixture(&directory, mode),
                &Cancellation::default(),
                policy,
                |_| {},
            )
            .await
            .unwrap_err();
            assert!(
                error.is::<Interrupted>() && error.to_string().contains(expected),
                "{error:#}"
            );
            tokio::time::sleep(Duration::from_millis(750)).await;
            assert_eq!(
                std::fs::read_to_string(directory.path().join("marker")).unwrap(),
                "ready"
            );
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_waiter_retains_worker_until_child_is_reaped() {
        let directory = tempfile::tempdir().unwrap();
        let command = fixture(&directory, "silent");
        let slot = Arc::new(tokio::sync::Semaphore::new(1));
        let permit = Arc::clone(&slot).acquire_owned().await.unwrap();
        let handle = tokio::runtime::Handle::current();
        let (done, finished) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(run_owned(move |cancel| {
            let _permit = permit;
            let result = handle.block_on(run(command, &cancel, limits(), |_| {}));
            done.send(result).unwrap();
        }));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !directory.path().join("marker").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let result = tokio::time::timeout(Duration::from_secs(5), finished)
            .await
            .unwrap()
            .unwrap();
        assert!(result.unwrap_err().is::<Interrupted>());
        let _permit = tokio::time::timeout(Duration::from_secs(5), slot.acquire())
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(750)).await;
        assert_eq!(
            std::fs::read_to_string(directory.path().join("marker")).unwrap(),
            "ready"
        );
    }

    #[test]
    fn diagnostic_retention_and_encoding_limits_are_bounded() {
        let mut tail = Vec::with_capacity(DIAGNOSTIC_LIMIT);
        for _ in 0..1000 {
            append_tail(&mut tail, &[b'x'; 4096]);
            assert!(tail.len() <= DIAGNOSTIC_LIMIT);
        }
        append_tail(&mut tail, b"end");
        assert!(tail.ends_with(b"end"));
        assert_eq!(Limits::encoding(1).deadline, Duration::from_secs(300));
        assert_eq!(
            Limits::encoding(u64::MAX).deadline,
            Duration::from_secs(21_600)
        );
    }
}
