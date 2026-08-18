#[cfg(target_os = "windows")]
mod windows_probe {
    use std::path::PathBuf;
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
        let mut max_slot_tick_drops = 0_u64;
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
                "--max-slot-tick-drops" => {
                    max_slot_tick_drops = args
                        .next()
                        .context("--max-slot-tick-drops requires a value")?
                        .parse::<u64>()
                        .context("--max-slot-tick-drops must be an integer")?;
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
        let mut session =
            NativeRecorderSession::start(&target, &ffmpeg, &AudioSource::Silent, &output)?;
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
            telemetry.slot_tick_drops <= max_slot_tick_drops && telemetry.unstaged_tick_drops == 0,
            "native CFR drop ceiling exceeded: slot={} max_slot={} unstaged={}",
            telemetry.slot_tick_drops,
            max_slot_tick_drops,
            telemetry.unstaged_tick_drops
        );
        let total_tick_drops = telemetry
            .slot_tick_drops
            .checked_add(telemetry.unstaged_tick_drops)
            .context("native CFR drop accounting overflowed")?;
        let expected_encoded_ticks = expected_ticks
            .checked_sub(total_tick_drops)
            .context("native CFR drops exceeded scheduled ticks")?;
        ensure!(
            telemetry.encode.submitted_frames == expected_encoded_ticks
                && telemetry.encode.completed_frames == expected_encoded_ticks
                && telemetry.mux.encoded_frames == expected_encoded_ticks,
            "native MP4 accounting mismatch: scheduled={expected_ticks} expected_encoded={expected_encoded_ticks} submitted={} completed={} muxed={}",
            telemetry.encode.submitted_frames,
            telemetry.encode.completed_frames,
            telemetry.mux.encoded_frames
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

        println!(
            "CHRONOBREAK_NATIVE_MP4_PASS ticks={} media_time_base={}/{} first_source_qpc_100ns={} latest_source_qpc_100ns={} cfr_discards={} cfr_duplicates={} late_ticks={} catch_up_ticks={} maximum_lateness_100ns={} latest_source_age_100ns={} maximum_source_age_100ns={} maximum_catch_up_batch={} injected_worker_stalls={} injected_worker_stall_100ns={} submitted={} completed={} mux_frames={} mux_progress_bytes={} output_bytes={} output_time_us={} max_in_flight={} slot_tick_drops={} unstaged_tick_drops={} source_arrivals={} source_handoff_drops={} source_recreations={} processor_recreations={} processor_state_configurations={} source_snapshot_allocations={} source_snapshot_copies={} output={}",
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
            telemetry.encode.max_in_flight,
            telemetry.slot_tick_drops,
            telemetry.unstaged_tick_drops,
            telemetry.capture.arrivals,
            telemetry.capture.handoff_drops,
            telemetry.capture.recreations,
            telemetry.conversion.processor_recreations,
            telemetry.conversion.processor_state_configurations,
            telemetry.conversion.source_snapshot_allocations,
            telemetry.conversion.source_snapshot_copies,
            output.display()
        );
        Ok(())
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
