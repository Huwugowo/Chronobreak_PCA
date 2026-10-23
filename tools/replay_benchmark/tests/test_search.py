"""Real-input search contract: coalescing, backend work, admission and paint."""
import copy
from pathlib import Path
import tempfile
import unittest

from test_analyze import BundleBuilder, analyze


def builder(root):
    value = BundleBuilder(root, trial_ids=["1"])
    value.manifest["scenarios"][0].update(kind="library_search", iterations=1, expected_action_count=0)
    value.events = []
    value.requests = []

    def emit(at, kind, action=None, **payload):
        value.events.append(value.envelope("1", at, "frontend", kind, action_id=action, payload=payload))

    emit(10000, "scenario_started")
    emit(10001, "library_requested")
    emit(10002, "library_useful")
    emit(10003, "search_preconditioning_started")
    emit(10004, "search_preconditioning_ready", catalog_count=2, iterations=1)
    emit(10005, "library_search_input", "char-1", text_length=1)
    emit(10006, "library_search_scheduled", "char-1", delay_ms=180)
    emit(10065, "library_search_input", "char-2", text_length=2)
    emit(10066, "library_search_coalesced", "char-1", delay_ms=0)
    emit(10066, "library_search_scheduled", "char-2", delay_ms=180)
    for index, (stratum, action, length, at, rows) in enumerate((
        ("typing", "char-2", 2, 10065, 1),
        ("settled", "settled-1", 6, 10400, 0),
        ("clear", "clear-1", 0, 10700, 2),
    ), 1):
        if stratum != "typing":
            emit(at, "library_search_input", action, text_length=length)
            emit(at + 1, "library_search_scheduled", action, delay_ms=180 if length else 0)
        emit(at + 2, "library_search_sequence", action, stratum=stratum, input_count=2 if stratum == "typing" else 1)
        for offset, kind in ((180, "scheduled"), (181, "dispatched"), (190, "completed"), (191, "admitted")):
            emit(at + offset, "history_query_" + kind, action, query_generation=index, row_count=rows if offset >= 190 else None)
        emit(at + 210, "library_search_painted", action, query_generation=index, row_count=rows,
             authority="two-animation-frame-paint-opportunity")
    emit(10950, "scenario_completed", search_sequences=3)
    value.events.sort(key=lambda event: event["monotonic_ms"])
    return value


class SearchEvidenceTests(unittest.TestCase):
    def test_indexing_diagnostic_preserves_superseded_queries_and_bounded_import_evidence(self):
        for superseded in (False, True):
            with self.subTest(superseded=superseded), tempfile.TemporaryDirectory() as temporary:
                value = builder(Path(temporary))
                scenario = value.manifest["scenarios"][0]
                scenario["scenario_id"] = "cold-index-search"
                value.events = [event for event in value.events if event["kind"] != "library_useful"]
                for at, kind, payload in (
                    (10003, "indexing_diagnostic_started", {"indexing": True}),
                    (10003, "indexing_diagnostic_progress", {"indexing": True}),
                    (10940, "indexing_diagnostic_progress", {"indexing": False}),
                    (10941, "indexing_diagnostic_completed", {"indexing_observed": True, "import_completed": True, "search_sequences": 3}),
                ):
                    event = value.envelope("1", at, "frontend", kind, payload=payload)
                    event["scenario_id"] = "cold-index-search"
                    value.events.append(event)
                if superseded:
                    value.events = [event for event in value.events if not (
                        event.get("action_id") == "char-2" and event["kind"] in {"history_query_admitted", "library_search_painted"})]
                    event = value.envelope("1", 10260, "frontend", "history_query_superseded",
                                           action_id="char-2", payload={"query_generation": 1})
                    event["scenario_id"] = "cold-index-search"
                    value.events.append(event)
                value.events.sort(key=lambda event: event["monotonic_ms"])
                key = analyze.TrialKey("cold-index-search", "1")
                analyze._validate_scenario_evidence(key, scenario, value.events)
                errored = copy.deepcopy(value.events)
                errored.append(dict(copy.deepcopy(errored[0]), kind="history_query_error", action_id="char-2"))
                with self.assertRaises(analyze.InvalidData):
                    analyze._validate_scenario_evidence(key, scenario, errored)
                completed = next(event for event in value.events if event["kind"] == "indexing_diagnostic_completed")
                completed["payload"]["import_completed"] = False
                with self.assertRaises(analyze.InvalidData):
                    analyze._validate_search_evidence(scenario, value.events)

    def test_search_preserves_coalescence_and_reports_each_latency_stratum(self):
        with tempfile.TemporaryDirectory() as temporary:
            value = builder(Path(temporary))
            report = analyze.analyze(value.persist())
            self.assertEqual(report["status"], "valid")
            metrics = {item["metric"]: item["distribution"] for item in report["summaries"] if item["trial_statistic"] == "median"}
            for stratum in ("typing", "settled", "clear"):
                self.assertEqual(metrics[f"library_search_{stratum}_input_to_painted_ms"]["median"], 210)
                self.assertEqual(metrics[f"library_search_{stratum}_dispatch_to_completed_ms"]["median"], 9)
            self.assertEqual(metrics["library_search_coalesced_count"]["median"], 1)

    def test_missing_duplicate_stale_wrong_clock_and_unreconciled_evidence_reject(self):
        mutations = []
        for kind in ("library_search_input", "library_search_scheduled", "history_query_dispatched",
                     "history_query_completed", "history_query_admitted", "library_search_painted"):
            mutations += [
                ("missing " + kind, lambda events, k=kind: events.remove(next(e for e in events if e["kind"] == k and e.get("action_id") == "char-2"))),
                ("duplicate " + kind, lambda events, k=kind: events.append(copy.deepcopy(next(e for e in events if e["kind"] == k and e.get("action_id") == "char-2")))),
            ]
        for field, bad in (("query_generation", 99), ("authority", "approximate"), ("row_count", 9)):
            mutations.append((field, lambda events, f=field, b=bad:
                              next(e for e in events if e["kind"] == "library_search_painted")["payload"].update({f: b})))
        mutations += [
            ("clock", lambda events: next(e for e in events if e["kind"] == "library_search_painted").update(source="app")),
            ("order", lambda events: next(e for e in events if e["kind"] == "library_search_painted").update(monotonic_ms=10070)),
            ("count", lambda events: next(e for e in events if e["kind"] == "scenario_completed")["payload"].update(search_sequences=4)),
            ("stratum", lambda events: next(e for e in events if e["kind"] == "library_search_sequence")["payload"].update(stratum="clear")),
            ("unaccounted", lambda events: events.remove(next(e for e in events if e["kind"] == "library_search_coalesced"))),
            ("generic request", lambda events: events.append(dict(copy.deepcopy(next(e for e in events if e["kind"] == "library_search_input")), kind="seek_requested"))),
        ]
        for label, mutate in mutations:
            with self.subTest(case=label), tempfile.TemporaryDirectory() as temporary:
                value = builder(Path(temporary))
                mutate(value.events)
                value.events.sort(key=lambda event: event["monotonic_ms"])
                with self.assertRaises(analyze.InvalidData):
                    analyze.analyze(value.persist())
