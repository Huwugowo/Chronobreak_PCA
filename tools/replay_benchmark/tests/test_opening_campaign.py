import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import opening_campaign as campaign
import test_matrix as matrix_fixtures
write_json = matrix_fixtures.write_json

TOOLS = Path(__file__).resolve().parents[1]


@unittest.skipUnless(sys.platform == "win32", "matrix manifests require Windows")
class OpeningCampaignTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / campaign.matrix.SENTINEL_NAME
        for folder in ("library", "config", "app-data/ddragon", "scratch", "results", "inputs", "matrix-plans", "tools/schemas"):
            (self.root / folder).mkdir(parents=True, exist_ok=True)
        (self.root / "config/config.toml").write_text("[storage]\nauto_delete_days = 0\n", encoding="utf-8")
        for name in campaign.TOOLS:
            shutil.copyfile(TOOLS / name, self.root / "tools" / name)
        for path in (TOOLS / "schemas").glob("*.json"):
            shutil.copyfile(path, self.root / "tools/schemas" / path.name)
        subjects = {}
        for arm in campaign.ARMS:
            subject = self.root / "subjects" / arm / "app.exe"
            subject.parent.mkdir(parents=True)
            subject.write_bytes(arm.encode())
            runtime = subject.parent / "resources/media-runtime"
            runtime.mkdir(parents=True)
            (runtime / "runtime.json").write_text('{"runtime":"synthetic"}', encoding="utf-8")
            archive = subject.parent / "source.tar"
            archive.write_bytes(campaign.COMMITS[arm].encode())
            receipt = subject.parent / "build.json"
            write_json(receipt, {"source_commit": campaign.COMMITS[arm], "exit_code": 0,
                                 "command": campaign.BUILD_COMMAND,
                                 "source_archive_sha256": campaign.digest(archive),
                                 "binary_sha256": campaign.digest(subject)})
            subjects[arm] = {"path": str(subject), "source_commit": campaign.COMMITS[arm],
                             "source_archive": str(archive), "build_receipt": str(receipt)}
        helper = matrix_fixtures.MatrixPlannerTests()
        helper.sentinel = self.root
        template = helper.make_template()
        template["observer_profile"] = "minimal"
        template["timeout_seconds"] = 180
        matrices = []
        for cell in campaign.CELLS:
            for mode in campaign.MODES:
                for arm in campaign.ARMS:
                    name = f"{cell.lower()}-{mode}-{arm}"
                    current = copy.deepcopy(template)
                    current["app_binary"] = subjects[arm]["path"]
                    current["run_id"] = name
                    current["result_root"] = str(self.root / "results" / name)
                    template_path = self.root / "inputs" / f"{name}-template.json"
                    spec_path = self.root / "inputs" / f"{name}-spec.json"
                    scenario = {"id": f"qb010-library-v2-replay-opening-{cell.lower()}-{mode}",
                                "kind": mode + "_open", "fixture_ids": ["representative-h264"], "warmup_seconds": 5}
                    if mode == "warm":
                        scenario["iterations"] = 6
                    spec = {"schema_version": 1, "matrix_id": name, "seed": campaign.SEED,
                            "cooldown_seconds": 5, "isolate_launch_roots": True,
                            "plan_root": str(self.root / "matrix-plans" / name),
                            "arms": [{"id": arm, "observer_profile": "minimal", "repetitions": 5, "scenario": scenario}],
                            "order": [{"arm_id": arm, "trial_id": f"trial-{i}"} for i in range(1, 6)]}
                    write_json(template_path, current)
                    write_json(spec_path, spec)
                    path = campaign.matrix.plan_matrix(template_path, spec_path)
                    matrices.append({"cell": cell, "mode": mode, "arm": arm, "path": str(path)})
        self.spec_path = self.root / "inputs/campaign-spec.json"
        self.spec = {"schema_version": 1, "sentinel_root": str(self.root),
                     "subjects": subjects, "matrices": matrices, "tool_root": str(self.root / "tools"),
                     "runner_environment": {"environment": {"cpu": "synthetic"}, "media_runtime_id": "synthetic"},
                     "conditions": {"display": "visible_always_on_top", "operator_input": "none",
                                    "background_description": "fake runner, no app launches"}}
        environment_path = self.root / "inputs/environment.json"
        write_json(environment_path, {"schema_version": 1,
                                    "runner_environment": self.spec["runner_environment"],
                                    "installed_webview_sha256": "d" * 64})
        self.spec["environment_receipt"] = str(environment_path)
        write_json(self.spec_path, self.spec)
        self.environment_mock = patch.object(campaign, "_verify_environment")
        self.environment_mock.start()
        self.addCleanup(self.environment_mock.stop)
        self.path = campaign.create_plan(self.spec_path)
        self.plan = campaign.read(self.path)
        self.sha = campaign.digest(self.path)
        self.clock = datetime(2026, 9, 23, tzinfo=timezone.utc)

    def fake_validation(self, entry, plan):
        return campaign.read(Path(entry["result_root"]) / "fake-validation.json")

    def complete(self, sequence, start=None):
        entry = self.plan["launches"][sequence - 1]
        self.clock = start or self.clock
        receipt_start = self.clock
        runner_start = receipt_start + timedelta(seconds=1)
        runner_end = runner_start + timedelta(seconds=1)
        receipt_end = runner_end + timedelta(seconds=1)
        result = Path(entry["result_root"])
        result.mkdir()
        validation = {"runner_started_utc": runner_start.isoformat(), "runner_completed_utc": runner_end.isoformat(),
                      "artifacts": {"synthetic": "a" * 64}, "bundle_status": "valid"}
        write_json(result / "fake-validation.json", validation)
        receipts, live = self.root / "receipts", self.root / "live"
        receipts.mkdir(exist_ok=True)
        live.mkdir(exist_ok=True)
        (live / f"{sequence:03d}.log").write_text("fake", encoding="utf-8")
        identity = campaign._receipt_identity(entry, self.sha)
        started = receipts / f"{sequence:03d}.started.json"
        campaign.write_new(started, {**identity, "started_utc": receipt_start.isoformat()})
        campaign.write_new(receipts / f"{sequence:03d}.completed.json", {
            **identity, "started_receipt_sha256": campaign.digest(started),
            "completed_utc": receipt_end.isoformat(), "validation": validation})
        self.clock = receipt_end + timedelta(seconds=5)

    def checked(self, **kwargs):
        with patch.object(campaign, "_validate_result", side_effect=self.fake_validation):
            return campaign.verify_campaign(self.path, **kwargs)

    def preflight_receipt(self):
        folder = self.root / "preflight"
        folder.mkdir()
        logs = {}
        for i in range(1, 101):
            path = folder / f"{i:03d}.log"
            path.write_text("fake preflight", encoding="utf-8")
            logs[path.name] = campaign.digest(path)
        campaign.write_new(folder / "complete.json", {
            "schema_version": 1, "campaign_sha256": self.sha, "count": 100, "logs": logs})

    def test_fixed_order_membership_and_isolation(self):
        checked = self.checked()
        self.assertEqual(checked["completed_count"], 0)
        launches = self.plan["launches"]
        self.assertEqual(len(launches), 100)
        self.assertEqual(len({r["pair"] for r in launches}), 50)
        self.assertEqual(len({campaign.read(r["manifest_path"])["app_data_root"] for r in launches}), 100)
        for i, (a, b) in enumerate(zip(launches[::2], launches[1::2])):
            self.assertEqual((a["cell"], a["mode"], a["round"]), (b["cell"], b["mode"], b["round"]))
            self.assertEqual(a["comparison_identity_sha256"], b["comparison_identity_sha256"])
            self.assertEqual(a["arm"], "reference" if i % 2 == 0 else "candidate")
        with self.assertRaises(campaign.Error):
            campaign.create_plan(self.spec_path)

    def test_valid_interrupted_prefix_and_incomplete_final_matrix(self):
        self.complete(1)
        self.complete(2)
        self.assertEqual(self.checked()["completed_count"], 2)
        with self.assertRaises(campaign.Error):
            self.checked(require_results=True)

    def test_cross_plan_order_and_cooldown_rejected(self):
        initial = self.clock
        self.complete(1)
        self.complete(2, initial + timedelta(seconds=2))
        with self.assertRaisesRegex(campaign.Error, "global start order"):
            self.checked()

    def test_gap_extra_receipt_extra_result_and_extra_log_rejected(self):
        for mutation in ("gap", "receipt", "result", "log"):
            with self.subTest(mutation=mutation):
                if mutation == "gap":
                    self.complete(2)
                    with self.assertRaises(campaign.Error):
                        self.checked()
                    # Fixtures are disposable; restore only this synthetic test slot.
                    shutil.rmtree(Path(self.plan["launches"][1]["result_root"]))
                    for p in (self.root / "receipts").iterdir():
                        p.unlink()
                    for p in (self.root / "live").iterdir():
                        p.unlink()
                else:
                    target = (self.root / "receipts/101.started.json" if mutation == "receipt"
                              else self.root / "results/unplanned" if mutation == "result"
                              else self.root / "live/101.log")
                    if mutation == "result":
                        target.mkdir()
                    else:
                        target.write_text("{}", encoding="utf-8")
                    with self.assertRaises(campaign.Error):
                        self.checked()
                    target.rmdir() if target.is_dir() else target.unlink()

    def test_started_without_completion_cannot_resume(self):
        self.preflight_receipt()
        calls = []
        def fail(entry, plan, log):
            calls.append(entry["sequence"])
            Path(log).write_text("failed", encoding="utf-8")
            return 1
        with self.assertRaises(campaign.Error):
            campaign.run_campaign(self.path, invoke=fail)
        with self.assertRaises(campaign.Error):
            campaign.run_campaign(self.path, resume=True, invoke=fail)
        self.assertEqual(calls, [1])

    def test_changed_subject_manifest_plan_and_config_rejected(self):
        targets = [Path(self.plan["subjects"]["candidate"]["path"]),
                   Path(self.plan["launches"][0]["manifest_path"]),
                   self.path, self.root / "config/config.toml"]
        for target in targets:
            original = target.read_bytes()
            try:
                target.write_bytes(original + b" ")
                with self.subTest(target=target.name), self.assertRaises(campaign.Error):
                    self.checked()
            finally:
                target.write_bytes(original)

    def test_lock_excludes_a_second_driver(self):
        with campaign.campaign_lock(self.root):
            with self.assertRaises(OSError):
                with campaign.campaign_lock(self.root):
                    self.fail("second owner acquired lock")

    def test_resume_only_advances_next_unused_slot(self):
        self.preflight_receipt()
        self.complete(1)
        calls = []
        def stop_next(entry, plan, log):
            calls.append(entry["sequence"])
            Path(log).write_text("stop", encoding="utf-8")
            return 1
        with patch.object(campaign, "_validate_result", side_effect=self.fake_validation):
            with self.assertRaises(campaign.Error):
                campaign.run_campaign(self.path, resume=True, invoke=stop_next,
                                      now=lambda: self.clock, sleep=lambda _: None)
        self.assertEqual(calls, [2])

    def test_final_verifier_calls_every_local_result_verifier(self):
        for i in range(1, 101):
            self.complete(i)
        original = campaign.matrix.verify_matrix
        verified = []
        def verify(path, require_results=False):
            if require_results:
                verified.append(str(path))
                return {}
            return original(path)
        with patch.object(campaign.matrix, "verify_matrix", side_effect=verify):
            result = self.checked(require_results=True)
        self.assertEqual(result["completed_count"], 100)
        self.assertEqual(len(set(verified)), 20)

    def test_pair_identity_mismatch_is_rejected_before_launch(self):
        # Change the source template then regenerate its exact local plan to
        # establish locally valid, globally unmatched evidence.
        local_path = Path(self.spec["matrices"][1]["path"])
        local = campaign.read(local_path)
        template_path = Path(local["template"]["path"])
        template = campaign.read(template_path)
        template["fixtures"][0]["files"][0]["sha256"] = "e" * 64
        write_json(template_path, template)
        shutil.rmtree(local_path.parent)
        campaign.matrix.plan_matrix(template_path, Path(local["specification"]["path"]))
        with self.assertRaisesRegex(campaign.Error, "unmatched comparison"):
            campaign.compile_plan(self.spec_path)

    def test_exact_approved_source_and_distinct_binaries_are_required(self):
        spec = copy.deepcopy(self.spec)
        spec["subjects"]["reference"]["source_commit"] = campaign.COMMITS["candidate"]
        write_json(self.spec_path, spec)
        with self.assertRaisesRegex(campaign.Error, "source commit"):
            campaign.compile_plan(self.spec_path)
        write_json(self.spec_path, self.spec)
        source = Path(self.spec["subjects"]["candidate"]["path"])
        source.write_bytes(Path(self.spec["subjects"]["reference"]["path"]).read_bytes())
        receipt = Path(self.spec["subjects"]["candidate"]["build_receipt"])
        built = campaign.read(receipt)
        built["binary_sha256"] = campaign.digest(source)
        write_json(receipt, built)
        with self.assertRaisesRegex(campaign.Error, "binaries must differ"):
            campaign.compile_plan(self.spec_path)

    def test_fake_driver_stops_at_exactly_one_hundred_and_cannot_restart(self):
        self.preflight_receipt()
        calls = []
        def invoke(entry, plan, log):
            calls.append(entry["sequence"])
            Path(log).write_text("fake launch", encoding="utf-8")
            result = Path(entry["result_root"])
            result.mkdir()
            begin = self.clock + timedelta(seconds=1)
            end = begin + timedelta(seconds=1)
            write_json(result / "fake-validation.json", {
                "runner_started_utc": begin.isoformat(), "runner_completed_utc": end.isoformat(),
                "artifacts": {"synthetic": "a" * 64}, "bundle_status": "valid"})
            self.clock = end + timedelta(seconds=1)
            return 0
        def sleep(seconds):
            self.clock += timedelta(seconds=seconds)
        with patch.object(campaign, "compile_plan", return_value=self.plan), \
             patch.object(campaign, "_validate_result", side_effect=self.fake_validation), \
             patch.object(campaign.matrix, "verify_matrix", return_value={}):
            campaign.run_campaign(self.path, invoke=invoke, now=lambda: self.clock, sleep=sleep)
            self.assertEqual(campaign.verify_campaign(self.path, require_results=True)["completed_count"], 100)
            with self.assertRaises(campaign.Error):
                campaign.run_campaign(self.path, resume=True, invoke=invoke)
        self.assertEqual(calls, list(range(1, 101)))

    def test_environment_drift_stops_before_creating_an_attempt(self):
        self.preflight_receipt()
        calls = []
        with patch.object(campaign, "_verify_environment", side_effect=campaign.Error("environment drift")):
            with self.assertRaisesRegex(campaign.Error, "environment drift"):
                campaign.run_campaign(self.path, invoke=lambda *args: calls.append(args))
        self.assertEqual(calls, [])
        self.assertEqual(list((self.root / "receipts").iterdir()), [])


if __name__ == "__main__":
    unittest.main()
