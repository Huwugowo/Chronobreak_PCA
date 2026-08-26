from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any


MATRIX_PATH = Path(__file__).resolve().parents[1] / "matrix.py"
SCHEMA_ROOT = MATRIX_PATH.parent / "schemas"
SPEC = importlib.util.spec_from_file_location("replay_benchmark_matrix", MATRIX_PATH)
assert SPEC is not None and SPEC.loader is not None
matrix = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = matrix
SPEC.loader.exec_module(matrix)


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


@unittest.skipUnless(sys.platform == "win32", "replay launch manifests use absolute Windows paths")
class MatrixPlannerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.sentinel = Path(self.temporary.name) / matrix.SENTINEL_NAME
        for relative in (
            "library",
            "config",
            "app-data/ddragon",
            "results",
            "scratch",
            "inputs",
            "matrix-plans",
        ):
            (self.sentinel / relative).mkdir(parents=True, exist_ok=True)
        (self.sentinel / "config" / "config.toml").write_text(
            "[storage]\nauto_delete_days = 0\n", encoding="utf-8"
        )
        self.template_path = (self.sentinel / "inputs" / "prepared-template.json").resolve()
        self.specification_path = (self.sentinel / "inputs" / "matrix-spec.json").resolve()
        self.template = self.make_template()
        self.specification = self.make_specification()
        self.write_sources()

    def make_template(self) -> dict[str, Any]:
        run_id = "prepared-template"
        fixture = {
            "id": "representative-h264",
            "alias": "representative-h264",
            "game_timestamp": "1700000000",
            "kind": "recording_bundle",
            "relative_path": "games\\1700000000",
            "backend": "native",
            "codec": "h264",
            "negative": False,
            # The path deliberately does not exist. Planning must carry this identity,
            # never open or hash a media body.
            "files": [
                {
                    "relative_path": "games\\1700000000\\video.mp4",
                    "size_bytes": 999999999,
                    "sha256": "a" * 64,
                    "last_write_utc": "2026-08-25T00:00:00Z",
                }
            ],
            "media_validation": [],
        }
        return {
            "schema_version": 1,
            "run_id": run_id,
            "sentinel_root": str(self.sentinel.resolve()),
            "library_root": str((self.sentinel / "library").resolve()),
            "config_path": str((self.sentinel / "config" / "config.toml").resolve()),
            "app_data_root": str((self.sentinel / "app-data").resolve()),
            "result_root": str((self.sentinel / "results" / run_id).resolve()),
            "scratch_root": str((self.sentinel / "scratch").resolve()),
            "observer_profile": "full",
            "ddragon": {
                "mode": "offline",
                "cache_root": str((self.sentinel / "app-data" / "ddragon").resolve()),
                "cache_fingerprint": "sha256:" + "b" * 64,
            },
            "fixtures": [fixture],
            "scenarios": [
                {
                    "id": "prepared-placeholder",
                    "kind": "cold_open",
                    "fixture_ids": ["representative-h264"],
                    "trial_id": "prepared-placeholder",
                }
            ],
            "app_binary": r"C:\QueueBack\release\league-replay-app.exe",
            "timeout_seconds": 3600,
        }

    @staticmethod
    def control_scenario() -> dict[str, Any]:
        return {
            "id": "representative-seek-control",
            "kind": "seek",
            "fixture_ids": ["representative-h264"],
            "seek_reason": "benchmark",
            "seek_playback_mode": "paused",
            "warmup_seconds": 5,
            "duration_seconds": 60,
            "target_times_ms": [10000, 30000, 90000],
            "distance_classes": ["near", "far", "forward", "backward"],
            "observer_control": True,
        }

    def make_specification(self) -> dict[str, Any]:
        return {
            "schema_version": 1,
            "matrix_id": "baseline-test",
            "seed": 20260825,
            "cooldown_seconds": 120,
            "plan_root": str(
                (self.sentinel / "matrix-plans" / "baseline-test").resolve()
            ),
            "arms": [
                {
                    "id": "cold-open-full",
                    "observer_profile": "full",
                    "repetitions": 2,
                    "scenario": {
                        "id": "representative-cold-open",
                        "kind": "cold_open",
                        "fixture_ids": ["representative-h264"],
                        "warmup_seconds": 5,
                    },
                },
                {
                    "id": "seek-control-minimal",
                    "observer_profile": "minimal",
                    "repetitions": 2,
                    "observer_control": {
                        "group_id": "seek-observer-control",
                        "role": "minimal",
                    },
                    "scenario": self.control_scenario(),
                },
                {
                    "id": "seek-control-full",
                    "observer_profile": "full",
                    "repetitions": 2,
                    "observer_control": {
                        "group_id": "seek-observer-control",
                        "role": "full",
                    },
                    "scenario": self.control_scenario(),
                },
            ],
            "order": [
                {"arm_id": "cold-open-full", "trial_id": "cold-01"},
                {
                    "arm_id": "seek-control-minimal",
                    "trial_id": "pair-01",
                    "pair_id": "pair-01",
                },
                {
                    "arm_id": "seek-control-full",
                    "trial_id": "pair-01",
                    "pair_id": "pair-01",
                },
                {"arm_id": "cold-open-full", "trial_id": "cold-02"},
                {
                    "arm_id": "seek-control-full",
                    "trial_id": "pair-02",
                    "pair_id": "pair-02",
                },
                {
                    "arm_id": "seek-control-minimal",
                    "trial_id": "pair-02",
                    "pair_id": "pair-02",
                },
            ],
        }

    def write_sources(self) -> None:
        write_json(self.template_path, self.template)
        write_json(self.specification_path, self.specification)

    def create_result_bundles(self, plan: dict[str, Any]) -> None:
        base = datetime(2026, 8, 25, tzinfo=timezone.utc)
        for index, launch in enumerate(plan["launches"]):
            launch_manifest = json.loads(Path(launch["manifest_path"]).read_text(encoding="utf-8"))
            scenario = launch_manifest["scenarios"][0]
            result_root = Path(launch["result_root"])
            result_root.mkdir()
            started = base + timedelta(seconds=index * 180)
            completed = started + timedelta(seconds=30)
            write_json(result_root / "manifest.json", launch_manifest)
            write_json(
                result_root / "terminal.json",
                {
                    "schema_version": 1,
                    "run_id": launch["run_id"],
                    "scenario_id": scenario["id"],
                    "trial_id": scenario["trial_id"],
                    "status": "complete",
                    "finished": True,
                    "telemetry_complete": True,
                    "media_valid": True,
                    "source_hash_unchanged": True,
                },
            )
            write_json(
                result_root / "runner-metadata.json",
                {
                    "schema_version": 1,
                    "run_id": launch["run_id"],
                    "started_utc": started.isoformat().replace("+00:00", "Z"),
                },
            )
            write_json(
                result_root / "runner-result.json",
                {
                    "schema_version": 1,
                    "run_id": launch["run_id"],
                    "status": "complete",
                    "completed_utc": completed.isoformat().replace("+00:00", "Z"),
                },
            )
        final_launch = plan["launches"][-1]
        final_manifest = json.loads(
            Path(final_launch["manifest_path"]).read_text(encoding="utf-8")
        )
        final_root = Path(final_launch["result_root"])
        final_started = base + timedelta(seconds=(len(plan["launches"]) - 1) * 180)
        write_json(
            final_root / "post-hashes.json",
            {
                "schema_version": 1,
                "checked_utc": (final_started + timedelta(seconds=20))
                .isoformat()
                .replace("+00:00", "Z"),
                "all_match": True,
                "benchmark_config": {
                    "expected_sha256": "c" * 64,
                    "actual_sha256": "c" * 64,
                    "exists": True,
                    "match": True,
                },
                "ddragon_cache": {
                    "expected_fingerprint": final_manifest["ddragon"]["cache_fingerprint"],
                    "actual_fingerprint": final_manifest["ddragon"]["cache_fingerprint"],
                    "exists": True,
                    "match": True,
                },
                "files": [
                    {
                        "fixture_id": fixture["id"],
                        "relative_path": file_identity["relative_path"],
                        "expected_sha256": file_identity["sha256"],
                        "actual_sha256": file_identity["sha256"],
                        "exists": True,
                        "match": True,
                    }
                    for fixture in final_manifest["fixtures"]
                    for file_identity in fixture["files"]
                ],
            },
        )

    def test_plan_expands_deterministic_immutable_launches(self) -> None:
        fixture_identity = copy.deepcopy(self.template["fixtures"])
        plan_path = matrix.plan_matrix(self.template_path, self.specification_path)
        plan = json.loads(plan_path.read_text(encoding="utf-8"))

        self.assertEqual(plan["expected_arm_count"], 3)
        self.assertEqual(plan["expected_trial_count"], 6)
        self.assertEqual(plan["seed"], 20260825)
        self.assertEqual(plan["cooldown_seconds"], 120.0)
        self.assertEqual([entry["sequence"] for entry in plan["order"]], list(range(1, 7)))
        self.assertEqual(len({item["run_id"] for item in plan["launches"]}), 6)
        self.assertEqual(len({item["result_root"] for item in plan["launches"]}), 6)

        for launch in plan["launches"]:
            manifest_path = Path(launch["manifest_path"])
            generated = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(generated["fixtures"], fixture_identity)
            self.assertEqual(generated["run_id"], launch["run_id"])
            self.assertEqual(generated["result_root"], launch["result_root"])
            self.assertEqual(generated["observer_profile"], launch["observer_profile"])
            self.assertEqual(generated["scenarios"][0]["trial_id"], launch["trial_id"])
            self.assertEqual(generated["scenarios"][0]["seed"], 20260825)
            self.assertFalse(Path(launch["result_root"]).exists())

        self.assertEqual(
            matrix.verify_matrix(plan_path),
            {"arm_count": 3, "trial_count": 6, "result_count": 0},
        )

    def test_plan_can_override_the_launch_watchdog_for_one_bounded_arm(self) -> None:
        self.specification["timeout_seconds"] = 180
        self.specification["arms"][0]["timeout_seconds"] = 900
        self.write_sources()
        plan_path = matrix.plan_matrix(self.template_path, self.specification_path)
        plan = json.loads(plan_path.read_text(encoding="utf-8"))

        self.assertEqual(plan["timeout_seconds"], 180)
        for launch in plan["launches"]:
            manifest = json.loads(Path(launch["manifest_path"]).read_text(encoding="utf-8"))
            expected = 900 if launch["arm_id"] == "cold-open-full" else 180
            self.assertEqual(manifest["timeout_seconds"], expected)

    def test_matrix_timeout_override_is_finite_and_bounded(self) -> None:
        self.specification["timeout_seconds"] = 29
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "timeout_seconds"):
            matrix.plan_matrix(self.template_path, self.specification_path)

        self.specification = self.make_specification()
        self.specification["arms"][0]["timeout_seconds"] = 29
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "timeout_seconds"):
            matrix.plan_matrix(self.template_path, self.specification_path)

    def test_observer_control_requires_matching_effective_timeouts(self) -> None:
        self.specification["timeout_seconds"] = 180
        self.specification["arms"][1]["timeout_seconds"] = 900
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "timeouts do not match"):
            matrix.plan_matrix(self.template_path, self.specification_path)

    def test_plan_refuses_to_overwrite_existing_plan_root(self) -> None:
        matrix.plan_matrix(self.template_path, self.specification_path)
        with self.assertRaisesRegex(matrix.MatrixError, "refusing overwrite"):
            matrix.plan_matrix(self.template_path, self.specification_path)

    def test_repetition_omission_is_rejected(self) -> None:
        self.specification["order"].pop(3)
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "declares 2 repetitions"):
            matrix.plan_matrix(self.template_path, self.specification_path)

    def test_observer_control_requires_matched_scenarios(self) -> None:
        self.specification["arms"][2]["scenario"]["duration_seconds"] = 61
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "scenarios are not identical"):
            matrix.plan_matrix(self.template_path, self.specification_path)

    def test_observer_control_requires_adjacent_alternating_pairs(self) -> None:
        self.specification["order"][2], self.specification["order"][3] = (
            self.specification["order"][3],
            self.specification["order"][2],
        )
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "must be adjacent"):
            matrix.plan_matrix(self.template_path, self.specification_path)

        self.specification = self.make_specification()
        self.specification["order"][4], self.specification["order"][5] = (
            self.specification["order"][5],
            self.specification["order"][4],
        )
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "must alternate"):
            matrix.plan_matrix(self.template_path, self.specification_path)

    def test_unsafe_sentinel_and_plan_paths_are_rejected(self) -> None:
        self.template["sentinel_root"] = str(self.sentinel.parent.resolve())
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "sentinel_root must itself end"):
            matrix.plan_matrix(self.template_path, self.specification_path)

        self.template = self.make_template()
        self.specification["plan_root"] = str(
            (self.sentinel.parent / "baseline-test").resolve()
        )
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "strict descendant"):
            matrix.plan_matrix(self.template_path, self.specification_path)

        self.specification = self.make_specification()
        self.specification["plan_root"] = str(
            (self.sentinel / "scratch" / "baseline-test").resolve()
        )
        self.write_sources()
        with self.assertRaisesRegex(matrix.MatrixError, "overlaps template scratch_root"):
            matrix.plan_matrix(self.template_path, self.specification_path)

    def test_missing_prepared_root_is_rejected_without_reading_media(self) -> None:
        (self.sentinel / "config" / "config.toml").unlink()
        with self.assertRaisesRegex(matrix.MatrixError, "config_path"):
            matrix.plan_matrix(self.template_path, self.specification_path)

    def test_verify_detects_missing_or_changed_launch(self) -> None:
        plan_path = matrix.plan_matrix(self.template_path, self.specification_path)
        plan = json.loads(plan_path.read_text(encoding="utf-8"))
        missing = Path(plan["launches"][0]["manifest_path"])
        missing.unlink()
        with self.assertRaisesRegex(matrix.MatrixError, "is missing"):
            matrix.verify_matrix(plan_path)

    def test_verify_recompiles_source_specification(self) -> None:
        plan_path = matrix.plan_matrix(self.template_path, self.specification_path)
        self.specification["cooldown_seconds"] = 121
        write_json(self.specification_path, self.specification)
        with self.assertRaisesRegex(matrix.MatrixError, "no longer matches"):
            matrix.verify_matrix(plan_path)

    def test_require_results_detects_omission_and_accepts_complete_set(self) -> None:
        plan_path = matrix.plan_matrix(self.template_path, self.specification_path)
        plan = json.loads(plan_path.read_text(encoding="utf-8"))
        with self.assertRaisesRegex(matrix.MatrixError, "required result is missing"):
            matrix.verify_matrix(plan_path, require_results=True)

        self.create_result_bundles(plan)
        self.assertEqual(
            matrix.verify_matrix(plan_path, require_results=True),
            {"arm_count": 3, "trial_count": 6, "result_count": 6},
        )
        first_terminal = Path(plan["launches"][0]["result_root"]) / "terminal.json"
        terminal = json.loads(first_terminal.read_text(encoding="utf-8"))
        terminal["source_hash_unchanged"] = False
        write_json(first_terminal, terminal)
        with self.assertRaisesRegex(matrix.MatrixError, "source_hash_unchanged"):
            matrix.verify_matrix(plan_path, require_results=True)

    def test_require_results_enforces_order_cooldown_and_final_hash_coverage(self) -> None:
        plan_path = matrix.plan_matrix(self.template_path, self.specification_path)
        plan = json.loads(plan_path.read_text(encoding="utf-8"))
        self.create_result_bundles(plan)

        second_metadata = Path(plan["launches"][1]["result_root"]) / "runner-metadata.json"
        metadata = json.loads(second_metadata.read_text(encoding="utf-8"))
        metadata["started_utc"] = "2026-08-25T00:01:00Z"
        write_json(second_metadata, metadata)
        with self.assertRaisesRegex(matrix.MatrixError, "order/cooldown"):
            matrix.verify_matrix(plan_path, require_results=True)

        metadata["started_utc"] = "2026-08-25T00:03:00Z"
        write_json(second_metadata, metadata)
        final_hashes = Path(plan["launches"][-1]["result_root"]) / "post-hashes.json"
        hashes = json.loads(final_hashes.read_text(encoding="utf-8"))
        hashes["files"] = []
        write_json(final_hashes, hashes)
        with self.assertRaisesRegex(matrix.MatrixError, "exactly cover"):
            matrix.verify_matrix(plan_path, require_results=True)

    def test_cli_plan_and_verify(self) -> None:
        planned = subprocess.run(
            [
                sys.executable,
                str(MATRIX_PATH),
                "plan",
                "--template",
                str(self.template_path),
                "--spec",
                str(self.specification_path),
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(planned.returncode, 0, planned.stderr)
        plan_path = Path(self.specification["plan_root"]) / "matrix-plan.json"
        verified = subprocess.run(
            [sys.executable, str(MATRIX_PATH), "verify", "--plan", str(plan_path)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertIn("6 trial(s)", verified.stdout)

    def test_schema_and_example_are_parseable_and_example_is_accepted(self) -> None:
        schema = json.loads((SCHEMA_ROOT / "matrix-v1.schema.json").read_text(encoding="utf-8"))
        example = json.loads((SCHEMA_ROOT / "matrix-v1.example.json").read_text(encoding="utf-8"))
        self.assertEqual(schema["$defs"]["scenario"]["additionalProperties"], False)
        self.assertEqual(example["schema_version"], 1)
        example["matrix_id"] = "example-test"
        example["plan_root"] = str(
            (self.sentinel / "matrix-plans" / "example-test").resolve()
        )
        self.specification = example
        self.write_sources()
        plan_path = matrix.plan_matrix(self.template_path, self.specification_path)
        self.assertEqual(matrix.verify_matrix(plan_path)["trial_count"], 6)


if __name__ == "__main__":
    unittest.main()
