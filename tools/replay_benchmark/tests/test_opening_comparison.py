import copy
import json
from pathlib import Path
import sys
import unittest
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import analyze
import opening_comparison as opening


def cycle(index=0, payload_after_frame=False, warm=False):
    start, generation = index * 1000, index + 1

    def event(kind, offset, **payload):
        return {"kind": kind, "source": "frontend", "monotonic_ms": start + offset,
                "generation": generation - 1 if kind == "viewer_mounted" else generation, "payload": payload}

    rows = [event("replay_requested" if index == 0 else "viewer_reopen_requested", 0),
            event("playback_payload_ready", 250 if payload_after_frame else 20),
            event("viewer_mounted", 30), event("media_loadstart", 40),
            event("metadata_ready", 80), event("media_canplay", 90),
            event("first_presented_frame", 100, authoritative=True),
            event("viewer_cycle_completed" if warm else "scenario_completed", 300)]
    if warm and index == 5:
        rows.append(event("scenario_completed", 301, viewer_cycles=6))
    return sorted(rows, key=lambda row: row["monotonic_ms"])


def processes(arm, *, warm=False):
    result = []
    for i in range(5):
        rows = [row for ordinal in range(6 if warm else 1) for row in cycle(ordinal, warm=warm)]
        cycles = opening.open_cycles(rows, "warm_open" if warm else "cold_open", 6 if warm else 1)
        result.append({"run_alias": f"{arm}-{i}", "cell": "S", "mode": "warm" if warm else "cold",
                       "arm": arm, "fingerprint_sha256": "a" * 64, "subject_sha256": ("b" if arm == "reference" else "c") * 64,
                       "cycles": cycles, "process_values": {name: 100.0 for name in opening.REQUIRED_METRICS}})
    for row in result:
        for name in opening.REQUIRED_METRICS:
            if name.endswith("_flag") or name == "server_error_count" or name in opening.RELIABILITY_TOKENS:
                row["process_values"][name] = 0
    return result


class OpeningComparisonTests(unittest.TestCase):
    def test_reference_and_descriptor_order_with_signed_diagnostic(self):
        before = opening.open_cycles(cycle(), "cold_open", 1)[0]
        after = opening.open_cycles(cycle(payload_after_frame=True), "cold_open", 1)[0]
        self.assertEqual(before["full_payload_ready_minus_mount_ms"], -10)
        self.assertEqual(after["full_payload_ready_minus_mount_ms"], 220)
        self.assertEqual(after["request_to_first_presented_frame_ms"], 100)

    def test_all_six_warm_cycles_and_final_completion_required(self):
        rows = [row for i in range(6) for row in cycle(i, i % 2 == 0, warm=True)]
        result = opening.open_cycles(rows, "warm_open", 6)
        self.assertEqual([r["role"] for r in result], ["initial_mount"] + ["warm_remount"] * 5)
        self.assertTrue(all(r["request_includes_remount_timer"] for r in result[1:]))
        for kind in ("viewer_cycle_completed", "scenario_completed"):
            with self.subTest(kind=kind), self.assertRaises(analyze.InvalidData):
                opening.open_cycles([r for r in rows if r["kind"] != kind], "warm_open", 6)

    def test_equal_time_ties_keep_jsonl_order_across_boundaries(self):
        rows = [r for i in range(6) for r in cycle(i, warm=True)]
        first_terminal = next(r for r in rows if r["kind"] == "viewer_cycle_completed")
        first_terminal["monotonic_ms"] = 1000
        self.assertEqual(len(opening.open_cycles(rows, "warm_open", 6)), 6)
        next_boundary = next(r for r in rows if r["kind"] == "viewer_reopen_requested")
        a, b = rows.index(first_terminal), rows.index(next_boundary)
        rows[a], rows[b] = rows[b], rows[a]
        with self.assertRaises(analyze.InvalidData):
            opening.open_cycles(rows, "warm_open", 6)

    def test_missing_duplicate_stale_and_non_authoritative_evidence_rejected(self):
        for kind in ("playback_payload_ready", "viewer_mounted", "media_loadstart",
                     "metadata_ready", "media_canplay", "first_presented_frame", "scenario_completed"):
            rows = cycle()
            with self.subTest(kind=kind), self.assertRaises(analyze.InvalidData):
                opening.open_cycles([r for r in rows if r["kind"] != kind], "cold_open", 1)
            if kind != "media_canplay":
                with self.subTest(duplicate=kind), self.assertRaises(analyze.InvalidData):
                    opening.open_cycles(rows + [copy.deepcopy(next(r for r in rows if r["kind"] == kind))], "cold_open", 1)
        for kind in ("media_loadstart", "metadata_ready", "media_canplay", "first_presented_frame"):
            rows = cycle()
            next(r for r in rows if r["kind"] == kind)["generation"] = 99
            with self.subTest(stale=kind), self.assertRaises(analyze.InvalidData):
                opening.open_cycles(rows, "cold_open", 1)
        rows = cycle()
        next(r for r in rows if r["kind"] == "first_presented_frame")["payload"]["authoritative"] = False
        with self.assertRaises(analyze.InvalidData):
            opening.open_cycles(rows, "cold_open", 1)

    def test_payload_after_completion_or_crossing_boundary_rejects(self):
        for late in (350, 1020):
            rows = [r for i in range(6) for r in cycle(i, warm=True)]
            next(r for r in rows if r["kind"] == "playback_payload_ready")["monotonic_ms"] = late
            with self.subTest(late=late), self.assertRaises(analyze.InvalidData):
                opening.open_cycles(rows, "warm_open", 6)

    def test_lone_stale_payload_identity_is_explicitly_unobservable(self):
        rows = cycle(payload_after_frame=True)
        payload = next(r for r in rows if r["kind"] == "playback_payload_ready")
        payload.pop("generation")  # Real payload telemetry has no usable load key.
        self.assertEqual(len(opening.open_cycles(rows, "cold_open", 1)), 1)
        self.assertTrue(any("indistinguishable lone stale payload" in s for s in opening.LIMITATIONS))

    def test_native_offset_is_not_joined_and_canplay_uses_first(self):
        rows = cycle()
        rows.append({"kind": "playback_payload_ready", "source": "app", "monotonic_ms": 0})
        extra = copy.deepcopy(next(r for r in rows if r["kind"] == "media_canplay"))
        extra["monotonic_ms"] = 120
        rows.append(extra)
        self.assertEqual(opening.open_cycles(rows, "cold_open", 1)[0]["request_to_media_canplay_ms"], 90)

    def test_negative_phase_reused_generation_and_wrong_iteration_count_reject(self):
        rows = cycle()
        next(r for r in rows if r["kind"] == "media_loadstart")["monotonic_ms"] = 25
        with self.assertRaises(analyze.InvalidData):
            opening.open_cycles(rows, "cold_open", 1)
        rows = [r for i in range(6) for r in cycle(i, warm=True)]
        for row in rows:
            row["generation"] = 1
        with self.assertRaises(analyze.InvalidData):
            opening.open_cycles(rows, "warm_open", 6)
        for iterations in (1, 5, 7, True):
            with self.assertRaises(analyze.InvalidData):
                opening.open_cycles(rows, "warm_open", iterations)

    def test_required_inventory_balanced_five_processes_and_identity(self):
        reference, candidate = processes("reference"), processes("candidate")
        result = opening.compare_group(reference, candidate, "S", "cold")
        self.assertEqual(len(result["comparison"]["metrics"]), len(opening.REQUIRED_METRICS))
        for mutation in ("missing", "nan", "identity", "duplicate", "count", "subject"):
            current = copy.deepcopy(candidate)
            if mutation == "missing":
                current[0]["process_values"].pop("request_to_first_presented_frame_ms")
            elif mutation == "nan":
                current[0]["process_values"]["request_to_first_presented_frame_ms"] = float("nan")
            elif mutation == "identity":
                current[0]["fingerprint_sha256"] = "d" * 64
            elif mutation == "subject":
                current[0]["subject_sha256"] = "d" * 64
            elif mutation == "duplicate":
                current[0]["run_alias"] = current[1]["run_alias"]
            else:
                current.pop()
            with self.subTest(mutation=mutation), self.assertRaises(analyze.InvalidData):
                opening.compare_group(reference, current, "S", "cold")

    def test_threshold_uses_process_medians_and_signed_metric_is_neutral(self):
        reference, candidate = processes("reference", warm=True), processes("candidate", warm=True)
        for i, row in enumerate(reference):
            row["process_values"]["request_to_first_presented_frame_ms"] = [90, 95, 100, 105, 110][i]
        for row in candidate:
            row["process_values"]["request_to_first_presented_frame_ms"] = 120
            row["process_values"]["full_payload_ready_minus_mount_ms"] = 999
        result = opening.compare_group(reference, candidate, "S", "warm")
        metrics = {r["metric"]: r for r in result["comparison"]["metrics"]}
        frame = metrics["request_to_first_presented_frame_ms"]
        self.assertEqual(frame["reference_mad"], 5)
        self.assertEqual(frame["repeatability_band"], 15)
        self.assertEqual(frame["baseline_trials"], 5)
        self.assertTrue(frame["disposition_requiring"])
        self.assertFalse(metrics["full_payload_ready_minus_mount_ms"]["disposition_requiring"])
        self.assertEqual(result["reference_process_summaries"][0]["eligible_trials"], 5)

    def test_individual_growth_cannot_be_hidden_by_median(self):
        reference, candidate = processes("reference"), processes("candidate")
        candidate[0]["process_values"]["working_set_bytes_sustained_monotonic_growth_flag"] = 1
        result = opening.compare_group(reference, candidate, "S", "cold")
        self.assertTrue(result["disposition_required"])
        self.assertEqual(len(result["individual_signals"]), 1)

    def test_output_does_not_copy_unreviewed_fields(self):
        reference, candidate = processes("reference"), processes("candidate")
        for row in reference + candidate:
            row["private_path"] = r"C:\Users\private\recording"
            row["capability"] = "secret-bearer"
        result = json.dumps(opening.compare_group(reference, candidate, "S", "cold"))
        self.assertNotIn("secret-bearer", result)
        self.assertNotIn("Users", result)

    def test_extract_uses_remount_median_and_retains_initial_mount(self):
        events = [r for i in range(6) for r in cycle(i, warm=True)]
        for event in events:
            if event["kind"] == "first_presented_frame":
                index = event["generation"] - 1
                event["monotonic_ms"] = index * 1000 + [290, 100, 110, 120, 130, 280][index]
        # Use a hashable scenario key as supplied by the analyzer.
        key = analyze.TrialKey("synthetic", "1")
        bundle = SimpleNamespace(expected={key: {"kind": "warm_open", "iterations": 6}},
                                 manifest={"observer_profile": "minimal"}, events=events,
                                 identity_by_scenario={"synthetic": "a" * 64}, subject_by_scenario={"synthetic": "b" * 64})
        with patch.object(opening, "process_metrics", return_value={name: 0 for name in opening.PROCESS_METRICS}):
            result = opening.extract(bundle, "S", "warm", "reference", 1)
        self.assertEqual(result["process_values"]["request_to_first_presented_frame_ms"], 120)
        self.assertEqual(result["cycles"][0]["request_to_first_presented_frame_ms"], 290)
        self.assertEqual(len(result["cycles"]), 6)

    def test_recovery_degraded_and_nonfatal_errors_require_disposition(self):
        for kind, metric in (("recovery_started", "reliability_recovery_events_count"),
                             ("decoder_degraded_detail", "reliability_recovery_events_count"),
                             ("optional_asset_error", "reliability_error_events_count")):
            samples = [{f"io_{kind}_{unit}": i for kind in ("read", "write", "other")
                        for unit in ("bytes", "operations")} for i in range(3)]
            bundle = SimpleNamespace(expected={"key": {}}, events=[{"kind": kind}], requests=[], samples=samples)
            raw = {name: [0] for name in opening.PROCESS_METRICS}
            with patch.object(analyze, "_trial_metrics", return_value=raw):
                measured = opening.process_metrics(bundle)
            self.assertEqual(measured[metric], 1)
            reference, candidate = processes("reference"), processes("candidate")
            candidate[0]["process_values"][metric] = measured[metric]
            result = opening.compare_group(reference, candidate, "S", "cold")
            self.assertTrue(result["disposition_required"])
            self.assertEqual(result["individual_signals"][0]["metric"], metric)


if __name__ == "__main__":
    unittest.main()
