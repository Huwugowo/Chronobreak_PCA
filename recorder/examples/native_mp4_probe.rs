#[cfg(target_os = "windows")]
mod windows_probe {
    use std::path::PathBuf;
    use std::time::Duration;

    use anyhow::{Context, Result, bail, ensure};
    use league_replay_recorder::encoder::AudioSource;
    use league_replay_recorder::native::NativeRecorderSession;
    use league_replay_recorder::platform::capture_target_for_process;

    pub fn run() -> Result<()> {
        let mut args = std::env::args().skip(1);
        let mut pid = None;
        let mut ffmpeg = None;
        let mut output = None;
        let mut duration_seconds = 10_u64;
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
        println!(
            "CHRONOBREAK_NATIVE_MP4_STARTED pid={pid} duration_seconds={duration_seconds} ffmpeg={} output={}",
            ffmpeg.display(),
            session.output().display()
        );
        session.run_for(Duration::from_secs(duration_seconds))?;
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
            telemetry.slot_tick_drops == 0 && telemetry.unstaged_tick_drops == 0,
            "native CFR lost ticks: slot={} unstaged={}",
            telemetry.slot_tick_drops,
            telemetry.unstaged_tick_drops
        );
        ensure!(
            telemetry.encode.submitted_frames == expected_ticks
                && telemetry.encode.completed_frames == expected_ticks
                && telemetry.mux.encoded_frames == expected_ticks,
            "native MP4 accounting mismatch: expected={expected_ticks} submitted={} completed={} muxed={}",
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
        ensure!(
            telemetry.conversion.source_snapshot_allocations
                == telemetry.capture.recreations.saturating_add(1)
                && telemetry.conversion.processor_recreations == telemetry.capture.recreations
                && telemetry.conversion.source_snapshot_copies > 0,
            "native CFR source snapshot was not resize-bounded: capture_recreations={} processor_recreations={} allocations={} copies={}",
            telemetry.capture.recreations,
            telemetry.conversion.processor_recreations,
            telemetry.conversion.source_snapshot_allocations,
            telemetry.conversion.source_snapshot_copies
        );

        println!(
            "CHRONOBREAK_NATIVE_MP4_PASS ticks={} cfr_discards={} cfr_duplicates={} submitted={} completed={} mux_frames={} mux_progress_bytes={} output_bytes={} output_time_us={} max_in_flight={} slot_tick_drops={} unstaged_tick_drops={} source_arrivals={} source_handoff_drops={} source_recreations={} source_snapshot_allocations={} source_snapshot_copies={} output={}",
            telemetry.cfr.scheduled_ticks,
            telemetry.cfr.source_discards,
            telemetry.cfr.duplicate_ticks,
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
