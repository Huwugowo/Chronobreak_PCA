use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Cursor, ErrorKind, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

use super::{NativeMuxPlan, NativeMuxProcess};
use crate::encoder::AudioSource;

const PACKET_BUNDLE_MAGIC: &[u8; 8] = b"CBPKT001";
const FIXTURE_FPS: u32 = 60;
const AUDIO_SAMPLE_RATE: usize = 48_000;
const AUDIO_CHANNELS: usize = 2;
const AUDIO_SAMPLE_BYTES: usize = 2;
const AUDIO_SAMPLES_PER_FRAME: usize = AUDIO_SAMPLE_RATE / FIXTURE_FPS as usize;
const AUDIO_BYTES_PER_FRAME: usize =
    AUDIO_SAMPLES_PER_FRAME * AUDIO_CHANNELS * AUDIO_SAMPLE_BYTES;
const START_DELAY: Duration = Duration::from_millis(250);
const AUDIO_ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);
const FIXTURE_OUTPUT_SENTINEL: &str = "build/perf/qb-replay-012-native-mux-av";

fn required_env_path(name: &str) -> Result<PathBuf> {
    let value = env::var_os(name).with_context(|| format!("{name} is not set"))?;
    let path = PathBuf::from(value);
    ensure!(path.is_file(), "{name} is not a file: {}", path.display());
    Ok(path)
}

fn required_output_path(name: &str) -> Result<PathBuf> {
    let value = env::var_os(name).with_context(|| format!("{name} is not set"))?;
    let path = PathBuf::from(value);
    ensure!(!path.as_os_str().is_empty(), "{name} is empty");
    ensure!(path.is_absolute(), "{name} must be an absolute path");
    let file_name = path
        .file_name()
        .context("native mux fixture output has no file name")?;
    let parent = path
        .parent()
        .context("native mux fixture output has no parent directory")?;
    ensure!(
        parent.is_dir(),
        "native mux fixture output parent is not an existing directory: {}",
        parent.display()
    );
    ensure_reparse_free_chain(parent, "native mux fixture output parent")?;

    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("recorder manifest directory has no repository parent")?
        .canonicalize()
        .context("could not canonicalize repository root")?;
    let sentinel = repository_root.join(FIXTURE_OUTPUT_SENTINEL);
    ensure!(
        sentinel.is_dir(),
        "native mux fixture sentinel does not exist: {}",
        sentinel.display()
    );
    ensure_reparse_free_chain(&sentinel, "native mux fixture sentinel")?;
    let canonical_sentinel = sentinel
        .canonicalize()
        .context("could not canonicalize native mux fixture sentinel")?;
    let canonical_parent = parent
        .canonicalize()
        .context("could not canonicalize native mux fixture output parent")?;
    ensure!(
        canonical_parent != canonical_sentinel
            && canonical_parent.starts_with(&canonical_sentinel),
        "native mux fixture output must be inside a unique directory under {}",
        canonical_sentinel.display()
    );
    match fs::symlink_metadata(&path) {
        Ok(_) => bail!(
            "refusing to overwrite native mux fixture output {}",
            path.display()
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("could not inspect native mux fixture output {}", path.display())
            });
        }
    }
    Ok(canonical_parent.join(file_name))
}

fn ensure_reparse_free_chain(path: &Path, label: &str) -> Result<()> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let mut cursor = path;
    loop {
        let metadata = fs::symlink_metadata(cursor)
            .with_context(|| format!("could not inspect {label} component {}", cursor.display()))?;
        ensure!(
            !metadata.file_type().is_symlink()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "{label} traverses a reparse point at {}",
            cursor.display()
        );
        let Some(parent) = cursor.parent() else {
            return Ok(());
        };
        if parent == cursor {
            return Ok(());
        }
        cursor = parent;
    }
}

fn read_u32_le(cursor: &mut Cursor<Vec<u8>>, label: &str) -> Result<u32> {
    let mut bytes = [0_u8; 4];
    cursor
        .read_exact(&mut bytes)
        .with_context(|| format!("packet bundle ended while reading {label}"))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_packet_bundle(path: &Path) -> Result<Vec<Vec<u8>>> {
    let bytes = fs::read(path)
        .with_context(|| format!("could not read packet bundle {}", path.display()))?;
    let total_len = bytes.len() as u64;
    let mut cursor = Cursor::new(bytes);
    let mut magic = [0_u8; 8];
    cursor
        .read_exact(&mut magic)
        .context("packet bundle is missing its magic")?;
    ensure!(&magic == PACKET_BUNDLE_MAGIC, "packet bundle magic is invalid");
    let count = usize::try_from(read_u32_le(&mut cursor, "packet count")?)
        .context("packet count does not fit usize")?;
    ensure!(count >= 12, "packet bundle must contain at least 12 access units");
    ensure!(count <= 10_000, "packet bundle contains too many access units");

    let mut packets = Vec::with_capacity(count);
    for index in 0..count {
        let length = usize::try_from(read_u32_le(&mut cursor, "packet length")?)
            .context("packet length does not fit usize")?;
        ensure!(length > 0, "packet {index} is empty");
        ensure!(length <= 16 * 1024 * 1024, "packet {index} is unreasonably large");
        let mut packet = vec![0_u8; length];
        cursor
            .read_exact(&mut packet)
            .with_context(|| format!("packet bundle ended inside packet {index}"))?;
        packets.push(packet);
    }
    ensure!(
        cursor.position() == total_len,
        "packet bundle has trailing bytes"
    );
    Ok(packets)
}

fn sleep_until(target: Instant) {
    if let Some(delay) = target.checked_duration_since(Instant::now()) {
        thread::sleep(delay);
    }
}

fn frame_target(start: Instant, frame: usize) -> Instant {
    start
        + Duration::from_nanos(
            u64::try_from(frame).unwrap_or(u64::MAX).saturating_mul(1_000_000_000)
                / u64::from(FIXTURE_FPS),
        )
}

fn spawn_paced_audio(
    listener: TcpListener,
    pcm: Vec<u8>,
    accepted: mpsc::Sender<()>,
    start: mpsc::Receiver<Instant>,
) -> thread::JoinHandle<Result<()>> {
    thread::spawn(move || {
        let (mut stream, peer) = listener
            .accept()
            .context("could not accept native mux fixture audio consumer")?;
        ensure!(peer.ip().is_loopback(), "fixture audio consumer is not loopback");
        stream
            .set_nodelay(true)
            .context("could not configure native mux fixture audio socket")?;
        accepted
            .send(())
            .context("could not publish native mux fixture audio acceptance")?;
        let start_at = start
            .recv_timeout(AUDIO_ACCEPT_TIMEOUT)
            .context("native mux fixture never received its common start instant")?;

        for (frame, chunk) in pcm.chunks_exact(AUDIO_BYTES_PER_FRAME).enumerate() {
            sleep_until(frame_target(start_at, frame));
            stream
                .write_all(chunk)
                .with_context(|| format!("could not write audio frame {frame}"))?;
        }
        ensure!(
            pcm.chunks_exact(AUDIO_BYTES_PER_FRAME).remainder().is_empty(),
            "fixture PCM is not frame aligned"
        );
        stream.flush().context("could not flush fixture audio socket")?;
        Ok(())
    })
}

fn find_exact_window(arguments: &[OsString], expected: &[&str]) -> Option<usize> {
    arguments.windows(expected.len()).position(|window| {
        window
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual == OsStr::new(expected))
    })
}

fn replace_fixture_video_input(plan: &mut NativeMuxPlan, mode: &str) -> Result<()> {
    let fps = FIXTURE_FPS.to_string();
    let production_v6 = [
        "-use_wallclock_as_timestamps",
        "1",
        "-r",
        fps.as_str(),
        "-f",
        "h264",
        "-i",
        "pipe:0",
    ];
    let start = find_exact_window(&plan.arguments, &production_v6)
        .context("fixture could not locate the exact v6 H.264 input window")?;
    let replacement: Vec<OsString> = match mode {
        "baseline" | "skew" => ["-r", fps.as_str(), "-f", "h264", "-i", "pipe:0"]
            .into_iter()
            .map(OsString::from)
            .collect(),
        "wallclock" => [
            "-use_wallclock_as_timestamps",
            "1",
            "-r",
            fps.as_str(),
            "-f",
            "h264",
            "-i",
            "pipe:0",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        other => bail!("unsupported native mux fixture mode {other:?}"),
    };
    plan.arguments
        .splice(start..start + production_v6.len(), replacement);

    for removed in [
        "-fflags",
        "nobuffer",
        "-probesize",
        "-analyzeduration",
        "-fpsprobesize",
    ] {
        ensure!(
            !plan
                .arguments
                .iter()
                .any(|argument| argument == OsStr::new(removed)),
            "fixture candidate unexpectedly retained {removed}"
        );
    }
    let expected = if mode == "wallclock" {
        vec![
            "-use_wallclock_as_timestamps",
            "1",
            "-r",
            fps.as_str(),
            "-f",
            "h264",
            "-i",
            "pipe:0",
        ]
    } else {
        vec!["-r", fps.as_str(), "-f", "h264", "-i", "pipe:0"]
    };
    ensure!(
        find_exact_window(&plan.arguments, &expected).is_some(),
        "fixture candidate H.264 input window is not exact"
    );
    Ok(())
}

fn inject_eight_ms_late_marker_skew(plan: &mut NativeMuxPlan, frame_count: usize) -> Result<()> {
    let late_frame = (5 * frame_count) / 6;
    let samples_per_frame = AUDIO_SAMPLE_RATE / FIXTURE_FPS as usize;
    let skew_samples = 384; // exactly 8 ms at 48 kHz
    let original = format!(
        "setts=pts=N:dts=N:duration=1:time_base=1/{FIXTURE_FPS}:prescale=1"
    );
    let skewed = format!(
        "setts=pts=N*{samples_per_frame}+not(N-{late_frame})*{skew_samples}:\
         dts=N*{samples_per_frame}+not(N-{late_frame})*{skew_samples}:\
         duration={samples_per_frame}:time_base=1/{AUDIO_SAMPLE_RATE}:prescale=1"
    )
    .replace(' ', "");
    let Some(index) = plan
        .arguments
        .windows(2)
        .position(|pair| pair[0] == OsStr::new("-bsf:v") && pair[1] == OsStr::new(original.as_str()))
    else {
        bail!("fixture could not locate production native setts bitstream filter");
    };
    plan.arguments[index + 1] = skewed.into();
    Ok(())
}

fn run_native_mux_fixture() -> Result<()> {
    let mode = env::var("QUEUEBACK_NATIVE_MUX_FIXTURE_MODE")
        .context("QUEUEBACK_NATIVE_MUX_FIXTURE_MODE is not set")?;
    ensure!(
        matches!(mode.as_str(), "baseline" | "wallclock" | "skew"),
        "fixture mode must be baseline, wallclock, or skew"
    );
    let ffmpeg = required_env_path("QUEUEBACK_NATIVE_MUX_FIXTURE_FFMPEG")?;
    let packet_bundle = required_env_path("QUEUEBACK_NATIVE_MUX_FIXTURE_PACKETS")?;
    let pcm_path = required_env_path("QUEUEBACK_NATIVE_MUX_FIXTURE_PCM")?;
    let output = required_output_path("QUEUEBACK_NATIVE_MUX_FIXTURE_OUTPUT")?;

    let packets = read_packet_bundle(&packet_bundle)?;
    let pcm = fs::read(&pcm_path)
        .with_context(|| format!("could not read fixture PCM {}", pcm_path.display()))?;
    let expected_pcm_bytes = packets
        .len()
        .checked_mul(AUDIO_BYTES_PER_FRAME)
        .context("fixture PCM size overflow")?;
    ensure!(
        pcm.len() == expected_pcm_bytes,
        "fixture PCM length {} does not match {} video access units (expected {expected_pcm_bytes})",
        pcm.len(),
        packets.len()
    );

    let listener = TcpListener::bind(("127.0.0.1", 0))
        .context("could not bind native mux fixture PCM endpoint")?;
    let address = listener
        .local_addr()
        .context("could not resolve native mux fixture PCM endpoint")?;
    let endpoint = format!("tcp://127.0.0.1:{}", address.port());
    let (accepted_tx, accepted_rx) = mpsc::channel();
    let (start_tx, start_rx) = mpsc::channel();
    let audio_thread = spawn_paced_audio(listener, pcm, accepted_tx, start_rx);

    let mut plan = NativeMuxPlan::h264(
        FIXTURE_FPS,
        &AudioSource::ReplayTimeFixturePcm(endpoint),
        &output,
    )?;
    replace_fixture_video_input(&mut plan, &mode)?;
    if mode == "skew" {
        inject_eight_ms_late_marker_skew(&mut plan, packets.len())?;
    }

    let mut mux = NativeMuxProcess::start(&ffmpeg, &plan)?;
    let mut writer = mux.take_video_writer()?;
    accepted_rx
        .recv_timeout(AUDIO_ACCEPT_TIMEOUT)
        .context("FFmpeg did not connect to the fixture PCM endpoint")?;

    let start_at = Instant::now() + START_DELAY;
    start_tx
        .send(start_at)
        .context("could not publish common native mux fixture start")?;
    for (frame, packet) in packets.iter().enumerate() {
        sleep_until(frame_target(start_at, frame));
        writer
            .write_all(packet)
            .with_context(|| format!("could not write H.264 access unit {frame}"))?;
    }
    writer.flush().context("could not final-flush fixture video pipe")?;
    drop(writer);

    audio_thread
        .join()
        .map_err(|_| anyhow::anyhow!("native mux fixture audio thread panicked"))??;
    let telemetry = mux.finish()?;
    ensure!(telemetry.output_file_bytes > 0, "native mux fixture produced no MP4 bytes");
    ensure!(
        telemetry.encoded_frames == packets.len() as u64,
        "native mux fixture FFmpeg progress reported {} frames for {} input access units",
        telemetry.encoded_frames,
        packets.len()
    );
    println!(
        "CHRONOBREAK_NATIVE_MUX_FIXTURE_PASS mode={} packets={} mux_frames={} output_bytes={} writer_calls={}",
        mode,
        packets.len(),
        telemetry.encoded_frames,
        telemetry.output_file_bytes,
        telemetry.video_writer_calls,
    );
    Ok(())
}

#[test]
#[ignore = "requires an explicit packaged FFmpeg path and generated replay-time fixture"]
fn native_mux_av_fixture() {
    run_native_mux_fixture().expect("native mux A/V fixture failed");
}
