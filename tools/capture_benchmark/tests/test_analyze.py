from __future__ import annotations

import csv
import hashlib
import json
import math
import shutil
import tempfile
import unittest
from datetime import UTC, datetime, timedelta
from pathlib import Path

from tools.capture_benchmark import analyze


FIXTURES = Path(__file__).parent / "fixtures"
MIB = 1024 * 1024
GIB = 1024 * MIB


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_presentmon(path: Path, interval_ms: float) -> None:
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(
            [
                "Application",
                "ProcessID",
                "SwapChainAddress",
                "CPUStartQPCTime",
                "MsBetweenPresents",
                "DisplayedTime",
            ]
        )
        timestamp = 0.0
        row_number = 0
        while timestamp <= 180_000.0:
            writer.writerow(
                [
                    "League of Legends.exe",
                    1000,
                    "0xprimary",
                    timestamp,
                    0 if row_number == 0 else interval_ms,
                    interval_ms,
                ]
            )
            timestamp += interval_ms
            row_number += 1


def process_row(
    role: str,
    pid: int,
    second: int,
    cpu_raw_percent: float,
    private_bytes: int,
    io_write_per_second: int,
) -> dict[str, int | str]:
    timestamp = second * 10_000_000
    return {
        "role": role,
        "pid": pid,
        "processor_time_raw": int(second * 10_000_000 * cpu_raw_percent / 100),
        "timestamp_100ns": timestamp,
        "working_set_bytes": private_bytes + 16 * MIB,
        "private_bytes": private_bytes,
        "io_read_bytes_raw": second * 1000,
        "io_write_bytes_raw": second * io_write_per_second,
        "io_other_bytes_raw": second * 500,
        "timestamp_perf": timestamp,
        "frequency_perf": 10_000_000,
    }


def write_telemetry(path: Path, capture: bool, missing_source: str | None = None) -> None:
    total_memory = 32 * GIB
    league_gpu_raw = 0
    encode_gpu_raw = 0
    with path.open("w", encoding="utf-8", newline="\n") as handle:
        for second in range(181):
            if second:
                league_gpu_raw += (72 if capture else 70) * 100_000
                if capture:
                    encode_gpu_raw += 20 * 100_000
            processes = [process_row("league", 1000, second, 240, 800 * MIB, 1_000_000)]
            gpu_engines = [
                {
                    "name": "pid_1000_luid_0xA_0xB_phys_0_eng_0_engtype_3D",
                    "utilization_raw": league_gpu_raw,
                    "timestamp_100ns": second * 10_000_000,
                }
            ]
            if capture:
                processes.extend(
                    [
                        process_row("recorder", 2000, second, 12, 70 * MIB + second * 1024, 10_000),
                        process_row("ffmpeg", 3000, second, 24, 120 * MIB + second * 1024, 1_000_000),
                    ]
                )
                gpu_engines.append(
                    {
                        "name": "pid_3000_luid_0xA_0xB_phys_0_eng_1_engtype_VideoEncode",
                        "utilization_raw": encode_gpu_raw,
                        "timestamp_100ns": second * 10_000_000,
                    }
                )
            query_status = {
                "system_cpu": True,
                "system_memory": True,
                "processes": True,
                "gpu_engine": True,
                "gpu_adapter_memory": True,
                "gpu_process_memory": True,
            }
            if missing_source is not None:
                query_status[missing_source] = False
            sample = {
                "schema_version": "1",
                "sequence": second,
                "utc": (datetime(2026, 8, 11, tzinfo=UTC) + timedelta(seconds=second)).isoformat().replace(
                    "+00:00", "Z"
                ),
                "elapsed_seconds": second,
                "query_status": query_status,
                "query_errors": (
                    [f"{missing_source}:FixtureError"] if missing_source is not None else []
                ),
                "system": {
                    "processor_idle_raw": second * 5_000_000,
                    "processor_timestamp_100ns": second * 10_000_000,
                    "available_memory_bytes": 19 * GIB if capture else 20 * GIB,
                    "total_memory_bytes": total_memory,
                    "committed_memory_bytes": 10 * GIB,
                },
                "processes": processes,
                "gpu_engines": gpu_engines,
                "gpu_adapter_memory": [
                    {
                        "name": "luid_0xA_0xB_phys_0",
                        "dedicated_bytes": (2600 if capture else 2400) * MIB,
                        "shared_bytes": (500 if capture else 450) * MIB,
                    }
                ],
                "gpu_process_memory": [],
                "output": (
                    {
                        "relative_path": "library/games/fixture/video.mp4",
                        "size_bytes": (100 + second) * MIB,
                    }
                    if capture
                    else None
                ),
            }
            handle.write(json.dumps(sample, sort_keys=True, separators=(",", ":")) + "\n")


def make_manifest(
    frame_mode: str,
    condition: str,
    run_number: int,
    sequence: int,
    presentmon: Path,
    telemetry: Path,
    path_leak: bool = False,
) -> dict:
    capture = condition == "capture"
    started = datetime(2026, 8, 11, tzinfo=UTC) + timedelta(minutes=sequence * 5)
    gpu_name = "NVIDIA GeForce RTX 4060"
    os_identity = {"caption": "Windows 11", "version": "10.0.26100"}
    if path_leak:
        os_identity[r"C:\Users\Alice\private\diagnostic-key"] = "fixture"
    manifest = {
        "schema_version": "1",
        "run_id": f"{frame_mode}-{condition}-{run_number}",
        "frame_mode": frame_mode,
        "condition": condition,
        "run_number": run_number,
        "timing": {
            "warmup_seconds": 60,
            "measurement_seconds": 180,
            "measurement_elapsed_seconds": 180,
            "sample_interval_seconds": 1,
            "measurement_started_utc": started.isoformat().replace("+00:00", "Z"),
            "measurement_ended_utc": (started + timedelta(seconds=180)).isoformat().replace("+00:00", "Z"),
        },
        "target": {
            "process_name": "League of Legends.exe",
            "configured_fps_limit": 144 if frame_mode == "capped" else None,
            "protocol_attested": True,
        },
        "configuration": {
            "league_config_sha256": "capped-config" if frame_mode == "capped" else "uncapped-config",
            "league_config_sha256_after": "capped-config" if frame_mode == "capped" else "uncapped-config",
            "width": 1920,
            "height": 1080,
            "display_mode": "borderless",
            "vsync": False,
        },
        "processes": {
            "league": 1000,
            "recorder": 2000 if capture else None,
            "ffmpeg": 3000 if capture else None,
        },
        "environment": {
            "system_fingerprint_sha256": "fixture-system",
            "os": os_identity,
            "cpu": {"name": "AMD Ryzen 5 5600X", "logical_processors": 12},
            "gpus": [{"name": gpu_name, "driver_version": "fixture-driver"}],
            "display": {
                "width": 1920,
                "height": 1080,
                "refresh_hz": 144,
                "adapter_name": "NVIDIA GeForce RTX 4060",
                "video_mode": "1920 x 1080 x 4294967296 colors",
            },
            "league_version": "fixture-league",
            "recorder": {"revision": "0123456789abcdef", "dirty": True},
            "collector": {"schema_version": "1", "sha256": "fixture-collector"},
            "presentmon": {"version": "2.4.1", "sha256": "fixture-presentmon"},
        },
        "artifacts": {"presentmon": presentmon.name, "telemetry": telemetry.name},
        "artifact_sha256": {"presentmon": sha256(presentmon), "telemetry": sha256(telemetry)},
        "telemetry": {
            "format": "raw-cim-ndjson-v1",
            "required_sources": list(analyze.REQUIRED_QUERY_SOURCES),
            "optional_sources": ["gpu_process_memory", "per_process_gpu_engine"],
            "process_cpu_normalization": "raw_percent / logical_processors",
            "sample_overruns": 0,
        },
        "capture": None,
    }
    if capture:
        manifest["capture"] = {
            "finalized": True,
            "recording_relative_path": "library/games/fixture/video.mp4",
            "source_config_sha256": "fixture-source-config",
            "recorder_config_sha256": f"fixture-isolated-config-{sequence}",
            "recorder_binary_sha256": "fixture-recorder-binary",
            "ffmpeg_binary_sha256": "fixture-ffmpeg-binary",
            "media_sha256": "a" * 64,
            "media_size_bytes": 250 * MIB,
            "media_validation": {
                "ffprobe_ok": True,
                "decode_ok": True,
                "video_streams": 1,
                "audio_streams": 1,
                "duration_seconds": 250,
                "codec": "hevc",
                "codec_profile": "Main",
                "width": 1920,
                "height": 1080,
                "average_frame_rate": 60,
            },
            "recording_metadata": {
                "encoder_used": "nvenc",
                "codec": "hevc",
                "profile": "high",
                "width": 1920,
                "height": 1080,
                "fps": 60,
                "duration_ms": 250000,
            },
            "diagnostics": {
                "encoder_errors": [],
                "poller_errors": [],
                "poller": {
                    "calibration_requests": "1",
                    "event_requests": "120",
                    "snapshot_requests": "120",
                    "successful_responses": "241",
                    "average_response_latency_ms": "1.2",
                    "maximum_response_latency_ms": "3.4",
                    "captured_events": "4",
                    "captured_snapshots": "120",
                    "game_log_bytes": "4096",
                    "json_writes": "120",
                    "slowest_json_write_ms": "2.0",
                    "event_failure_reason": "none",
                    "snapshot_failure_reason": "none",
                },
                "recorder_disappeared": False,
                "ffmpeg_disappeared": False,
                "capture_target_fallback": False,
                "recording_pid": 1000,
                "audio_source": (
                    r"DirectShow device C:\Users\Alice\private\audio-device"
                    if path_leak
                    else "silent stereo fallback"
                ),
            },
        }
    return manifest


def build_matrix(
    root: Path,
    path_leak: bool = False,
    schema_version: str = "1",
    target_encoder: str = "nvenc",
) -> None:
    expected_runs = (
        analyze.EXPECTED_RUNS_V1 if schema_version == "1" else analyze.EXPECTED_RUNS_V2
    )
    for sequence, (frame_mode, condition, run_number) in enumerate(expected_runs):
        directory = root / frame_mode / condition / f"run-{run_number}"
        directory.mkdir(parents=True)
        presentmon = directory / "presentmon.csv"
        telemetry = directory / "telemetry.ndjson"
        if frame_mode == "capped":
            interval = 1000 / (142 if condition == "capture" else 144)
        else:
            interval = 4.1 if condition == "capture" else 4.0
        write_presentmon(presentmon, interval)
        write_telemetry(telemetry, condition == "capture")
        if schema_version == "2":
            samples = [json.loads(line) for line in telemetry.read_text(encoding="utf-8").splitlines()]
            for sample in samples:
                sample["schema_version"] = "2"
                if condition == "capture" and isinstance(sample.get("output"), dict):
                    sample["output"]["relative_path"] = (
                        "library/games/fixture/video-candidate-0.mp4"
                    )
            telemetry.write_text(
                "".join(
                    json.dumps(sample, sort_keys=True, separators=(",", ":")) + "\n"
                    for sample in samples
                ),
                encoding="utf-8",
                newline="\n",
            )
        manifest = make_manifest(
            frame_mode,
            condition,
            run_number,
            sequence,
            presentmon,
            telemetry,
            path_leak,
        )
        if schema_version == "2":
            interop = analyze.EXPECTED_INTEROP[target_encoder]
            manifest["schema_version"] = "2"
            manifest["feature_id"] = "QB-PERF-002"
            manifest["environment"]["collector"]["schema_version"] = "2"
            manifest["benchmark_contract"] = {
                "target_encoder": target_encoder,
                "capture_backend": "windows_graphics_capture_d3d11",
                "diagnostics_abi": 1,
                "encoder_interop": interop,
                "support_labels": ["optimized-unvalidated", "optimized-validated"],
                "required_pair_count": 1,
            }
            manifest["artifact_sha256"]["telemetry"] = sha256(telemetry)
            if condition == "capture":
                manifest["capture"]["measurement_output_relative_path"] = (
                    "library/games/fixture/video-candidate-0.mp4"
                )
                capture_metadata = {
                    "schema_version": 1,
                    "backend": "windows_graphics_capture_d3d11",
                    "diagnostics_abi": 1,
                    "support_label": "optimized-unvalidated",
                    "capture_adapter_luid": "0000000b0000000a",
                    "encoder_adapter_luid": "0000000b0000000a",
                    "capture_adapter_name": "NVIDIA GeForce RTX 4060",
                    "capture_output": r"\\.\DISPLAY1",
                    "encoder_backend": target_encoder,
                    "encoder_interop": interop,
                    "media_runtime_id": "queueback-ffmpeg-fixture",
                    "source_format": "d3d11_bgra",
                    "converted_format": "d3d11_nv12",
                    "host_readback": False,
                    "gpu_stages": [
                        "windows_graphics_capture_bgra_d3d11",
                        "scale_d3d11_video_processor_nv12",
                        interop,
                    ],
                    "frame_pool_capacity": 2,
                    "capture_output_pool_capacity": 8,
                    "filter_buffered_frame_limit": 32,
                    "encoder_depth": 16 if target_encoder == "nvenc" else 4,
                    "progress_stall_timeout_seconds": 15,
                    "maximum_texture_bytes": 232_243_200,
                    "source_frames_surfaced": 15_000,
                    "source_frames_superseded": 2,
                    "encoded_frames": 15_000,
                    "muxed_bytes": 250 * MIB,
                    "cfr_duplicates": 1,
                    "cfr_discards": 0,
                    "pool_recreations": 0,
                    "first_qpc_100ns": 100_000,
                    "latest_qpc_100ns": 2_500_100_000,
                    "terminal_progress": True,
                }
                manifest["capture"]["recording_metadata"]["encoder_used"] = target_encoder
                manifest["capture"]["recording_metadata"]["capture"] = capture_metadata
                progress_points = []
                for elapsed_ms in range(1_000, 240_000, 10_000):
                    progress_points.append(
                        {
                            "elapsed_ms": elapsed_ms,
                            "source_frames_surfaced": elapsed_ms * 15_000 // 240_000,
                            "source_frames_superseded": 0,
                            "encoded_frames": elapsed_ms * 15_000 // 240_000,
                            "muxed_bytes": elapsed_ms * 250 * MIB // 240_000,
                            "latest_qpc_100ns": elapsed_ms * 10_000,
                            "cfr_duplicates": 0,
                            "cfr_discards": 0,
                            "pool_recreations": 0,
                            "terminal": False,
                        }
                    )
                progress_points.append(
                    {
                        "elapsed_ms": 240_000,
                        "source_frames_surfaced": 15_000,
                        "source_frames_superseded": 2,
                        "encoded_frames": 15_000,
                        "muxed_bytes": 250 * MIB,
                        "latest_qpc_100ns": 2_500_100_000,
                        "cfr_duplicates": 1,
                        "cfr_discards": 0,
                        "pool_recreations": 0,
                        "terminal": True,
                    }
                )
                manifest["capture"]["diagnostics"]["capture_progress"] = progress_points
        if condition == "capture":
            recording = directory / "library" / "games" / "fixture" / "video.mp4"
            recording.parent.mkdir(parents=True)
            recording.write_bytes(b"fixture-media")
            manifest["capture"]["media_sha256"] = sha256(recording)
            manifest["capture"]["media_size_bytes"] = recording.stat().st_size
            manifest["artifacts"]["recording"] = recording.relative_to(directory).as_posix()
            manifest["artifact_sha256"]["recording"] = sha256(recording)
            ffprobe_payload = {
                "streams": [
                    {
                        "index": 0,
                        "codec_type": "video",
                        "codec_name": "hevc",
                        "profile": "Main",
                        "width": 1920,
                        "height": 1080,
                        "avg_frame_rate": "60/1",
                    },
                    {"index": 1, "codec_type": "audio", "codec_name": "aac"},
                ],
                "format": {"duration": "250.000"},
            }
            recorder_diagnostics = {
                key: manifest["capture"]["diagnostics"][key]
                for key in ((
                    "encoder_errors",
                    "poller_errors",
                    "poller",
                    "capture_target_fallback",
                    "recording_pid",
                    "audio_source",
                ) + (("capture_progress",) if schema_version == "2" else ()))
            }
            final_artifacts = {
                "ffprobe": ("ffprobe.json", json.dumps(ffprobe_payload, sort_keys=True) + "\n"),
                "decode_log": ("ffmpeg-decode.log", ""),
                "recorder_diagnostics": (
                    "recorder-diagnostics.json",
                    json.dumps(recorder_diagnostics, sort_keys=True) + "\n",
                ),
            }
            for key, (name, content) in final_artifacts.items():
                artifact = directory / name
                artifact.write_text(content, encoding="utf-8", newline="\n")
                manifest["artifacts"][key] = name
                manifest["artifact_sha256"][key] = sha256(artifact)
        (directory / "manifest.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n"
        )


def load_manifest(run_directory: Path) -> dict:
    return json.loads((run_directory / "manifest.json").read_text(encoding="utf-8"))


def save_manifest(run_directory: Path, manifest: dict) -> None:
    (run_directory / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n"
    )


def load_telemetry(run_directory: Path) -> list[dict]:
    return [
        json.loads(line)
        for line in (run_directory / "telemetry.ndjson").read_text(encoding="utf-8").splitlines()
    ]


def save_telemetry(run_directory: Path, samples: list[dict]) -> None:
    telemetry = run_directory / "telemetry.ndjson"
    telemetry.write_text(
        "".join(json.dumps(sample, sort_keys=True, separators=(",", ":")) + "\n" for sample in samples),
        encoding="utf-8",
        newline="\n",
    )
    manifest = load_manifest(run_directory)
    manifest["artifact_sha256"]["telemetry"] = sha256(telemetry)
    save_manifest(run_directory, manifest)


class FrameCalculationTests(unittest.TestCase):
    def test_known_presentmon_fixture(self) -> None:
        frame = analyze.parse_presentmon(
            FIXTURES / "presentmon-known.csv", 4242, "League of Legends.exe", 0.05
        )
        self.assertAlmostEqual(frame["average_fps"], 80.0)
        self.assertAlmostEqual(frame["one_percent_low_fps"], 50.0)
        self.assertAlmostEqual(frame["p50_frametime_ms"], 12.5)
        self.assertAlmostEqual(frame["p95_frametime_ms"], 19.25)
        self.assertAlmostEqual(frame["p99_frametime_ms"], 19.85)
        self.assertEqual(frame["dropped_frames"], 1)
        self.assertEqual(frame["present_count"], 5)
        self.assertEqual(frame["other_swap_chain_count"], 1)

    def test_malformed_presentmon_is_invalid(self) -> None:
        with self.assertRaisesRegex(analyze.InvalidData, "malformed ProcessID"):
            analyze.parse_presentmon(
                FIXTURES / "presentmon-malformed.csv", 4242, "League of Legends.exe", 180
            )

    def test_truncated_presentmon_row_is_invalid_not_an_exception(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "truncated.csv"
            path.write_text(
                "Application,ProcessID,CPUStartTime,DisplayedTime\nLeague of Legends.exe\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(analyze.InvalidData, "malformed ProcessID"):
                analyze.parse_presentmon(path, 1, "League of Legends.exe", 180)

    def test_truncated_dropped_field_is_invalid_not_an_exception(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "truncated-dropped.csv"
            path.write_text(
                "Application,ProcessID,CPUStartTime,Dropped\n"
                "League of Legends.exe,1,0,0\n"
                "League of Legends.exe,1,10\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(analyze.InvalidData, "Dropped is missing"):
                analyze.parse_presentmon(path, 1, "League of Legends.exe", 0.01)

    def test_short_presentmon_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "short.csv"
            path.write_text(
                "Application,ProcessID,CPUStartTime,DisplayedTime\n"
                "League of Legends.exe,1,0,1\n"
                "League of Legends.exe,1,100000,1\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(analyze.InvalidData, "coverage is short"):
                analyze.parse_presentmon(path, 1, "League of Legends.exe", 180)

    def test_sparse_explicit_intervals_cannot_claim_high_fps(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "sparse.csv"
            path.write_text(
                "Application,ProcessID,SwapChainAddress,CPUStartQPCTime,MsBetweenPresents,DisplayedTime\n"
                "League of Legends.exe,1,0x1,0,0,1\n"
                "League of Legends.exe,1,0x1,179000,6.944,1\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(analyze.InvalidData, "temporally inconsistent"):
                analyze.parse_presentmon(path, 1, "League of Legends.exe", 180)

    def test_present_intervals_may_vary_from_cpu_start_intervals(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "present-offsets.csv"
            path.write_text(
                "Application,ProcessID,CPUStartQPCTime,MsBetweenPresents,DisplayedTime\n"
                "League of Legends.exe,1,0,99,1\n"
                "League of Legends.exe,1,10,12,1\n"
                "League of Legends.exe,1,30,18,1\n"
                "League of Legends.exe,1,45,15,1\n",
                encoding="utf-8",
            )
            frame = analyze.parse_presentmon(path, 1, "League of Legends.exe", 0.045)
            self.assertAlmostEqual(frame["average_fps"], 1000 / 15)
            self.assertEqual(frame["interval_source"], "MsBetweenPresents")

    def test_ms_between_app_start_is_forward_looking(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "app-start.csv"
            path.write_text(
                "Application,ProcessID,CPUStartQPCTime,MsBetweenAppStart,DisplayedTime\n"
                "League of Legends.exe,1,0,10,1\n"
                "League of Legends.exe,1,10,20,1\n"
                "League of Legends.exe,1,30,15,1\n"
                "League of Legends.exe,1,45,NA,1\n",
                encoding="utf-8",
            )
            frame = analyze.parse_presentmon(path, 1, "League of Legends.exe", 0.045)
            self.assertAlmostEqual(frame["average_fps"], 1000 / 15)
            self.assertEqual(frame["interval_source"], "MsBetweenAppStart")

    def test_out_of_order_presentmon_rows_are_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "unordered.csv"
            path.write_text(
                "Application,ProcessID,CPUStartQPCTime,DisplayedTime\n"
                "League of Legends.exe,1,0,1\n"
                "League of Legends.exe,1,20,1\n"
                "League of Legends.exe,1,10,1\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(analyze.InvalidData, "out of order"):
                analyze.parse_presentmon(path, 1, "League of Legends.exe", 0.02)


class CounterCalculationTests(unittest.TestCase):
    def test_sanitizer_redacts_share_root_and_single_component_absolute_paths(self) -> None:
        sanitized = analyze.sanitize(
            {
                r"\\server\share": r"\\server\share",
                "/tmp": "/tmp",
                "url": "https://example.invalid/reference",
            }
        )
        serialized = json.dumps(sanitized, sort_keys=True)
        self.assertNotIn(r"\\server\share", serialized)
        self.assertNotIn("/tmp", serialized)
        self.assertIn("https://example.invalid/reference", serialized)

    def test_process_cpu_is_normalized_to_machine_capacity(self) -> None:
        self.assertEqual(analyze.normalize_process_cpu(120, 12), 10)
        self.assertEqual(analyze.normalize_process_cpu(600, 12), 50)

    def test_gpu_engine_aggregation_keeps_engine_families_separate(self) -> None:
        rows = json.loads((FIXTURES / "gpu-engines.json").read_text(encoding="utf-8"))
        aggregate = analyze.aggregate_gpu_engine_values(rows, "0xa_0xb")
        self.assertEqual(aggregate["total"]["3d"], 100)
        self.assertEqual(aggregate["total"]["video_encode"], 25)
        self.assertEqual(aggregate["processes"]["100"]["3d"], 60)
        self.assertEqual(aggregate["processes"]["300"]["video_encode"], 25)
        self.assertNotIn("400", aggregate["processes"])
        self.assertEqual(aggregate["unclamped_node_maximum_percent"], 110)

    def test_system_cpu_inverse_and_io_rate_use_raw_counter_time(self) -> None:
        idle = analyze.cook_active_percent(
            {"raw": 0, "timestamp_100ns": 0},
            {"raw": 5_000_000, "timestamp_100ns": 10_000_000},
            "idle",
        )
        self.assertEqual(100 - idle, 50)
        self.assertEqual(
            analyze.cook_io_rate(
                {"io": 0, "timestamp_perf": 0},
                {"io": 1_000_000, "timestamp_perf": 10_000_000, "frequency_perf": 10_000_000},
                "io",
                "fixture",
            ),
            1_000_000,
        )
        with self.assertRaisesRegex(analyze.InvalidData, "reset"):
            analyze.cook_active_percent(
                {"raw": 10, "timestamp_100ns": 0},
                {"raw": 9, "timestamp_100ns": 1},
                "fixture",
            )
        with self.assertRaisesRegex(analyze.InvalidData, "reset"):
            analyze.cook_io_rate(
                {"io": 10, "timestamp_perf": 0},
                {"io": 9, "timestamp_perf": 1, "frequency_perf": 1},
                "io",
                "fixture",
            )

    def test_saturation_uses_inclusive_threshold_and_reports_duration(self) -> None:
        result = analyze.saturation_summary([0, 1, 3], [1, 3, 4], [89, 90, 95], 4, 90)
        self.assertEqual(result["sample_proportion"], 2 / 3)
        self.assertEqual(result["seconds"], 3)
        self.assertEqual(result["covered_seconds"], 4)
        gappy = analyze.saturation_summary([0, 3], [1, 4], [95, 95], 4, 90)
        self.assertEqual(gappy["seconds"], 2)
        self.assertEqual(gappy["covered_seconds"], 2)

    def test_memory_growth_requires_all_three_heuristic_arms(self) -> None:
        self.assertTrue(
            analyze.memory_growth_is_sustained(
                analyze.MEMORY_GROWTH_MIN_BYTES,
                analyze.MEMORY_GROWTH_MIN_FRACTION,
                analyze.MEMORY_GROWTH_MIN_BYTES_PER_SECOND,
            )
        )
        self.assertFalse(
            analyze.memory_growth_is_sustained(
                analyze.MEMORY_GROWTH_MIN_BYTES - 1,
                analyze.MEMORY_GROWTH_MIN_FRACTION,
                analyze.MEMORY_GROWTH_MIN_BYTES_PER_SECOND,
            )
        )
        self.assertFalse(
            analyze.memory_growth_is_sustained(
                analyze.MEMORY_GROWTH_MIN_BYTES,
                analyze.MEMORY_GROWTH_MIN_FRACTION - 0.000001,
                analyze.MEMORY_GROWTH_MIN_BYTES_PER_SECOND,
            )
        )
        self.assertFalse(
            analyze.memory_growth_is_sustained(
                analyze.MEMORY_GROWTH_MIN_BYTES,
                analyze.MEMORY_GROWTH_MIN_FRACTION,
                analyze.MEMORY_GROWTH_MIN_BYTES_PER_SECOND - 1,
            )
        )

    def test_output_stall_boundary_is_inclusive(self) -> None:
        exact = analyze.output_growth_assessment([0, 1, 16], [0, 1, 1])
        below = analyze.output_growth_assessment([0, 1, 15.999], [0, 1, 1])
        self.assertTrue(exact["stalled"])
        self.assertFalse(below["stalled"])
        initial_exact = analyze.output_growth_assessment([0, 15, 16], [100, 101, 102])
        initial_below = analyze.output_growth_assessment([0, 14.999, 16], [100, 101, 102])
        self.assertTrue(initial_exact["stalled"])
        self.assertFalse(initial_below["stalled"])


class GateTests(unittest.TestCase):
    def baseline_frame(self) -> dict:
        return {
            "average_fps": 100,
            "one_percent_low_fps": 100,
            "p95_frametime_ms": 10,
            "p99_frametime_ms": 10,
            "dropped_rate": 0,
        }

    def boundary_capture_frame(self) -> dict:
        return {
            "average_fps": 98,
            "one_percent_low_fps": 95,
            "p95_frametime_ms": 10.5,
            "p99_frametime_ms": 10.8,
            "dropped_rate": 0.001,
        }

    def test_all_exact_frame_gate_boundaries_pass(self) -> None:
        result = analyze.evaluate_frame_gates(self.baseline_frame(), self.boundary_capture_frame())
        self.assertTrue(result["passed"])
        self.assertTrue(all(gate["passed"] for gate in result["gates"].values()))

    def test_epsilon_over_each_frame_gate_fails(self) -> None:
        fields = {
            "average_fps": 97.999,
            "one_percent_low_fps": 94.999,
            "p95_frametime_ms": 10.5001,
            "p99_frametime_ms": 10.8001,
            "dropped_rate": 0.001001,
        }
        for field, value in fields.items():
            with self.subTest(field=field):
                capture = self.boundary_capture_frame()
                capture[field] = value
                self.assertFalse(analyze.evaluate_frame_gates(self.baseline_frame(), capture)["passed"])

    def test_exact_resource_saturation_boundaries_pass(self) -> None:
        baseline = {
            "system_cpu_percent": {"saturation": {"sample_proportion": 0.10}},
            "gpu": {"three_d_percent": {"saturation": {"sample_proportion": 0.20}}},
        }
        capture = {
            "system_cpu_percent": {"saturation": {"sample_proportion": 0.15}},
            "gpu": {"three_d_percent": {"saturation": {"sample_proportion": 0.25}}},
        }
        result = analyze.evaluate_resource_gates(
            baseline,
            capture,
            [{"run_id": "capture", "resource_safety": {"passed": True, "checks": {}}}],
        )
        self.assertTrue(result["passed"])
        capture["gpu"]["three_d_percent"]["saturation"]["sample_proportion"] = 0.250001
        self.assertFalse(
            analyze.evaluate_resource_gates(
                baseline,
                capture,
                [{"run_id": "capture", "resource_safety": {"passed": True, "checks": {}}}],
            )["passed"]
        )

    def test_count_derived_exact_gate_boundaries_pass(self) -> None:
        baseline_frame = self.baseline_frame()
        capture_frame = self.baseline_frame()
        baseline_frame["dropped_rate"] = 1 / 21_000
        capture_frame["dropped_rate"] = 22 / 21_000
        self.assertTrue(analyze.evaluate_frame_gates(baseline_frame, capture_frame)["passed"])
        capture_frame["dropped_rate"] += 1e-12
        self.assertFalse(analyze.evaluate_frame_gates(baseline_frame, capture_frame)["passed"])

        baseline = {
            "system_cpu_percent": {"saturation": {"sample_proportion": 19 / 171}},
            "gpu": {"three_d_percent": {"saturation": {"sample_proportion": 0.0}}},
        }
        capture = {
            "system_cpu_percent": {"saturation": {"sample_proportion": 29 / 180}},
            "gpu": {"three_d_percent": {"saturation": {"sample_proportion": 0.0}}},
        }
        capture_run = {"run_id": "capture", "resource_safety": {"passed": True, "checks": {}}}
        self.assertTrue(analyze.evaluate_resource_gates(baseline, capture, [capture_run])["passed"])
        capture["system_cpu_percent"]["saturation"]["sample_proportion"] += 1e-10
        self.assertFalse(analyze.evaluate_resource_gates(baseline, capture, [capture_run])["passed"])

    def test_aggregate_uses_median_run_ratio_not_ratio_of_median_counts(self) -> None:
        aggregate = analyze.median_tree(
            [
                {"sample_proportion": 1 / 2, "saturated_samples": 1, "sample_count": 2},
                {"sample_proportion": 40 / 100, "saturated_samples": 40, "sample_count": 100},
                {"sample_proportion": 3 / 4, "saturated_samples": 3, "sample_count": 4},
            ]
        )
        self.assertEqual(aggregate["sample_proportion"], 0.5)
        self.assertNotEqual(
            aggregate["sample_proportion"],
            aggregate["saturated_samples"] / aggregate["sample_count"],
        )

    def test_diagnostic_thresholds_are_inclusive(self) -> None:
        resource_deltas = {
            "system_cpu_percentage_points": {"median": 2.0},
            "gpu_3d_percentage_points": {"median": 0.0},
            "system_ram_bytes": {"median": 0.0},
            "gpu_dedicated_memory_bytes": {"median": 0.0},
            "processes": {},
        }
        self.assertEqual(
            [item["metric"] for item in analyze.diagnostic_resource_findings(resource_deltas)],
            ["system_cpu_median"],
        )
        resource_deltas["system_cpu_percentage_points"]["median"] = 2.0 - 1e-6
        self.assertEqual(analyze.diagnostic_resource_findings(resource_deltas), [])

        frame_deltas = {name: 0.0 for name in analyze.FRAME_BUDGET}
        frame_deltas["average_fps_loss_percent"] = 2.0
        self.assertEqual(
            [item["metric"] for item in analyze.diagnostic_frame_findings(frame_deltas)],
            ["average_fps_loss_percent"],
        )
        frame_deltas["average_fps_loss_percent"] = 2.0 - 1e-6
        self.assertEqual(analyze.diagnostic_frame_findings(frame_deltas), [])

    def test_variability_exact_boundaries_pass_and_epsilon_fails(self) -> None:
        runs = [
            {"frame": {"average_fps": 98.5, "p99_frametime_ms": 9.5}},
            {"frame": {"average_fps": 100, "p99_frametime_ms": 10}},
            {"frame": {"average_fps": 101.5, "p99_frametime_ms": 10.5}},
        ]
        self.assertTrue(analyze.validate_capped_variability(runs, "fixture")["average_fps_valid"])
        runs[2]["frame"]["average_fps"] = 101.5001
        with self.assertRaisesRegex(analyze.InvalidData, "variability"):
            analyze.validate_capped_variability(runs, "fixture")

        float_boundary = [
            {"frame": {"average_fps": value, "p99_frametime_ms": 10.0}}
            for value in (49.299249999999994, 50.05, 50.800749999999994)
        ]
        self.assertTrue(
            analyze.validate_capped_variability(float_boundary, "fixture")["average_fps_valid"]
        )


class MatrixIntegrationTests(unittest.TestCase):
    def test_valid_matrix_passes_and_uncapped_stays_diagnostic(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "pass")
            self.assertEqual(report["uncapped"]["status"], "diagnostic")
            self.assertEqual(len(report["runs"]), 8)
            self.assertEqual(len(report["capped"]["paired_frame_deltas"]), 3)
            self.assertIn("ffmpeg", report["capped"]["resource_deltas"]["processes"])
            self.assertGreater(report["uncapped"]["frame_deltas"]["average_fps_loss_percent"], 2)
            self.assertTrue(
                any(
                    item["metric"] == "average_fps_loss_percent"
                    for item in report["uncapped"]["diagnostic_findings"]
                )
            )
            self.assertTrue(
                any("Per-process GPU memory" in item for item in report["limitations"])
            )
            self.assertTrue(
                any("inferred as 0%" in item for item in report["limitations"])
            )

    def test_schema_v2_uses_one_capped_pair_and_one_uncapped_pair(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root, schema_version="2")
            report = analyze.analyze_dataset(root, "nvenc")
            self.assertEqual(report["status"], "pass")
            self.assertEqual(report["feature_id"], "QB-PERF-002")
            self.assertEqual(len(report["runs"]), 4)
            self.assertEqual(len(report["capped"]["paired_frame_deltas"]), 1)
            self.assertNotIn("variability", report["capped"])
            self.assertEqual(
                report["capped"]["repeat_policy"],
                "fresh_pair_on_invalid_or_doubtful_evidence",
            )

    def test_schema_v2_changes_encoder_identity_without_changing_budgets(self) -> None:
        reference_gates = None
        for encoder in ("nvenc", "amf", "qsv"):
            with self.subTest(encoder=encoder), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                build_matrix(root, schema_version="2", target_encoder=encoder)
                report = analyze.analyze_dataset(root, encoder)
                self.assertEqual(report["status"], "pass")
                self.assertEqual(report["capture_configuration"]["encoder"], encoder)
                gates = report["capped"]["frame_gates"]["gates"]
                if reference_gates is None:
                    reference_gates = gates
                else:
                    self.assertEqual(gates, reference_gates)

    def test_schema_v2_rejects_cross_adapter_or_host_readback_claims(self) -> None:
        for field, value, issue in (
            ("encoder_adapter_luid", "0000000c0000000a", "adapters do not match"),
            ("host_readback", True, "host-readback"),
        ):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                build_matrix(root, schema_version="2")
                run = root / "capped" / "capture" / "run-1"
                manifest = load_manifest(run)
                manifest["capture"]["recording_metadata"]["capture"][field] = value
                save_manifest(run, manifest)
                report = analyze.analyze_dataset(root, "nvenc")
                self.assertEqual(report["status"], "invalid")
                self.assertTrue(any(issue in item for item in report["issues"]))

    def test_config_adjacent_league_version_fallback_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            for frame_mode, condition, run_number in analyze.EXPECTED_RUNS:
                run = root / frame_mode / condition / f"run-{run_number}"
                manifest = load_manifest(run)
                manifest["environment"]["league_version_source"] = "configured_installation_executable"
                save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "pass")
            self.assertTrue(
                any("adjacent to the operator-supplied game.cfg" in item for item in report["limitations"])
            )

    def test_process_role_pid_must_match_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            samples = load_telemetry(run)
            samples[50]["processes"][0]["pid"] = 9999
            save_telemetry(run, samples)
            with self.assertRaisesRegex(analyze.InvalidData, "PID does not match"):
                analyze.analyze_run(run, analyze.RunKey("capped", "capture", 1))

    def test_capture_requires_observed_video_encode_engine(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            samples = load_telemetry(run)
            for sample in samples:
                sample["gpu_engines"] = [
                    row for row in sample["gpu_engines"] if "VideoEncode" not in row["name"]
                ]
            save_telemetry(run, samples)
            with self.assertRaisesRegex(analyze.InvalidData, "Video Encode"):
                analyze.analyze_run(run, analyze.RunKey("capped", "capture", 1))

    def test_target_adapter_3d_cannot_disappear(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            samples = load_telemetry(run)
            for sample in samples:
                sample["gpu_engines"] = [row for row in sample["gpu_engines"] if "engtype_3D" not in row["name"]]
            save_telemetry(run, samples)
            with self.assertRaisesRegex(analyze.InvalidData, "GPU 3D"):
                analyze.analyze_run(run, analyze.RunKey("capped", "capture", 1))

    def test_ffprobe_stream_must_match_recorder_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            manifest = load_manifest(run)
            ffprobe = run / manifest["artifacts"]["ffprobe"]
            payload = json.loads(ffprobe.read_text(encoding="utf-8"))
            payload["streams"][0]["width"] = 1280
            ffprobe.write_text(json.dumps(payload, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
            manifest["artifact_sha256"]["ffprobe"] = sha256(ffprobe)
            save_manifest(run, manifest)
            with self.assertRaisesRegex(analyze.InvalidData, "resolution disagrees"):
                analyze.analyze_run(run, analyze.RunKey("capped", "capture", 1))

    def test_capture_media_hash_must_match_recording_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            manifest = load_manifest(run)
            manifest["capture"]["media_sha256"] = "b" * 64
            save_manifest(run, manifest)
            with self.assertRaisesRegex(analyze.InvalidData, "disagrees"):
                analyze.analyze_run(run, analyze.RunKey("capped", "capture", 1))

    def test_capture_output_samples_bind_to_final_media(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            samples = load_telemetry(run)
            samples[90]["output"]["relative_path"] = "library/games/other/video.mp4"
            save_telemetry(run, samples)
            with self.assertRaisesRegex(analyze.InvalidData, "output path"):
                analyze.analyze_run(run, analyze.RunKey("capped", "capture", 1))

    def test_sparse_two_second_telemetry_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            samples = load_telemetry(run)[::2]
            for sequence, sample in enumerate(samples):
                sample["sequence"] = sequence
            save_telemetry(run, samples)
            manifest = load_manifest(run)
            manifest["telemetry"]["sample_overruns"] = 90
            save_manifest(run, manifest)
            with self.assertRaisesRegex(analyze.InvalidData, "protocol sample count"):
                analyze.analyze_run(run, analyze.RunKey("capped", "baseline", 1))

    def test_machine_normalized_cpu_over_capacity_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            samples = load_telemetry(run)
            for second, sample in enumerate(samples):
                league = next(row for row in sample["processes"] if row["role"] == "league")
                league["processor_time_raw"] = int(second * 10_000_000 * 1300 / 100)
            save_telemetry(run, samples)
            with self.assertRaisesRegex(analyze.InvalidData, "normalized CPU"):
                analyze.analyze_run(run, analyze.RunKey("capped", "baseline", 1))

    def test_telemetry_schema_sequence_and_query_errors_are_validated(self) -> None:
        mutations = (
            ("schema", lambda samples: samples[10].__setitem__("schema_version", "2"), "schema version"),
            ("sequence", lambda samples: samples[10].__setitem__("sequence", 9), "sequence"),
            (
                "errors",
                lambda samples: (
                    samples[10]["query_status"].__setitem__("system_cpu", False),
                    samples[10].__setitem__("query_errors", []),
                ),
                "lacks an error",
            ),
        )
        for name, mutate, expected in mutations:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                build_matrix(root)
                run = root / "capped" / "baseline" / "run-1"
                samples = load_telemetry(run)
                mutate(samples)
                save_telemetry(run, samples)
                with self.assertRaisesRegex(analyze.InvalidData, expected):
                    analyze.analyze_run(run, analyze.RunKey("capped", "baseline", 1))

    def test_null_telemetry_row_collection_is_invalid_not_an_exception(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            samples = load_telemetry(run)
            samples[10]["gpu_engines"] = None
            save_telemetry(run, samples)
            with self.assertRaisesRegex(analyze.InvalidData, "malformed gpu_engines"):
                analyze.analyze_run(run, analyze.RunKey("capped", "baseline", 1))

    def test_null_capture_manifest_is_invalid_not_an_exception(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            manifest = load_manifest(run)
            manifest["capture"] = None
            save_manifest(run, manifest)
            with self.assertRaisesRegex(analyze.InvalidData, "capture manifest section"):
                analyze.analyze_run(run, analyze.RunKey("capped", "capture", 1))

    def test_display_must_be_attached_to_target_gpu(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            manifest = load_manifest(run)
            manifest["environment"]["display"]["adapter_name"] = "AMD Radeon RX 7900 XTX"
            save_manifest(run, manifest)
            with self.assertRaisesRegex(analyze.InvalidData, "display is not attached"):
                analyze.analyze_run(run, analyze.RunKey("capped", "baseline", 1))

    def test_missing_post_run_config_fingerprint_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            manifest = load_manifest(run)
            del manifest["configuration"]["league_config_sha256_after"]
            save_manifest(run, manifest)
            with self.assertRaisesRegex(analyze.InvalidData, "post-measurement"):
                analyze.analyze_run(run, analyze.RunKey("capped", "baseline", 1))

    def test_malformed_required_source_manifest_is_invalid_not_an_exception(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            manifest = load_manifest(run)
            manifest["telemetry"]["required_sources"] = [{}]
            save_manifest(run, manifest)
            with self.assertRaisesRegex(analyze.InvalidData, "required telemetry source"):
                analyze.analyze_run(run, analyze.RunKey("capped", "baseline", 1))

    def test_incomplete_poller_diagnostics_are_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            manifest = load_manifest(run)
            del manifest["capture"]["diagnostics"]["poller"]["snapshot_failure_reason"]
            save_manifest(run, manifest)
            with self.assertRaisesRegex(analyze.InvalidData, "incomplete"):
                analyze.analyze_run(run, analyze.RunKey("capped", "capture", 1))

    def test_capture_source_and_binary_identity_must_be_consistent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-2"
            manifest = load_manifest(run)
            manifest["capture"]["source_config_sha256"] = "different-source-config"
            save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "invalid")
            self.assertTrue(any("recorder_source_config_sha256" in issue for issue in report["issues"]))

    def test_overlapping_run_windows_are_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            first = load_manifest(root / "capped" / "baseline" / "run-1")
            run = root / "capped" / "capture" / "run-1"
            manifest = load_manifest(run)
            start = datetime.fromisoformat(first["timing"]["measurement_started_utc"].replace("Z", "+00:00"))
            manifest["timing"]["measurement_started_utc"] = (start + timedelta(seconds=60)).isoformat().replace(
                "+00:00", "Z"
            )
            manifest["timing"]["measurement_ended_utc"] = (start + timedelta(seconds=240)).isoformat().replace(
                "+00:00", "Z"
            )
            save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "invalid")
            self.assertTrue(any("overlap" in issue for issue in report["issues"]))

    def test_one_missing_required_sample_within_coverage_and_gap_tolerance_is_valid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            telemetry = run / "telemetry.ndjson"
            samples = [json.loads(line) for line in telemetry.read_text(encoding="utf-8").splitlines()]
            samples[90]["query_status"]["system_cpu"] = False
            samples[90]["query_errors"] = ["system_cpu:FixtureError"]
            telemetry.write_text(
                "".join(json.dumps(sample, sort_keys=True, separators=(",", ":")) + "\n" for sample in samples),
                encoding="utf-8",
                newline="\n",
            )
            manifest = load_manifest(run)
            manifest["artifact_sha256"]["telemetry"] = sha256(telemetry)
            save_manifest(run, manifest)
            self.assertEqual(analyze.analyze_dataset(root)["status"], "pass")

    def test_required_counter_gap_over_three_seconds_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            telemetry = run / "telemetry.ndjson"
            samples = [json.loads(line) for line in telemetry.read_text(encoding="utf-8").splitlines()]
            for index in range(89, 93):
                samples[index]["query_status"]["system_cpu"] = False
                samples[index]["query_errors"] = ["system_cpu:FixtureError"]
            telemetry.write_text(
                "".join(json.dumps(sample, sort_keys=True, separators=(",", ":")) + "\n" for sample in samples),
                encoding="utf-8",
                newline="\n",
            )
            manifest = load_manifest(run)
            manifest["artifact_sha256"]["telemetry"] = sha256(telemetry)
            save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "invalid")
            self.assertTrue(any("gap greater than three seconds" in issue for issue in report["issues"]))

    def test_missing_required_counter_makes_matrix_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            telemetry = run / "telemetry.ndjson"
            write_telemetry(telemetry, False, missing_source="system_cpu")
            manifest = load_manifest(run)
            manifest["artifact_sha256"]["telemetry"] = sha256(telemetry)
            save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "invalid")
            self.assertTrue(any("system_cpu" in issue for issue in report["issues"]))

    def test_invalid_media_takes_precedence_over_gate_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            manifest = load_manifest(run)
            manifest["capture"]["media_validation"]["decode_ok"] = False
            save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "invalid")
            self.assertTrue(any("decode_ok" in issue for issue in report["issues"]))

    def test_non_nvenc_capture_is_invalid_for_target_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            manifest = load_manifest(run)
            manifest["capture"]["recording_metadata"]["encoder_used"] = "qsv"
            save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "invalid")
            self.assertTrue(any("NVENC" in issue for issue in report["issues"]))

    def test_hardware_suffix_variants_do_not_match_target(self) -> None:
        variants = (
            ("cpu", "AMD Ryzen 5 5600X3D", "Ryzen 5 5600X"),
            ("gpu", "NVIDIA GeForce RTX 4060 Ti", "RTX 4060"),
        )
        for kind, model, expected in variants:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                build_matrix(root)
                run = root / "capped" / "baseline" / "run-1"
                manifest = load_manifest(run)
                if kind == "cpu":
                    manifest["environment"]["cpu"]["name"] = model
                else:
                    manifest["environment"]["gpus"][0]["name"] = model
                save_manifest(run, manifest)
                with self.assertRaisesRegex(analyze.InvalidData, expected):
                    analyze.analyze_run(run, analyze.RunKey("capped", "baseline", 1))

    def test_artifact_hash_mismatch_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "baseline" / "run-1"
            with (run / "presentmon.csv").open("a", encoding="utf-8") as handle:
                handle.write("tampered\n")
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "invalid")
            self.assertTrue(any("SHA-256" in issue for issue in report["issues"]))

    def test_output_stall_is_a_valid_resource_safety_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            telemetry = run / "telemetry.ndjson"
            samples = [json.loads(line) for line in telemetry.read_text(encoding="utf-8").splitlines()]
            for sample in samples:
                if sample["sequence"] > 1:
                    sample["output"]["size_bytes"] = 101 * MIB
            telemetry.write_text(
                "".join(json.dumps(sample, sort_keys=True, separators=(",", ":")) + "\n" for sample in samples),
                encoding="utf-8",
                newline="\n",
            )
            manifest = load_manifest(run)
            manifest["artifact_sha256"]["telemetry"] = sha256(telemetry)
            save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "fail")
            safety = next(item for item in report["runs"] if item["run_id"] == "capped-capture-1")
            self.assertFalse(safety["resource_safety"]["checks"]["no_output_stall"])

    def test_encoder_error_is_a_valid_resource_safety_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            manifest = load_manifest(run)
            manifest["capture"]["diagnostics"]["encoder_errors"] = ["ffmpeg_exited"]
            diagnostics_path = run / manifest["artifacts"]["recorder_diagnostics"]
            diagnostics = json.loads(diagnostics_path.read_text(encoding="utf-8"))
            diagnostics["encoder_errors"] = ["ffmpeg_exited"]
            diagnostics_path.write_text(
                json.dumps(diagnostics, sort_keys=True) + "\n", encoding="utf-8", newline="\n"
            )
            manifest["artifact_sha256"]["recorder_diagnostics"] = sha256(diagnostics_path)
            save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "fail")
            safety = next(item for item in report["runs"] if item["run_id"] == "capped-capture-1")
            self.assertFalse(safety["resource_safety"]["checks"]["no_encoder_errors"])

    def test_poller_lifecycle_error_is_a_valid_resource_safety_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "capped" / "capture" / "run-1"
            manifest = load_manifest(run)
            manifest["capture"]["diagnostics"]["poller_errors"] = ["poller_task_panicked"]
            diagnostics_path = run / manifest["artifacts"]["recorder_diagnostics"]
            diagnostics = json.loads(diagnostics_path.read_text(encoding="utf-8"))
            diagnostics["poller_errors"] = ["poller_task_panicked"]
            diagnostics_path.write_text(
                json.dumps(diagnostics, sort_keys=True) + "\n", encoding="utf-8", newline="\n"
            )
            manifest["artifact_sha256"]["recorder_diagnostics"] = sha256(diagnostics_path)
            save_manifest(run, manifest)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "fail")
            safety = next(item for item in report["runs"] if item["run_id"] == "capped-capture-1")
            self.assertFalse(safety["resource_safety"]["checks"]["no_poller_errors"])

    def test_target_adapter_identity_must_be_consistent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            run = root / "uncapped" / "capture" / "run-1"
            samples = load_telemetry(run)
            for sample in samples:
                for row in sample["gpu_engines"]:
                    row["name"] = row["name"].replace("luid_0xA_0xB", "luid_0xC_0xD")
                for row in sample["gpu_adapter_memory"]:
                    row["name"] = row["name"].replace("luid_0xA_0xB", "luid_0xC_0xD")
            save_telemetry(run, samples)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "invalid")
            self.assertTrue(any("target_adapter_id" in issue for issue in report["issues"]))

    def test_missing_uncapped_pair_invalidates_complete_matrix(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build_matrix(root)
            missing = root / "uncapped" / "capture" / "run-1"
            shutil.rmtree(missing)
            report = analyze.analyze_dataset(root)
            self.assertEqual(report["status"], "invalid")
            self.assertTrue(any("uncapped/capture/run-1" in issue for issue in report["issues"]))

    def test_reports_are_sanitized_and_byte_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "raw"
            build_matrix(root, path_leak=True)
            report = analyze.analyze_dataset(root)
            first_json = Path(temporary) / "one" / "report.json"
            first_md = Path(temporary) / "one" / "report.md"
            second_json = Path(temporary) / "two" / "report.json"
            second_md = Path(temporary) / "two" / "report.md"
            analyze.write_report(report, first_json, first_md)
            analyze.write_report(report, second_json, second_md)
            self.assertEqual(first_json.read_bytes(), second_json.read_bytes())
            self.assertEqual(first_md.read_bytes(), second_md.read_bytes())
            combined = first_json.read_text(encoding="utf-8") + first_md.read_text(encoding="utf-8")
            self.assertNotIn(r"C:\Users\Alice", combined)
            self.assertIn("<absolute-path>", combined)


if __name__ == "__main__":
    unittest.main()
