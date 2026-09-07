from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
from datetime import datetime
from fractions import Fraction
import wave

ROOT = Path(__file__).resolve().parents[2]
FIXTURE_TOOLS_PATH = ROOT / "tools" / "replay_time" / "fixture_tools.py"
PACKET_BUNDLE_MAGIC = b"CBPKT001"
FPS = 60
SAMPLE_RATE = 48_000
SAMPLES_PER_FRAME = SAMPLE_RATE // FPS
FRAME_COUNT = 360
WIDTH = 256
HEIGHT = 192
AUDIO_ID_TARGETS = {"early": 10_000, "middle": 20_000, "late": 30_000}
OUTPUT_SENTINEL = ROOT / "build" / "perf" / "qb-replay-012-native-mux-av"


def load_fixture_tools():
    spec = importlib.util.spec_from_file_location("chronobreak_fixture_tools", FIXTURE_TOOLS_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Could not load {FIXTURE_TOOLS_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ft = load_fixture_tools()


def tool_from_runtime(runtime_root: Path, stem: str) -> Path:
    candidates = [runtime_root / "bin" / f"{stem}.exe", runtime_root / "bin" / stem]
    for candidate in candidates:
        if candidate.is_file():
            return candidate.resolve()
    raise RuntimeError(
        f"Could not find {stem} under {runtime_root / 'bin'}; pass the staged packaged runtime root."
    )


def run_logged(command: list[str], *, cwd: Path, log: Path, env: dict[str, str] | None = None) -> None:
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w", encoding="utf-8", newline="") as output:
        process = subprocess.run(
            command,
            cwd=cwd,
            env=env,
            stdout=output,
            stderr=subprocess.STDOUT,
            text=True,
        )
    if process.returncode != 0:
        lines = log.read_text(encoding="utf-8", errors="replace").splitlines()
        tail = "\n".join(lines[-80:])
        raise RuntimeError(
            f"Command failed with exit {process.returncode}: {' '.join(command)}\n"
            f"--- tail of {log} ---\n{tail}"
        )


def run_capture_json(command: list[str], *, cwd: Path) -> dict:
    process = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if process.returncode != 0:
        raise RuntimeError(
            f"Command failed with exit {process.returncode}: {' '.join(command)}\n"
            f"{process.stderr[-8000:]}"
        )
    return json.loads(process.stdout)


def generate_source(source: Path, ffmpeg: Path, ffprobe: Path) -> tuple[Path, Path, Path]:
    source.mkdir(parents=True, exist_ok=True)
    yuv = source / "source.yuv"
    wav = source / "source.wav"
    manifest = source / "manifest.json"
    h264 = source / "source.h264"
    packet_probe = source / "source-packets.json"
    packet_bundle = source / "source.packets"
    pcm = source / "source-stereo.s16le"

    ft.command_generate(
        argparse.Namespace(
            video=str(yuv),
            audio=str(wav),
            manifest=str(manifest),
            width=WIDTH,
            height=HEIGHT,
            frames=FRAME_COUNT,
            rate="60/1",
            result=None,
        )
    )

    run_logged(
        [
            str(ffmpeg),
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "yuv420p",
            "-video_size",
            f"{WIDTH}x{HEIGHT}",
            "-framerate",
            str(FPS),
            "-i",
            str(yuv),
            "-frames:v",
            str(FRAME_COUNT),
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-tune",
            "zerolatency",
            "-profile:v",
            "high",
            "-qp",
            "1",
            "-x264-params",
            "repeat-headers=1:aud=1:keyint=120:min-keyint=120:scenecut=0:bframes=0",
            "-f",
            "h264",
            "-y",
            str(h264),
        ],
        cwd=ROOT,
        log=source / "encode-source.log",
    )

    packet_json = run_capture_json(
        [
            str(ffprobe),
            "-v",
            "error",
            "-f",
            "h264",
            "-show_packets",
            "-show_entries",
            "packet=pos,size",
            "-of",
            "json",
            str(h264),
        ],
        cwd=ROOT,
    )
    packet_probe.write_text(json.dumps(packet_json, indent=2), encoding="utf-8")
    packets = packet_json.get("packets")
    if not isinstance(packets, list) or len(packets) != FRAME_COUNT:
        raise RuntimeError(
            f"Expected {FRAME_COUNT} H.264 packets/access units, got "
            f"{len(packets) if isinstance(packets, list) else 'invalid packet JSON'}"
        )
    h264_bytes = h264.read_bytes()
    slices: list[bytes] = []
    sequential = 0
    for index, packet in enumerate(packets):
        size = int(packet["size"])
        pos_value = packet.get("pos")
        pos = int(pos_value) if pos_value not in (None, "N/A") else sequential
        if size <= 0 or pos < 0 or pos + size > len(h264_bytes):
            raise RuntimeError(f"Invalid raw-H.264 packet extent at packet {index}: pos={pos} size={size}")
        slices.append(h264_bytes[pos : pos + size])
        sequential = pos + size
    if sequential != len(h264_bytes):
        raise RuntimeError(
            f"Raw H.264 packet extents end at {sequential}, file has {len(h264_bytes)} bytes"
        )
    with packet_bundle.open("wb") as output:
        output.write(PACKET_BUNDLE_MAGIC)
        output.write(struct.pack("<I", len(slices)))
        for packet in slices:
            output.write(struct.pack("<I", len(packet)))
            output.write(packet)

    with wave.open(str(wav), "rb") as reader:
        if (
            reader.getnchannels() != 1
            or reader.getsampwidth() != 2
            or reader.getframerate() != SAMPLE_RATE
        ):
            raise RuntimeError("Generated source WAV is not mono 48 kHz s16le")
        mono = reader.readframes(reader.getnframes())
    if len(mono) != FRAME_COUNT * SAMPLES_PER_FRAME * 2:
        raise RuntimeError("Generated source WAV has unexpected sample coverage")

    manifest_value = json.loads(manifest.read_text(encoding="utf-8"))
    sample_count = FRAME_COUNT * SAMPLES_PER_FRAME
    right = [0] * sample_count
    identity_manifest = {"sample_rate": SAMPLE_RATE, "markers": []}
    for marker in manifest_value["markers"]:
        name = str(marker["name"])
        start = int(marker["sample_index"])
        target_peak = AUDIO_ID_TARGETS[name]
        identity_manifest["markers"].append(
            {"name": name, "sample_index": start, "target_peak": target_peak}
        )
        for impulse_offset in range(ft.IMPULSE_LENGTH_SAMPLES):
            sample_index = start + impulse_offset
            if sample_index >= sample_count:
                break
            value = ft._impulse_value(impulse_offset)
            right[sample_index] = round(value * target_peak / ft.IMPULSE_AMPLITUDE)

    stereo = bytearray(sample_count * 4)
    for sample_index in range(sample_count):
        left_sample = mono[sample_index * 2 : sample_index * 2 + 2]
        struct.pack_into(
            "<hh",
            stereo,
            sample_index * 4,
            struct.unpack("<h", left_sample)[0],
            right[sample_index],
        )
    pcm.write_bytes(stereo)
    (source / "audio-identity.json").write_text(
        json.dumps(identity_manifest, indent=2), encoding="utf-8"
    )

    return packet_bundle, pcm, manifest


def run_verifier_unit_controls(out: Path) -> None:
    print("[1/3] Verifier controls: constant offset invariance / +6 ms / signed drift / lineage swap")
    run_logged(
        [
            sys.executable,
            "-m",
            "unittest",
            "tools.replay_time.tests.test_fixture_tools.NativeMuxMarkerVerifierTests",
            "-v",
        ],
        cwd=ROOT,
        log=out / "verifier-unit-controls.log",
    )


def run_rust_mux_arm(
    *,
    mode: str,
    ffmpeg: Path,
    packet_bundle: Path,
    pcm: Path,
    output: Path,
    log: Path,
) -> None:
    environment = os.environ.copy()
    environment.update(
        {
            "QUEUEBACK_NATIVE_MUX_FIXTURE_MODE": mode,
            "QUEUEBACK_NATIVE_MUX_FIXTURE_FFMPEG": str(ffmpeg),
            "QUEUEBACK_NATIVE_MUX_FIXTURE_PACKETS": str(packet_bundle),
            "QUEUEBACK_NATIVE_MUX_FIXTURE_PCM": str(pcm),
            "QUEUEBACK_NATIVE_MUX_FIXTURE_OUTPUT": str(output),
            "RUST_BACKTRACE": "1",
        }
    )
    run_logged(
        [
            "cargo",
            "test",
            "--manifest-path",
            "recorder/Cargo.toml",
            "--lib",
            "--features",
            "replay-time-fixture",
            "native_mux_av_fixture",
            "--",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ],
        cwd=ROOT,
        log=log,
        env=environment,
    )


def decode_and_probe(*, ffmpeg: Path, ffprobe: Path, media: Path, arm: Path) -> tuple[Path, Path, Path, Path]:
    decoded_video = arm / "decoded.yuv"
    decoded_audio = arm / "decoded.wav"
    decoded_audio_id = arm / "decoded-audio-id.wav"
    frame_probe = arm / "frames.ffprobe.json"

    run_logged(
        [
            str(ffmpeg),
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            str(media),
            "-map",
            "0:v:0",
            "-pix_fmt",
            "yuv420p",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "-y",
            str(decoded_video),
        ],
        cwd=ROOT,
        log=arm / "decode-video.log",
    )
    run_logged(
        [
            str(ffmpeg),
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            str(media),
            "-map",
            "0:a:0",
            "-af",
            "pan=mono|c0=FL",
            "-ar",
            str(SAMPLE_RATE),
            "-c:a",
            "pcm_s16le",
            "-y",
            str(decoded_audio),
        ],
        cwd=ROOT,
        log=arm / "decode-audio.log",
    )
    run_logged(
        [
            str(ffmpeg),
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            str(media),
            "-map",
            "0:a:0",
            "-af",
            "pan=mono|c0=FR",
            "-ar",
            str(SAMPLE_RATE),
            "-c:a",
            "pcm_s16le",
            "-y",
            str(decoded_audio_id),
        ],
        cwd=ROOT,
        log=arm / "decode-audio-id.log",
    )
    probe = run_capture_json(
        [
            str(ffprobe),
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_streams",
            "-show_frames",
            "-show_entries",
            "frame=media_type,best_effort_timestamp:stream=codec_type,time_base",
            "-of",
            "json",
            str(media),
        ],
        cwd=ROOT,
    )
    frame_probe.write_text(json.dumps(probe, indent=2), encoding="utf-8")
    return decoded_video, decoded_audio, decoded_audio_id, frame_probe


def verify_audio_identity(decoded_audio_id: Path, manifest: Path) -> dict:
    manifest_value = json.loads(manifest.read_text(encoding="utf-8"))
    with wave.open(str(decoded_audio_id), "rb") as reader:
        if (
            reader.getnchannels() != 1
            or reader.getsampwidth() != 2
            or reader.getframerate() != SAMPLE_RATE
        ):
            raise RuntimeError("Decoded audio identity channel is not mono 48 kHz s16le")
        frames = reader.getnframes()
        peaks: dict[str, dict[str, int]] = {}
        for marker in manifest_value["markers"]:
            name = str(marker["name"])
            expected = int(marker["sample_index"])
            first = max(0, expected - ft.MAX_AV_DISAGREEMENT_SAMPLES)
            last = min(frames, expected + ft.MAX_AV_DISAGREEMENT_SAMPLES + 1)
            reader.setpos(first)
            payload = reader.readframes(last - first)
            best_peak = -1
            best_sample = -1
            for offset in range(last - first):
                value = abs(struct.unpack_from("<h", payload, offset * 2)[0])
                if value > best_peak:
                    best_peak = value
                    best_sample = first + offset
            if best_peak < ft.IMPULSE_THRESHOLD:
                raise RuntimeError(f"Audio identity marker {name} is missing")
            peaks[name] = {"sample": best_sample, "peak": best_peak}

    ordered = [peaks[name]["peak"] for name in ("early", "middle", "late")]
    if not (ordered[0] + 2_000 < ordered[1] and ordered[1] + 2_000 < ordered[2]):
        raise RuntimeError(
            "Decoded audio identity markers are not independently distinguishable: "
            f"early/middle/late peaks={ordered}"
        )
    return peaks


def exact_grid(frame_probe: Path) -> dict:
    probe = json.loads(frame_probe.read_text(encoding="utf-8"))
    time_base, pts = ft._native_video_pts(probe)
    if len(pts) != FRAME_COUNT:
        raise RuntimeError(f"Expected {FRAME_COUNT} finalized frames, got {len(pts)}")
    step_exact = Fraction(1, FPS) / time_base
    if step_exact.denominator != 1:
        raise RuntimeError(f"Finalized video time base {time_base} cannot represent exact 60 Hz")
    step = step_exact.numerator
    steps = {later - earlier for earlier, later in zip(pts, pts[1:])}
    if pts[0] != 0 or steps != {step}:
        raise RuntimeError(
            f"Finalized video grid is not exact zero-based 60 Hz: first={pts[0]} steps={sorted(steps)} expected={step}"
        )
    return {
        "time_base": str(time_base),
        "first_pts": pts[0],
        "last_pts": pts[-1],
        "step_pts": step,
        "frames": len(pts),
    }


def verify_arm(*, mode: str, media: Path, arm: Path, manifest: Path, ffmpeg: Path, ffprobe: Path) -> dict:
    decoded_video, decoded_audio, decoded_audio_id, frame_probe = decode_and_probe(
        ffmpeg=ffmpeg, ffprobe=ffprobe, media=media, arm=arm
    )
    result_path = arm / "verify-native-mux-media.result.json"
    verifier_mode = "wallclock" if mode == "wallclock" else "cfr"
    result = ft.command_verify_native_mux_media(
        argparse.Namespace(
            source_media=str(media),
            video=str(decoded_video),
            audio=str(decoded_audio),
            probe=str(frame_probe),
            manifest=str(manifest),
            input_timestamp_mode=verifier_mode,
            result=str(result_path),
        )
    )
    grid = exact_grid(frame_probe)
    result["exact_grid"] = grid
    result["audio_marker_identity"] = verify_audio_identity(decoded_audio_id, manifest)
    return result


def measurement_map(result: dict) -> dict[str, dict]:
    return {str(item["name"]): item for item in result["measurements"]}


def concise_result(result: dict) -> dict:
    measurements = measurement_map(result)
    return {
        "passed": bool(result["passed"]),
        "maximum_observed_drift_samples": int(result["maximum_observed_drift_samples"]),
        "maximum_observed_drift_ms": result["maximum_observed_drift_samples"] * 1000 / SAMPLE_RATE,
        "early_av_ms": measurements["early"]["av_disagreement_ms"],
        "middle_av_ms": measurements["middle"]["av_disagreement_ms"],
        "late_av_ms": measurements["late"]["av_disagreement_ms"],
        "middle_drift_ms": measurements["middle"]["drift_from_early_ms"],
        "late_drift_ms": measurements["late"]["drift_from_early_ms"],
        "exact_grid": result["exact_grid"],
        "audio_marker_identity": result["audio_marker_identity"],
    }


def run_negative_control(
    *,
    root: Path,
    ffmpeg: Path,
    ffprobe: Path,
    packet_bundle: Path,
    pcm: Path,
    manifest: Path,
) -> dict:
    print("[2/3] Real-mux negative control: inject +8 ms only at the late video marker; verifier must reject drift")
    arm = root / "negative-8ms-video-skew"
    arm.mkdir()
    media = arm / "video.mp4"
    run_rust_mux_arm(
        mode="skew",
        ffmpeg=ffmpeg,
        packet_bundle=packet_bundle,
        pcm=pcm,
        output=media,
        log=arm / "cargo-native-mux.log",
    )
    try:
        verify_arm(
            mode="skew",
            media=media,
            arm=arm,
            manifest=manifest,
            ffmpeg=ffmpeg,
            ffprobe=ffprobe,
        )
    except ft.FixtureError as error:
        message = str(error)
        if "5 ms drift" not in message:
            raise RuntimeError(
                "The injected real-mux skew failed, but not at the signed 5 ms drift gate: " + message
            ) from error
        (arm / "expected-failure.txt").write_text(message + "\n", encoding="utf-8")
        return {"passed": True, "expected_error": message}
    raise RuntimeError("The real-mux +8 ms late-marker skew unexpectedly passed")


def run_ab_pairs(
    *,
    root: Path,
    pairs: int,
    ffmpeg: Path,
    ffprobe: Path,
    packet_bundle: Path,
    pcm: Path,
    manifest: Path,
) -> list[dict]:
    print(f"[3/3] Actual A/B: {pairs} paced baseline/wallclock pair(s), 6 s each")
    results: list[dict] = []
    for pair in range(1, pairs + 1):
        for mode in ("baseline", "wallclock"):
            print(f"  pair {pair}/{pairs} {mode} ...", flush=True)
            arm = root / f"pair-{pair:02d}-{mode}"
            arm.mkdir()
            media = arm / "video.mp4"
            run_rust_mux_arm(
                mode=mode,
                ffmpeg=ffmpeg,
                packet_bundle=packet_bundle,
                pcm=pcm,
                output=media,
                log=arm / "cargo-native-mux.log",
            )
            verified = verify_arm(
                mode=mode,
                media=media,
                arm=arm,
                manifest=manifest,
                ffmpeg=ffmpeg,
                ffprobe=ffprobe,
            )
            item = {"pair": pair, "mode": mode, **concise_result(verified)}
            results.append(item)
            print(
                "    PASS | max drift "
                f"{item['maximum_observed_drift_samples']} samples | "
                f"A/V early/mid/late = {item['early_av_ms']}, {item['middle_av_ms']}, {item['late_av_ms']} ms"
            )
    return results


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run the controlled post-capture native-mux A/V proof through the real "
            "NativeMuxPlan/NativeMuxProcess and packaged FFmpeg."
        )
    )
    parser.add_argument(
        "--runtime-root",
        default="build/media-runtime/windows-x86_64",
        help="staged packaged runtime root containing bin/ffmpeg.exe and bin/ffprobe.exe",
    )
    parser.add_argument("--pairs", type=int, default=3)
    parser.add_argument(
        "--output-root",
        default="build/perf/qb-replay-012-native-mux-av",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if os.name != "nt":
        raise SystemExit("This fixture must be run on Windows because NativeMuxProcess is Windows-only.")
    if args.pairs < 1 or args.pairs > 10:
        raise SystemExit("--pairs must be in 1..=10")

    runtime_root = (ROOT / args.runtime_root).resolve() if not Path(args.runtime_root).is_absolute() else Path(args.runtime_root).resolve()
    ffmpeg = tool_from_runtime(runtime_root, "ffmpeg")
    ffprobe = tool_from_runtime(runtime_root, "ffprobe")
    requested_output_base = (
        (ROOT / args.output_root)
        if not Path(args.output_root).is_absolute()
        else Path(args.output_root)
    )
    output_base = requested_output_base.resolve()
    output_sentinel = OUTPUT_SENTINEL.resolve()
    if output_base != output_sentinel and output_sentinel not in output_base.parents:
        raise SystemExit(
            f"--output-root must be the dedicated sentinel or its descendant: {output_sentinel}"
        )
    ft._reject_reparse_chain(requested_output_base, "native mux output sentinel")
    output_base.mkdir(parents=True, exist_ok=True)
    ft._reject_reparse_chain(output_base, "native mux output sentinel")
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    run_root = output_base / stamp
    run_root.mkdir()

    print(f"Run root: {run_root}")
    print(f"FFmpeg:   {ffmpeg}")
    print(f"ffprobe:  {ffprobe}")

    run_verifier_unit_controls(run_root)
    source = run_root / "source"
    packet_bundle, pcm, manifest = generate_source(source, ffmpeg, ffprobe)
    negative = run_negative_control(
        root=run_root,
        ffmpeg=ffmpeg,
        ffprobe=ffprobe,
        packet_bundle=packet_bundle,
        pcm=pcm,
        manifest=manifest,
    )
    results = run_ab_pairs(
        root=run_root,
        pairs=args.pairs,
        ffmpeg=ffmpeg,
        ffprobe=ffprobe,
        packet_bundle=packet_bundle,
        pcm=pcm,
        manifest=manifest,
    )

    baseline = [item for item in results if item["mode"] == "baseline"]
    wallclock = [item for item in results if item["mode"] == "wallclock"]
    summary = {
        "schema_version": 1,
        "run_root": str(run_root),
        "fixture": {
            "frames": FRAME_COUNT,
            "fps": FPS,
            "seconds": FRAME_COUNT / FPS,
            "video_feed": "controlled real-time 60 Hz access-unit pacing",
            "audio_feed": "controlled real-time 48 kHz stereo PCM, 800 samples/channel per video tick",
            "capture_path_in_scope": False,
            "production_mux_process_in_scope": True,
        },
        "negative_control": negative,
        "results": results,
        "baseline_passes": sum(bool(item["passed"]) for item in baseline),
        "wallclock_passes": sum(bool(item["passed"]) for item in wallclock),
        "pairs": args.pairs,
    }
    summary_path = run_root / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")

    print("\n=== RESULT ===")
    print(f"Verifier controls: PASS")
    print(f"Real-mux +8 ms negative control: PASS (rejected as expected)")
    print(f"Baseline:  {summary['baseline_passes']}/{args.pairs} pass")
    print(f"Wallclock: {summary['wallclock_passes']}/{args.pairs} pass")
    print(f"Summary:   {summary_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
