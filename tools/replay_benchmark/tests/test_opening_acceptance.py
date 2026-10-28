import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import analyze
import opening_acceptance as opening
from test_analyze import BundleBuilder


SCENARIO = "qb010-opening-v1-s-cold"
FIXTURE = "representative-h264"


def evidence(role="candidate", kind="cold_open", details_first=False):
    spec = {"kind": kind, "fixture_ids": [FIXTURE], "iterations": 6, "observer_control": kind == "seek"}

    def event(name, at, cycle=None, generation=None, **fields):
        payload = dict(fields)
        if cycle is not None:
            payload.update(measurement_contract=opening.CONTRACT, fixture_id=FIXTURE, cycle=cycle)
        value = dict(kind=name, source="frontend", monotonic_ms=at, payload=payload)
        if generation is not None:
            value["generation"] = generation
        return value

    events = [event("library_requested", 0), event("opening_library_usable", 5,
              measurement_contract=opening.CONTRACT, fixture_id=FIXTURE,
              gate="snapshot" if role == "candidate" else "legacy-resources",
              after_library_paint=True, game_count=3, clip_count=1)]
    for cycle in range(6 if kind == "warm_open" else 1):
        start = 10 + 1000 * cycle
        generation = cycle + 10  # Mount/initial observer generation is deliberately different.
        events.append(event("opening_requested", start, cycle))
        if role == "candidate":
            events.extend([event("opening_descriptor_requested", start + 1, cycle),
                           event("opening_descriptor_ready", start + 2, cycle)])
        events.extend([
            event("opening_details_ready", start + (3 if details_first or role == "reference" else 90), cycle),
            event("opening_media_open_requested", start + 4, cycle),
            event("opening_media_open_bound", start + 5, cycle, generation),
            event("viewer_mounted", start + 5, generation=generation - 1),
            event("media_loadstart", start + 6, cycle, generation),
            event("metadata_ready", start + 20, cycle, generation),
            event("first_presented_frame", start + 40, cycle, generation, authoritative=True),
            event("media_canplay", start + 45, cycle, generation),
            event("media_canplay", start + 50, cycle, generation),
            event("viewer_cycle_completed" if kind == "warm_open" else "scenario_completed", start + 100, cycle, generation),
        ])
        if kind != "seek":
            for index, name in enumerate(("play_requested", "play_complete", "steady_playback", "pause_requested", "pause_complete")):
                events.append(event(name, start + 60 + index, cycle, generation))
    return spec, sorted(events, key=lambda item: item["monotonic_ms"])


def measurement(role="candidate", kind="cold_open", **kwargs):
    spec, events = evidence(role, kind, **kwargs)
    return analyze.opening_measurements(SCENARIO, spec, events, role)


def matrix_rows():
    return [{"cell": cell, "kind": kind, "role": role, "round": index,
             "comparison_fingerprint": "a" * 64,
             "process_metrics": {metric: 90 if role == "candidate" else 100 for metric in opening.SHARED_METRICS}}
            for cell in opening.CELLS for kind in opening.KINDS for role in opening.ROLES for index in range(1, 6)]


def raw_bundle(root, role="candidate"):
    value = BundleBuilder(root)
    spec, events = evidence(role)
    value.manifest["fixtures"][0]["codec"] = "h264"
    value.manifest["scenarios"][0].update(spec, scenario_id=SCENARIO, expected_action_count=1)
    # Retain the existing valid seek action / request / samples to exercise the
    # common raw-bundle validator, and add the opening workload's required taps.
    for event in value.events:
        if event["kind"] == "scenario_end":
            event["kind"] = "diagnostic_end"
    value.events.extend(value.envelope("1", 10_000 + event["monotonic_ms"], event["source"], event["kind"],
                                      generation=event.get("generation"), payload=event["payload"]) for event in events)
    for name in ("replay_requested", "playback_payload_ready"):
        value.events.append(value.envelope("1", 10_090, "frontend", name))
    for records in (value.events, value.requests, value.samples):
        for event in records:
            event["scenario_id"] = SCENARIO
    value.events.sort(key=lambda event: event["monotonic_ms"])
    return value


def campaign_fixture(root):
    """116 real synthetic bundles through the existing collector/analyzer contract."""
    campaign = {"schema_version": 1, "measurement_contract": opening.CONTRACT,
                "subjects": {}, "cells": [], "control": {}, "runs": []}
    subjects = {"reference": ("a" * 40, "b" * 64), "candidate": ("c" * 40, "d" * 64)}
    for role in subjects:
        (root / f"{role}.patch").write_text(f"synthetic {role} observer patch", encoding="utf-8")
        revision, binary = subjects[role]
        (root / f"{role}-build.json").write_text(json.dumps({
            "status": "passed", "source_revision": revision, "binary_sha256": binary,
            "source_patch_sha256": opening.file_digest(root / f"{role}.patch"),
        }), encoding="utf-8")
    order = [(role, "R", "seek", profile, index) for role in opening.ROLES
             for index in range(1, 5) for profile in ("minimal", "full")]
    order += [(role, cell, kind, "full", index) for index in range(1, 6)
              for cell in opening.CELLS for kind in opening.KINDS for role in opening.ROLES]
    for sequence, (role, cell, kind, profile, index) in enumerate(order, 1):
        name = f"qb010-opening-v1-{cell.lower()}-{kind.replace('_', '-')}"
        value = BundleBuilder(root, run_id=f"run-{sequence:03d}", observer_profile=profile)
        if kind == "seek":
            value.as_observer_control()
        spec, events = evidence(role, kind)
        value.manifest["scenarios"][0].update(spec, scenario_id=name)
        # Production matrix manifests omit analyzer-injected optional defaults.
        for field in ("expected_action_count", "max_telemetry_gap_ms"):
            value.manifest["scenarios"][0].pop(field)
        value.manifest["fixtures"][0]["codec"] = "h264"
        revision, binary = subjects[role]
        # The runner checkout is shared; each archived application's source is
        # separately frozen in the build receipt, alongside its measured binary.
        value.runner_metadata["source"]["revision"] = "e" * 40
        value.runner_metadata["app_binary_sha256"] = binary
        # Real launch manifests have no synthetic analyzer fingerprint object.
        del value.manifest["fingerprints"]
        value.events = [event for event in value.events if event["kind"] != "scenario_end"]
        for event in events:
            value.events.append(value.envelope("1", 10000 + event["monotonic_ms"], event["source"], event["kind"],
                                               generation=event.get("generation"), payload=event["payload"]))
        for name_ in ("replay_requested", "playback_payload_ready"):
            value.events.append(value.envelope("1", 10_090, "frontend", name_))
        if kind == "warm_open":
            for cycle in range(1, 6):
                value.events.append(value.envelope("1", 10_010 + 1000 * cycle, "frontend", "viewer_reopen_requested"))
            value.events.append(value.envelope("1", 15_120, "frontend", "scenario_completed"))
        # A control has the same 60-second observed action window in each profile.
        if kind == "seek":
            completed = next(event for event in value.events if event["kind"] == "scenario_completed")
            completed["monotonic_ms"] = 70_000
            completed["payload"].update(total_frames=1000, dropped_frames=0)
        value.events.sort(key=lambda event: event["monotonic_ms"])
        first = value.samples[0]
        value.samples = []
        for second in range(61):
            sample = copy.deepcopy(first)
            sample["monotonic_ms"] = 10_000 + 1000 * second
            sample["payload"].update(process_tree_cpu_time_ms=second * 10, cpu_accounting_quantum_ms=15.625)
            value.samples.append(sample)
        value.manifest["scenarios"][0]["trial_ids"] = [str(index)]
        for records in (value.events, value.samples, value.requests):
            for event in records:
                event.update(scenario_id=name, trial_id=str(index))
        planned_input = opening.input_fingerprint(
            value.manifest, value.manifest["scenarios"][0], value.runner_metadata["config_sha256"])
        path = value.persist()
        bundle = analyze.load_bundle(path, f"run-{sequence:03d}")
        identity = {"scenario_id": name, "fixture_id": FIXTURE,
                    "config_sha256": value.runner_metadata["config_sha256"]}
        if role not in campaign["subjects"]:
            patch = root / f"{role}.patch"
            campaign["subjects"][role] = {"source_revision": revision, "binary_sha256": binary,
                "source_patch": {"path": patch.name, "sha256": opening.file_digest(patch)},
                "build_receipt": {"path": f"{role}-build.json", "sha256": opening.file_digest(root / f"{role}-build.json")}}
        if kind == "seek":
            campaign["control"].update(identity)
            campaign["control"].setdefault("input_fingerprints", {})[profile] = planned_input
        elif role == "reference" and index == 1:
            campaign["cells"].append({**identity, "cell": cell, "kind": kind,
                                      "input_fingerprint": planned_input})
        campaign["runs"].append({"sequence": sequence, "role": role, "cell": cell, "kind": kind,
            "observer_profile": profile, "round": index, "path": str(path.relative_to(root)),
            "manifest_sha256": opening.file_digest(path / "manifest.json")})
    path = root / "campaign.json"
    path.write_text(json.dumps(campaign), encoding="utf-8")
    return path, campaign


class OpeningMeasurementTests(unittest.TestCase):
    def test_details_order_is_independent_of_media_and_mount_generation(self):
        for first in (False, True):
            result = measurement(details_first=first)
            self.assertEqual(result["library_request_to_games_usable_ms"], 5)
            row = result["cycles"][0]
            self.assertEqual(row["generation"], 10)
            self.assertEqual(row["details_before_first_frame"], first)
            self.assertEqual(row["metrics"]["intent_to_first_frame_ms"], 40)
        reference = measurement("reference")["cycles"][0]
        self.assertTrue(reference["details_before_first_frame"])
        self.assertFalse(any("descriptor" in key for key in reference["metrics"]))

    def test_warm_uses_five_remounts_per_process_and_keeps_initial(self):
        result = measurement(kind="warm_open")
        result["cycles"][0]["metrics"]["intent_to_first_frame_ms"] = 1000
        for index, row in enumerate(result["cycles"][1:]):
            row["metrics"]["intent_to_first_frame_ms"] = 101 + index
        self.assertEqual(opening.process_values(result, "warm_open")["intent_to_first_frame_ms"], 103)
        self.assertEqual([row["phase"] for row in result["cycles"]], ["initial"] + ["remount"] * 5)
        with self.assertRaises(analyze.InvalidData):
            opening.process_values({**result, "cycles": result["cycles"][:-1]}, "warm_open")

    def test_native_and_collector_offsets_do_not_change_frontend_deltas(self):
        spec, events = evidence()
        expected = analyze.opening_measurements(SCENARIO, spec, events, "candidate")
        for source in ("app", "collector"):
            events.append(dict(kind="native_diagnostic", source=source, monotonic_ms=-10000, payload={"elapsed_ms": 123}))
        self.assertEqual(analyze.opening_measurements(SCENARIO, spec, events, "candidate"), expected)
        for event in events:
            if event["source"] == "frontend":
                event["monotonic_ms"] += 20000
        self.assertEqual(analyze.opening_measurements(SCENARIO, spec, events, "candidate"), expected)

    def test_missing_duplicate_wrong_clock_and_invalid_milestones_reject(self):
        names = ["library_requested", "opening_library_usable", *analyze.OPENING_TAPS,
                 "media_loadstart", "metadata_ready", "first_presented_frame", "scenario_completed"]
        for name in names:
            for mutation in ("missing", "duplicate", "clock", "nan", "bool"):
                with self.subTest(name=name, mutation=mutation):
                    spec, events = evidence()
                    event = next(e for e in events if e["kind"] == name)
                    if mutation == "missing": events.remove(event)
                    elif mutation == "duplicate": events.append(copy.deepcopy(event))
                    elif mutation == "clock": event["source"] = "app"
                    else: event["monotonic_ms"] = float("nan") if mutation == "nan" else True
                    with self.assertRaises(analyze.InvalidData):
                        analyze.opening_measurements(SCENARIO, spec, events, "candidate")

    def test_identity_order_generation_and_authority_rejections(self):
        mutations = [
            ("opening_library_usable", "payload", "after_library_paint", False),
            ("opening_library_usable", "payload", "game_count", True),
            ("opening_library_usable", "payload", "gate", "legacy-resources"),
            ("opening_details_ready", "payload", "cycle", 1),
            ("opening_details_ready", "payload", "cycle", False),
            ("opening_details_ready", "payload", "fixture_id", "other"),
            ("opening_details_ready", "payload", "measurement_contract", "replay-opening-v2"),
            ("first_presented_frame", "payload", "authoritative", False),
            ("first_presented_frame", None, "generation", 11),
            ("media_canplay", None, "generation", 11),
            ("media_loadstart", None, "generation", True),
            ("opening_media_open_bound", None, "generation", 9),
            ("first_presented_frame", None, "monotonic_ms", 29),
            ("opening_descriptor_ready", None, "monotonic_ms", 15),
            ("opening_details_ready", None, "monotonic_ms", 120),
        ]
        for name, nest, field, bad in mutations:
            with self.subTest(name=name, field=field, bad=bad):
                spec, events = evidence()
                event = next(e for e in events if e["kind"] == name)
                (event[nest] if nest else event)[field] = bad
                with self.assertRaises(analyze.InvalidData):
                    analyze.opening_measurements(SCENARIO, spec, events, "candidate")
        spec, events = evidence(kind="warm_open")
        for event in events:
            if event.get("generation") == 11:
                event["generation"] = 10
        with self.assertRaises(analyze.InvalidData):
            analyze.opening_measurements(SCENARIO, spec, events, "candidate")

    def test_reference_role_cannot_hide_missing_candidate_descriptors(self):
        spec, events = evidence("reference")
        with self.assertRaises(analyze.InvalidData):
            analyze.opening_measurements(SCENARIO, spec, events, "candidate")
        for scenario in ("qb010-opening-v2-s-cold", "historical", "qb010-opening-v1-"):
            with self.assertRaises(analyze.InvalidData):
                analyze.opening_measurements(scenario, spec, events, "reference")
        for change in ({"kind": "app_idle"}, {"kind": "warm_open", "iterations": 5}, {"kind": "seek", "observer_control": False}):
            with self.assertRaises(analyze.InvalidData):
                analyze.opening_measurements(SCENARIO, {**spec, **change}, events, "reference")

    def test_every_warm_cycle_requires_its_own_playback_actions(self):
        spec, events = evidence(kind="warm_open")
        events = [event for event in events if not (event["kind"] == "steady_playback" and event.get("generation") == 12)]
        with self.assertRaises(analyze.InvalidData):
            analyze.opening_measurements(SCENARIO, spec, events, "candidate")

    def test_action_identity_must_match_its_opening_cycle(self):
        for name in analyze.OPENING_ACTIONS:
            for field, value in (("measurement_contract", None), ("measurement_contract", "old"),
                                 ("cycle", None), ("cycle", 2), ("fixture_id", None), ("fixture_id", "other")):
                spec, events = evidence(kind="warm_open")
                action = next(event for event in events if event["kind"] == name)
                if value is None:
                    del action["payload"][field]
                else:
                    action["payload"][field] = value
                with self.subTest(name=name, field=field, value=value), self.assertRaises(analyze.InvalidData):
                    analyze.opening_measurements(SCENARIO, spec, events, "candidate")

    def test_new_namespace_uses_complete_raw_validation_without_legacy_library_gate(self):
        with tempfile.TemporaryDirectory() as directory:
            value = raw_bundle(Path(directory))
            report = analyze.analyze(value.persist())
            self.assertEqual(report["status"], "valid")
            self.assertFalse(any(row["metric"].startswith("replay_request_to_") for row in report["summaries"]))
            value.events = [event for event in value.events if event["kind"] != "opening_details_ready"]
            with self.assertRaises(analyze.InvalidData):
                analyze.analyze(value.persist())


class OpeningMatrixTests(unittest.TestCase):
    def test_planned_input_matches_normalized_scenario_and_preserves_counts(self):
        for kind in ("seek", "cold_open", "warm_open"):
            for profile in ("minimal", "full"):
                with self.subTest(kind=kind, profile=profile):
                    scenario = dict(id=SCENARIO, kind=kind, fixture_ids=[FIXTURE], trial_ids=["1"])
                    manifest = dict(observer_profile=profile, scenarios=[scenario])
                    original = copy.deepcopy(scenario)
                    fingerprints = []
                    for count in (None, 0, 1):
                        if count is not None:
                            scenario["expected_action_count"] = count
                        normalized = next(iter(analyze._normalize_scenarios(manifest).values()))
                        planned = opening.input_fingerprint(manifest, scenario, "a" * 64)
                        self.assertEqual(planned, opening.input_fingerprint(manifest, normalized, "a" * 64))
                        fingerprints.append(planned)
                    self.assertEqual(len(set(fingerprints)), 3)
                    scenario.pop("expected_action_count")
                    self.assertEqual(scenario, original)

    def test_planned_input_resolves_action_count_aliases_without_mutation(self):
        for field in ("expected_action_count", "action_count", "actions"):
            fingerprints = []
            for count in (None, 0, 1, [], ["seek"]):
                with self.subTest(field=field, count=count):
                    scenario = dict(id=SCENARIO, kind="seek", trial_ids=["1"], **{field: count})
                    manifest = dict(scenarios=[scenario])
                    normalized = next(iter(analyze._normalize_scenarios(manifest).values()))
                    original = copy.deepcopy((scenario, normalized))
                    planned = opening.input_fingerprint(manifest, scenario, "a" * 64)
                    self.assertEqual(planned, opening.input_fingerprint(manifest, normalized, "a" * 64))
                    self.assertEqual((scenario, normalized), original)
                    fingerprints.append(planned)
            self.assertEqual(len(set(fingerprints[:3])), 3)

    def test_frozen_campaign_runs_end_to_end_and_rejects_identity_mutations(self):
        with tempfile.TemporaryDirectory() as directory:
            path, campaign = campaign_fixture(Path(directory))
            report, generic = opening.acceptance(path)
            self.assertEqual(len(report["runs"]), 100)
            self.assertEqual(len(report["comparisons"]), 70)
            self.assertEqual(len([row for run in report["runs"] for row in run["cycles"]]), 350)
            self.assertEqual(generic["reference"]["run_count"], 50)
            self.assertTrue(all(control["gate_passed"] for control in generic["observer_controls"].values()))
            # Synthetic equal Games times intentionally cannot pass N/E improvement.
            self.assertFalse(report["latency_gate_passed"])
            self.assertNotIn(str(path.parent), json.dumps(report))
            for mutation in ("role", "source", "binary", "patch", "build", "config", "runtime", "duplicate", "manifest", "absolute_run"):
                bad = copy.deepcopy(campaign)
                if mutation == "role": bad["runs"][0]["role"] = "candidate"
                elif mutation in {"source", "binary"}:
                    field = "source_revision" if mutation == "source" else "binary_sha256"
                    bad["subjects"]["reference"][field] = "e" * (40 if mutation == "source" else 64)
                elif mutation in {"config", "runtime"}:
                    if mutation == "config": bad["control"]["config_sha256"] = "e" * 64
                    else: bad["control"]["input_fingerprints"]["minimal"] = "e" * 64
                elif mutation == "patch":
                    bad["subjects"]["reference"]["source_patch"] = bad["subjects"]["candidate"]["source_patch"]
                elif mutation == "build":
                    bad["subjects"]["reference"]["build_receipt"] = bad["subjects"]["candidate"]["build_receipt"]
                elif mutation == "duplicate": bad["runs"][1] = {**bad["runs"][0], "sequence": 2}
                elif mutation == "absolute_run": bad["runs"][0]["path"] = str(path.parent / bad["runs"][0]["path"])
                else: bad["runs"][0]["manifest_sha256"] = "e" * 64
                path.write_text(json.dumps(bad), encoding="utf-8")
                with self.subTest(mutation=mutation), self.assertRaises(analyze.InvalidData):
                    opening.acceptance(path)

    def test_all_ten_cells_and_seventy_shared_metrics_use_independent_processes(self):
        result = opening.compare_processes(matrix_rows())
        self.assertEqual(len(result), 70)
        self.assertTrue(all(row["gate"] == "improved" for row in result))
        self.assertTrue(all(row["independent_processes_per_arm"] == 5 for row in result))
        self.assertFalse(any("p95" in key or "p99" in key for row in result for key in row))

    def test_missing_cell_arm_process_primary_and_mismatched_identity_reject(self):
        for mutation in ("cell", "arm", "process", "metric", "fingerprint", "round"):
            rows = matrix_rows()
            if mutation == "cell": rows = [row for row in rows if row["cell"] != "L"]
            elif mutation == "arm": rows = [row for row in rows if row["role"] != "candidate"]
            elif mutation == "process": rows.pop()
            elif mutation == "metric": del rows[-1]["process_metrics"]["intent_to_first_frame_ms"]
            elif mutation == "fingerprint": rows[-1]["comparison_fingerprint"] = "b" * 64
            else: rows[-1]["round"] = 4
            with self.subTest(mutation=mutation), self.assertRaises(analyze.InvalidData):
                opening.compare_processes(rows)

    def test_materiality_and_required_games_improvement_are_distinct(self):
        rows = matrix_rows()
        for row in rows:
            if row["role"] == "candidate":
                row["process_metrics"]["library_request_to_games_usable_ms"] = 99
                row["process_metrics"]["intent_to_first_frame_ms"] = 110
        result = opening.compare_processes(rows)
        required = [row for row in result if row["gate"] == "required_improvement_missing"]
        self.assertEqual(len(required), 4)
        self.assertEqual(len([row for row in result if row["gate"] == "disposition_required"]), 10)

    def test_schemas_reject_unknown_version_and_unfrozen_campaign(self):
        for name in ("opening-campaign.schema.json", "opening-report.schema.json"):
            with self.assertRaises(analyze.InvalidData):
                opening.validate_schema({"schema_version": 2, "measurement_contract": opening.CONTRACT}, name)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "patch.diff"
            path.write_text("frozen observation patch", encoding="utf-8")
            frozen = {"path": "patch.diff", "sha256": opening.file_digest(path)}
            self.assertEqual(opening.checked_file(path.parent, frozen), path.resolve())
            path.write_text("modified observation patch", encoding="utf-8")
            with self.assertRaises(analyze.InvalidData):
                opening.checked_file(path.parent, frozen)

    def test_campaign_artifacts_must_remain_owned_and_reparse_free(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "patch.diff"
            artifact.write_text("frozen patch", encoding="utf-8")
            for name in (str(artifact), "../patch.diff", "nested/../../patch.diff", "C:\\foreign\\patch.diff", "/foreign/patch.diff", "\\\\server\\share\\patch.diff"):
                with self.subTest(name=name), self.assertRaises(analyze.InvalidData):
                    opening.checked_file(root, {"path": name, "sha256": opening.file_digest(artifact)})
            from unittest.mock import patch
            with patch.object(Path, "is_symlink", return_value=True), self.assertRaises(analyze.InvalidData):
                opening.checked_file(root, {"path": artifact.name, "sha256": opening.file_digest(artifact)})


if __name__ == "__main__":
    unittest.main()
