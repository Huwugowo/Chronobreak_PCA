//! Opt-in real media checks; every input and output belongs to a temporary library.

use super::*;
use queueback_media_runtime::MediaTools;
use serde_json::json;
use sha2::{Digest, Sha256};

async fn fixture_command(executable: &Path, arguments: &[&str], output: Option<&Path>) -> Vec<u8> {
    let mut command = Command::new(executable);
    command.args(arguments);
    if let Some(output) = output {
        command.arg(output);
    }
    hide_console(&mut command);
    let result = run_bounded_process(command, Duration::from_secs(60), 2 * 1024 * 1024)
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

async fn write_source(root: &Path, tools: &MediaTools, audio: bool) -> (PathBuf, MediaTimelineV2) {
    let game = root.join("games/1786000000");
    fs::create_dir_all(&game).unwrap();
    let source = game.join("video.mp4");
    let mut arguments = vec![
        "-v",
        "error",
        "-nostdin",
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x180:rate=30:duration=6",
    ];
    if audio {
        arguments.extend([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:sample_rate=48000:duration=6",
        ]);
    }
    arguments.extend([
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
    ]);
    if audio {
        arguments.extend(["-c:a", "aac"]);
    }
    fixture_command(tools.ffmpeg(), &arguments, Some(&source)).await;
    let summary = fixture_command(
        tools.ffprobe(),
        &[
            "-v",
            "error",
            "-show_entries",
            FFPROBE_SUMMARY_SHOW_ENTRIES,
            "-of",
            "json",
        ],
        Some(&source),
    )
    .await;
    let timeline = parse_ffprobe_media_timeline(
        std::str::from_utf8(&summary).unwrap(),
        FinalizationExpectationsV2 {
            media_id: MediaId::parse("123e4567-e89b-42d3-a456-426614174000").unwrap(),
            expected_video_codec: "h264".to_owned(),
            expected_audio_codec: audio.then(|| "aac".to_owned()),
            producer: ProducerEvidenceV2 {
                backend: "generated-export-fixture".to_owned(),
                expected_frame_rate: Rational::positive(30, 1, "fixture rate").unwrap(),
                expected_frame_count: FrameBoundary::new(180).unwrap(),
                media_runtime_id: tools.runtime_id().to_owned(),
            },
            capture: None,
            require_zero_video_start: true,
        },
    )
    .unwrap();
    assert_eq!(timeline.audio.present, audio);
    fs::write(
        game.join("metadata.json"),
        serde_json::to_vec(&json!({
            "schema_version": 2,
            "media_id": timeline.media_id,
            "media_timeline": timeline,
            "recorded_at": "2026-10-26T00:00:00Z",
            "encoder_used": "software",
            "recording_codec": "h264",
            "recording_profile": "high",
            "recording_resolution": "320x180",
            "capture_backend": "generated-export-fixture",
            "media_runtime_id": tools.runtime_id(),
            "capture_support_label": "fixture",
            "source_frames_surfaced": 0,
            "source_frames_superseded": 0,
            "cfr_duplicates": 0,
            "cfr_discards": 0,
            "pool_recreations": 0,
            "saved": false
        }))
        .unwrap(),
    )
    .unwrap();
    (source, timeline)
}

fn export_request(timeline: &MediaTimelineV2, music: ClipMusicSource) -> ClipExportRequest {
    ClipExportRequest {
        game_timestamp: "1786000000".to_owned(),
        media_id: timeline.media_id.clone(),
        start_frame: FrameBoundary::new(30).unwrap(),
        end_frame_exclusive: FrameBoundary::new(180).unwrap(),
        presets: vec![
            ClipExportPreset::Discord,
            ClipExportPreset::Horizontal,
            ClipExportPreset::Vertical,
        ],
        vertical_focus: 0.72,
        vertical_position: 0.5,
        music,
        game_audio_volume: 0.25,
        music_volume: 0.75,
    }
}

async fn audio_peak(tools: &MediaTools, media: &Path) -> f32 {
    let samples = fixture_command(
        tools.ffmpeg(),
        &[
            "-v",
            "error",
            "-nostdin",
            "-i",
            media.to_str().unwrap(),
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "f32le",
            "pipe:1",
        ],
        None,
    )
    .await;
    assert!(samples.len() >= 5 * 48_000 * 4);
    samples
        .chunks_exact(4)
        .map(|bytes| {
            let sample = f32::from_le_bytes(bytes.try_into().unwrap());
            assert!(sample.is_finite());
            sample.abs()
        })
        .fold(0.0, f32::max)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires staged runtime via QUEUEBACK_TEST_MEDIA_RUNTIME"]
async fn silent_source_packaged_export_matrix() {
    let runtime =
        std::env::var_os("QUEUEBACK_TEST_MEDIA_RUNTIME").expect("set staged runtime root");
    let tools = queueback_media_runtime::resolve_root(Path::new(&runtime))
        .await
        .unwrap();
    let library = tempfile::tempdir().unwrap();
    let music_directory = music::install(library.path()).unwrap();
    let imported = library.path().join("imported.wav");
    fixture_command(
        tools.ffmpeg(),
        &[
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-c:a",
            "pcm_s16le",
        ],
        Some(&imported),
    )
    .await;

    for source_audio in [false, true] {
        let (source, timeline) = write_source(library.path(), &tools, source_audio).await;
        let original = Sha256::digest(fs::read(&source).unwrap());
        for music in [
            ClipMusicSource::None,
            ClipMusicSource::Builtin {
                filename: "momentum.mp3".to_owned(),
            },
            ClipMusicSource::File {
                path: imported.to_str().unwrap().to_owned(),
            },
        ] {
            let expect_sound = source_audio || music != ClipMusicSource::None;
            let request = export_request(&timeline, music);
            let timing = validate_for_metadata(&request, &timeline).unwrap();
            let result = export(
                library.path(),
                &music_directory,
                tools.ffmpeg(),
                tools.ffprobe(),
                request.clone(),
                Channel::new(|_| Ok(())),
                &Cancellation::default(),
            )
            .await
            .unwrap();
            assert_eq!(result.outputs.len(), 3);
            for output in result.outputs {
                let media = Path::new(&output.output_path);
                let profile = export_profile(&request, &timing, output.preset, None).unwrap();
                let validation =
                    validate_exported_media(tools.ffprobe(), media, &request, &timing, &profile)
                        .await
                        .unwrap();
                assert_eq!(validation.frame_count.get(), 150);
                assert_eq!(validation.video_replay_end, 5 * REPLAY_TICKS_PER_SECOND);
                assert!(Path::new(&output.thumbnail_path).is_file());
                if output.preset == ClipExportPreset::Discord {
                    assert!(output.file_size_bytes < DISCORD_LIMIT_BYTES);
                }
                fixture_command(
                    tools.ffmpeg(),
                    &[
                        "-v",
                        "error",
                        "-xerror",
                        "-nostdin",
                        "-threads",
                        "1",
                        "-i",
                        media.to_str().unwrap(),
                        "-f",
                        "null",
                        "NUL",
                    ],
                    None,
                )
                .await;
                let peak = audio_peak(&tools, media).await;
                if expect_sound {
                    assert!(peak > 0.001, "selected audio was lost: {output:?}");
                } else {
                    assert_eq!(peak, 0.0, "silent source gained sound");
                }
                println!(
                    "source_audio={source_audio} music={:?} preset={:?}: 150 frames, H.264/AAC, full decode, audio peak={peak}",
                    request.music, output.preset
                );
            }
            assert_eq!(Sha256::digest(fs::read(&source).unwrap()), original);
        }
    }

    // Inconsistent metadata must not make an invalid source publishable.
    let (source, timeline) = write_source(library.path(), &tools, false).await;
    let mut request = export_request(&timeline, ClipMusicSource::None);
    request.presets = vec![ClipExportPreset::Horizontal];
    let published_before = fs::read_dir(library.path().join("clips")).unwrap().count();
    fs::write(&source, b"invalid dedicated test media").unwrap();
    let invalid_bytes = fs::read(&source).unwrap();
    assert!(
        export(
            library.path(),
            &music_directory,
            tools.ffmpeg(),
            tools.ffprobe(),
            request,
            Channel::new(|_| Ok(())),
            &Cancellation::default()
        )
        .await
        .is_err()
    );
    assert_eq!(fs::read(&source).unwrap(), invalid_bytes);
    assert_eq!(
        fs::read_dir(library.path().join("clips")).unwrap().count(),
        published_before
    );

    // Dropping the real owned waiter while encoding cancels the child and waits
    // for the blocking worker's own cleanup before inspecting the library.
    let (source, timeline) = write_source(library.path(), &tools, false).await;
    let original = Sha256::digest(fs::read(&source).unwrap());
    let root = library.path().to_owned();
    let ffmpeg = tools.ffmpeg().to_owned();
    let ffprobe = tools.ffprobe().to_owned();
    let request = export_request(&timeline, ClipMusicSource::None);
    let (progress_tx, progress_rx) = tokio::sync::oneshot::channel();
    let progress_tx = std::sync::Mutex::new(Some(progress_tx));
    let progress = Channel::new(move |_| {
        if let Some(sender) = progress_tx.lock().unwrap().take() {
            sender.send(()).ok();
        }
        Ok(())
    });
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let handle = tokio::runtime::Handle::current();
    let waiter = tokio::spawn(export_process::run_owned(move |cancellation| {
        let result = handle.block_on(export(
            &root,
            &music_directory,
            &ffmpeg,
            &ffprobe,
            request,
            progress,
            &cancellation,
        ));
        finished_tx
            .send(result.map(|_| ()).map_err(|error| error.to_string()))
            .ok();
    }));
    tokio::time::timeout(Duration::from_secs(60), progress_rx)
        .await
        .unwrap()
        .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    let result = tokio::time::timeout(Duration::from_secs(15), finished_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(result.unwrap_err().contains("export cancelled"));
    assert_eq!(Sha256::digest(fs::read(source).unwrap()), original);
    assert_eq!(
        fs::read_dir(library.path().join("clips")).unwrap().count(),
        published_before
    );
    println!(
        "invalid-source and in-flight cancellation: no staged/published additions; source preserved"
    );
}
