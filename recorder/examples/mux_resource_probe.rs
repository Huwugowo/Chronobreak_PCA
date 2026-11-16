//! Paced prerecorded access units through the production mux, without capture hardware.

#[cfg(all(windows, feature = "replay-time-fixture"))]
mod probe {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, ensure};
    use chronobreak_replay_time::{
        AudioExpectation, FinalizationExpectationsV2, FrameBoundary, MediaId, ProducerEvidenceV2,
        Rational,
    };
    use league_replay_recorder::encoder::AudioSource;
    use league_replay_recorder::native::{
        MuxResourceAudioFixture, NativeMuxPlan, NativeMuxProcess,
    };
    use serde::Serialize;

    #[derive(Clone, Copy, Default, Serialize)]
    struct FrameSample {
        frame: usize,
        elapsed_ns: u64,
        late_ns: u64,
        write_ns: u64,
    }

    #[derive(Clone, Copy, Default, Serialize)]
    struct ProgressSample {
        elapsed_ns: u64,
        frames: u64,
        bytes: u64,
        observation_ns: u64,
    }

    fn nanos(duration: Duration) -> u64 {
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
    }

    fn sleep_until(deadline: Instant) {
        if let Some(delay) = deadline.checked_duration_since(Instant::now()) {
            thread::sleep(delay);
        }
    }

    pub fn run() -> Result<()> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        ensure!(
            args.len() == 6,
            "expected runtime-root, h264, packet-sizes.json, output-directory, seconds, audio-mode"
        );
        let runtime_root = PathBuf::from(&args[0]);
        let source = PathBuf::from(&args[1]);
        let sizes_path = PathBuf::from(&args[2]);
        let directory = PathBuf::from(&args[3]).canonicalize()?;
        let seconds: usize = args[4].to_str().context("seconds must be UTF-8")?.parse()?;
        ensure!(
            (10..=600).contains(&seconds),
            "duration must be 10-600 seconds"
        );
        let audio = match args[5].to_str() {
            Some("video") => AudioSource::Unavailable("mux resource fixture".into()),
            Some("silence") => AudioSource::Silent,
            Some("pcm") => AudioSource::NativePcm("fixture".into()),
            _ => anyhow::bail!("audio mode must be video, silence or pcm"),
        };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("repository root")?;
        let sentinel = root.join("build/perf/qb-perf-006").canonicalize()?;
        ensure!(
            directory != sentinel && directory.starts_with(&sentinel),
            "output must be in a unique directory under build/perf/qb-perf-006"
        );
        let output = directory.join("video.partial.mp4");
        let trace = directory.join("writer.json");
        ensure!(
            !output.exists() && !trace.exists(),
            "refusing to replace probe output"
        );
        let data = fs::read(&source)?;
        ensure!(data.len() <= 64 * 1024 * 1024, "source exceeds 64 MiB");
        let sizes: Vec<usize> = serde_json::from_slice(&fs::read(sizes_path)?)?;
        ensure!(
            !sizes.is_empty() && sizes.len() <= 600,
            "expected 1-600 prerecorded access units"
        );
        let mut offset = 0_usize;
        let mut packets = Vec::with_capacity(sizes.len());
        for size in sizes {
            let end = offset.checked_add(size).context("packet offset overflow")?;
            ensure!(size > 0 && end <= data.len(), "invalid packet extent");
            packets.push(&data[offset..end]);
            offset = end;
        }
        ensure!(offset == data.len(), "unaccounted source bytes");

        let tools = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(queueback_media_runtime::resolve_root(&runtime_root))?;
        let frame_count = seconds * 60;
        // Commit measurement storage before the resource baseline, with a fixed bound.
        let mut frames = vec![FrameSample::default(); frame_count];
        let mut progress = vec![ProgressSample::default(); seconds * 4];
        let plan = NativeMuxPlan::h264(60, &audio, &output)?;
        let start = Instant::now()
            + if matches!(audio, AudioSource::NativePcm(_)) {
                Duration::ZERO
            } else {
                Duration::from_secs(2)
            };
        let (mut native_audio, mut mux, plan) = if matches!(audio, AudioSource::NativePcm(_)) {
            let (audio, mux, plan) =
                MuxResourceAudioFixture::start(tools.ffmpeg(), &output, start)?;
            (Some(audio), mux, plan)
        } else {
            (None, NativeMuxProcess::start(tools.ffmpeg(), &plan)?, plan)
        };
        let mut writer = mux.take_video_writer()?;
        println!("READY {}", std::process::id());
        std::io::stdout().flush()?;
        sleep_until(start);
        println!("ACTIVE");
        std::io::stdout().flush()?;
        if let Some(audio) = &mut native_audio
            && !audio.await_ready(start + Duration::from_secs(15))
        {
            audio.stop(0);
            let summary = audio.finish();
            drop(writer);
            anyhow::bail!("PCM bootstrap failed: {summary:?}; mux={:?}", mux.finish());
        }
        let preparation_ns = nanos(start.elapsed());
        // Preserve the production common epoch and bootstrap catch-up. Moving
        // video zero past the PCM bootstrap changes audio lead/queue behavior.
        // The analyzer reports startup separately and uses >=6 s for steady data.
        for (index, sample) in frames.iter_mut().enumerate() {
            let deadline = start + Duration::from_nanos(index as u64 * 1_000_000_000 / 60);
            sleep_until(deadline);
            let before = Instant::now();
            writer.write_all(packets[index % packets.len()])?;
            let after = Instant::now();
            *sample = FrameSample {
                frame: index,
                elapsed_ns: nanos(before.duration_since(start)),
                late_ns: nanos(before.saturating_duration_since(deadline)),
                write_ns: nanos(after.duration_since(before)),
            };
            if index % 15 == 0 {
                let before = Instant::now();
                let snapshot = mux.telemetry();
                progress[index / 15] = ProgressSample {
                    elapsed_ns: nanos(before.duration_since(start)),
                    frames: snapshot.encoded_frames,
                    bytes: snapshot.muxed_bytes,
                    observation_ns: nanos(before.elapsed()),
                };
                ensure!(snapshot.reader_errors == 0, "mux reader protocol error");
            }
        }
        sleep_until(start + Duration::from_secs(seconds as u64));
        println!("DRAIN");
        std::io::stdout().flush()?;
        let drain_started = Instant::now();
        if let Some(audio) = &native_audio {
            audio.stop(frame_count as u64);
        }
        writer.flush()?;
        drop(writer);
        let terminal = mux.finish()?;
        let drain_ns = nanos(drain_started.elapsed());
        let audio_summary = native_audio.as_mut().map(MuxResourceAudioFixture::finish);
        ensure!(
            terminal.encoded_frames == frame_count as u64,
            "mux frame count differs"
        );
        let rate = Rational::new(60, 1)?;
        let count = FrameBoundary::new(frame_count as u64)?;
        let validated = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(league_replay_recorder::validate_finalizer_fixture(
                tools.ffprobe(),
                directory.clone(),
                output,
                rate,
                count,
                FinalizationExpectationsV2 {
                    media_id: MediaId::new_v4(),
                    expected_video_codec: "h264".into(),
                    expected_audio: if matches!(audio, AudioSource::Unavailable(_)) {
                        AudioExpectation::Absent
                    } else {
                        AudioExpectation::RequiredCodec("aac".into())
                    },
                    producer: ProducerEvidenceV2 {
                        backend: "native-mux-resource-fixture".into(),
                        expected_frame_rate: rate,
                        expected_frame_count: count,
                        media_runtime_id: tools.runtime_id().into(),
                    },
                    capture: None,
                    require_zero_video_start: true,
                },
            ))?;
        let result = serde_json::json!({
            "runtime_id": tools.runtime_id(),
            "arguments": plan.arguments().iter().map(|x| x.to_string_lossy()).collect::<Vec<_>>(),
            "source_bytes": data.len(), "source_packets": packets.len(),
            "frames": frames, "progress": progress, "drain_ns": drain_ns,
            "preparation_ns": preparation_ns,
            "validated_timeline": validated.timeline,
            "audio_summary": audio_summary,
            "terminal": {
                "frames": terminal.encoded_frames, "bytes": terminal.output_file_bytes,
                "writer_calls": terminal.video_writer_calls,
                "writer_duration_100ns": terminal.video_writer_duration_100ns,
                "writer_max_100ns": terminal.maximum_video_writer_duration_100ns,
                "slow_writes": terminal.slow_video_writer_calls,
                "flush_calls": terminal.explicit_flush_calls,
                "flush_duration_100ns": terminal.explicit_flush_duration_100ns,
                "reader_errors": terminal.reader_errors,
                "progress_end": terminal.progress_end,
            },
        });
        serde_json::to_writer(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(trace)?,
            &result,
        )?;
        println!(
            "PASS frames={frame_count} bytes={}",
            terminal.output_file_bytes
        );
        Ok(())
    }
}

#[cfg(all(windows, feature = "replay-time-fixture"))]
fn main() -> anyhow::Result<()> {
    probe::run()
}

#[cfg(not(all(windows, feature = "replay-time-fixture")))]
fn main() {
    eprintln!("mux_resource_probe requires Windows and --features replay-time-fixture");
}
