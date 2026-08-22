#[cfg(target_os = "windows")]
mod windows_capture {
    use std::io::{self, Write};
    use std::path::PathBuf;
    use std::time::Duration;

    use anyhow::{Context, Result, bail};
    use league_replay_recorder::config::RecordingProfile;
    use league_replay_recorder::encoder::{
        AudioSource, CAPTURE_DIAGNOSTIC_ABI, EncoderKind, Ffmpeg, RecordingEvidence, RecordingPlan,
        VideoCodec,
    };
    use league_replay_recorder::platform::{
        capture_target_for_process, validate_capture_target_identity,
    };

    pub async fn run() -> Result<()> {
        let mut arguments = std::env::args().skip(1);
        let mut pid = None;
        let mut output = None;
        let mut duration = Duration::from_secs(10);
        let mut encoder = EncoderKind::Nvenc;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--pid" => {
                    pid = Some(
                        arguments
                            .next()
                            .context("--pid requires a value")?
                            .parse::<u32>()
                            .context("--pid must be an integer")?,
                    );
                }
                "--output" => {
                    output = Some(PathBuf::from(
                        arguments.next().context("--output requires a value")?,
                    ));
                }
                "--duration-seconds" => {
                    duration = Duration::from_secs(
                        arguments
                            .next()
                            .context("--duration-seconds requires a value")?
                            .parse::<u64>()
                            .context("--duration-seconds must be an integer")?,
                    );
                }
                "--encoder" => {
                    encoder = match arguments
                        .next()
                        .context("--encoder requires nvenc, amf, or qsv")?
                        .as_str()
                    {
                        "nvenc" => EncoderKind::Nvenc,
                        "amf" => EncoderKind::Amf,
                        "qsv" => EncoderKind::Qsv,
                        value => bail!("unsupported fixture encoder {value:?}"),
                    };
                }
                value => bail!("unknown argument {value:?}"),
            }
        }

        let pid = pid.context("--pid is required")?;
        let output = output.context("--output is required")?;
        if output.exists() || output.file_name().is_none() {
            bail!("fixture output must be a new dedicated bundle directory");
        }
        std::fs::create_dir_all(&output)
            .with_context(|| format!("could not create fixture output {}", output.display()))?;
        std::fs::write(output.join(".queueback-wgc-fixture"), b"fixture-only\n")?;

        let target = capture_target_for_process(pid)?;
        let ffmpeg = Ffmpeg::resolve().await?;
        let plan = RecordingPlan {
            encoder,
            codec: VideoCodec::H264,
            profile: RecordingProfile::High,
        };
        let session = ffmpeg
            .start_recording(output.clone(), &target, plan, &AudioSource::Silent)
            .await?;
        let evidence = session.evidence_receiver();
        let capture_started = tokio::time::Instant::now();
        println!(
            "QUEUEBACK_WGC_RECORDING_READY runtime={} target={}",
            ffmpeg.runtime_id(),
            target.description()
        );
        let initial = evidence.borrow().clone();
        print_evidence("QUEUEBACK_WGC_LIVE", &initial, capture_started.elapsed());
        let deadline = tokio::time::Instant::now() + duration;
        let mut target_failure = None;
        let mut progress_failure = None;
        let mut last = initial;
        let now = tokio::time::Instant::now();
        let mut source_advanced_at = now;
        let mut encoded_advanced_at = now;
        let mut muxed_advanced_at = now;
        let mut next_report = now + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            tokio::time::sleep(remaining.min(Duration::from_millis(200))).await;
            if let Err(error) = validate_capture_target_identity(&target) {
                target_failure = Some(error);
                break;
            }
            let snapshot = evidence.borrow().clone();
            let now = tokio::time::Instant::now();
            if snapshot.latest_qpc.unwrap_or_default() > last.latest_qpc.unwrap_or_default() {
                source_advanced_at = now;
            }
            if snapshot.encoded_frames > last.encoded_frames {
                encoded_advanced_at = now;
            }
            if snapshot.muxed_bytes > last.muxed_bytes {
                muxed_advanced_at = now;
            }
            last = snapshot.clone();
            for (label, advanced_at) in [
                ("WGC source timestamp", source_advanced_at),
                ("encoded frame count", encoded_advanced_at),
                ("muxed byte count", muxed_advanced_at),
            ] {
                if now.saturating_duration_since(advanced_at) >= Duration::from_secs(15) {
                    progress_failure = Some(format!("{label} did not advance for 15 seconds"));
                    break;
                }
            }
            if progress_failure.is_some() {
                break;
            }
            if now >= next_report {
                print_evidence("QUEUEBACK_WGC_LIVE", &snapshot, capture_started.elapsed());
                next_report = now + Duration::from_secs(5);
            }
        }
        let stop_result = session.stop().await;
        let evidence = evidence.borrow().clone();
        print_evidence(
            "QUEUEBACK_WGC_EVIDENCE",
            &evidence,
            capture_started.elapsed(),
        );
        let completed = stop_result?;
        if let Some(error) = target_failure {
            bail!("the dedicated capture target became invalid: {error:#}");
        }
        if let Some(error) = progress_failure {
            bail!("the dedicated capture graph stalled: {error}");
        }
        println!("QUEUEBACK_WGC_RECORDING_COMPLETE {}", completed.display());
        Ok(())
    }

    fn print_evidence(marker: &str, evidence: &RecordingEvidence, elapsed: Duration) {
        println!(
            "{marker} {}",
            serde_json::json!({
                "diagnostics_abi": CAPTURE_DIAGNOSTIC_ABI,
                "elapsed_ms": u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
                "capture_ready": evidence.capture_ready,
                "capture_terminal": evidence.capture_terminal,
                "frame_pool_capacity": evidence.frame_pool_capacity,
                "output_pool_capacity": evidence.output_pool_capacity,
                "source_frames_surfaced": evidence.source_frames_surfaced,
                "source_frames_superseded": evidence.source_frames_superseded,
                "pool_recreations": evidence.pool_recreations,
                "first_qpc": evidence.first_qpc,
                "latest_qpc": evidence.latest_qpc,
                "encoded_frames": evidence.encoded_frames,
                "muxed_bytes": evidence.muxed_bytes,
                "output_time_us": evidence.output_time_us,
                "cfr_duplicates": evidence.cfr_duplicates,
                "cfr_discards": evidence.cfr_discards,
                "progress_end": evidence.progress_end,
                "protocol_error": evidence.protocol_error,
            })
        );
        io::stdout().flush().ok();
    }
}

#[cfg(target_os = "windows")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .try_init();
    windows_capture::run().await
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("The QueueBack WGC capture fixture is Windows-only.");
}
