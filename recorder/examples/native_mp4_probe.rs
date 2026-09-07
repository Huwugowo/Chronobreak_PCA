#[cfg(target_os = "windows")]
mod windows_probe {
    use std::fs::{self, OpenOptions};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use anyhow::{Context, Result, anyhow, bail, ensure};
    use league_replay_recorder::encoder::AudioSource;
    use league_replay_recorder::native::NativeRecorderSession;
    use league_replay_recorder::platform::capture_target_for_process;

    pub fn run() -> Result<()> {
        let mut args = std::env::args().skip(1);
        let mut pid = None;
        let mut ffmpeg = None;
        let mut output = None;
        let mut duration_seconds = 10_u64;
        let mut fail_nvenc_after_ticks = None;
        let mut stall_worker_after_ticks = None;
        let mut stall_worker_ms = None;
        let mut stall_mux_after_writes = None;
        let mut stall_mux_ms = None;
        let mut fail_mux_after_writes = None;
        let mut fail_mux_after_fragment_after_writes = None;
        let mut fixture_pcm = None;
        let mut fixture_start_signal = None;
        let mut max_no_slot_admission_failures = 0_u64;
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--pid" => {
                    pid = Some(
                        args.next()
                            .context("--pid requires a value")?
                            .parse::<u32>()
                            .context("--pid must be an integer")?,
                    );
                }
                "--ffmpeg" => {
                    ffmpeg = Some(PathBuf::from(
                        args.next().context("--ffmpeg requires a path")?,
                    ));
                }
                "--output" => {
                    output = Some(PathBuf::from(
                        args.next().context("--output requires a path")?,
                    ));
                }
                "--duration-seconds" => {
                    duration_seconds = args
                        .next()
                        .context("--duration-seconds requires a value")?
                        .parse::<u64>()
                        .context("--duration-seconds must be an integer")?;
                }
                "--fail-nvenc-after-ticks" => {
                    fail_nvenc_after_ticks = Some(
                        args.next()
                            .context("--fail-nvenc-after-ticks requires a value")?
                            .parse::<u64>()
                            .context("--fail-nvenc-after-ticks must be an integer")?,
                    );
                }
                "--stall-worker-after-ticks" => {
                    stall_worker_after_ticks = Some(
                        args.next()
                            .context("--stall-worker-after-ticks requires a value")?
                            .parse::<u64>()
                            .context("--stall-worker-after-ticks must be an integer")?,
                    );
                }
                "--stall-worker-ms" => {
                    stall_worker_ms = Some(
                        args.next()
                            .context("--stall-worker-ms requires a value")?
                            .parse::<u64>()
                            .context("--stall-worker-ms must be an integer")?,
                    );
                }
                "--stall-mux-after-writes" => {
                    stall_mux_after_writes = Some(
                        args.next()
                            .context("--stall-mux-after-writes requires a value")?
                            .parse::<u64>()
                            .context("--stall-mux-after-writes must be an integer")?,
                    );
                }
                "--stall-mux-ms" => {
                    stall_mux_ms = Some(
                        args.next()
                            .context("--stall-mux-ms requires a value")?
                            .parse::<u64>()
                            .context("--stall-mux-ms must be an integer")?,
                    );
                }
                "--fail-mux-after-writes" => {
                    fail_mux_after_writes = Some(
                        args.next()
                            .context("--fail-mux-after-writes requires a value")?
                            .parse::<u64>()
                            .context("--fail-mux-after-writes must be an integer")?,
                    );
                }
                "--fail-mux-after-fragment-after-writes" => {
                    fail_mux_after_fragment_after_writes = Some(
                        args.next()
                            .context("--fail-mux-after-fragment-after-writes requires a value")?
                            .parse::<u64>()
                            .context("--fail-mux-after-fragment-after-writes must be an integer")?,
                    );
                }
                "--fixture-pcm" => {
                    fixture_pcm = Some(
                        args.next()
                            .context("--fixture-pcm requires a TCP endpoint")?,
                    );
                }
                "--fixture-start-signal" => {
                    fixture_start_signal = Some(PathBuf::from(
                        args.next()
                            .context("--fixture-start-signal requires a path")?,
                    ));
                }
                "--max-no-slot-admission-failures" | "--max-slot-tick-drops" => {
                    max_no_slot_admission_failures = args
                        .next()
                        .context("no-slot admission ceiling requires a value")?
                        .parse::<u64>()
                        .context("no-slot admission ceiling must be an integer")?;
                }
                other => bail!("unknown argument {other:?}"),
            }
        }
        let pid = pid.context("--pid is required")?;
        let ffmpeg = ffmpeg.context("--ffmpeg is required")?;
        let output = output.context("--output is required")?;
        ensure!(duration_seconds > 0, "duration must be positive");

        let target = capture_target_for_process(pid)
            .with_context(|| format!("could not resolve exact HWND for fixture PID {pid}"))?;
        #[cfg(feature = "replay-time-fixture")]
        let audio = fixture_pcm
            .as_ref()
            .map(|endpoint| AudioSource::ReplayTimeFixturePcm(endpoint.clone()))
            .unwrap_or(AudioSource::Silent);
        #[cfg(not(feature = "replay-time-fixture"))]
        let audio = {
            ensure!(
                fixture_pcm.is_none() && fixture_start_signal.is_none(),
                "fixture PCM and start signaling require the replay-time-fixture Cargo feature"
            );
            AudioSource::Silent
        };
        #[cfg(feature = "replay-time-fixture")]
        if let Some(signal) = fixture_start_signal.as_ref() {
            ensure!(
                fixture_pcm.is_some(),
                "--fixture-start-signal requires --fixture-pcm"
            );
            ensure!(
                !signal.exists(),
                "fixture start signal must not exist before the shared epoch"
            );
            println!(
                "CHRONOBREAK_NATIVE_MP4_READY_FOR_START_SIGNAL path={}",
                signal.display()
            );
            io::stdout()
                .flush()
                .context("could not publish native fixture start readiness")?;
        }
        let mut session = NativeRecorderSession::start(&target, &ffmpeg, &audio, &output)?;
        #[cfg(feature = "replay-time-fixture")]
        if let Some(signal) = fixture_start_signal.as_ref() {
            publish_fixture_start_signal(signal)?;
            println!(
                "CHRONOBREAK_NATIVE_MP4_START_SIGNAL_PUBLISHED path={}",
                signal.display()
            );
            io::stdout()
                .flush()
                .context("could not publish native fixture epoch release")?;
        }
        #[cfg(feature = "native-failure-injection")]
        if let Some(ticks) = fail_nvenc_after_ticks {
            session.inject_nvenc_failure_after_ticks(ticks)?;
            println!("CHRONOBREAK_NATIVE_FAILURE_INJECTION nvenc_after_ticks={ticks}");
        }
        #[cfg(feature = "native-failure-injection")]
        match (stall_worker_after_ticks, stall_worker_ms) {
            (Some(ticks), Some(milliseconds)) => {
                session
                    .inject_worker_stall_after_ticks(ticks, Duration::from_millis(milliseconds))?;
                println!(
                    "CHRONOBREAK_NATIVE_FAILURE_INJECTION worker_stall_after_ticks={ticks} worker_stall_ms={milliseconds}"
                );
            }
            (None, None) => {}
            _ => {
                bail!("--stall-worker-after-ticks and --stall-worker-ms must be provided together")
            }
        }
        #[cfg(feature = "native-failure-injection")]
        match (stall_mux_after_writes, stall_mux_ms) {
            (Some(write_index), Some(milliseconds)) => {
                session.inject_mux_writer_stall_after_writes(
                    write_index,
                    Duration::from_millis(milliseconds),
                )?;
                println!(
                    "CHRONOBREAK_NATIVE_FAILURE_INJECTION mux_stall_after_writes={write_index} mux_stall_ms={milliseconds}"
                );
            }
            (None, None) => {}
            _ => bail!("--stall-mux-after-writes and --stall-mux-ms must be provided together"),
        }
        #[cfg(feature = "native-failure-injection")]
        ensure!(
            fail_mux_after_writes.is_none() || fail_mux_after_fragment_after_writes.is_none(),
            "exact and post-fragment mux failure injections are mutually exclusive"
        );
        #[cfg(feature = "native-failure-injection")]
        if let Some(write_count) = fail_mux_after_writes {
            session.inject_mux_failure_after_writes(write_count)?;
            println!("CHRONOBREAK_NATIVE_FAILURE_INJECTION mux_failure_after_writes={write_count}");
        }
        #[cfg(feature = "native-failure-injection")]
        if let Some(minimum_write_count) = fail_mux_after_fragment_after_writes {
            session.inject_mux_failure_after_fragment_after_writes(minimum_write_count)?;
            println!(
                "CHRONOBREAK_NATIVE_FAILURE_INJECTION mux_failure_after_fragment_after_writes={minimum_write_count}"
            );
        }
        #[cfg(not(feature = "native-failure-injection"))]
        {
            ensure!(
                fail_nvenc_after_ticks.is_none(),
                "--fail-nvenc-after-ticks requires the native-failure-injection Cargo feature"
            );
            ensure!(
                stall_worker_after_ticks.is_none() && stall_worker_ms.is_none(),
                "worker stall injection requires the native-failure-injection Cargo feature"
            );
            ensure!(
                stall_mux_after_writes.is_none() && stall_mux_ms.is_none(),
                "mux writer stall injection requires the native-failure-injection Cargo feature"
            );
            ensure!(
                fail_mux_after_writes.is_none() && fail_mux_after_fragment_after_writes.is_none(),
                "mux failure injection requires the native-failure-injection Cargo feature"
            );
        }
        println!(
            "CHRONOBREAK_NATIVE_MP4_STARTED pid={pid} duration_seconds={duration_seconds} ffmpeg={} output={}",
            ffmpeg.display(),
            session.output().display()
        );
        if let Err(run_error) = session.run_for(Duration::from_secs(duration_seconds)) {
            return Err(match session.finish() {
                Ok(_) => run_error.context("native recording failed before bounded shutdown"),
                Err(shutdown_error) => anyhow!(
                    "native recording failed: {run_error:#}; bounded shutdown also failed: {shutdown_error:#}"
                ),
            });
        }
        let telemetry = session.finish()?;
        let expected_ticks = duration_seconds
            .checked_mul(60)
            .context("expected CFR tick count overflowed")?;
        ensure!(
            telemetry.cfr.scheduled_ticks == expected_ticks,
            "scheduled {} CFR ticks instead of {expected_ticks}",
            telemetry.cfr.scheduled_ticks
        );
        ensure!(
            telemetry.no_slot_admission_failures <= max_no_slot_admission_failures
                && telemetry.unstaged_tick_admission_failures == 0,
            "native CFR admission-failure ceiling exceeded: no_slot={} max_no_slot={} unstaged={}",
            telemetry.no_slot_admission_failures,
            max_no_slot_admission_failures,
            telemetry.unstaged_tick_admission_failures
        );
        ensure!(
            telemetry.encode.submitted_frames == expected_ticks
                && telemetry.encode.completed_frames == expected_ticks
                && telemetry.mux.encoded_frames == expected_ticks,
            "native MP4 accounting mismatch: committed={expected_ticks} submitted={} completed={} muxed={}",
            telemetry.encode.submitted_frames,
            telemetry.encode.completed_frames,
            telemetry.mux.encoded_frames
        );
        ensure!(
            telemetry.maximum_catch_up_batch <= 2,
            "native CFR catch-up batch exceeded 2: {}",
            telemetry.maximum_catch_up_batch
        );
        ensure!(
            telemetry.encode.max_in_flight <= 4
                && telemetry.encode.submission_queue_failures == 0
                && telemetry.encode.completion_errors == 0,
            "native encoder violated bounded M4 contract: max_in_flight={} queue_failures={} completion_errors={}",
            telemetry.encode.max_in_flight,
            telemetry.encode.submission_queue_failures,
            telemetry.encode.completion_errors
        );
        // Minimize/restore can make WGC recreate through a transient size for
        // which no frame is admitted. The converter must rebuild only for
        // dimensions that actually reach its persistent source snapshot.
        ensure!(
            telemetry.conversion.source_snapshot_allocations
                == telemetry.conversion.processor_recreations.saturating_add(1)
                && telemetry.conversion.processor_recreations <= telemetry.capture.recreations
                && telemetry.conversion.source_snapshot_copies > 0,
            "native CFR source snapshot was not resize-bounded: capture_recreations={} processor_recreations={} allocations={} copies={}",
            telemetry.capture.recreations,
            telemetry.conversion.processor_recreations,
            telemetry.conversion.source_snapshot_allocations,
            telemetry.conversion.source_snapshot_copies
        );
        ensure!(
            telemetry.conversion.processor_state_configurations
                == telemetry.conversion.processor_recreations.saturating_add(1),
            "native video-processor state was not recreation-bounded: recreations={} state_configurations={}",
            telemetry.conversion.processor_recreations,
            telemetry.conversion.processor_state_configurations
        );
        ensure!(
            telemetry.capture.pending_frame_high_water_mark <= 1,
            "native WGC pending-frame bound exceeded: {} > 1",
            telemetry.capture.pending_frame_high_water_mark
        );
        ensure!(
            telemetry.conversion.source_snapshot_copies <= expected_ticks.saturating_add(1),
            "native WGC copied {} source snapshots for {expected_ticks} ticks",
            telemetry.conversion.source_snapshot_copies
        );
        let accounted_admitted_sources = telemetry
            .conversion
            .source_snapshot_copies
            .saturating_add(telemetry.capture.pending_frame_replacements)
            .saturating_add(telemetry.capture.worker_frame_discards);
        ensure!(
            telemetry.capture.admitted == accounted_admitted_sources,
            "native WGC admitted-source accounting mismatch: admitted={} copies={} pending_replacements={} worker_discards={}",
            telemetry.capture.admitted,
            telemetry.conversion.source_snapshot_copies,
            telemetry.capture.pending_frame_replacements,
            telemetry.capture.worker_frame_discards
        );
        ensure!(
            telemetry.cfr.source_discards == telemetry.capture.pending_frame_replacements,
            "native CFR discard accounting mismatch: cfr={} pending_replacements={}",
            telemetry.cfr.source_discards,
            telemetry.capture.pending_frame_replacements
        );

        println!(
            "CHRONOBREAK_NATIVE_MP4_PASS ticks={} media_time_base={}/{} first_source_qpc_100ns={} latest_source_qpc_100ns={} cfr_discards={} cfr_duplicates={} late_ticks={} catch_up_ticks={} maximum_lateness_100ns={} latest_source_age_100ns={} maximum_source_age_100ns={} maximum_catch_up_batch={} injected_worker_stalls={} injected_worker_stall_100ns={} submitted={} completed={} mux_frames={} mux_progress_bytes={} output_bytes={} output_time_us={} video_writer_calls={} video_writer_duration_100ns={} maximum_video_writer_duration_100ns={} slow_video_writer_calls={} explicit_flush_calls={} explicit_flush_duration_100ns={} maximum_explicit_flush_duration_100ns={} injected_mux_writer_stalls={} injected_mux_writer_stall_100ns={} max_in_flight={} no_slot_admission_failures={} unstaged_tick_admission_failures={} source_arrivals={} source_admitted={} source_handoff_drops={} pending_frame_replacements={} pending_frame_high_water_mark={} worker_frame_discards={} source_recreations={} processor_recreations={} processor_state_configurations={} source_snapshot_allocations={} source_snapshot_copies={} output={}",
            telemetry.cfr.scheduled_ticks,
            telemetry.cfr.media_time_base_numerator,
            telemetry.cfr.media_time_base_denominator,
            telemetry.cfr.first_source_qpc_100ns,
            telemetry.cfr.latest_source_qpc_100ns,
            telemetry.cfr.source_discards,
            telemetry.cfr.duplicate_ticks,
            telemetry.cfr.late_ticks,
            telemetry.cfr.catch_up_ticks,
            telemetry.cfr.maximum_lateness_100ns,
            telemetry.cfr.latest_source_age_100ns,
            telemetry.cfr.maximum_source_age_100ns,
            telemetry.maximum_catch_up_batch,
            telemetry.injected_worker_stalls,
            telemetry.injected_worker_stall_100ns,
            telemetry.encode.submitted_frames,
            telemetry.encode.completed_frames,
            telemetry.mux.encoded_frames,
            telemetry.mux.muxed_bytes,
            telemetry.mux.output_file_bytes,
            telemetry.mux.output_time_us.unwrap_or_default(),
            telemetry.mux.video_writer_calls,
            telemetry.mux.video_writer_duration_100ns,
            telemetry.mux.maximum_video_writer_duration_100ns,
            telemetry.mux.slow_video_writer_calls,
            telemetry.mux.explicit_flush_calls,
            telemetry.mux.explicit_flush_duration_100ns,
            telemetry.mux.maximum_explicit_flush_duration_100ns,
            telemetry.mux.injected_mux_writer_stalls,
            telemetry.mux.injected_mux_writer_stall_100ns,
            telemetry.encode.max_in_flight,
            telemetry.no_slot_admission_failures,
            telemetry.unstaged_tick_admission_failures,
            telemetry.capture.arrivals,
            telemetry.capture.admitted,
            telemetry.capture.handoff_drops,
            telemetry.capture.pending_frame_replacements,
            telemetry.capture.pending_frame_high_water_mark,
            telemetry.capture.worker_frame_discards,
            telemetry.capture.recreations,
            telemetry.conversion.processor_recreations,
            telemetry.conversion.processor_state_configurations,
            telemetry.conversion.source_snapshot_allocations,
            telemetry.conversion.source_snapshot_copies,
            output.display()
        );
        Ok(())
    }
    #[cfg(feature = "replay-time-fixture")]
    fn publish_fixture_start_signal(signal: &Path) -> Result<()> {
        let mut temporary_name = signal.as_os_str().to_os_string();
        temporary_name.push(".tmp");
        let temporary = PathBuf::from(temporary_name);
        ensure!(
            !temporary.exists(),
            "fixture start-signal staging file already exists"
        );
        let result = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .with_context(|| {
                    format!(
                        "could not create fixture start-signal staging file {}",
                        temporary.display()
                    )
                })?;
            file.write_all(b"start\n")
                .context("could not write fixture start signal")?;
            file.sync_all()
                .context("could not flush fixture start signal")?;
            drop(file);
            fs::rename(&temporary, signal).with_context(|| {
                format!(
                    "could not atomically publish fixture start signal {}",
                    signal.display()
                )
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    windows_probe::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("native_mp4_probe is Windows-only");
}
