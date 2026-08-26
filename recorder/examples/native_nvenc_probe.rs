#[cfg(target_os = "windows")]
mod windows_probe {
    use std::fs::File;
    use std::io::{self, BufWriter, Write};
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, bail};
    use league_replay_recorder::native::{NativeNvencEncoder, NativeWgcSource};
    use league_replay_recorder::platform::capture_target_for_process;

    pub fn run() -> Result<()> {
        let mut args = std::env::args().skip(1);
        let mut pid = None;
        let mut output = None;
        let mut duration = Duration::from_secs(10);
        let mut fail_output_after = None;
        let mut block_output_after = None;
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
                "--output" => {
                    output = Some(PathBuf::from(
                        args.next().context("--output requires a path")?,
                    ));
                }
                "--duration-seconds" => {
                    duration = Duration::from_secs(
                        args.next()
                            .context("--duration-seconds requires a value")?
                            .parse::<u64>()
                            .context("--duration-seconds must be an integer")?,
                    );
                }
                "--fail-output-after-bytes" => {
                    fail_output_after = Some(
                        args.next()
                            .context("--fail-output-after-bytes requires a value")?
                            .parse::<u64>()
                            .context("--fail-output-after-bytes must be an integer")?,
                    );
                }
                "--block-output-after-bytes" => {
                    block_output_after = Some(
                        args.next()
                            .context("--block-output-after-bytes requires a value")?
                            .parse::<u64>()
                            .context("--block-output-after-bytes must be an integer")?,
                    );
                }
                other => bail!("unknown argument {other:?}"),
            }
        }
        let pid = pid.context("--pid is required")?;
        let output = output.context("--output is required")?;
        if fail_output_after.is_some() && block_output_after.is_some() {
            bail!("output failure and blocking injection are mutually exclusive");
        }

        let target = capture_target_for_process(pid)
            .with_context(|| format!("could not resolve exact HWND for fixture PID {pid}"))?;
        let mut source = NativeWgcSource::start(&target)?;
        let mut converter = source.create_nv12_converter(1920, 1080, 60)?;
        let file = File::create(&output)
            .with_context(|| format!("could not create H.264 output at {}", output.display()))?;
        let writer = ProbeWriter {
            inner: BufWriter::with_capacity(1024 * 1024, file),
            bytes_accepted: 0,
            failure_after: fail_output_after,
            block_after: block_output_after,
        };
        let mut encoder = NativeNvencEncoder::new(&source, &converter, writer)?;

        println!(
            "CHRONOBREAK_NATIVE_NVENC_STARTED adapter_luid={:016x} adapter_name={:?} output={}",
            source.adapter_luid(),
            source.adapter_name(),
            output.display(),
        );

        let deadline = Instant::now() + duration;
        let mut submitted = 0_u64;
        let mut latest_qpc = None;
        while Instant::now() < deadline {
            if let Some(frame) = source.recv_timeout(Duration::from_millis(250))? {
                let dimensions = frame.dimensions();
                if dimensions != source.pool_dimensions() {
                    frame.close()?;
                    encoder.drain()?;
                    source.recreate_for_content_size(dimensions.0, dimensions.1)?;
                    converter.reconfigure_input(dimensions.0, dimensions.1)?;
                    continue;
                }

                let qpc = frame.qpc_100ns();
                if latest_qpc.is_some_and(|last| qpc <= last) {
                    bail!("native WGC SystemRelativeTime/QPC did not advance monotonically");
                }
                latest_qpc = Some(qpc);
                if let Some(converted) = converter.convert(&frame)? {
                    encoder.submit(converted)?;
                    submitted = submitted.saturating_add(1);
                }
                frame.close()?;
            }
            if source.telemetry().closed {
                break;
            }
        }

        let capture = source.close()?;
        let encode = encoder.finish()?;
        let conversion = converter.telemetry();
        if submitted == 0 {
            bail!("native NVENC probe submitted no frames");
        }
        if encode.completed_frames != submitted
            || encode.output_bytes == 0
            || encode.submission_queue_failures != 0
            || encode.completion_errors != 0
        {
            bail!(
                "native NVENC accounting mismatch: submitted={} completed={} bytes={} queue_failures={} completion_errors={}",
                submitted,
                encode.completed_frames,
                encode.output_bytes,
                encode.submission_queue_failures,
                encode.completion_errors,
            );
        }
        if converter.free_slot_count() != 4 {
            bail!(
                "native NV12 ring retained {} slots after NVENC finish",
                4_usize.saturating_sub(converter.free_slot_count())
            );
        }
        let output_size = std::fs::metadata(&output)
            .with_context(|| format!("could not stat H.264 output at {}", output.display()))?
            .len();
        if output_size != encode.output_bytes {
            bail!(
                "H.264 file size {output_size} differs from NVENC output bytes {}",
                encode.output_bytes
            );
        }

        println!(
            "CHRONOBREAK_NATIVE_NVENC_PASS submitted={} completed={} bytes={} max_in_flight={} no_free_slot_admission_failures={} input_view_creations={} input_view_replacements={} capture_arrivals={} capture_handoff_drops={} callback_errors={} output={}",
            submitted,
            encode.completed_frames,
            encode.output_bytes,
            encode.max_in_flight,
            conversion.no_free_slot_admission_failures,
            conversion.input_view_creations,
            conversion.input_view_replacements,
            capture.arrivals,
            capture.handoff_drops,
            capture.callback_errors,
            output.display(),
        );
        Ok(())
    }

    struct ProbeWriter {
        inner: BufWriter<File>,
        bytes_accepted: u64,
        failure_after: Option<u64>,
        block_after: Option<u64>,
    }

    impl Write for ProbeWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self
                .failure_after
                .is_some_and(|limit| self.bytes_accepted >= limit)
            {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "injected native output failure",
                ));
            }
            if self
                .block_after
                .is_some_and(|limit| self.bytes_accepted >= limit)
            {
                loop {
                    std::thread::park();
                }
            }
            let written = self.inner.write(buffer)?;
            self.bytes_accepted = self.bytes_accepted.saturating_add(written as u64);
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    windows_probe::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("native_nvenc_probe is Windows-only");
}
