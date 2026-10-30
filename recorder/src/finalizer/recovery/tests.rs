use super::*;
use crate::finalizer::{CompletedVideoCandidate, publish_validated_candidate, validate_candidate};
use crate::poller::{PollerSummary, RecordingDetails};
use chronobreak_replay_time::{FrameBoundary, Rational};
use std::process::{Command, Stdio};

fn inspect(directory: &Path) -> Result<Inspection> {
    inspect_when_idle(directory, &mut || true)
}

async fn reconcile(
    directory: &Path,
    ffprobe: &Path,
    probes: &mut usize,
    deadline: Instant,
) -> Result<(Disposition, &'static str)> {
    reconcile_when_idle(directory, ffprobe, probes, deadline, &mut || true).await
}

fn metadata(timeline: chronobreak_replay_time::MediaTimelineV2) -> RecordingMetadata {
    let details = RecordingDetails {
        codec: timeline.video.codec.clone(),
        capture_backend: timeline.producer.backend.clone(),
        media_runtime_id: timeline.producer.media_runtime_id.clone(),
        ..Default::default()
    };
    RecordingMetadata::new(
        "2026-10-30T00:00:00Z".to_owned(),
        timeline,
        PollerSummary::default(),
        details,
    )
}

fn stage(directory: &Path, private: bool) {
    std::fs::create_dir_all(directory).unwrap();
    let path = directory.join(if private {
        VIDEO_PARTIAL_MP4
    } else {
        VIDEO_MP4
    });
    std::fs::write(&path, b"generated non-media fixture").unwrap();
    let metadata = metadata(crate::finalizer::tests::timeline());
    let receipt = Receipt::capture(
        &DirectoryPins::open(directory).unwrap(),
        &publication::open_file(&path, false).unwrap(),
        &metadata.media_id,
    )
    .unwrap();
    let mut value = serde_json::to_value(metadata).unwrap();
    value["publication"] = serde_json::to_value(receipt).unwrap();
    std::fs::write(
        directory.join(METADATA_PENDING_JSON),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
}

fn tree(directory: &Path) -> Vec<(std::ffi::OsString, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), std::fs::read(entry.path()).unwrap())
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[test]
fn publication_states_require_receipt_and_preserve_conflicts() {
    for private in [true, false] {
        let root = tempfile::tempdir().unwrap();
        stage(root.path(), private);
        assert!(matches!(
            inspect(root.path()).unwrap(),
            Inspection::Candidate(_)
        ));
        let original = tree(root.path());
        std::fs::write(root.path().join(METADATA_JSON), b"foreign canonical").unwrap();
        assert!(matches!(
            inspect(root.path()).unwrap(),
            Inspection::Untouched(Disposition::CanonicalUntouched, _)
        ));
        let pins = DirectoryPins::open(root.path()).unwrap();
        let source =
            publication::open_file(&root.path().join(METADATA_PENDING_JSON), true).unwrap();
        assert!(pins.rename(&source, METADATA_JSON).is_err());
        assert_eq!(
            std::fs::read(root.path().join(METADATA_JSON)).unwrap(),
            b"foreign canonical"
        );
        drop(source);
        drop(pins);
        std::fs::remove_file(root.path().join(METADATA_JSON)).unwrap();
        assert_eq!(tree(root.path()), original);
        let other = if private {
            VIDEO_MP4
        } else {
            VIDEO_PARTIAL_MP4
        };
        std::fs::write(root.path().join(other), b"foreign media").unwrap();
        assert!(matches!(
            inspect(root.path()).unwrap(),
            Inspection::Untouched(Disposition::Ambiguous, _)
        ));
    }
}

#[test]
fn missing_malformed_and_mismatched_evidence_never_promotes_bytes() {
    let root = tempfile::tempdir().unwrap();
    stage(root.path(), true);
    let pending = root.path().join(METADATA_PENDING_JSON);
    let original = std::fs::read(&pending).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    value["foreign_field"] = serde_json::json!(1);
    std::fs::write(&pending, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        inspect(root.path()).unwrap(),
        Inspection::Untouched(Disposition::Ambiguous, _)
    ));
    value.as_object_mut().unwrap().remove("foreign_field");
    value["media_id"] = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee".into();
    std::fs::write(&pending, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        inspect(root.path()).unwrap(),
        Inspection::Untouched(Disposition::Ambiguous, _)
    ));
    std::fs::write(&pending, &original).unwrap();
    std::fs::write(root.path().join(VIDEO_PARTIAL_MP4), b"replaced media").unwrap();
    assert!(matches!(
        inspect(root.path()).unwrap(),
        Inspection::Untouched(Disposition::Ambiguous, _)
    ));
    for bytes in [
        b"{torn".to_vec(),
        vec![b' '; publication::MAX_METADATA_BYTES + 1],
    ] {
        std::fs::write(&pending, bytes).unwrap();
        assert!(!matches!(
            inspect(root.path()),
            Ok(Inspection::Candidate(_))
        ));
    }
    std::fs::remove_file(&pending).unwrap();
    assert!(matches!(
        inspect(root.path()).unwrap(),
        Inspection::Untouched(Disposition::MissingEvidence, _)
    ));
    assert!(root.path().join(VIDEO_PARTIAL_MP4).exists());
    assert!(!root.path().join(METADATA_JSON).exists());
}

#[tokio::test]
async fn probe_and_admission_budgets_leave_candidates_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let game = root.path().join("games/42");
    stage(&game, false);
    let original = tree(&game);
    let mut probes = MAX_PROBES;
    assert_eq!(
        reconcile(
            &game,
            Path::new("must-not-spawn"),
            &mut probes,
            Instant::now() + ADMISSION_BUDGET
        )
        .await
        .unwrap()
        .0,
        Disposition::Deferred
    );
    let mut probes = 0;
    assert_eq!(
        reconcile(
            &game,
            Path::new("must-not-spawn"),
            &mut probes,
            Instant::now()
        )
        .await
        .unwrap()
        .0,
        Disposition::Deferred
    );
    assert_eq!(probes, 0);
    let absent = root.path().join("must-not-be-opened-or-created");
    let report = run(&absent, Path::new("must-not-spawn"), || false)
        .await
        .unwrap();
    assert_eq!(report.inspected, 0);
    assert_eq!(report.probes, 0);
    assert!(!absent.exists());
    // A fresh game observation after admission also refuses an actual media probe.
    let mut probes = 0;
    assert_eq!(
        reconcile_when_idle(
            &game,
            Path::new("must-not-spawn"),
            &mut probes,
            Instant::now() + ADMISSION_BUDGET,
            &mut || false
        )
        .await
        .unwrap()
        .0,
        Disposition::Deferred
    );
    assert_eq!(probes, 0);
    assert_eq!(tree(&game), original);
}

#[tokio::test]
async fn league_appearance_stops_before_discovery_and_does_not_resume_or_save() {
    for appears_at in [2, 5] {
        let root = tempfile::tempdir().unwrap();
        let games = root.path().join("games");
        std::fs::create_dir(&games).unwrap();
        stage(&games.join("42"), false);
        let pins = DirectoryPins::open(&games).unwrap();
        let mut cursor = discovery::Cursor::open(&games, &pins).unwrap();
        cursor.save("42").unwrap();
        drop(cursor);
        let original = std::fs::read(games.join(discovery::CURSOR)).unwrap();
        let mut calls = 0;
        let report = run(root.path(), Path::new("must-not-spawn"), || {
            calls += 1;
            calls != appears_at // Seen before inspection, or after the exact-prefix probe.
        })
        .await
        .unwrap();
        assert_eq!(report.searches, 0);
        assert_eq!(report.inspected, usize::from(appears_at == 5));
        assert_eq!(report.probes, usize::from(appears_at == 5));
        assert_eq!(calls, appears_at);
        assert_eq!(
            std::fs::read(games.join(discovery::CURSOR)).unwrap(),
            original
        );
    }
}

#[test]
fn reparse_and_copied_bundle_are_preserved() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("42");
    let second = root.path().join("43");
    stage(&first, false);
    std::fs::create_dir(&second).unwrap();
    for entry in std::fs::read_dir(&first).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), second.join(entry.file_name())).unwrap();
    }
    assert!(matches!(
        inspect(&second).unwrap(),
        Inspection::Untouched(Disposition::Ambiguous, _)
    ));
    let link = root.path().join("44");
    let result = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&first)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(inspect(&link).is_err());
    // A new game stops even candidate ancestry inspection, which would reject
    // this generated junction if it ran.
    assert!(matches!(
        inspect_when_idle(&link, &mut || false).unwrap(),
        Inspection::Untouched(Disposition::Deferred, _)
    ));
    std::fs::write(
        first.join(METADATA_JSON),
        b"canonical, deliberately not parsed",
    )
    .unwrap();
    // Pinning this junction would fail. Existing canonical metadata is only a
    // cheap reason to decline recovery, so no ancestry handles are acquired.
    assert!(matches!(
        inspect_when_idle(&link, &mut || panic!(
            "canonical skip must not query processes"
        ))
        .unwrap(),
        Inspection::Untouched(Disposition::CanonicalUntouched, _)
    ));
    // Remove only this generated junction, never recurse into its destination.
    std::fs::remove_dir(&link).unwrap();
    assert!(first.join(VIDEO_MP4).exists());
}

#[tokio::test]
async fn bounded_discovery_advances_past_large_prefix_and_invalid_probes_across_reopens() {
    let root = tempfile::tempdir().unwrap();
    let games = root.path().join("games");
    std::fs::create_dir(&games).unwrap();
    for suffix in 1..=600 {
        let game = games.join(format!("1000000000-{suffix}"));
        std::fs::create_dir(&game).unwrap();
        std::fs::write(game.join(METADATA_JSON), b"canonical").unwrap();
    }
    let names = ["1000000000".to_owned()]
        .into_iter()
        .chain((700..707).map(|suffix| format!("1000000000-{suffix}")))
        .collect::<Vec<_>>();
    for name in &names {
        stage(&games.join(name), false);
    }
    let original = names
        .iter()
        .map(|name| tree(&games.join(name)))
        .collect::<Vec<_>>();
    let mut attempted = std::collections::BTreeSet::new();
    let started = Instant::now();
    let mut passes = 0;
    let mut total_entries = 0;
    let mut total_probes = 0;
    // Each invocation opens a new on-disk cursor; no process-global state helps.
    while attempted.len() < names.len() && passes < 64 {
        let report = if passes == 0 {
            let mut watcher =
                crate::watcher::ProcessWatcher::new("QueueBack.absent-test-process.exe");
            let measured = Instant::now();
            let report = run(
                root.path(),
                Path::new("no-generated-media-probe-executable"),
                || watcher.refresh().is_none(),
            )
            .await
            .unwrap();
            println!(
                "first maintenance pass with real process queries: {:?}, queries={}, entries={}",
                measured.elapsed(),
                watcher.telemetry().refreshes,
                report.inspected
            );
            report
        } else {
            run(
                root.path(),
                Path::new("no-generated-media-probe-executable"),
                || true,
            )
            .await
            .unwrap()
        };
        passes += 1;
        assert!(report.inspected <= MAX_ENTRIES);
        assert!(report.searches <= MAX_SEARCHES);
        assert!(report.probes <= MAX_PROBES);
        total_entries += report.inspected;
        total_probes += report.probes;
        for path in report.attempted {
            attempted.insert(path.file_name().unwrap().to_str().unwrap().to_owned());
        }
    }
    assert_eq!(attempted, names.iter().cloned().collect());
    for (name, original) in names.iter().zip(original) {
        assert_eq!(tree(&games.join(name)), original);
    }
    println!(
        "restart discovery: {passes} passes, {total_entries} entries, {total_probes} failed probe admissions, elapsed={:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn cursor_conflicts_and_copied_generations_are_preserved() {
    let root = tempfile::tempdir().unwrap();
    let games = root.path().join("games");
    std::fs::create_dir(&games).unwrap();
    let pins = DirectoryPins::open(&games).unwrap();
    let path = games.join(discovery::CURSOR);
    let foreign = b"foreign cursor bytes";
    std::fs::write(&path, foreign).unwrap();
    assert!(discovery::Cursor::open(&games, &pins).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), foreign);
    // A damaged/foreign scheduling hint cannot prohibit otherwise safe media
    // inspection. It is left untouched; cross-exit progress is unavailable.
    stage(&games.join("42"), false);
    let report = run(root.path(), Path::new("must-not-exist"), || true)
        .await
        .unwrap();
    assert_eq!(report.probes, 1);
    assert_eq!(std::fs::read(&path).unwrap(), foreign);
    std::fs::remove_file(&path).unwrap(); // This generated foreign fixture only.
    let mut cursor = discovery::Cursor::open(&games, &pins).unwrap();
    cursor.save("1000000000-7").unwrap();
    drop(cursor);
    assert_eq!(
        discovery::Cursor::open(&games, &pins).unwrap().prefix,
        "1000000000-7"
    );
    let other = root.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let copied = other.join(discovery::CURSOR);
    std::fs::copy(&path, &copied).unwrap();
    let bytes = std::fs::read(&copied).unwrap();
    assert!(discovery::Cursor::open(&other, &DirectoryPins::open(&other).unwrap()).is_err());
    assert_eq!(std::fs::read(copied).unwrap(), bytes);
    let mut bytes = std::fs::read(&path).unwrap();
    *bytes.last_mut().unwrap() = b'/';
    std::fs::write(&path, &bytes).unwrap();
    assert!(discovery::Cursor::open(&games, &pins).is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

pub(crate) fn publication_checkpoint(stage: &str) {
    if std::env::var("QUEUEBACK_RECOVERY_CHILD_STAGE")
        .ok()
        .as_deref()
        == Some(stage)
    {
        let marker = PathBuf::from(std::env::var_os("QUEUEBACK_RECOVERY_CHILD_MARKER").unwrap());
        std::fs::write(marker, stage).unwrap();
        // Parent kills/reaps the isolated test process at this exact transition.
        std::thread::sleep(Duration::from_secs(30));
        panic!("parent did not terminate publication fixture");
    }
}

#[tokio::test]
async fn publication_child() {
    let Some(directory) = std::env::var_os("QUEUEBACK_RECOVERY_CHILD_DIRECTORY") else {
        return;
    };
    let directory = PathBuf::from(directory);
    let probe = PathBuf::from(std::env::var_os("QUEUEBACK_RECOVERY_CHILD_PROBE").unwrap());
    let candidate = CompletedVideoCandidate::new(
        directory.clone(),
        directory.join(VIDEO_PARTIAL_MP4),
        Rational::new(60, 1).unwrap(),
        FrameBoundary::new(120).unwrap(),
    )
    .unwrap();
    let timeline = crate::finalizer::tests::timeline();
    let expectations = FinalizationExpectationsV2 {
        media_id: timeline.media_id,
        expected_video_codec: "h264".into(),
        expected_audio_codec: Some("aac".into()),
        producer: chronobreak_replay_time::ProducerEvidenceV2 {
            expected_frame_count: FrameBoundary::new(120).unwrap(),
            ..timeline.producer
        },
        capture: None,
        require_zero_video_start: true,
    };
    let validated = validate_candidate(&probe, candidate, expectations)
        .await
        .unwrap();
    let metadata = metadata(validated.timeline().clone());
    publish_validated_candidate(validated, &metadata.media_id, &metadata)
        .await
        .unwrap();
}

fn wait_marker(child: &mut std::process::Child, marker: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !marker.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "fixture exited before publication checkpoint"
        );
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("publication fixture timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
}

#[tokio::test]
#[ignore = "requires QUEUEBACK_TEST_MEDIA_RUNTIME; generated temporary media only"]
async fn packaged_publication_crashes_recover_once_and_seek() {
    let runtime = PathBuf::from(std::env::var_os("QUEUEBACK_TEST_MEDIA_RUNTIME").unwrap());
    let tools = queueback_media_runtime::resolve_root(&runtime)
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("generated.mp4");
    let output = Command::new(tools.ffmpeg())
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1920x1080:rate=60:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=2",
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-threads",
            "1",
            "-profile:v",
            "high",
            "-bf",
            "0",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "30",
            "-c:a",
            "aac",
            "-movflags",
            crate::encoder::FRAGMENTED_MP4_FLAGS,
            "-avoid_negative_ts",
            "disabled",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source_bytes = std::fs::read(&source).unwrap();
    for (index, stage) in ["pending", "video", "metadata"].into_iter().enumerate() {
        let game = root.path().join("games").join((42 + index).to_string());
        std::fs::create_dir_all(&game).unwrap();
        std::fs::copy(&source, game.join(VIDEO_PARTIAL_MP4)).unwrap();
        let marker = root.path().join(format!("{stage}.ready"));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "finalizer::recovery::tests::publication_child",
                "--nocapture",
            ])
            .env("QUEUEBACK_RECOVERY_CHILD_DIRECTORY", &game)
            .env("QUEUEBACK_RECOVERY_CHILD_PROBE", tools.ffprobe())
            .env("QUEUEBACK_RECOVERY_CHILD_STAGE", stage)
            .env("QUEUEBACK_RECOVERY_CHILD_MARKER", &marker)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        wait_marker(&mut child, &marker);
        let started = Instant::now();
        let mut probes = 0;
        let disposition = if stage == "pending" {
            // Actual valid footage must get past invalid candidates across exits,
            // rather than relying only on individual reconcile calls.
            let games = root.path().join("games");
            for suffix in 1..=6 {
                self::stage(&games.join(format!("40-{suffix}")), false);
            }
            let pins = DirectoryPins::open(&games).unwrap();
            let mut cursor = discovery::Cursor::open(&games, &pins).unwrap();
            cursor.save("40").unwrap();
            drop(cursor);
            drop(pins);
            let mut passes = 0;
            while !game.join(METADATA_JSON).exists() && passes < 16 {
                let report = run(root.path(), tools.ffprobe(), || true).await.unwrap();
                assert!(report.probes <= MAX_PROBES && report.inspected <= MAX_ENTRIES);
                probes += report.probes;
                passes += 1;
            }
            assert!(game.join(METADATA_JSON).exists());
            println!(
                "valid footage beyond invalid candidates recovered in {passes} reopened maintenance passes"
            );
            Disposition::Recovered
        } else {
            reconcile(
                &game,
                tools.ffprobe(),
                &mut probes,
                Instant::now() + ADMISSION_BUDGET,
            )
            .await
            .unwrap()
            .0
        };
        assert_eq!(
            disposition,
            if stage == "metadata" {
                Disposition::CanonicalUntouched
            } else {
                Disposition::Recovered
            }
        );
        assert_eq!(std::fs::read(game.join(VIDEO_MP4)).unwrap(), source_bytes);
        let original = tree(&game);
        let mut repeated_probes = 0;
        assert_eq!(
            reconcile(
                &game,
                tools.ffprobe(),
                &mut repeated_probes,
                Instant::now() + ADMISSION_BUDGET
            )
            .await
            .unwrap()
            .0,
            Disposition::CanonicalUntouched
        );
        assert_eq!(repeated_probes, 0);
        assert_eq!(tree(&game), original);
        assert_eq!(original.len(), 2);
        let decode = Command::new(tools.ffmpeg())
            .args(["-v", "error", "-xerror", "-threads", "1", "-i"])
            .arg(game.join(VIDEO_MP4))
            .args(["-f", "null", "NUL"])
            .output()
            .unwrap();
        assert!(
            decode.status.success(),
            "{}",
            String::from_utf8_lossy(&decode.stderr)
        );
        for seek in ["0.1", "1.0", "1.8"] {
            let seeked = Command::new(tools.ffmpeg())
                .args(["-v", "error", "-xerror", "-ss", seek, "-i"])
                .arg(game.join(VIDEO_MP4))
                .args(["-frames:v", "1", "-f", "null", "NUL"])
                .output()
                .unwrap();
            assert!(seeked.status.success());
        }
        println!(
            "{stage}: {disposition:?}, probes={probes}, repeated_probes={repeated_probes}, elapsed={:?}; full decode and 3 seeks passed",
            started.elapsed()
        );
    }
    // Simulate an abruptly terminated recording before graceful mux finalization.
    let partial_dir = root.path().join("games/45");
    std::fs::create_dir_all(&partial_dir).unwrap();
    let partial = partial_dir.join(VIDEO_PARTIAL_MP4);
    let mut mux = Command::new(tools.ffmpeg())
        .args([
            "-v",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=60",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "30",
            "-movflags",
            crate::encoder::FRAGMENTED_MP4_FLAGS,
            "-flush_packets",
            "1",
        ])
        .arg(&partial)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while std::fs::metadata(&partial).map_or(0, |m| m.len()) < 20_000 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    mux.kill().unwrap();
    mux.wait().unwrap();
    assert!(std::fs::metadata(&partial).unwrap().len() > 20_000);
    let original = tree(&partial_dir);
    let mut probes = 0;
    assert_eq!(
        reconcile(
            &partial_dir,
            tools.ffprobe(),
            &mut probes,
            Instant::now() + ADMISSION_BUDGET
        )
        .await
        .unwrap()
        .0,
        Disposition::MissingEvidence
    );
    assert_eq!(tree(&partial_dir), original);
    assert_eq!(probes, 0);
    if std::env::var_os("QUEUEBACK_RECOVERY_KEEP_FIXTURE").is_some() {
        std::fs::write(
            root.path().join("QUEUEBACK_GENERATED_RECOVERY_FIXTURE"),
            b"generated test media only",
        )
        .unwrap();
        println!("RECOVERY_FIXTURE={}", root.keep().display());
    }
}
