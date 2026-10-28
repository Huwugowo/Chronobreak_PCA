#![cfg(windows)]

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chronobreak_recorder_control::windows::{ControlOwner, FileLease, Namespace, query};
use chronobreak_recorder_control::{AvailabilityEvent, CaptureActivity, ServiceState};

fn namespace(root: &Path) -> Namespace {
    Namespace {
        lease: root.join("owner.lock"),
        pipe: format!(
            r"\\.\pipe\Chronobreak.Test.{}",
            root.file_name().unwrap().to_string_lossy()
        ),
    }
}

struct FixtureChild(Child);
impl Drop for FixtureChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn(root: &Path, id: &str) -> FixtureChild {
    FixtureChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fixture_child", "--nocapture"])
            .env("CHRONOBREAK_CONTROL_FIXTURE", root)
            .env("CHRONOBREAK_CONTROL_FIXTURE_ID", id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

fn wait_for(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "missing fixture receipt: {path:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn fixture_child() {
    let Some(root) = std::env::var_os("CHRONOBREAK_CONTROL_FIXTURE") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    assert!(root.join("fixture-sentinel").is_file());
    let id = std::env::var("CHRONOBREAK_CONTROL_FIXTURE_ID").unwrap();
    let owner = ControlOwner::start(&namespace(&root)).unwrap();
    std::fs::write(
        root.join(format!("result-{id}")),
        if owner.is_some() {
            "owner"
        } else {
            "duplicate"
        },
    )
    .unwrap();
    let Some(owner) = owner else {
        return;
    };
    owner.apply(AvailabilityEvent::Initialized);
    let start = Instant::now();
    while !root.join("stop-fixture").exists() && start.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn lease_rejects_writers_and_deletion_and_releases_without_unlinking() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("owner.lock");
    let owner = FileLease::try_acquire(&path).unwrap().unwrap();
    assert!(FileLease::try_acquire(&path).unwrap().is_none());
    assert!(std::fs::remove_file(&path).is_err());
    drop(owner);
    assert!(path.exists());
    assert!(FileLease::try_acquire(&path).unwrap().is_some());
}

#[test]
fn ten_concurrent_contenders_cannot_start_another_owner_and_crash_releases() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("fixture-sentinel"), "fixture").unwrap();
    let mut first = spawn(root.path(), "first");
    wait_for(&root.path().join("result-first"));
    assert_eq!(
        std::fs::read_to_string(root.path().join("result-first")).unwrap(),
        "owner"
    );
    let mut children: Vec<_> = (0..10)
        .map(|i| spawn(root.path(), &i.to_string()))
        .collect();
    for (i, child) in children.iter_mut().enumerate() {
        wait_for(&root.path().join(format!("result-{i}")));
        assert_eq!(
            std::fs::read_to_string(root.path().join(format!("result-{i}"))).unwrap(),
            "duplicate"
        );
        assert!(child.0.wait().unwrap().success());
    }
    first.0.kill().unwrap(); // Only this test's generated non-capture child.
    first.0.wait().unwrap();
    assert!(
        ControlOwner::start(&namespace(root.path()))
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn real_pipe_checks_image_protocol_and_separate_capture_status() {
    let root = tempfile::tempdir().unwrap();
    let ns = namespace(root.path());
    let owner = ControlOwner::start(&ns).unwrap().unwrap();
    let exe = std::env::current_exe().unwrap();
    let status = query(&ns, &exe).await.unwrap();
    assert_eq!(status.service_state, ServiceState::Starting);
    owner.apply(AvailabilityEvent::Initialized);
    owner.apply(AvailabilityEvent::Capture(CaptureActivity::Recording));
    // A previous client's disconnect may still be completing. Bound retries so
    // a broken listener fails this test rather than hanging the test process.
    let started = Instant::now();
    let status = loop {
        match query(&ns, &exe).await {
            Ok(status) => break status,
            Err(error) => {
                assert!(
                    started.elapsed() < Duration::from_secs(5),
                    "listener unavailable: {error:#}"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    };
    assert_eq!(status.service_state, ServiceState::Available);
    assert_eq!(status.capture_activity, CaptureActivity::Recording);
    assert!(
        query(&ns, &exe.with_file_name("wrong-image.exe"))
            .await
            .is_err()
    );
    drop(owner);
    assert!(ControlOwner::start(&ns).unwrap().is_some());
}

#[tokio::test]
async fn malformed_and_silent_clients_do_not_hold_owner_shutdown() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::windows::named_pipe::ClientOptions;
    let root = tempfile::tempdir().unwrap();
    let ns = namespace(root.path());
    let owner = ControlOwner::start(&ns).unwrap().unwrap();
    let mut client = ClientOptions::new().open(&ns.pipe).unwrap();
    client.write_u32_le(5000).await.unwrap();
    let start = Instant::now();
    drop(owner);
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn bad_protocol_and_silent_client_expire_without_failing_the_service() {
    use chronobreak_recorder_control::{Status, StatusRequest, read_frame, write_frame};
    use tokio::net::windows::named_pipe::ClientOptions;
    let root = tempfile::tempdir().unwrap();
    let ns = namespace(root.path());
    let owner = ControlOwner::start(&ns).unwrap().unwrap();
    owner.apply(AvailabilityEvent::Initialized);
    let mut client = ClientOptions::new().open(&ns.pipe).unwrap();
    write_frame(
        &mut client,
        &StatusRequest {
            protocol_version: 999,
        },
    )
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), read_frame::<Status>(&mut client))
            .await
            .unwrap()
            .is_err()
    );
    drop(client);
    let silent = ClientOptions::new().open(&ns.pipe).unwrap();
    tokio::time::sleep(Duration::from_millis(2100)).await;
    drop(silent);
    let status = query(&ns, &std::env::current_exe().unwrap()).await.unwrap();
    assert_eq!(status.service_state, ServiceState::Available);
    drop(owner);
}

#[tokio::test]
#[ignore = "five-minute isolated ownership/observer resource acceptance"]
async fn five_minute_idle_observers_and_launch_triggers_remain_bounded() {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
    fn resources() -> (u32, u32) {
        let mut handles = 0;
        // SAFETY: valid pseudo-process handle and writable count output.
        unsafe {
            GetProcessHandleCount(GetCurrentProcess(), &mut handles).unwrap();
        }
        // SAFETY: read-only system thread snapshot, uniquely owned below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).unwrap() };
        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut threads = 0;
        // SAFETY: live snapshot and initialized correctly sized entry buffer.
        let mut result = unsafe { Thread32First(snapshot, &mut entry) };
        while result.is_ok() {
            if entry.th32OwnerProcessID == std::process::id() {
                threads += 1;
            }
            // SAFETY: same live snapshot and entry buffer for the next item.
            result = unsafe { Thread32Next(snapshot, &mut entry) };
        }
        // SAFETY: exactly one release of this fixture's snapshot handle.
        unsafe {
            CloseHandle(snapshot).unwrap();
        }
        (handles, threads)
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("fixture-sentinel"), "fixture").unwrap();
    let ns = namespace(root.path());
    let owner = ControlOwner::start(&ns).unwrap().unwrap();
    owner.apply(AvailabilityEvent::Initialized);
    let exe = std::env::current_exe().unwrap();
    for _ in 0..3 {
        query(&ns, &exe).await.unwrap();
    }
    let baseline = resources();
    let start = Instant::now();
    let mut samples = Vec::new();
    while start.elapsed() < Duration::from_secs(300) {
        let status = query(&ns, &exe).await.unwrap();
        assert_eq!(status.service_state, ServiceState::Available);
        let mut trigger = spawn(root.path(), "soak-trigger");
        assert!(trigger.0.wait().unwrap().success());
        drop(trigger);
        let sample = resources();
        assert!(
            sample.0 <= baseline.0 + 4 && sample.1 <= baseline.1 + 1,
            "unbounded resources: {baseline:?} -> {sample:?}"
        );
        samples.push((start.elapsed().as_secs(), sample));
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    println!("control_resource_acceptance baseline={baseline:?} samples={samples:?}");
    drop(owner);
}
