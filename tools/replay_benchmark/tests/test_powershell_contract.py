from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import jsonschema


TOOLS_ROOT = Path(__file__).resolve().parents[1]
SCHEMA_ROOT = TOOLS_ROOT / "schemas"
PREPARE = TOOLS_ROOT / "prepare.ps1"
RUN = TOOLS_ROOT / "run.ps1"


def powershell() -> str:
    executable = (
        os.environ.get("QUEUEBACK_TEST_POWERSHELL")
        or shutil.which("pwsh")
        or shutil.which("powershell")
    )
    if not executable:
        raise unittest.SkipTest("PowerShell is not installed")
    return executable


def run_script(script: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            powershell(),
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            str(script),
            *arguments,
        ],
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=60,
        check=False,
    )


class ReplayPowerShellContractTests(unittest.TestCase):
    def test_export_validation_uses_the_canonical_sentinel_clips_label(self):
        runner = RUN.read_text(encoding="utf-8")
        self.assertIn(r'clips_root = "library_root\clips"', runner)
        self.assertNotIn(r'clips_root = "library_root\\clips"', runner)

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="queueback-replay-benchmark-")
        self.base = Path(self.temporary.name).resolve()
        self.root = self.base / "sentinel" / ".chronobreak-replay-benchmark"
        self.root.mkdir(parents=True)
        self.source = self.base / "explicit-input"
        self.source.mkdir()
        (self.source / "video.mp4").write_bytes(b"sentinel-owned-changing-media-placeholder")
        self.release = self.root / "release"
        self.release.mkdir()
        self.app = self.release / "league-replay-app.exe"
        self.app.write_bytes(b"MZ" + (b"\0" * 126))
        self.analyzer = self.root / "tools" / "analyze.py"
        self.analyzer.parent.mkdir()
        self.analyzer.write_text("raise SystemExit(0)\n", encoding="utf-8")
        self.spec = self.root / "manifests" / "prepare.json"
        self.spec.parent.mkdir()
        self.manifest = self.root / "manifests" / "run.json"
        self.run_id = "powershell-contract-001"
        self.result = self.root / "results" / self.run_id
        self.specification = {
            "schema_version": 1,
            "run_id": self.run_id,
            "sentinel_root": str(self.root),
            "library_root": str(self.root / "library"),
            "config_path": str(self.root / "config" / "config.toml"),
            "app_data_root": str(self.root / "app-data"),
            "result_root": str(self.result),
            "scratch_root": str(self.root / "scratch"),
            "observer_profile": "minimal",
            "ddragon": {
                "mode": "offline",
                "cache_root": str(self.root / "app-data" / "ddragon"),
                "cache_fingerprint": None,
            },
            "fixtures": [
                {
                    "id": "short-h264",
                    "alias": "short-h264",
                    "game_timestamp": "1700000000",
                    "kind": "recording_bundle",
                    "source_path": str(self.source),
                    "destination_relative_path": os.path.join("games", "1700000000"),
                    "backend": "native",
                    "codec": "h264",
                    "negative": False,
                }
            ],
            "scenarios": [
                {
                    "id": "short-seek",
                    "kind": "seek",
                    "fixture_ids": ["short-h264"],
                    "trial_id": "1",
                    "seed": 7,
                    "seek_playback_mode": "paused",
                    "target_times_ms": [1000, 2000],
                    "distance_classes": ["near", "far"],
                    "observer_control": True,
                }
            ],
            "app_binary": str(self.app),
            "analyzer_path": str(self.analyzer),
            "python_path": shutil.which("python") or "python",
            "timeout_seconds": 60,
        }
        self.spec.write_text(json.dumps(self.specification), encoding="utf-8")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_schema_examples_validate(self) -> None:
        for stem in ("prepare-v1", "manifest-v1"):
            schema = json.loads((SCHEMA_ROOT / f"{stem}.schema.json").read_text(encoding="utf-8"))
            example = json.loads((SCHEMA_ROOT / f"{stem}.example.json").read_text(encoding="utf-8"))
            jsonschema.Draft202012Validator(schema).validate(example)

    def test_runner_has_a_distinct_finite_frontend_startup_gate(self) -> None:
        runner = RUN.read_text(encoding="utf-8")
        self.assertIn("function Test-FrontendStartupObserved", runner)
        self.assertIn('kind -eq "frontend_session_requested"', runner)
        self.assertIn('kind -eq "frontend_initialized"', runner)
        self.assertIn('Stop-CreatedProcessTree -Reason "frontend-startup-timeout"', runner)
        self.assertIn('Stop-Benchmark "APP_STARTUP_TIMEOUT"', runner)

    def test_runner_avoids_managed_module_enumeration_during_shutdown(self) -> None:
        runner = RUN.read_text(encoding="utf-8")
        self.assertNotIn("process.MainModule", runner)
        self.assertIn("QueryFullProcessImageName", runner)
        self.assertIn("GetProcessMemoryInfo", runner)
        loop = runner.index("while ($true) {", runner.index("$eventsPath ="))
        root_exit_check = runner.index("$script:AppProcess.HasExited", loop)
        process_snapshot = runner.index("$rows = Get-ProcessRows", loop)
        self.assertLess(root_exit_check, process_snapshot)

    def test_prepare_and_runner_preflight_contract(self) -> None:
        preparation_check = run_script(
            PREPARE,
            "-Spec",
            str(self.spec),
            "-Manifest",
            str(self.manifest),
            "-PreflightOnly",
        )
        self.assertEqual(
            preparation_check.returncode,
            0,
            preparation_check.stdout + preparation_check.stderr,
        )
        self.assertIn("QB-REPLAY-PREPARE-PREFLIGHT-OK", preparation_check.stdout)
        self.assertFalse(self.manifest.exists())

        prepared = run_script(
            PREPARE,
            "-Spec",
            str(self.spec),
            "-Manifest",
            str(self.manifest),
        )
        self.assertEqual(prepared.returncode, 0, prepared.stdout + prepared.stderr)
        self.assertTrue(self.manifest.is_file())
        self.assertFalse(self.result.exists())
        config_text = (self.root / "config" / "config.toml").read_text(encoding="utf-8")
        self.assertIn(f'output_path = "{str(self.root / "library").replace(chr(92), chr(92) * 2)}"', config_text)
        self.assertIn("auto_delete_days = 0", config_text)
        self.assertIn('[recording]\nprofile = "auto"\ncodec = "auto"', config_text)
        self.assertIn('[app]\nautostart = true\nhevc_playback_supported = false', config_text)
        runtime_manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        self.assertNotIn("source_path", json.dumps(runtime_manifest))
        self.assertEqual(
            runtime_manifest["ddragon"]["cache_fingerprint"],
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        )
        self.assertEqual(runtime_manifest["fixtures"][0]["files"][0]["sha256"],
                         __import__("hashlib").sha256((self.source / "video.mp4").read_bytes()).hexdigest())

        run_check = run_script(RUN, "-Manifest", str(self.manifest), "-PreflightOnly")
        self.assertEqual(run_check.returncode, 0, run_check.stdout + run_check.stderr)
        self.assertIn("QB-REPLAY-RUN-PREFLIGHT-OK", run_check.stdout)
        self.assertFalse(self.result.exists())

    def test_runner_rejects_existing_result_root(self) -> None:
        prepared = run_script(PREPARE, "-Spec", str(self.spec), "-Manifest", str(self.manifest))
        self.assertEqual(prepared.returncode, 0, prepared.stdout + prepared.stderr)
        self.result.mkdir(parents=True)
        marker = self.result / "must-survive.txt"
        marker.write_text("preserve me", encoding="utf-8")

        rejected = run_script(RUN, "-Manifest", str(self.manifest), "-PreflightOnly")
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("REPLAY-BENCHMARK-RESULT_EXISTS", rejected.stderr)
        self.assertEqual(marker.read_text(encoding="utf-8"), "preserve me")

    def test_runner_rejects_changed_benchmark_config(self) -> None:
        prepared = run_script(PREPARE, "-Spec", str(self.spec), "-Manifest", str(self.manifest))
        self.assertEqual(prepared.returncode, 0, prepared.stdout + prepared.stderr)
        config = self.root / "config" / "config.toml"
        config.write_text(config.read_text(encoding="utf-8").replace(
            "auto_delete_days = 0", "auto_delete_days = 30"
        ), encoding="utf-8")

        rejected = run_script(RUN, "-Manifest", str(self.manifest), "-PreflightOnly")
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("REPLAY-BENCHMARK-CONFIG_PATH", rejected.stderr)
        self.assertFalse(self.result.exists())

    def test_runner_rejects_changed_ddragon_cache(self) -> None:
        prepared = run_script(PREPARE, "-Spec", str(self.spec), "-Manifest", str(self.manifest))
        self.assertEqual(prepared.returncode, 0, prepared.stdout + prepared.stderr)
        cache_file = self.root / "app-data" / "ddragon" / "changed-after-prepare.json"
        cache_file.write_text("{}\n", encoding="utf-8")

        rejected = run_script(RUN, "-Manifest", str(self.manifest), "-PreflightOnly")
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("REPLAY-BENCHMARK-DDRAGON", rejected.stderr)
        self.assertFalse(self.result.exists())

    def test_runner_requires_prepared_media_tools_beside_production_app(self) -> None:
        prepared = run_script(PREPARE, "-Spec", str(self.spec), "-Manifest", str(self.manifest))
        self.assertEqual(prepared.returncode, 0, prepared.stdout + prepared.stderr)

        tool_source = self.root / "tool-source"
        tool_source.mkdir()
        source_identities: dict[str, dict[str, object]] = {}
        for name, content in (("ffmpeg", b"prepared-ffmpeg"), ("ffprobe", b"prepared-ffprobe")):
            path = tool_source / f"{name}.exe"
            path.write_bytes(content)
            source_identities[name] = {
                "path": str(path),
                "size_bytes": len(content),
                "sha256": hashlib.sha256(content).hexdigest(),
                "version_line": f"{name} test version",
            }
        runtime_id = "queueback-test-runtime"
        launch_manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        launch_manifest["media_tools"] = {
            "runtime_id": runtime_id,
            **source_identities,
        }
        self.manifest.write_text(json.dumps(launch_manifest), encoding="utf-8")

        missing = run_script(RUN, "-Manifest", str(self.manifest), "-PreflightOnly")
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn("REPLAY-BENCHMARK-PACKAGED_RUNTIME", missing.stderr)

        packaged = self.release / "resources" / "media-runtime"
        (packaged / "bin").mkdir(parents=True)
        (packaged / "runtime-manifest.json").write_text(
            json.dumps({"runtime_id": runtime_id}), encoding="utf-8"
        )
        for name in ("ffmpeg", "ffprobe"):
            shutil.copyfile(tool_source / f"{name}.exe", packaged / "bin" / f"{name}.exe")

        accepted = run_script(RUN, "-Manifest", str(self.manifest), "-PreflightOnly")
        self.assertEqual(accepted.returncode, 0, accepted.stdout + accepted.stderr)
        self.assertIn("packaged-runtime identity", accepted.stdout)

        (packaged / "bin" / "ffprobe.exe").write_bytes(b"changed-packaged-ffprobe")
        changed = run_script(RUN, "-Manifest", str(self.manifest), "-PreflightOnly")
        self.assertNotEqual(changed.returncode, 0)
        self.assertIn("REPLAY-BENCHMARK-PACKAGED_RUNTIME", changed.stderr)
        self.assertFalse(self.result.exists())

    def test_failed_created_process_is_observed_and_preserved(self) -> None:
        disposable_process = shutil.which("where.exe")
        if not disposable_process:
            self.skipTest("where.exe is unavailable")
        self.specification["app_binary"] = disposable_process
        self.spec.write_text(json.dumps(self.specification), encoding="utf-8")
        prepared = run_script(PREPARE, "-Spec", str(self.spec), "-Manifest", str(self.manifest))
        self.assertEqual(prepared.returncode, 0, prepared.stdout + prepared.stderr)

        failed = run_script(RUN, "-Manifest", str(self.manifest))
        self.assertNotEqual(failed.returncode, 0)
        self.assertTrue(self.result.is_dir())
        self.assertTrue((self.result / "manifest.json").is_file())
        self.assertTrue(
            (self.result / "observer.jsonl").is_file(),
            failed.stdout + failed.stderr,
        )
        self.assertTrue((self.result / "runner-error.json").is_file())
        error = json.loads((self.result / "runner-error.json").read_text(encoding="utf-8"))
        self.assertTrue(error["preserved"])
        self.assertEqual(error["status"], "failed")
        self.assertNotIn("PROCESS_JOB", error["error"])
        self.assertIn("APP_EXIT", error["error"])

    def test_prepare_rejects_relative_sentinel_root(self) -> None:
        self.specification["sentinel_root"] = "relative\\benchmark"
        self.spec.write_text(json.dumps(self.specification), encoding="utf-8")
        rejected = run_script(
            PREPARE,
            "-Spec",
            str(self.spec),
            "-Manifest",
            str(self.manifest),
            "-PreflightOnly",
        )
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("REPLAY-BENCHMARK-PATH", rejected.stderr)
        self.assertFalse(self.manifest.exists())

    def test_prepare_rejects_overlapping_mutable_roots(self) -> None:
        self.specification["scratch_root"] = self.specification["library_root"]
        self.spec.write_text(json.dumps(self.specification), encoding="utf-8")

        rejected = run_script(
            PREPARE,
            "-Spec",
            str(self.spec),
            "-Manifest",
            str(self.manifest),
            "-PreflightOnly",
        )
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("REPLAY-BENCHMARK-ROOT_OVERLAP", rejected.stderr)
        self.assertFalse(self.manifest.exists())

    def test_prepare_never_overwrites_noncanonical_config(self) -> None:
        config = self.root / "config" / "config.toml"
        config.parent.mkdir()
        config.write_text("must_survive = true\n", encoding="utf-8")

        rejected = run_script(
            PREPARE,
            "-Spec",
            str(self.spec),
            "-Manifest",
            str(self.manifest),
            "-PreflightOnly",
        )
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("REPLAY-BENCHMARK-CONFIG_EXISTS", rejected.stderr)
        self.assertEqual(config.read_text(encoding="utf-8"), "must_survive = true\n")

    def test_export_scenario_requires_explicit_media_contract(self) -> None:
        scenario = self.specification["scenarios"][0]
        scenario.update(
            {
                "kind": "export",
                "export_presets": ["horizontal", "discord"],
                "clip_start_ms": 10_000,
                "clip_end_ms": 30_000,
                "expected_duration_ms": 19_000,
                "expected_video_codec": "h264",
                "expected_audio_codec": "aac",
            }
        )
        self.spec.write_text(json.dumps(self.specification), encoding="utf-8")

        rejected = run_script(
            PREPARE,
            "-Spec",
            str(self.spec),
            "-Manifest",
            str(self.manifest),
            "-PreflightOnly",
        )
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("expected_duration_ms must equal", rejected.stderr)


if __name__ == "__main__":
    unittest.main()
