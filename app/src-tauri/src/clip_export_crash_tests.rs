//! Real process death, contained in a kill-on-close Windows Job Object.
use super::*;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

fn contained_job() -> OwnedHandle {
    // SAFETY: own the returned job handle; the writable limit structure lives
    // through the call. Assign before spawning any media process. No inheritable
    // handle is created, so death closes the only job handle and kills descendants.
    unsafe {
        let raw = CreateJobObjectW(None, None).unwrap();
        let job = OwnedHandle::from_raw_handle(raw.0);
        let mut information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            raw,
            JobObjectExtendedLimitInformation,
            &information as *const _ as *const _,
            std::mem::size_of_val(&information) as u32,
        )
        .unwrap();
        AssignProcessToJobObject(raw, GetCurrentProcess()).unwrap();
        job
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "internal self-executing crash fixture"]
async fn export_crash_child() {
    let Some(root) = std::env::var_os("CHRONOBREAK_EXPORT_CRASH_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    assert_eq!(
        fs::read(root.join(".crash-fixture")).unwrap(),
        b"dedicated temporary export fixture"
    );
    let _job = contained_job();
    let runtime = std::env::var_os("QUEUEBACK_TEST_MEDIA_RUNTIME").unwrap();
    let tools = queueback_media_runtime::resolve_root(Path::new(&runtime))
        .await
        .unwrap();
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("games/1786000000/metadata.json")).unwrap())
            .unwrap();
    let timeline: MediaTimelineV2 =
        serde_json::from_value(metadata["media_timeline"].clone()).unwrap();
    let result = export(
        ExportDestination {
            root: &ApprovedRoot::new(&root).unwrap(),
            recovery_mode: RecoveryMode::Normal,
        },
        &root.join("music"),
        tools.ffmpeg(),
        tools.ffprobe(),
        export_request(&timeline, ClipMusicSource::None),
        Channel::new(|_| Ok(())),
        &Cancellation::default(),
    )
    .await;
    assert!(
        std::env::var_os("CHRONOBREAK_EXPECT_COLLISION").is_some() && result.is_err(),
        "crash fixture unexpectedly returned: {result:?}"
    );
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
    }
}

async fn wait_gate(child: &mut ChildGuard, marker: &Path) -> serde_json::Value {
    let started = Instant::now();
    loop {
        if let Ok(bytes) = fs::read(marker)
            && let Ok(value) = serde_json::from_slice(&bytes)
        {
            return value;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "crash child exited before gate"
        );
        assert!(
            started.elapsed() < Duration::from_secs(90),
            "crash gate timed out"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn spawn_child(root: &Path, marker: &Path, label: &str, collision: bool) -> ChildGuard {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "clip_export::packaged_tests::crash_tests::export_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("CHRONOBREAK_EXPORT_CRASH_ROOT", root)
        .env("CHRONOBREAK_EXPORT_CRASH_MARKER", marker)
        .env("CHRONOBREAK_EXPORT_CRASH_GATE", label);
    if collision {
        command.env("CHRONOBREAK_EXPECT_COLLISION", "1");
    }
    ChildGuard(command.spawn().unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires packaged runtime; hard-kills only sentinel-owned test processes"]
async fn packaged_export_crash_recovery_matrix() {
    let runtime =
        std::env::var_os("QUEUEBACK_TEST_MEDIA_RUNTIME").expect("set staged runtime root");
    let tools = queueback_media_runtime::resolve_root(Path::new(&runtime))
        .await
        .unwrap();
    let source_fixture = tempfile::tempdir().unwrap();
    let (source, timeline) = write_source(source_fixture.path(), &tools, false).await;
    let original_source = fs::read(&source).unwrap();
    let original_metadata = fs::read(source.parent().unwrap().join("metadata.json")).unwrap();
    let labels = [
        "encoding",
        "probe",
        "thumbnail",
        "publish-0-mp4",
        "publish-0-jpg",
        "publish-1-mp4",
        "publish-1-jpg",
        "publish-2-mp4",
        "publish-2-jpg",
    ];
    for (case, label) in labels.iter().enumerate() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::write(
            root.join(".crash-fixture"),
            b"dedicated temporary export fixture",
        )
        .unwrap();
        fs::create_dir_all(root.join("games/1786000000")).unwrap();
        fs::create_dir(root.join("clips")).unwrap();
        fs::write(root.join("games/1786000000/video.mp4"), &original_source).unwrap();
        fs::write(
            root.join("games/1786000000/metadata.json"),
            &original_metadata,
        )
        .unwrap();
        let preexisting = root.join("clips/1786000000_1.mp4");
        let preexisting_thumbnail = preexisting.with_extension("jpg");
        fs::write(&preexisting, &original_source).unwrap();
        fs::write(&preexisting_thumbnail, b"preexisting thumbnail sentinel").unwrap();
        let approved = ApprovedRoot::new(root).unwrap();
        let marker = root.join("crash-gate.json");
        let mut child = spawn_child(root, &marker, label, false);
        let gate = wait_gate(&mut child, &marker).await;
        assert_eq!(gate["label"], *label);
        // Retain the actual tool handle so PID reuse cannot confuse termination.
        let tool = gate["pid"].as_u64().map(|pid| unsafe {
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, false, pid as u32).unwrap();
            OwnedHandle::from_raw_handle(handle.0)
        });
        assert!(
            crate::export_staging::recover(&approved, RecoveryMode::Normal, || Ok(()))
                .unwrap()
                .active
        );
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        if let Some(tool) = tool {
            use std::os::windows::io::AsRawHandle;
            // SAFETY: this owned process handle remains valid for the bounded wait.
            assert_eq!(
                unsafe { WaitForSingleObject(HANDLE(tool.as_raw_handle()), 10_000) },
                WAIT_OBJECT_0
            );
        }
        let report =
            crate::export_staging::recover(&approved, RecoveryMode::Normal, || Ok(())).unwrap();
        assert_eq!(report.recovered, 1, "{label}: {report:?}");
        assert_eq!(report.retained, 0);
        let snapshot =
            library::build_library_snapshot(root, "http://fixture", "snapshot".into(), || Ok(()))
                .unwrap();
        let expected_new = if case < 3 { 0 } else { (case - 3) / 2 + 1 };
        assert_eq!(snapshot.clips.len(), expected_new + 1, "{label}");
        assert_eq!(snapshot.usage.clip_count, (expected_new + 1) as u64);
        let mut expected_bytes = original_source.len() as u64;
        let request = export_request(&timeline, ClipMusicSource::None);
        let timing = validate_for_metadata(&request, &timeline).unwrap();
        let mut published: Vec<_> = snapshot
            .clips
            .iter()
            .filter(|clip| clip.filename != "1786000000_1")
            .collect();
        published.sort_by(|a, b| a.filename.cmp(&b.filename));
        for (index, clip) in published.iter().enumerate() {
            let media = root.join("clips").join(format!("{}.mp4", clip.filename));
            let profile = export_profile(&request, &timing, request.presets[index], None).unwrap();
            let validation =
                validate_exported_media(tools.ffprobe(), &media, &request, &timing, &profile)
                    .await
                    .unwrap();
            assert_eq!(validation.frame_count.get(), 150);
            fixture_command(
                tools.ffmpeg(),
                &[
                    "-v",
                    "error",
                    "-xerror",
                    "-nostdin",
                    "-i",
                    media.to_str().unwrap(),
                    "-f",
                    "null",
                    "NUL",
                ],
                None,
            )
            .await;
            expected_bytes += fs::metadata(media).unwrap().len();
        }
        assert_eq!(snapshot.usage.clips_bytes, expected_bytes);
        assert_eq!(
            Sha256::digest(fs::read(root.join("games/1786000000/video.mp4")).unwrap()),
            Sha256::digest(&original_source)
        );
        assert_eq!(fs::read(&preexisting).unwrap(), original_source);
        assert_eq!(
            fs::read(&preexisting_thumbnail).unwrap(),
            b"preexisting thumbnail sentinel"
        );
        assert_eq!(
            fs::read_dir(root.join("clips/.chronobreak-export"))
                .unwrap()
                .count(),
            1
        );
        assert_eq!(
            crate::export_staging::recover(&approved, RecoveryMode::Normal, || Ok(()))
                .unwrap()
                .recovered,
            0
        );
        println!(
            "hard-kill {label}: {expected_new} validated published members preserved, staging reclaimed, source/preexisting bytes unchanged"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires packaged runtime; generated temporary media only"]
async fn packaged_export_collision_and_benchmark_isolation() {
    let runtime = std::env::var_os("QUEUEBACK_TEST_MEDIA_RUNTIME").unwrap();
    let tools = queueback_media_runtime::resolve_root(Path::new(&runtime))
        .await
        .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (source, timeline) = write_source(root, &tools, false).await;
    let original = Sha256::digest(fs::read(&source).unwrap());
    fs::write(
        root.join(".crash-fixture"),
        b"dedicated temporary export fixture",
    )
    .unwrap();
    let marker = root.join("crash-gate.json");
    let mut child = spawn_child(root, &marker, "before-publish", true);
    wait_gate(&mut child, &marker).await;
    let staging = root.join("clips/.chronobreak-export");
    let batch = fs::read_dir(&staging)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.is_dir())
        .unwrap();
    let owner: serde_json::Value =
        serde_json::from_slice(&fs::read(batch.join("owner.json")).unwrap()).unwrap();
    // First MP4 succeeds; its JPEG and the next MP4 race with existing user files.
    let first_jpg = root.join("clips").join(format!(
        "{}.jpg",
        owner["destinations"][0].as_str().unwrap()
    ));
    let second_mp4 = root.join("clips").join(format!(
        "{}.mp4",
        owner["destinations"][1].as_str().unwrap()
    ));
    fs::write(&first_jpg, b"user thumbnail at publication boundary").unwrap();
    fs::write(&second_mp4, b"user clip at publication boundary").unwrap();
    fs::write(marker.with_extension("release"), b"resume").unwrap();
    let started = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(15));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        fs::read(first_jpg).unwrap(),
        b"user thumbnail at publication boundary"
    );
    assert_eq!(
        fs::read(second_mp4).unwrap(),
        b"user clip at publication boundary"
    );
    assert_eq!(Sha256::digest(fs::read(&source).unwrap()), original);
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 1);

    // An explicit benchmark export can write its new batch but cannot clean a
    // previous interrupted batch; background benchmark refresh does no I/O.
    let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let old = staging.join(id);
    fs::create_dir(&old).unwrap();
    fs::write(
        old.join("owner.json"),
        serde_json::to_vec(&json!({"version":1,"batch_id":id,"destinations":["1786000000_2"]}))
            .unwrap(),
    )
    .unwrap();
    fs::write(old.join("0.mp4"), b"old staged sentinel").unwrap();
    let approved = ApprovedRoot::new(root).unwrap();
    let mut request = export_request(&timeline, ClipMusicSource::None);
    request.presets = vec![ClipExportPreset::Horizontal];
    export(
        ExportDestination {
            root: &approved,
            recovery_mode: RecoveryMode::ReadOnly,
        },
        &root.join("music"),
        tools.ffmpeg(),
        tools.ffprobe(),
        request,
        Channel::new(|_| Ok(())),
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(fs::read(old.join("0.mp4")).unwrap(), b"old staged sentinel");
    assert_eq!(
        crate::export_staging::recover(&approved, RecoveryMode::Normal, || Ok(()))
            .unwrap()
            .recovered,
        1
    );
}
