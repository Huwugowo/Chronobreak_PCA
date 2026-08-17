use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::encoder::{
    AudioSource, FRAGMENTED_MP4_FLAGS, append_windows_audio_arguments, push_args,
};

/// FFmpeg is retained only as the audio encoder and fragmented-MP4 muxer. The
/// native encoder writes Annex-B H.264 packets to stdin and closes the pipe for
/// bounded end-of-stream; FFmpeg must perform no video capture, filtering or
/// encoding on this path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMuxPlan {
    arguments: Vec<OsString>,
    output: PathBuf,
}

impl NativeMuxPlan {
    pub fn h264(frames_per_second: u32, audio: &AudioSource, output: &Path) -> Result<Self> {
        if frames_per_second == 0 || frames_per_second > 240 {
            bail!("native mux FPS must be in 1..=240");
        }
        if output.as_os_str().is_empty() {
            bail!("native mux output path is empty");
        }

        let mut arguments = Vec::new();
        push_args(
            &mut arguments,
            &[
                "-hide_banner",
                "-loglevel",
                "info",
                "-nostats",
                "-stats_period",
                "0.25",
                "-progress",
                "pipe:1",
                // Raw Annex-B packets do not carry MP4 timestamps. As an input
                // option, -r explicitly generates the configured CFR timeline.
                "-r",
            ],
        );
        arguments.push(frames_per_second.to_string().into());
        push_args(
            &mut arguments,
            &["-fflags", "+genpts", "-f", "h264", "-i", "pipe:0"],
        );

        append_windows_audio_arguments(&mut arguments, audio);
        push_args(
            &mut arguments,
            &[
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-shortest",
                "-flush_packets",
                "1",
                "-movflags",
                FRAGMENTED_MP4_FLAGS,
                "-y",
            ],
        );
        arguments.push(output.as_os_str().to_owned());

        let plan = Self {
            arguments,
            output: output.to_path_buf(),
        };
        plan.validate_mux_only()?;
        Ok(plan)
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    pub fn output(&self) -> &Path {
        &self.output
    }

    fn validate_mux_only(&self) -> Result<()> {
        require_pair(&self.arguments, "-c:v", "copy")?;
        require_pair(&self.arguments, "-f", "h264")?;
        require_pair(&self.arguments, "-i", "pipe:0")?;

        const FORBIDDEN_EXACT: &[&str] = &[
            "-vf",
            "-filter_complex",
            "-filter_hw_device",
            "-init_hw_device",
            "-pix_fmt",
            "-b:v",
            "-maxrate",
            "-bufsize",
            "-g",
            "-bf",
            "-surfaces",
            "gdigrab",
            "desktop",
        ];
        const FORBIDDEN_SUBSTRINGS: &[&str] = &[
            "gfxcapture",
            "scale=",
            "scale_d3d11",
            "hwmap=",
            "h264_nvenc",
            "hevc_nvenc",
            "h264_amf",
            "h264_qsv",
        ];
        for argument in &self.arguments {
            let value = argument.to_string_lossy();
            if FORBIDDEN_EXACT.iter().any(|forbidden| value == *forbidden)
                || FORBIDDEN_SUBSTRINGS
                    .iter()
                    .any(|forbidden| value.contains(forbidden))
            {
                bail!("native mux plan contains forbidden video work argument {value:?}");
            }
        }
        Ok(())
    }
}

const MUX_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_MUX_LINE_BYTES: usize = 16 * 1024;
const MAX_STDERR_TAIL_LINES: usize = 64;
const NATIVE_PIPE_BUFFER_BYTES: usize = 1024 * 1024;
const NATIVE_PIPE_FLUSH_BYTES: usize = 256 * 1024;
const NATIVE_PIPE_FLUSH_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMuxTelemetrySnapshot {
    pub encoded_frames: u64,
    pub muxed_bytes: u64,
    pub output_time_us: Option<i64>,
    pub output_file_bytes: u64,
    pub progress_end: bool,
    pub reader_errors: u64,
}

struct NativeMuxTelemetry {
    encoded_frames: AtomicU64,
    muxed_bytes: AtomicU64,
    output_time_us: AtomicI64,
    progress_end: AtomicBool,
    progress_pipe_closed: AtomicBool,
    reader_errors: AtomicU64,
    stderr_tail: Mutex<VecDeque<String>>,
}

impl Default for NativeMuxTelemetry {
    fn default() -> Self {
        Self {
            encoded_frames: AtomicU64::new(0),
            muxed_bytes: AtomicU64::new(0),
            output_time_us: AtomicI64::new(-1),
            progress_end: AtomicBool::new(false),
            progress_pipe_closed: AtomicBool::new(false),
            reader_errors: AtomicU64::new(0),
            stderr_tail: Mutex::new(VecDeque::with_capacity(MAX_STDERR_TAIL_LINES)),
        }
    }
}

impl NativeMuxTelemetry {
    fn snapshot(&self, output_file_bytes: u64) -> NativeMuxTelemetrySnapshot {
        let output_time_us = self.output_time_us.load(Ordering::Relaxed);
        NativeMuxTelemetrySnapshot {
            encoded_frames: self.encoded_frames.load(Ordering::Relaxed),
            muxed_bytes: self.muxed_bytes.load(Ordering::Relaxed),
            output_time_us: (output_time_us >= 0).then_some(output_time_us),
            output_file_bytes,
            progress_end: self.progress_end.load(Ordering::Acquire),
            reader_errors: self.reader_errors.load(Ordering::Relaxed),
        }
    }

    fn reader_error(&self) {
        self.reader_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn remember_stderr(&self, line: &str) {
        let Ok(mut tail) = self.stderr_tail.lock() else {
            self.reader_error();
            return;
        };
        if tail.len() == MAX_STDERR_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line.to_owned());
    }

    fn stderr_summary(&self) -> String {
        self.stderr_tail.lock().map_or_else(
            |_| "<stderr tail unavailable>".to_owned(),
            |tail| tail.iter().cloned().collect::<Vec<_>>().join(" | "),
        )
    }

    fn ensure_video_sink_open(&self) -> io::Result<()> {
        if self.progress_pipe_closed.load(Ordering::Acquire)
            && !self.progress_end.load(Ordering::Acquire)
        {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "mux-only FFmpeg exited before progress=end",
            ));
        }
        Ok(())
    }
}

/// Buffered Annex-B writer that keeps the large syscall-saving buffer while
/// observing FFmpeg process loss on every NVENC output frame.
pub struct NativeMuxVideoWriter {
    writer: BufWriter<ChildStdin>,
    telemetry: Arc<NativeMuxTelemetry>,
    unflushed_bytes: usize,
    last_flush: Instant,
}

impl Write for NativeMuxVideoWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.telemetry.ensure_video_sink_open()?;
        let written = self.writer.write(buffer)?;
        self.unflushed_bytes = self.unflushed_bytes.saturating_add(written);
        if self.unflushed_bytes >= NATIVE_PIPE_FLUSH_BYTES
            || self.last_flush.elapsed() >= NATIVE_PIPE_FLUSH_INTERVAL
        {
            self.writer.flush()?;
            self.unflushed_bytes = 0;
            self.last_flush = Instant::now();
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.telemetry.ensure_video_sink_open()?;
        self.writer.flush()?;
        self.unflushed_bytes = 0;
        self.last_flush = Instant::now();
        Ok(())
    }
}

/// Running FFmpeg mux/audio utility. Native NVENC owns the returned buffered
/// stdin writer; dropping it after EOS is the only normal video-input stop
/// signal. The child performs stream copy only, as enforced by `NativeMuxPlan`.
pub struct NativeMuxProcess {
    child: Child,
    progress_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
    telemetry: Arc<NativeMuxTelemetry>,
    output: PathBuf,
    writer_taken: bool,
    finished: bool,
}

impl NativeMuxProcess {
    pub fn start(ffmpeg: &Path, plan: &NativeMuxPlan) -> Result<Self> {
        ensure!(!ffmpeg.as_os_str().is_empty(), "FFmpeg path is empty");
        let mut command = Command::new(ffmpeg);
        command
            .args(plan.arguments())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command
            .spawn()
            .with_context(|| format!("could not start mux-only FFmpeg at {}", ffmpeg.display()))?;
        let Some(stdout) = child.stdout.take() else {
            terminate_child(&mut child);
            bail!("mux-only FFmpeg progress pipe was not created");
        };
        let Some(stderr) = child.stderr.take() else {
            terminate_child(&mut child);
            bail!("mux-only FFmpeg diagnostics pipe was not created");
        };
        let telemetry = Arc::new(NativeMuxTelemetry::default());
        let progress_telemetry = Arc::clone(&telemetry);
        let progress_thread = match thread::Builder::new()
            .name("queueback-native-mux-progress".to_owned())
            .spawn(move || {
                drain_bounded_lines(stdout, &progress_telemetry, |line, telemetry| {
                    ingest_progress_line(line, telemetry);
                });
                progress_telemetry
                    .progress_pipe_closed
                    .store(true, Ordering::Release);
            }) {
            Ok(thread) => thread,
            Err(error) => {
                terminate_child(&mut child);
                return Err(error).context("could not start native mux progress reader");
            }
        };
        let stderr_telemetry = Arc::clone(&telemetry);
        let stderr_thread = match thread::Builder::new()
            .name("queueback-native-mux-stderr".to_owned())
            .spawn(move || {
                drain_bounded_lines(stderr, &stderr_telemetry, |line, telemetry| {
                    telemetry.remember_stderr(line);
                });
            }) {
            Ok(thread) => thread,
            Err(error) => {
                terminate_child(&mut child);
                let _ = progress_thread.join();
                return Err(error).context("could not start native mux diagnostics reader");
            }
        };

        Ok(Self {
            child,
            progress_thread: Some(progress_thread),
            stderr_thread: Some(stderr_thread),
            telemetry,
            output: plan.output().to_path_buf(),
            writer_taken: false,
            finished: false,
        })
    }

    pub fn take_video_writer(&mut self) -> Result<NativeMuxVideoWriter> {
        ensure!(
            !self.writer_taken,
            "native mux video writer was already taken"
        );
        let stdin = self
            .child
            .stdin
            .take()
            .context("mux-only FFmpeg video pipe was not created")?;
        self.writer_taken = true;
        Ok(NativeMuxVideoWriter {
            writer: BufWriter::with_capacity(NATIVE_PIPE_BUFFER_BYTES, stdin),
            telemetry: Arc::clone(&self.telemetry),
            unflushed_bytes: 0,
            last_flush: Instant::now(),
        })
    }

    pub fn telemetry(&self) -> NativeMuxTelemetrySnapshot {
        let output_file_bytes =
            std::fs::metadata(&self.output).map_or(0, |metadata| metadata.len());
        self.telemetry.snapshot(output_file_bytes)
    }

    pub fn finish(mut self) -> Result<NativeMuxTelemetrySnapshot> {
        let result = self.finish_inner(MUX_STOP_TIMEOUT);
        self.finished = self.child.try_wait().ok().flatten().is_some();
        result
    }

    fn finish_inner(&mut self, timeout: Duration) -> Result<NativeMuxTelemetrySnapshot> {
        ensure!(
            self.writer_taken,
            "native mux cannot finish before NVENC takes and closes its video pipe"
        );
        let deadline = Instant::now() + timeout;
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = self
                .child
                .try_wait()
                .context("could not inspect mux-only FFmpeg")?
            {
                break Some(status);
            }
            if Instant::now() >= deadline {
                timed_out = true;
                let _ = self.child.kill();
                break self.child.wait().ok();
            }
            thread::sleep(Duration::from_millis(10));
        };
        let reader_join_error = self.join_readers();
        let output_file_bytes = std::fs::metadata(&self.output)
            .with_context(|| {
                format!(
                    "mux-only FFmpeg did not preserve output {}",
                    self.output.display()
                )
            })?
            .len();
        let snapshot = self.telemetry.snapshot(output_file_bytes);
        let stderr = self.telemetry.stderr_summary();

        ensure!(
            !timed_out,
            "mux-only FFmpeg did not stop within {} seconds; partial fragmented MP4 preserved at {} (stderr: {stderr})",
            timeout.as_secs_f64(),
            self.output.display()
        );
        let status = status.context("mux-only FFmpeg produced no terminal exit status")?;
        ensure!(
            status.success(),
            "mux-only FFmpeg exited with {status}; partial fragmented MP4 preserved at {} (stderr: {stderr})",
            self.output.display()
        );
        ensure!(
            reader_join_error.is_none() && snapshot.reader_errors == 0,
            "mux-only FFmpeg evidence readers failed: {}",
            reader_join_error.unwrap_or_else(|| "invalid/oversized progress output".to_owned())
        );
        ensure!(
            snapshot.progress_end,
            "mux-only FFmpeg omitted progress=end"
        );
        ensure!(
            snapshot.encoded_frames > 0,
            "mux-only FFmpeg reported no video frames"
        );
        ensure!(
            snapshot.muxed_bytes > 0 && snapshot.output_file_bytes > 0,
            "mux-only FFmpeg produced no MP4 bytes"
        );
        Ok(snapshot)
    }

    fn join_readers(&mut self) -> Option<String> {
        let mut error = None;
        for (label, thread) in [
            ("progress", self.progress_thread.take()),
            ("stderr", self.stderr_thread.take()),
        ] {
            if thread.is_some_and(|thread| thread.join().is_err()) && error.is_none() {
                error = Some(format!("native mux {label} reader panicked"));
            }
        }
        error
    }
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

impl Drop for NativeMuxProcess {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = self.join_readers();
    }
}

fn drain_bounded_lines<R, F>(mut reader: R, telemetry: &NativeMuxTelemetry, mut consume: F)
where
    R: Read,
    F: FnMut(&str, &NativeMuxTelemetry),
{
    let mut chunk = [0_u8; 4096];
    let mut line = Vec::with_capacity(1024);
    let mut overflow = false;
    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => {
                telemetry.reader_error();
                return;
            }
        };
        for byte in &chunk[..read] {
            if *byte == b'\n' {
                if overflow {
                    telemetry.reader_error();
                } else {
                    let text = String::from_utf8_lossy(&line);
                    consume(text.trim_end_matches('\r'), telemetry);
                }
                line.clear();
                overflow = false;
            } else if line.len() < MAX_MUX_LINE_BYTES {
                line.push(*byte);
            } else {
                overflow = true;
            }
        }
    }
    if overflow {
        telemetry.reader_error();
    } else if !line.is_empty() {
        let text = String::from_utf8_lossy(&line);
        consume(text.trim_end_matches('\r'), telemetry);
    }
}

fn ingest_progress_line(line: &str, telemetry: &NativeMuxTelemetry) {
    if line.is_empty() {
        return;
    }
    let Some((key, value)) = line.split_once('=') else {
        telemetry.reader_error();
        return;
    };
    match key {
        "frame" => ingest_monotonic_u64(value, &telemetry.encoded_frames, telemetry),
        "total_size" => ingest_monotonic_u64(value, &telemetry.muxed_bytes, telemetry),
        "out_time_us" if value != "N/A" => match value.parse::<i64>() {
            Ok(parsed) => {
                telemetry
                    .output_time_us
                    .fetch_max(parsed, Ordering::Relaxed);
            }
            Err(_) => telemetry.reader_error(),
        },
        "progress" => match value {
            "continue" => {}
            "end" => telemetry.progress_end.store(true, Ordering::Release),
            _ => telemetry.reader_error(),
        },
        _ => {}
    }
}

fn ingest_monotonic_u64(value: &str, destination: &AtomicU64, telemetry: &NativeMuxTelemetry) {
    match value.parse::<u64>() {
        Ok(parsed) if parsed >= destination.load(Ordering::Relaxed) => {
            destination.store(parsed, Ordering::Relaxed);
        }
        Ok(_) | Err(_) => telemetry.reader_error(),
    }
}

fn require_pair(arguments: &[OsString], option: &str, value: &str) -> Result<()> {
    arguments
        .windows(2)
        .any(|pair| pair[0] == OsStr::new(option) && pair[1] == OsStr::new(value))
        .then_some(())
        .with_context(|| format!("native mux plan is missing {option} {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(plan: &NativeMuxPlan) -> Vec<String> {
        plan.arguments()
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn h264_mux_plan_declares_cfr_streamcopy_and_fragmented_mp4() {
        let plan = NativeMuxPlan::h264(60, &AudioSource::Silent, Path::new("native.mp4"))
            .expect("valid mux plan");
        let arguments = strings(&plan);

        assert!(arguments.windows(2).any(|pair| pair == ["-r", "60"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-f", "h264"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-i", "pipe:0"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-c:v", "copy"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-c:a", "aac"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:v:0"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-map", "1:a:0"]));
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["-movflags", FRAGMENTED_MP4_FLAGS])
        );
        assert!(arguments.iter().any(|argument| argument == "-shortest"));
        assert_eq!(arguments.last().map(String::as_str), Some("native.mp4"));
        assert_eq!(plan.output(), Path::new("native.mp4"));
    }

    #[test]
    fn mux_plan_rejects_every_capture_filter_and_video_encode_token() {
        let forbidden = [
            "-vf",
            "-filter_complex",
            "-filter_hw_device",
            "-init_hw_device",
            "-pix_fmt",
            "-b:v",
            "-g",
            "-surfaces",
            "gfxcapture=hwnd=1",
            "scale_d3d11=width=1920",
            "h264_nvenc",
            "h264_amf",
            "h264_qsv",
        ];
        for token in forbidden {
            let mut plan = NativeMuxPlan::h264(60, &AudioSource::Silent, Path::new("native.mp4"))
                .expect("valid baseline mux plan");
            plan.arguments.push(token.into());
            assert!(
                plan.validate_mux_only().is_err(),
                "forbidden token was accepted: {token}"
            );
        }
    }

    #[test]
    fn mux_plan_rejects_invalid_rate_and_empty_output() {
        assert!(NativeMuxPlan::h264(0, &AudioSource::Silent, Path::new("native.mp4")).is_err());
        assert!(NativeMuxPlan::h264(241, &AudioSource::Silent, Path::new("native.mp4")).is_err());
        assert!(NativeMuxPlan::h264(60, &AudioSource::Silent, Path::new("")).is_err());
    }

    #[test]
    fn native_progress_evidence_is_monotonic_and_terminal() {
        let telemetry = NativeMuxTelemetry::default();
        for line in [
            "frame=1",
            "total_size=4096",
            "out_time_us=16666",
            "frame=2",
            "total_size=8192",
            "out_time_us=33333",
            "progress=end",
        ] {
            ingest_progress_line(line, &telemetry);
        }
        assert_eq!(
            telemetry.snapshot(9000),
            NativeMuxTelemetrySnapshot {
                encoded_frames: 2,
                muxed_bytes: 8192,
                output_time_us: Some(33333),
                output_file_bytes: 9000,
                progress_end: true,
                reader_errors: 0,
            }
        );

        ingest_progress_line("frame=1", &telemetry);
        assert_eq!(telemetry.reader_errors.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn video_sink_reports_early_mux_exit_without_waiting_for_buffer_flush() {
        let telemetry = NativeMuxTelemetry::default();
        telemetry
            .progress_pipe_closed
            .store(true, Ordering::Release);

        let error = telemetry.ensure_video_sink_open().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert!(error.to_string().contains("before progress=end"));

        telemetry.progress_end.store(true, Ordering::Release);
        telemetry.ensure_video_sink_open().unwrap();
    }
}

