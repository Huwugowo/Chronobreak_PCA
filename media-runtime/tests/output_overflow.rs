// No libtest harness: its stdout banner could exceed the cap before the child
// acquires the lock, invalidating the termination/lock-release assertion.
#[cfg(windows)]
fn main() {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Duration;

    use queueback_media_runtime::{BoundedProcessErrorKind, run_bounded_process};

    if std::env::args().nth(1).as_deref() == Some("--overflow-child") {
        let lock_path = std::env::args_os().nth(2).expect("fixture lock path");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(lock_path)
            .unwrap();
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(b"1234").unwrap();
        stdout.flush().unwrap();
        std::thread::sleep(Duration::from_secs(30));
        drop(lock);
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let lock_path = directory.path().join("overflow.lock");
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command.arg("--overflow-child").arg(&lock_path);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let error = runtime
        .block_on(run_bounded_process(command, Duration::from_secs(5), 3))
        .unwrap_err();
    assert_eq!(error.kind(), BoundedProcessErrorKind::OutputLimit);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .expect("overflowing child must be reaped and release its file lock");
}

#[cfg(not(windows))]
fn main() {}
