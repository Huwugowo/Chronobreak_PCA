"""Versioned startup evidence, including historical compatibility and clock origin."""
import copy
from pathlib import Path
import tempfile
import unittest

from test_analyze import BundleBuilder, analyze


SCENARIO = "qb010-library-v2-n-cold"


def builder(root, trials=1):
    value = BundleBuilder(root, trial_ids=[str(n + 1) for n in range(trials)])
    value.manifest["scenarios"][0].update(scenario_id=SCENARIO, kind="app_idle")
    for records in (value.events, value.requests, value.samples):
        for record in records:
            record["scenario_id"] = SCENARIO
    for n in range(trials):
        base = (n + 1) * 10_000
        payload = dict(measurement_contract="games-library-usable-v1",
                       refresh_request_token="1", snapshot_token="view-1", view_token="view-1",
                       game_count=50, clip_count=50, storage_game_count=50, storage_clip_count=50)
        for kind, at, data in (
            ("library_requested", base - 20, {}),
            ("library_view_admitted", base - 18, payload),
            ("games_library_usable", base - 15, dict(payload, after_library_paint=True)),
            ("library_useful", base - 10, {}),
        ):
            event = value.envelope(str(n + 1), at, "frontend", kind, payload=copy.deepcopy(data))
            event["scenario_id"] = SCENARIO
            value.events.append(event)
    value.events.sort(key=lambda event: event["monotonic_ms"])
    return value


class GamesUsableTests(unittest.TestCase):
    def test_primary_and_historical_metrics_keep_distinct_origins(self):
        with tempfile.TemporaryDirectory() as temporary:
            value = builder(Path(temporary), trials=5)
            # Legacy command origin stays historical; it cannot shift the new metric.
            for n in range(5):
                event = value.envelope(str(n + 1), (n + 1) * 10_000 - 50,
                                       "frontend", "library_games_requested")
                event["scenario_id"] = SCENARIO
                value.events.append(event)
            value.events.sort(key=lambda event: event["monotonic_ms"])
            report = analyze.analyze(value.persist())
            comparison = analyze.compare_reports(report, report)
            medians = {item["metric"]: item for item in comparison["metrics"]
                       if item["trial_statistic"] == "median"}
            self.assertEqual(medians["library_request_to_games_usable_ms"]["baseline_median"], 5)
            self.assertEqual(medians["library_request_to_useful_ms"]["baseline_median"], 40)
            self.assertTrue(medians["library_request_to_games_usable_ms"]["eligible"])
            for arm in ("reference", "candidate"):
                missing = copy.deepcopy(report)
                missing["summaries"] = [item for item in missing["summaries"]
                                        if item["metric"] != "library_request_to_games_usable_ms"]
                with self.subTest(arm=arm), self.assertRaisesRegex(analyze.InvalidData, "primary metric"):
                    analyze.compare_reports(missing if arm == "reference" else report,
                                            missing if arm == "candidate" else report)

    def test_historical_scenario_does_not_require_new_events(self):
        with tempfile.TemporaryDirectory() as temporary:
            value = builder(Path(temporary))
            value.manifest["scenarios"][0]["scenario_id"] = "historical"
            value.events = [event for event in value.events
                            if event["kind"] not in {"games_library_usable", "library_view_admitted"}]
            for records in (value.events, value.requests, value.samples):
                for event in records:
                    event["scenario_id"] = "historical"
            self.assertEqual(analyze.analyze(value.persist())["status"], "valid")

    def test_missing_duplicate_stale_wrong_clock_and_prepaint_events_reject(self):
        mutations = []
        for kind in ("library_requested", "library_view_admitted", "games_library_usable"):
            mutations.append((f"missing {kind}", lambda events, k=kind:
                              events.__setitem__(slice(None), [e for e in events if e["kind"] != k])))
            mutations.append((f"duplicate {kind}", lambda events, k=kind:
                              events.append(copy.deepcopy(next(e for e in events if e["kind"] == k)))))
            mutations.append((f"clock {kind}", lambda events, k=kind:
                              next(e for e in events if e["kind"] == k).update(source="app")))
        for kind in ("library_view_admitted", "games_library_usable"):
            for field, bad in (("measurement_contract", "other"), ("refresh_request_token", ""),
                               ("snapshot_token", "stale"), ("view_token", "stale"),
                               ("game_count", 51), ("clip_count", -1),
                               ("storage_game_count", True), ("storage_clip_count", "50")):
                mutations.append((f"{kind} {field}", lambda events, k=kind, f=field, b=bad:
                                  next(e for e in events if e["kind"] == k)["payload"].update({f: b})))
        for bad in (False, None):
            mutations.append((f"paint {bad}", lambda events, b=bad:
                              next(e for e in events if e["kind"] == "games_library_usable")["payload"].update(after_library_paint=b)))
        mutations.append(("before admission", lambda events:
                          next(e for e in events if e["kind"] == "games_library_usable").update(monotonic_ms=9981)))
        mutations.append(("before request", lambda events:
                          next(e for e in events if e["kind"] == "library_view_admitted").update(monotonic_ms=9979)))
        for label, mutate in mutations:
            with self.subTest(case=label), tempfile.TemporaryDirectory() as temporary:
                value = builder(Path(temporary))
                mutate(value.events)
                value.events.sort(key=lambda event: event["monotonic_ms"])
                with self.assertRaises(analyze.InvalidData):
                    analyze.analyze(value.persist())
