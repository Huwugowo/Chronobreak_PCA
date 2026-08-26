from __future__ import annotations

import copy
import importlib.util
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


ANALYZER_PATH = Path(__file__).resolve().parents[1] / "analyze.py"
SPEC = importlib.util.spec_from_file_location("replay_benchmark_analyze", ANALYZER_PATH)
assert SPEC is not None and SPEC.loader is not None
analyze = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = analyze
SPEC.loader.exec_module(analyze)


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, sort_keys=True) + "\n", encoding="utf-8")


def write_jsonl(path: Path, values: list[dict[str, Any]]) -> None:
    path.write_text(
        "".join(json.dumps(value, sort_keys=True) + "\n" for value in values),
        encoding="utf-8",
    )


class BundleBuilder:
    def __init__(
        self,
        root: Path,
        *,
        run_id: str = "benchmark-run",
        trial_ids: list[str] | None = None,
        latencies: list[float] | None = None,
        fingerprint: str = "same-environment",
        observer_profile: str = "full",
    ) -> None:
        root = root / analyze.SENTINEL_NAME
        root.mkdir(exist_ok=True)
        self.sentinel_root = root
        self.result_root = root / "results"
        self.result_root.mkdir(exist_ok=True)
        (root / "config").mkdir(exist_ok=True)
        (root / "config" / "config.toml").write_text(
            '[storage]\noutput_path = "benchmark"\n', encoding="utf-8"
        )
        self.bundle = self.result_root / run_id
        self.bundle.mkdir()
        trial_ids = trial_ids or ["1"]
        latencies = latencies or [100.0] * len(trial_ids)
        self.manifest = {
            "schema_version": 1,
            "benchmark_id": "QB-REPLAY-008",
            "run_id": run_id,
            "sentinel_root": str(root.resolve()),
            "library_root": str((root / "library").resolve()),
            "config_path": str((root / "config" / "config.toml").resolve()),
            "app_data_root": str((root / "app-data").resolve()),
            "result_root": str(self.result_root.resolve()),
            "scratch_root": str((root / "scratch").resolve()),
            "observer_profile": observer_profile,
            "ddragon": {"mode": "offline", "cache_sha256": "a" * 64},
            "fixtures": [
                {
                    "id": "representative-h264",
                    "media_sha256": "b" * 64,
                    "size_bytes": 123456,
                }
            ],
            "fingerprints": {
                "app_binary_sha256": "c" * 64,
                "media_runtime_id": "queueback-media-runtime-6",
                "environment_id": fingerprint,
                "private_path": str(root / "Hugo" / "recording.mp4"),
            },
            "scenarios": [
                {
                    "scenario_id": "seek-far-forward",
                    "fixture_id": "representative-h264",
                    "trial_ids": trial_ids,
                    "expected_action_count": 1,
                    "max_telemetry_gap_ms": 2500,
                }
            ],
            "future_manifest_field": {"accepted": True},
        }
        self.runner_metadata = {
            "schema_version": 1,
            "run_id": run_id,
            "started_utc": "2026-08-24T12:00:00Z",
            "manifest_path": str((root / "manifests" / "launch.json").resolve()),
            "manifest_sha256": "0" * 64,
            "config_sha256": hashlib.sha256(
                (root / "config" / "config.toml").read_bytes()
            ).hexdigest(),
            "app_binary": str((root / "bin" / "QueueBack.exe").resolve()),
            "app_binary_sha256": "c" * 64,
            "analyzer_path": str(ANALYZER_PATH),
            "python_path": sys.executable,
            "timeout_seconds": 3600,
            "observer_profile": observer_profile,
            "observer_cadence_ms": 1000,
            "logical_processors": 8,
            "cpu_accounting_quantum_ms": 15.625,
            "cpu_accounting_quantum_method": "test accounting probe",
            "cpu_accounting_reported_counter_unit_ms": 0.0001,
            "cpu_accounting_limitation": "synthetic fixture",
            "gpu_collection": "synthetic optional GPU counters",
            "process_tree_collection_method": "synthetic Job Object sampler",
            "process_tree_assignment_limitation": "synthetic fixture",
            "environment": {
                "os": {"caption": "Windows", "version": "test"},
                "computer": {"manufacturer": "test", "model": "test"},
                "processors": [{"name": "test CPU", "logical_processors": 8}],
                "video_controllers": [{"name": "test GPU", "driver_version": "1"}],
                "powershell_version": "7.5",
                "limitations": [],
            },
            "source": {"revision": "d" * 40, "dirty": False, "limitation": None},
            "media_runtime_id": None,
            "webview2_runtime_versions": ["test-webview2"],
            "webview2_runtime_limitation": None,
        }
        self.events: list[dict[str, Any]] = []
        self.requests: list[dict[str, Any]] = []
        self.samples: list[dict[str, Any]] = []
        for trial_index, (trial_id, latency) in enumerate(zip(trial_ids, latencies), 1):
            base = trial_index * 10_000.0
            action_id = f"seek-{trial_id}"
            generation = trial_index
            self.events.extend(
                [
                    self.envelope(trial_id, base, "app", "scenario_start"),
                    self.envelope(
                        trial_id,
                        base + 10,
                        "viewer",
                        "seek_requested",
                        generation=generation,
                        action_id=action_id,
                        payload={
                            "target_ms": 1000,
                            "metrics": {"open_latency_ms": latency},
                            "future": "ignored",
                        },
                    ),
                    self.envelope(
                        trial_id,
                        base + 20,
                        "viewer",
                        "seek_dispatched",
                        generation=generation,
                        action_id=action_id,
                    ),
                    self.envelope(
                        trial_id,
                        base + 50,
                        "viewer",
                        "seeked",
                        generation=generation,
                        action_id=action_id,
                    ),
                    self.envelope(
                        trial_id,
                        base + latency,
                        "viewer",
                        "seek_presented",
                        generation=generation,
                        action_id=action_id,
                        payload={"media_time_ms": 1001, "authoritative": True},
                    ),
                    self.envelope(
                        trial_id,
                        base + max(200, latency + 10),
                        "app",
                        "scenario_end",
                        payload={
                            "playback_rate": 1.0,
                            "total_frames": 1_000,
                            "dropped_frames": 0,
                        },
                    ),
                ]
            )
            self.requests.append(
                self.envelope(
                    trial_id,
                    base + 21,
                    "server",
                    "server_request",
                    generation=generation,
                    action_id=action_id,
                    payload={
                        "request_id": f"request-{trial_id}",
                        "route_class": "game_video",
                        "method": "GET",
                        "outcome": "completed",
                        "status": 206,
                        "started_ms": base + 21,
                        "first_byte_ms": base + 22,
                        "completed_ms": base + 30,
                        "declared_bytes": 4096,
                        "delivered_bytes": 4096,
                        "range_start": 0,
                        "range_end": 4095,
                        "peak_active_streams": 1,
                    },
                )
            )
            self.samples.extend(
                [
                    self.envelope(
                        trial_id,
                        base,
                        "collector",
                        "process_sample",
                        payload={
                            "process_count": 3,
                            "process_tree_cpu_percent": 2.0,
                            "process_tree_private_bytes": 100_000_000,
                            "read_bytes": 1000,
                        },
                    ),
                    self.envelope(
                        trial_id,
                        base + 1000,
                        "collector",
                        "process_sample",
                        payload={
                            "process_count": 3,
                            "process_tree_cpu_percent": 3.0,
                            "process_tree_private_bytes": 101_000_000,
                            "read_bytes": 9000,
                            "optional_gpu_field": None,
                        },
                    ),
                ]
            )
        self.terminal = {
            "schema_version": 1,
            "run_id": run_id,
            "status": "complete",
            "completed": True,
            "elapsed_ms": 60_000.0,
            "record_counts": {},
            "dropped_counts": {
                "events": 0,
                "server_requests": 0,
                "process_samples": 0,
            },
            "source_hash_unchanged": True,
            "media_integrity_ok": True,
            "export_integrity_ok": True,
            "future_terminal_field": "accepted",
        }
        self.collection = {
            "schema_version": 1,
            "run_id": run_id,
            "completed_utc": "2026-08-24T12:01:01Z",
            "duration_ms": 61_000.0,
            "post_hash_elapsed_ms": 10.0,
            "app_pid": 1234,
            "app_exit_code": 0,
            "timed_out": False,
            "startup_timed_out": False,
            "frontend_startup_observed": True,
            "frontend_startup_timeout_seconds": 30,
            "forced_termination_reason": None,
            "fixture_hashes_match": True,
            "job_accounting": {
                "cpu_time_100ns": 1_000_000,
                "total_processes": 3,
                "active_processes": 0,
                "terminated_processes": 0,
                "unobserved_process_count": 0,
                "new_process_notification_count": 3,
            },
        }
        self.export_validation: dict[str, Any] | None = None

    def enable_export_validation(self, *, preset: str = "horizontal") -> None:
        scenario = self.manifest["scenarios"][0]
        scenario.update(
            {
                "kind": "export",
                "export_presets": [preset],
                "expected_duration_ms": 10_000,
                "duration_tolerance_ms": 1_000,
                "expected_video_codec": "h264",
                "expected_audio_codec": "aac",
            }
        )
        trial_id = scenario["trial_ids"][0]
        base = 10_000.0
        self.events.extend(
            [
                self.envelope(trial_id, base - 20, "frontend", "library_requested"),
                self.envelope(trial_id, base - 10, "frontend", "library_useful"),
                self.envelope(trial_id, base + 5, "frontend", "export_requested"),
                self.envelope(
                    trial_id,
                    base + 180,
                    "frontend",
                    "export_completed",
                    payload={
                        "strategy": "full_reencode",
                        "copy_strategy": "not_implemented",
                        "hybrid_strategy": "not_implemented",
                        "benchmark_scope": "backend_command",
                        "ui_workflow_included": False,
                    },
                ),
            ]
        )
        next(event for event in self.events if event["kind"] == "scenario_end")[
            "kind"
        ] = "scenario_completed"
        self.events.sort(key=lambda event: float(event["monotonic_ms"]))
        filename = "1700000000_1700000100"
        size_bytes = 9_000_000 if preset == "discord" else 20_000_000
        self.export_validation = {
            "schema_version": 1,
            "run_id": self.manifest["run_id"],
            "scenario_id": scenario["scenario_id"],
            "trial_id": scenario["trial_ids"][0],
            "checked_utc": "2026-08-24T12:00:00Z",
            "clips_root": "library_root\\clips",
            "expected_output_count": 1,
            "expected_duration_ms": 10_000,
            "duration_tolerance_ms": 1_000,
            "expected_streams": {
                "video_count": 1,
                "video_codec": "h264",
                "audio_count": 1,
                "audio_codec": "aac",
            },
            "discord_limit_bytes_exclusive": 10_000_000,
            "validation_elapsed_ms": 125.5,
            "preexisting_entry_count": 0,
            "newly_created_entries": [
                {
                    "relative_path": f"clips\\{filename}.mp4",
                    "kind": "file",
                    "size_bytes": size_bytes,
                },
                {
                    "relative_path": f"clips\\{filename}.jpg",
                    "kind": "file",
                    "size_bytes": 12_345,
                },
            ],
            "outputs": [
                {
                    "filename": filename,
                    "preset": preset,
                    "relative_path": f"clips\\{filename}.mp4",
                    "thumbnail_relative_path": f"clips\\{filename}.jpg",
                    "size_bytes": size_bytes,
                    "sha256": "e" * 64,
                    "thumbnail_sha256": "f" * 64,
                    "duration_ms": 10_000.0,
                    "ffprobe": {
                        "format": {"duration": "10.000", "size": str(size_bytes)},
                        "streams": [
                            {"index": 0, "codec_type": "video", "codec_name": "h264"},
                            {"index": 1, "codec_type": "audio", "codec_name": "aac"},
                        ],
                    },
                    "full_single_thread_decode_ok": True,
                    "decode_stderr": "",
                    "discord_size_valid": True if preset == "discord" else None,
                    "valid": True,
                }
            ],
            "errors": [],
            "all_valid": True,
        }

    @staticmethod
    def envelope(
        trial_id: str,
        monotonic_ms: float,
        source: str,
        kind: str,
        *,
        generation: int | None = None,
        action_id: str | None = None,
        payload: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        record: dict[str, Any] = {
            "schema_version": 1,
            "run_id": "benchmark-run",
            "scenario_id": "seek-far-forward",
            "trial_id": trial_id,
            "monotonic_ms": monotonic_ms,
            "source": source,
            "kind": kind,
            "payload": payload or {},
            "future_envelope_field": "accepted",
        }
        if generation is not None:
            record["generation"] = generation
        if action_id is not None:
            record["action_id"] = action_id
        return record

    def persist(self) -> Path:
        run_id = self.manifest["run_id"]
        for records in (self.events, self.requests, self.samples):
            for record in records:
                record["run_id"] = run_id
        self.terminal["record_counts"] = {
            "events": len(self.events),
            "server_requests": len(self.requests),
            "process_samples": len(self.samples),
        }
        write_json(self.bundle / "manifest.json", self.manifest)
        self.runner_metadata["run_id"] = run_id
        self.runner_metadata["observer_profile"] = self.manifest["observer_profile"]
        self.runner_metadata["manifest_sha256"] = hashlib.sha256(
            (self.bundle / "manifest.json").read_bytes()
        ).hexdigest()
        write_json(self.bundle / "runner-metadata.json", self.runner_metadata)
        self.collection["run_id"] = run_id
        write_json(self.bundle / "collection-result.json", self.collection)
        write_jsonl(self.bundle / "events.jsonl", self.events)
        write_jsonl(self.bundle / "server_requests.jsonl", self.requests)
        write_jsonl(self.bundle / "process_samples.jsonl", self.samples)
        write_json(self.bundle / "terminal.json", self.terminal)
        if self.export_validation is not None:
            self.export_validation["run_id"] = run_id
            write_json(self.bundle / "export-validation.json", self.export_validation)
        return self.bundle


class ReplayAnalyzerTests(unittest.TestCase):
    @staticmethod
    def control_reports(
        minimal_root: Path,
        full_root: Path,
        *,
        pair_count: int = 4,
        minimal_latency: float = 100.0,
        full_latency: float = 104.0,
        full_memory_increase: float = 10 * 1024 * 1024,
        full_cpu_time_ms: float = 1_010.0,
    ) -> tuple[dict[str, Any], dict[str, Any]]:
        trial_ids = [str(index) for index in range(1, pair_count + 1)]
        minimal = analyze.analyze(
            BundleBuilder(
                minimal_root,
                trial_ids=trial_ids,
                latencies=[minimal_latency] * pair_count,
                observer_profile="minimal",
            ).persist()
        )
        full = analyze.analyze(
            BundleBuilder(
                full_root,
                trial_ids=trial_ids,
                latencies=[full_latency] * pair_count,
                observer_profile="full",
            ).persist()
        )
        for report, cpu_time in ((minimal, 1_000.0), (full, full_cpu_time_ms)):
            for trial in report["trials"]:
                evidence = trial["observer_control_evidence"]
                evidence["duration_ms"] = 60_000.0
                evidence["aggregate_cpu_time_ms"] = cpu_time
                evidence["cpu_accounting_quantum_ms"] = 15.625
                evidence["max_process_count"] = 3
        for before, after in zip(minimal["trials"], full["trials"]):
            after["observer_control_evidence"]["max_private_bytes"] = (
                before["observer_control_evidence"]["max_private_bytes"]
                + full_memory_increase
            )
        return minimal, full

    def test_valid_bundle_preserves_trials_and_type7_distributions(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = BundleBuilder(
                root,
                trial_ids=["1", "2", "3", "4", "5"],
                latencies=[90, 100, 110, 120, 130],
            ).persist()

            report = analyze.analyze(bundle)

            self.assertEqual(report["status"], "valid")
            self.assertEqual(report["trial_count"], 5)
            summary = next(
                item
                for item in report["summaries"]
                if item["metric"] == "open_latency_ms" and item["trial_statistic"] == "median"
            )
            self.assertEqual(summary["distribution"]["count"], 5)
            self.assertEqual(summary["distribution"]["median"], 110)
            self.assertEqual(summary["distribution"]["p95"], 128)
            self.assertEqual(summary["distribution"]["mad"], 10)

    def test_multiple_explicit_inputs_aggregate_without_copying_bundles(self) -> None:
        with tempfile.TemporaryDirectory() as first_temp, tempfile.TemporaryDirectory() as second_temp:
            first = BundleBuilder(
                Path(first_temp), run_id="run-one", trial_ids=["1"]
            ).persist()
            second = BundleBuilder(
                Path(second_temp), run_id="run-two", trial_ids=["2"]
            ).persist()

            report = analyze.analyze_many([first, second])

            self.assertEqual(report["run_count"], 2)
            self.assertEqual(report["trial_count"], 2)
            with self.assertRaisesRegex(analyze.InvalidData, "duplicate run bundle"):
                analyze.analyze_many([first, first])

    def test_reports_are_deterministic_and_do_not_expose_paths_or_run_ids(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = BundleBuilder(root, run_id="personal-Hugo-run").persist()
            report = analyze.analyze(bundle)
            first = root / "first"
            second = root / "second"

            analyze.write_reports(report, first)
            analyze.write_reports(report, second)

            self.assertEqual((first / "report.json").read_bytes(), (second / "report.json").read_bytes())
            self.assertEqual((first / "report.md").read_bytes(), (second / "report.md").read_bytes())
            text = (first / "report.json").read_text(encoding="utf-8")
            self.assertNotIn(str(root), text)
            self.assertNotIn("personal-Hugo-run", text)
            self.assertNotIn("recording.mp4", text)

    def test_missing_artifact_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            bundle = BundleBuilder(Path(temporary)).persist()
            (bundle / "server_requests.jsonl").unlink()
            with self.assertRaisesRegex(analyze.InvalidData, "missing required artifact"):
                analyze.analyze(bundle)

        with tempfile.TemporaryDirectory() as temporary:
            bundle = BundleBuilder(Path(temporary)).persist()
            (bundle / "runner-metadata.json").unlink()
            with self.assertRaisesRegex(analyze.InvalidData, "runner-metadata.json"):
                analyze.analyze(bundle)

        with tempfile.TemporaryDirectory() as temporary:
            bundle = BundleBuilder(Path(temporary)).persist()
            (bundle / "collection-result.json").unlink()
            with self.assertRaisesRegex(analyze.InvalidData, "collection-result.json"):
                analyze.analyze(bundle)

    def test_collector_finalization_must_remain_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.collection["duration_ms"] = (
                builder.terminal["elapsed_ms"]
                + analyze.MAX_COLLECTOR_FINALIZATION_DELAY_MS
                + 1
            )
            bundle = builder.persist()

            with self.assertRaisesRegex(analyze.InvalidData, "finalization exceeded"):
                analyze.analyze(bundle)

        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.collection["forced_termination_reason"] = "post-exit-child-grace"
            bundle = builder.persist()

            with self.assertRaisesRegex(analyze.InvalidData, "forced process-tree termination"):
                analyze.analyze(bundle)

    def test_sentinel_root_must_itself_be_the_named_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary)
            builder = BundleBuilder(parent)
            builder.manifest["sentinel_root"] = str(parent.resolve())
            bundle = builder.persist()

            with self.assertRaisesRegex(analyze.InvalidData, "directory named"):
                analyze.analyze(bundle)

    def test_event_loss_or_unfinished_terminal_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.terminal["dropped_counts"]["events"] = 1
            bundle = builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "lost events"):
                analyze.analyze(bundle)

        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.terminal["status"] = "running"
            bundle = builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "unfinished"):
                analyze.analyze(bundle)

        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.events.insert(
                -1,
                builder.envelope(
                    "1",
                    builder.events[-1]["monotonic_ms"] - 1,
                    "frontend",
                    "observer_reconciliation",
                    payload={"local_dropped": 1, "remote_dropped": 0},
                ),
            )
            bundle = builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "event loss"):
                analyze.analyze(bundle)

    def test_non_authoritative_presented_frame_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            presented = next(
                event for event in builder.events if event["kind"] == "seek_presented"
            )
            presented["payload"]["authoritative"] = False

            with self.assertRaisesRegex(analyze.InvalidData, "non-authoritative"):
                analyze.analyze(builder.persist())

    def test_production_scenario_contract_rejects_missing_or_mislabelled_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.manifest["scenarios"][0]["kind"] = "cold_open"
            with self.assertRaisesRegex(analyze.InvalidData, "missing required evidence"):
                analyze.analyze(builder.persist())

        export_events = [
            {"kind": "library_requested"},
            {"kind": "library_useful"},
            {"kind": "export_requested"},
            {
                "kind": "export_completed",
                "payload": {
                    "strategy": "full_reencode",
                    "copy_strategy": "not_implemented",
                    "hybrid_strategy": "not_implemented",
                    "benchmark_scope": "ui_workflow",
                    "ui_workflow_included": False,
                },
            },
            {"kind": "scenario_completed"},
        ]
        with self.assertRaisesRegex(analyze.InvalidData, "measured strategy/scope"):
            analyze._validate_scenario_evidence(
                analyze.TrialKey("export", "1"),
                {"kind": "export"},
                export_events,
            )
        export_events[3]["payload"]["benchmark_scope"] = "backend_command"
        analyze._validate_scenario_evidence(
            analyze.TrialKey("export", "1"),
            {"kind": "export"},
            export_events,
        )

    def test_seek_target_count_ignores_auxiliary_clip_loop_seeks(self) -> None:
        events = [
            {"kind": kind}
            for kind in (
                "library_requested",
                "library_useful",
                "replay_requested",
                "playback_payload_ready",
                "viewer_mounted",
                "media_loadstart",
                "metadata_ready",
                "media_canplay",
                "first_presented_frame",
                "clip_endpoint_updated",
                "scenario_completed",
            )
        ]
        for index in range(2):
            action_id = f"endpoint-{index}"
            events.extend(
                {"kind": kind, "action_id": action_id, "payload": {"reason": "endpoint-edit"}}
                for kind in ("seek_requested", "seek_dispatched", "seeked", "seek_presented")
            )
        events.extend(
            {"kind": kind, "action_id": "clip-loop", "payload": {"reason": "clip-loop"}}
            for kind in ("seek_requested", "seek_dispatched", "seeked", "seek_presented")
        )
        spec = {
            "kind": "seek",
            "seek_reason": "endpoint-edit",
            "target_times_ms": [10_000, 20_000],
        }

        analyze._validate_scenario_evidence(
            analyze.TrialKey("endpoint-edit", "1"), spec, events
        )

        events[11]["payload"]["reason"] = "clip-loop"
        with self.assertRaisesRegex(analyze.InvalidData, "requested count"):
            analyze._validate_scenario_evidence(
                analyze.TrialKey("endpoint-edit", "1"), spec, events
            )

    def test_telemetry_gap_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.samples[1]["monotonic_ms"] = builder.samples[0]["monotonic_ms"] + 3000
            bundle = builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "telemetry gap"):
                analyze.analyze(bundle)

        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.samples[1]["payload"]["cadence_gap"] = True
            bundle = builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "telemetry gap"):
                analyze.analyze(bundle)

    def test_cross_producer_event_arrival_may_be_out_of_monotonic_order(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.events.append(
                builder.envelope(
                    "1",
                    builder.events[0]["monotonic_ms"] - 1,
                    "frontend",
                    "frontend_initialized",
                )
            )

            report = analyze.analyze(builder.persist())

            self.assertEqual(report["status"], "valid")

    def test_top_level_process_sample_fields_are_summarized(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            for sample in builder.samples:
                sample.update(sample.pop("payload"))

            report = analyze.analyze(builder.persist())

            metric_names = {item["metric"] for item in report["summaries"]}
            self.assertIn("process_tree_cpu_percent_mean", metric_names)
            self.assertIn("process_tree_private_bytes_max", metric_names)

    def test_nested_job_and_gpu_samples_are_validated_and_summarized(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            for index, sample in enumerate(builder.samples):
                sample.update(sample.pop("payload"))
                sample["job_unobserved_process_count"] = 0
                sample["gpu"] = {
                    "available": True,
                    "limitation": None,
                    "engines": [{"utilization_percent": 10 + index}],
                    "memory": [{"dedicated_bytes": 1_000 + index, "shared_bytes": 500}],
                }

            report = analyze.analyze(builder.persist())
            metric_names = {item["metric"] for item in report["summaries"]}
            self.assertIn("process_tree_gpu_engine_utilization_percent_mean", metric_names)
            self.assertIn("process_tree_gpu_dedicated_memory_bytes_max", metric_names)
            self.assertIn("gpu_available_sample_count", metric_names)

        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.samples[0]["payload"]["job_unobserved_process_count"] = 1
            with self.assertRaisesRegex(analyze.InvalidData, "missed one or more Job Object"):
                analyze.analyze(builder.persist())

    def test_completed_head_request_declares_but_does_not_deliver_body_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.requests[0].update(
                {"method": "HEAD", "first_byte_ms": None, "delivered_bytes": 0}
            )
            report = analyze.analyze(builder.persist())
            self.assertEqual(report["status"], "valid")

    def test_incomplete_action_and_generation_mismatch_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.events = [event for event in builder.events if event["kind"] != "seek_presented"]
            bundle = builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "incomplete/impossible"):
                analyze.analyze(bundle)

        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.requests[0]["generation"] = 99
            bundle = builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "generation disagrees"):
                analyze.analyze(bundle)

    def test_replaced_pending_seek_is_a_reconciled_terminal_action(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.events = [
                event for event in builder.events if event["kind"] != "seek_presented"
            ]
            seeked = next(event for event in builder.events if event["kind"] == "seeked")
            seeked["kind"] = "seek_pending_replaced"

            report = analyze.analyze(builder.persist())

            self.assertEqual(report["status"], "valid")

    def test_unknown_action_reference_and_duplicate_request_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.requests[0]["action_id"] = "unknown-action"
            bundle = builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "unknown action"):
                analyze.analyze(bundle)

        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.requests.append(dict(builder.requests[0]))
            bundle = builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "duplicate server request_id"):
                analyze.analyze(bundle)

    def test_identity_mismatch_between_preserved_trials_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = BundleBuilder(
                root,
                run_id="run-a",
                trial_ids=["1"],
                fingerprint="environment-a",
            ).persist()
            second_builder = BundleBuilder(
                root,
                run_id="run-b",
                trial_ids=["2"],
                fingerprint="environment-b",
            )
            second = second_builder.persist()
            self.assertTrue(first.is_dir() and second.is_dir())
            with self.assertRaisesRegex(analyze.InvalidData, "inconsistent fingerprints"):
                analyze.analyze(first.parent)

    def test_runner_metadata_is_validated_and_folded_into_identity(self) -> None:
        invalid_cases = {
            "app binary": lambda metadata: metadata.update(
                {"app_binary_sha256": "not-a-hash"}
            ),
            "config SHA-256": lambda metadata: metadata.update(
                {"config_sha256": "a" * 64}
            ),
            "environment omissions": lambda metadata: metadata["environment"].update(
                {"os": None, "limitations": []}
            ),
            "source identity": lambda metadata: metadata["source"].update(
                {"revision": None, "limitation": None}
            ),
            "WebView2 runtime identity": lambda metadata: metadata.update(
                {
                    "webview2_runtime_versions": [],
                    "webview2_runtime_limitation": None,
                }
            ),
            "missing required field": lambda metadata: metadata.pop(
                "webview2_runtime_limitation"
            ),
        }
        for expected_error, mutate in invalid_cases.items():
            with self.subTest(expected_error=expected_error):
                with tempfile.TemporaryDirectory() as temporary:
                    builder = BundleBuilder(Path(temporary))
                    mutate(builder.runner_metadata)
                    with self.assertRaisesRegex(analyze.InvalidData, expected_error):
                        analyze.analyze(builder.persist())

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = BundleBuilder(root, run_id="run-a", trial_ids=["1"]).persist()
            second_builder = BundleBuilder(root, run_id="run-b", trial_ids=["2"])
            second_builder.runner_metadata["environment"]["os"]["version"] = "different"
            second_builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "inconsistent fingerprints"):
                analyze.analyze(first.parent)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = BundleBuilder(root, run_id="run-a", trial_ids=["1"]).persist()
            second_builder = BundleBuilder(root, run_id="run-b", trial_ids=["2"])
            second_builder.runner_metadata["app_binary_sha256"] = "e" * 64
            second_builder.persist()
            with self.assertRaisesRegex(analyze.InvalidData, "changes app identity"):
                analyze.analyze(first.parent)

    def test_comparison_applies_repeatability_and_five_percent_contract(self) -> None:
        with tempfile.TemporaryDirectory() as baseline_temp, tempfile.TemporaryDirectory() as after_temp:
            baseline = analyze.analyze(
                BundleBuilder(
                    Path(baseline_temp),
                    trial_ids=["1", "2", "3", "4", "5"],
                    latencies=[98, 99, 100, 101, 102],
                ).persist()
            )
            after = analyze.analyze(
                BundleBuilder(
                    Path(after_temp),
                    trial_ids=["1", "2", "3", "4", "5"],
                    latencies=[118, 119, 120, 121, 122],
                ).persist()
            )

            comparison = analyze.compare_reports(baseline, after)

            metric = next(
                item
                for item in comparison["metrics"]
                if item["metric"] == "open_latency_ms" and item["trial_statistic"] == "median"
            )
            self.assertTrue(metric["eligible"])
            self.assertTrue(metric["disposition_requiring"])
            self.assertEqual(metric["repeatability_band"], 5)
            self.assertTrue(comparison["disposition_required"])

    def test_malformed_comparison_report_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            current = analyze.analyze(BundleBuilder(Path(temporary)).persist())
            malformed = json.loads(json.dumps(current))
            malformed["summaries"][0].pop("distribution")

            with self.assertRaisesRegex(analyze.InvalidData, "distribution"):
                analyze.compare_reports(malformed, current)

    def test_comparison_requires_balanced_trials_and_matching_identity(self) -> None:
        with tempfile.TemporaryDirectory() as baseline_temp, tempfile.TemporaryDirectory() as after_temp:
            baseline = analyze.analyze(BundleBuilder(Path(baseline_temp)).persist())
            after = analyze.analyze(BundleBuilder(Path(after_temp)).persist())
            comparison = analyze.compare_reports(baseline, after)
            self.assertTrue(comparison["metrics"])
            self.assertTrue(all(not item["eligible"] for item in comparison["metrics"]))

        with tempfile.TemporaryDirectory() as baseline_temp, tempfile.TemporaryDirectory() as after_temp:
            baseline = analyze.analyze(
                BundleBuilder(Path(baseline_temp), fingerprint="a").persist()
            )
            after = analyze.analyze(BundleBuilder(Path(after_temp), fingerprint="b").persist())
            with self.assertRaisesRegex(analyze.InvalidData, "fingerprint differs"):
                analyze.compare_reports(baseline, after)

        with tempfile.TemporaryDirectory() as baseline_temp, tempfile.TemporaryDirectory() as after_temp:
            baseline_builder = BundleBuilder(Path(baseline_temp))
            after_builder = BundleBuilder(Path(after_temp))
            baseline_builder.manifest["scenarios"][0]["seed"] = 1
            after_builder.manifest["scenarios"][0]["seed"] = 2
            baseline = analyze.analyze(baseline_builder.persist())
            after = analyze.analyze(after_builder.persist())
            with self.assertRaisesRegex(analyze.InvalidData, "fingerprint differs"):
                analyze.compare_reports(baseline, after)

        with tempfile.TemporaryDirectory() as baseline_temp, tempfile.TemporaryDirectory() as after_temp:
            baseline = analyze.analyze(
                BundleBuilder(
                    Path(baseline_temp),
                    trial_ids=["1", "2", "3", "4", "5"],
                ).persist()
            )
            after = analyze.analyze(
                BundleBuilder(
                    Path(after_temp),
                    trial_ids=["1", "2", "3", "4", "5", "6"],
                ).persist()
            )
            comparison = analyze.compare_reports(baseline, after)
            self.assertTrue(all(not item["eligible"] for item in comparison["metrics"]))

    def test_terminal_payload_envelope_and_changed_app_subject_are_supported(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            payload = {
                key: builder.terminal.pop(key)
                for key in (
                    "status",
                    "completed",
                    "record_counts",
                    "dropped_counts",
                    "source_hash_unchanged",
                    "media_integrity_ok",
                    "export_integrity_ok",
                )
            }
            builder.terminal["payload"] = payload
            report = analyze.analyze(builder.persist())
            self.assertEqual(report["status"], "valid")

        with tempfile.TemporaryDirectory() as baseline_temp, tempfile.TemporaryDirectory() as after_temp:
            baseline_builder = BundleBuilder(
                Path(baseline_temp), trial_ids=["1", "2", "3", "4", "5"]
            )
            after_builder = BundleBuilder(
                Path(after_temp), trial_ids=["1", "2", "3", "4", "5"]
            )
            after_builder.manifest["fingerprints"]["app_binary_sha256"] = "d" * 64
            baseline = analyze.analyze(baseline_builder.persist())
            after = analyze.analyze(after_builder.persist())
            comparison = analyze.compare_reports(baseline, after)
            self.assertTrue(comparison["metrics"])
            self.assertNotEqual(
                comparison["metrics"][0]["baseline_subject_sha256"],
                comparison["metrics"][0]["current_subject_sha256"],
            )

    def test_prepared_fixture_file_list_and_plural_fixture_ids_are_supported(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.manifest["fixtures"] = [
                {
                    "id": "representative-h264",
                    "kind": "recording_bundle",
                    "relative_path": "games\\representative-h264",
                    "files": [
                        {
                            "relative_path": "games\\representative-h264\\video.mp4",
                            "size_bytes": 123456,
                            "sha256": "b" * 64,
                            "last_write_utc": "2026-08-24T12:00:00Z",
                        }
                    ],
                }
            ]
            scenario = builder.manifest["scenarios"][0]
            scenario.pop("fixture_id")
            scenario.pop("expected_action_count")
            scenario["fixture_ids"] = ["representative-h264"]

            report = analyze.analyze(builder.persist())

            self.assertEqual(report["trial_count"], 1)

    def test_export_validation_is_required_and_publishes_sanitized_metrics(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.enable_export_validation()
            report = analyze.analyze(builder.persist())

            summaries = {
                (item["metric"], item["trial_statistic"]): item
                for item in report["summaries"]
            }
            self.assertEqual(
                summaries[("export_validation_elapsed_ms", "median")]["distribution"][
                    "median"
                ],
                125.5,
            )
            self.assertEqual(
                summaries[("export_output_size_bytes", "median")]["distribution"][
                    "median"
                ],
                20_000_000,
            )
            self.assertEqual(
                summaries[("export_output_duration_ms", "median")]["distribution"][
                    "median"
                ],
                10_000,
            )
            serialized = json.dumps(report, sort_keys=True)
            self.assertNotIn("clips\\", serialized)
            self.assertNotIn("1700000000_1700000100", serialized)

        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.enable_export_validation()
            builder.export_validation = None
            with self.assertRaisesRegex(analyze.InvalidData, "export-validation.json"):
                analyze.analyze(builder.persist())

    def test_export_validation_rejects_invalid_integrity_evidence(self) -> None:
        cases = {
            "all_valid": lambda value: value.update({"all_valid": False}),
            "zero errors": lambda value: value["errors"].append("decode failed"),
            "output count": lambda value: value.update({"expected_output_count": 2}),
            "scenario_id": lambda value: value.update({"scenario_id": "wrong-scenario"}),
            "clips_root": lambda value: value.update({"clips_root": "outside\\clips"}),
            "sentinel-relative": lambda value: value["outputs"][0].update(
                {"relative_path": "clips\\..\\escaped.mp4"}
            ),
            "SHA-256": lambda value: value["outputs"][0].update({"sha256": "bad"}),
            "full decode": lambda value: value["outputs"][0].update(
                {"full_single_thread_decode_ok": False}
            ),
            "decode stderr": lambda value: value["outputs"][0].update(
                {"decode_stderr": None}
            ),
            "codec/stream": lambda value: value["outputs"][0]["ffprobe"]["streams"][
                0
            ].update({"codec_name": "hevc"}),
            "duration": lambda value: value["outputs"][0].update(
                {"duration_ms": 12_000}
            ),
            "elapsed": lambda value: value.update({"validation_elapsed_ms": -1}),
            "empty created output": lambda value: value["newly_created_entries"][1].update(
                {"size_bytes": 0}
            ),
        }
        for expected_error, mutate in cases.items():
            with self.subTest(expected_error=expected_error):
                with tempfile.TemporaryDirectory() as temporary:
                    builder = BundleBuilder(Path(temporary))
                    builder.enable_export_validation()
                    assert builder.export_validation is not None
                    mutate(builder.export_validation)
                    with self.assertRaisesRegex(analyze.InvalidData, expected_error):
                        analyze.analyze(builder.persist())

        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            builder.enable_export_validation(preset="discord")
            assert builder.export_validation is not None
            builder.export_validation["outputs"][0]["discord_size_valid"] = False
            with self.assertRaisesRegex(analyze.InvalidData, "Discord"):
                analyze.analyze(builder.persist())

    def test_production_event_shapes_publish_only_known_direct_metrics(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            event = builder.envelope
            output = {
                "filename": "opaque-output-id",
                "file_size_bytes": 8_000_000,
                "strategy": "full_reencode",
                "encoder_used": "h264_nvenc",
                "encode_elapsed_ms": 2_500,
                "thumbnail_elapsed_ms": 75,
                "retry_count": 1,
            }
            events = [
                event("1", 90, "app", "library_games_requested"),
                event("1", 100, "frontend", "library_requested"),
                event(
                    "1",
                    140,
                    "frontend",
                    "library_useful",
                    payload={"game_count": 4, "clip_count": 2},
                ),
                event("1", 200, "frontend", "replay_requested"),
                event(
                    "1",
                    205,
                    "app",
                    "playback_payload_requested",
                    payload={"game_timestamp": "private-game-id"},
                ),
                event(
                    "1",
                    225,
                    "app",
                    "playback_payload_backend_ready",
                    payload={"game_timestamp": "private-game-id", "elapsed_ms": 20},
                ),
                event(
                    "1",
                    240,
                    "viewer",
                    "playback_payload_ready",
                    payload={"event_count": 50, "participant_count": 10, "duration_ms": 900_000},
                ),
                event(
                    "1",
                    245,
                    "viewer",
                    "viewer_mounted",
                    payload={"duration_ms": 900_000, "recording_fps": 60, "has_video_url": True},
                ),
                event("1", 250, "viewer", "media_loadstart", payload={"network_state": 2}),
                event(
                    "1",
                    270,
                    "viewer",
                    "metadata_ready",
                    payload={
                        "duration_ms": 900_000,
                        "ready_state": 1,
                        "video_width": 1920,
                        "video_height": 1080,
                    },
                ),
                event("1", 280, "viewer", "media_canplay", payload={"ready_state": 3}),
                event(
                    "1",
                    300,
                    "viewer",
                    "first_presented_frame",
                    payload={"media_time_ms": 0, "ready_state": 3},
                ),
                event(
                    "1",
                    320,
                    "viewer",
                    "rate_requested",
                    payload={"rate": 2.0, "timestamp": 123456, "event_id": 987},
                ),
                event(
                    "1",
                    321,
                    "viewer",
                    "rate_applied",
                    payload={"requested_rate": 2.0, "actual_rate": 2.0},
                ),
                event(
                    "1",
                    1_320,
                    "viewer",
                    "rate_observed",
                    payload={
                        "requested_rate": 2.0,
                        "actual_rate": 2.0,
                        "effective_rate": 1.98,
                        "muted": False,
                        "volume": 1,
                        "dropped_frames": 3,
                    },
                ),
                event("1", 1_400, "viewer", "scrub_started", payload={"request_count": 6}),
                event(
                    "1",
                    1_525,
                    "viewer",
                    "scrub_settled",
                    payload={"request_count": 6, "media_time_ms": 15_000},
                ),
                event(
                    "1",
                    1_600,
                    "frontend",
                    "export_progress",
                    payload={
                        "stage": "encoding",
                        "percent": 50,
                        "preset": "discord",
                        "completed_outputs": 0,
                        "total_outputs": 1,
                    },
                ),
                event(
                    "1",
                    4_200,
                    "app",
                    "export_backend_completed",
                    payload={
                        "elapsed_ms": 2_600,
                        "setup_elapsed_ms": 15,
                        "source_probe_elapsed_ms": 2,
                        "finalize_elapsed_ms": 10,
                        "observed_command_ms": 2_605,
                        "total_file_size_bytes": 8_000_000,
                        "outputs": [output],
                    },
                ),
                event(
                    "1",
                    4_210,
                    "frontend",
                    "export_completed",
                    payload={
                        "elapsed_ms": 2_600,
                        "setup_elapsed_ms": 15,
                        "source_probe_elapsed_ms": 2,
                        "finalize_elapsed_ms": 10,
                        "total_file_size_bytes": 8_000_000,
                        "throughput_bytes_per_second": 3_076_923.0,
                        "outputs": [output],
                        "strategy": "full_reencode",
                    },
                ),
                event(
                    "1",
                    4_220,
                    "viewer",
                    "scenario_completed",
                    payload={"playback_rate": 2, "total_frames": 120, "dropped_frames": 3},
                ),
            ]

            metrics = analyze._trial_metrics(
                analyze.TrialKey("seek-far-forward", "1"), events, [], []
            )

            self.assertEqual(metrics["library_request_to_useful_ms"], [50.0])
            self.assertEqual(metrics["replay_request_to_payload_backend_ready_ms"], [25.0])
            self.assertEqual(metrics["replay_request_to_payload_ready_ms"], [40.0])
            self.assertEqual(metrics["replay_request_to_first_presented_frame_ms"], [100.0])
            self.assertEqual(metrics["playback_payload_backend_elapsed_ms"], [20.0])
            self.assertEqual(metrics["effective_playback_rate"], [1.98])
            self.assertEqual(metrics["scenario_total_frames"], [120.0])
            self.assertEqual(metrics["scenario_dropped_frames"], [3.0])
            self.assertEqual(metrics["scrub_settle_ms"], [125.0])
            self.assertEqual(metrics["export_progress_encoding_event_count"], [1.0])
            self.assertEqual(metrics["export_backend_observed_command_ms"], [2_605.0])
            self.assertEqual(metrics["export_throughput_bytes_per_second"], [3_076_923.0])
            self.assertEqual(metrics["export_backend_output_encode_elapsed_ms"], [2_500.0])
            self.assertEqual(metrics["export_output_encode_elapsed_ms"], [2_500.0])
            self.assertEqual(
                analyze._metric_unit("export_throughput_bytes_per_second"),
                "bytes_per_second",
            )
            self.assertEqual(analyze._direction("export_throughput_bytes_per_second"), "higher")
            self.assertNotIn("timestamp", metrics)
            self.assertNotIn("event_id", metrics)
            self.assertNotIn("game_timestamp", metrics)

            shared_name_events = [
                event("1", 10, "frontend", "replay_requested"),
                event(
                    "1",
                    25,
                    "app",
                    "playback_payload_ready",
                    payload={"elapsed_ms": 12},
                ),
                event(
                    "1",
                    40,
                    "frontend",
                    "playback_payload_ready",
                    payload={"event_count": 50, "participant_count": 10},
                ),
            ]
            shared_metrics = analyze._trial_metrics(
                analyze.TrialKey("seek-far-forward", "1"),
                shared_name_events,
                [],
                [],
            )
            self.assertEqual(shared_metrics["playback_payload_backend_elapsed_ms"], [12.0])
            self.assertEqual(
                shared_metrics["replay_request_to_payload_backend_ready_ms"], [15.0]
            )
            self.assertEqual(shared_metrics["replay_request_to_payload_ready_ms"], [30.0])

    def test_scenario_trial_id_is_the_preferred_single_trial_contract(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            scenario = builder.manifest["scenarios"][0]
            scenario["trial_id"] = "1"
            scenario["trial_count"] = 99
            scenario.pop("trial_ids")

            report = analyze.analyze(builder.persist())

            self.assertEqual(report["trial_count"], 1)
            self.assertEqual(report["trials"][0]["trial_alias"], "trial-001")

    def test_new_sustained_monotonic_resource_growth_requires_disposition(self) -> None:
        trial_ids = ["1", "2", "3", "4", "5"]
        with tempfile.TemporaryDirectory() as baseline_temp, tempfile.TemporaryDirectory() as after_temp:
            baseline_builder = BundleBuilder(Path(baseline_temp), trial_ids=trial_ids)
            after_builder = BundleBuilder(Path(after_temp), trial_ids=trial_ids)
            for builder, monotonic in ((baseline_builder, False), (after_builder, True)):
                additions = []
                for trial_id in trial_ids:
                    trial_samples = [
                        sample for sample in builder.samples if sample["trial_id"] == trial_id
                    ]
                    first_memory = trial_samples[0]["payload"]["process_tree_private_bytes"]
                    trial_samples[1]["payload"]["process_tree_private_bytes"] = (
                        first_memory + 5 if monotonic else first_memory - 5
                    )
                    additions.append(
                        builder.envelope(
                            trial_id,
                            trial_samples[1]["monotonic_ms"] + 500,
                            "collector",
                            "process_sample",
                            payload={
                                "process_count": 3,
                                "process_tree_cpu_percent": 2.5,
                                "process_tree_private_bytes": first_memory + 10,
                            },
                        )
                    )
                builder.samples.extend(additions)
                builder.samples.sort(key=lambda sample: sample["monotonic_ms"])
            baseline = analyze.analyze(baseline_builder.persist())
            after = analyze.analyze(after_builder.persist())

            comparison = analyze.compare_reports(baseline, after)

            growth = next(
                item
                for item in comparison["metrics"]
                if item["metric"]
                == "process_tree_private_bytes_sustained_monotonic_growth_flag"
                and item["trial_statistic"] == "median"
            )
            self.assertTrue(growth["hard_disposition_signal"])
            self.assertTrue(growth["disposition_requiring"])

    def test_observer_control_gate_passes_four_matched_pairs(self) -> None:
        with tempfile.TemporaryDirectory() as minimal_temp, tempfile.TemporaryDirectory() as full_temp:
            minimal, full = self.control_reports(Path(minimal_temp), Path(full_temp))

            control = analyze.validate_observer_control(minimal, full)

            self.assertTrue(control["gate_passed"])
            self.assertEqual(control["status"], "passed")
            self.assertEqual(control["matched_pair_count"], 4)
            self.assertTrue(control["outcomes_match"])
            self.assertTrue(all(gate["passed"] for gate in control["gates"].values()))
            for trial in minimal["trials"] + full["trials"]:
                evidence = trial["observer_control_evidence"]
                self.assertEqual(evidence["action_latency_scope"], "seek")
                self.assertEqual(len(evidence["action_latency_ms"]), 1)
            self.assertEqual(
                control["gates"]["aggregate_process_tree_cpu_time"][
                    "accounting_quantum_floor_ms"
                ],
                187.5,
            )

    def test_outcome_digest_ignores_transport_timing_and_frame_snapshot_noise(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            builder = BundleBuilder(Path(temporary))
            events = copy.deepcopy(builder.events)
            requests = copy.deepcopy(builder.requests)

            def cancelled_request(
                request_id: str,
                monotonic_ms: float,
                range_start: int,
                range_end: int,
                delivered_bytes: int,
            ) -> dict[str, Any]:
                return builder.envelope(
                    "1",
                    monotonic_ms,
                    "server",
                    "server_request",
                    payload={
                        "request_id": request_id,
                        "route_class": "game_video",
                        "method": "GET",
                        "outcome": "cancelled",
                        "status": 206,
                        "range_start": range_start,
                        "range_end": range_end,
                        "declared_bytes": range_end - range_start + 1,
                        "delivered_bytes": delivered_bytes,
                    },
                )

            minimal_requests = requests + [
                cancelled_request("cancel-minimal", 10_031, 4_096, 8_191, 512)
            ]
            full_requests = requests + [
                cancelled_request("cancel-full-a", 10_025, 8_192, 16_383, 0),
                cancelled_request("cancel-full-b", 10_045, 16_384, 32_767, 2_048),
            ]
            full_requests[0]["payload"].update(
                {
                    "range_start": 196_608,
                    "range_end": 200_703,
                    "declared_bytes": 4_096,
                    "delivered_bytes": 4_096,
                }
            )
            duplicate_completed_class = copy.deepcopy(full_requests[0])
            duplicate_completed_class["payload"].update(
                {
                    "request_id": "completed-full-b",
                    "range_start": 262_144,
                    "range_end": 266_239,
                }
            )
            full_requests.append(duplicate_completed_class)
            full_events = events + [
                builder.envelope(
                    "1",
                    10_075,
                    "viewer",
                    "video_quality_snapshot",
                    payload={"total_frames": 500, "dropped_frames": 7},
                )
            ]

            expected = analyze._outcome_digest(events, minimal_requests)
            self.assertEqual(expected, analyze._outcome_digest(full_events, full_requests))

            changed_status = copy.deepcopy(minimal_requests)
            changed_status[0]["payload"]["status"] = 200
            changed_status[0]["payload"]["range_start"] = None
            changed_status[0]["payload"]["range_end"] = None
            self.assertNotEqual(expected, analyze._outcome_digest(events, changed_status))

            changed_action = copy.deepcopy(events)
            next(event for event in changed_action if event["kind"] == "seek_requested")[
                "payload"
            ]["target_ms"] = 2_000
            self.assertNotEqual(
                expected, analyze._outcome_digest(changed_action, minimal_requests)
            )

            for outcome_kind in ("media_error", "recovery_attempted"):
                changed_events = events + [
                    builder.envelope("1", 10_090, "viewer", outcome_kind)
                ]
                self.assertNotEqual(
                    expected,
                    analyze._outcome_digest(changed_events, minimal_requests),
                )

    def test_observer_control_gate_reports_threshold_failures(self) -> None:
        with tempfile.TemporaryDirectory() as minimal_temp, tempfile.TemporaryDirectory() as full_temp:
            minimal, full = self.control_reports(
                Path(minimal_temp),
                Path(full_temp),
                full_latency=120.0,
                full_memory_increase=17 * 1024 * 1024,
                full_cpu_time_ms=1_100.0,
            )

            control = analyze.validate_observer_control(minimal, full)

            self.assertFalse(control["gate_passed"])
            self.assertEqual(control["status"], "failed")
            self.assertFalse(control["gates"]["median_action_latency"]["passed"])
            self.assertFalse(control["gates"]["p95_action_latency"]["passed"])
            self.assertFalse(control["gates"]["maximum_private_memory"]["passed"])
            self.assertFalse(
                control["gates"]["aggregate_process_tree_cpu_time"]["passed"]
            )

    def test_observer_control_gates_material_schedule_and_frame_regressions(self) -> None:
        with tempfile.TemporaryDirectory() as minimal_temp, tempfile.TemporaryDirectory() as full_temp:
            minimal, full = self.control_reports(Path(minimal_temp), Path(full_temp))
            for trial in full["trials"]:
                evidence = trial["observer_control_evidence"]
                evidence.update(
                    {
                        "server_request_count": 3.0,
                        "server_completed_count": 3.0,
                        "server_error_count": 2.0,
                        "server_range_count": 3.0,
                        "server_cancelled_count": 2.0,
                        "server_delivered_bytes": 12_288.0,
                        "scenario_dropped_frames": 3.0,
                        "scenario_dropped_frame_rate": 0.003,
                    }
                )

            control = analyze.validate_observer_control(minimal, full)

            self.assertFalse(control["gate_passed"])
            for gate_name in (
                "server_request_count",
                "server_completed_count",
                "server_error_count",
                "server_range_count",
                "server_cancelled_count",
                "server_delivered_bytes",
                "scenario_dropped_frame_rate",
            ):
                self.assertFalse(control["gates"][gate_name]["passed"], gate_name)

    def test_observer_control_rejects_too_few_or_mismatched_pairs(self) -> None:
        with tempfile.TemporaryDirectory() as minimal_temp, tempfile.TemporaryDirectory() as full_temp:
            minimal, full = self.control_reports(
                Path(minimal_temp), Path(full_temp), pair_count=3
            )
            with self.assertRaisesRegex(analyze.InvalidData, "at least four"):
                analyze.validate_observer_control(minimal, full)

        with tempfile.TemporaryDirectory() as minimal_temp, tempfile.TemporaryDirectory() as full_temp:
            minimal, full = self.control_reports(Path(minimal_temp), Path(full_temp))
            full["trials"][0]["outcome_sha256"] = "f" * 64
            with self.assertRaisesRegex(analyze.InvalidData, "outcomes differ"):
                analyze.validate_observer_control(minimal, full)

        with tempfile.TemporaryDirectory() as minimal_temp, tempfile.TemporaryDirectory() as full_temp:
            minimal, full = self.control_reports(Path(minimal_temp), Path(full_temp))
            minimal["evidence_guarantees"]["required_request_loss"] = 1
            with self.assertRaisesRegex(analyze.InvalidData, "zero required-event/request loss"):
                analyze.validate_observer_control(minimal, full)

        with tempfile.TemporaryDirectory() as minimal_temp, tempfile.TemporaryDirectory() as full_temp:
            minimal, full = self.control_reports(Path(minimal_temp), Path(full_temp))
            full["trials"][0]["subject_sha256"] = "e" * 64
            with self.assertRaisesRegex(analyze.InvalidData, "app identity differs"):
                analyze.validate_observer_control(minimal, full)

        with tempfile.TemporaryDirectory() as minimal_temp, tempfile.TemporaryDirectory() as full_temp:
            minimal, full = self.control_reports(Path(minimal_temp), Path(full_temp))
            minimal["trials"][0]["observer_control_evidence"]["duration_ms"] = 59_999
            with self.assertRaisesRegex(analyze.InvalidData, "shorter than 60 seconds"):
                analyze.validate_observer_control(minimal, full)

    def test_help_is_available(self) -> None:
        result = subprocess.run(
            [sys.executable, str(ANALYZER_PATH), "--help"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--input", result.stdout)
        self.assertIn("--run-root", result.stdout)
        self.assertIn("--compare", result.stdout)
        self.assertIn("--observer-control", result.stdout)


if __name__ == "__main__":
    unittest.main()
