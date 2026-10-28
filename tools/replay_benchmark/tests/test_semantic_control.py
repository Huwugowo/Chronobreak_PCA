"""Observer comparisons commute only attributed native/RVFC seek pairs."""
import copy
import tempfile
import unittest
from pathlib import Path

import test_analyze as fixtures
from test_analyze import BundleBuilder, analyze


class SemanticControlTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        builder = BundleBuilder(Path(self.temporary.name)).as_observer_control()
        self.events, self.requests = builder.events, builder.requests

    def digest(self, events=None, requests=None):
        return analyze._semantic_outcome_digest(
            self.events if events is None else events,
            self.requests if requests is None else requests,
            {"representative-h264": 1000 / 60},
        )

    def phase(self, events, kind):
        return next(e for e in events if e["kind"] == kind)

    def test_only_allowed_pair_swap_matches_while_raw_digest_stays_different(self):
        swapped = copy.deepcopy(self.events)
        native = next(i for i, e in enumerate(swapped) if e["kind"] == "seeked")
        swapped[native:native + 2] = reversed(swapped[native:native + 2])
        self.assertNotEqual(analyze._outcome_digest(self.events, self.requests),
                            analyze._outcome_digest(swapped, self.requests))
        self.assertEqual(self.digest(), self.digest(swapped))

    def test_missing_duplicate_and_wrong_attribution_fail(self):
        for kind in ("seek_requested", "seek_dispatched", "seeked", "seek_presented"):
            for mutation in ("missing", "duplicate", "action_id", "generation", "source", "source_token", "session_token", "source_url", "fixture_id", "reason", "target_ms", "stale"):
                with self.subTest(kind=kind, mutation=mutation):
                    events = copy.deepcopy(self.events)
                    event = self.phase(events, kind)
                    if mutation == "missing":
                        events.remove(event)
                    elif mutation == "duplicate":
                        events.insert(events.index(event), copy.deepcopy(event))
                    elif mutation in ("action_id", "generation", "source"):
                        event[mutation] = 99 if mutation == "generation" else "wrong"
                    else:
                        event["payload"][mutation] = 2000 if mutation == "target_ms" else True if mutation == "stale" else "wrong"
                    try:
                        result = self.digest(events)
                    except analyze.InvalidData:
                        continue
                    self.assertNotEqual(self.digest(), result)

    def test_other_behavior_cannot_be_commuted(self):
        events = copy.deepcopy(self.events)
        index = events.index(self.phase(events, "seek_presented"))
        marker = copy.deepcopy(events[0])
        marker["kind"] = "first_presented_frame"
        marker["payload"] = {"authoritative": True}
        events.insert(index, marker)
        swapped = copy.deepcopy(events)
        first, last = (next(i for i, e in enumerate(swapped) if e["kind"] == k)
                       for k in ("seeked", "seek_presented"))
        swapped[first], swapped[last] = swapped[last], swapped[first]
        self.assertNotEqual(self.digest(events), self.digest(swapped))

    def test_generic_completion_aliases_cannot_receive_the_ordering_exception(self):
        for kind, alias in (("seeked", "seek_native_complete"),
                            ("seek_presented", "first_presented_frame"),
                            ("seek_presented", "authoritative_frame_presented")):
            for swapped in (False, True):
                with self.subTest(alias=alias, swapped=swapped):
                    events = copy.deepcopy(self.events)
                    native = events.index(self.phase(events, "seeked"))
                    if swapped:
                        events[native:native + 2] = reversed(events[native:native + 2])
                    self.phase(events, kind)["kind"] = alias
                    with self.assertRaisesRegex(analyze.InvalidData, "exact native/RVFC"):
                        self.digest(events)

    def test_unattributed_seek_phases_cannot_hide_as_unrelated_observations(self):
        for kind in ("seek_requested", "seek_dispatched", "seeked", "seek_presented"):
            for duplicate in (False, True):
                with self.subTest(kind=kind, duplicate=duplicate):
                    events = copy.deepcopy(self.events)
                    phase = self.phase(events, kind)
                    if duplicate:
                        phase = copy.deepcopy(phase)
                        events.append(phase)
                    del phase["action_id"]
                    with self.assertRaisesRegex(analyze.InvalidData, "lacks action attribution"):
                        self.digest(events)

    def test_identical_target_actions_still_keep_their_requested_order(self):
        first = [e for e in copy.deepcopy(self.events) if e.get("action_id")]
        second = copy.deepcopy(first)
        for e in second:
            e["action_id"] = "second"
            e["monotonic_ms"] += 1000
        reordered = copy.deepcopy(second + first)
        for index, e in enumerate(reordered):
            e["monotonic_ms"] = 20000 + index
        self.assertNotEqual(self.digest(first + second), self.digest(reordered))

    def test_authority_recovery_request_and_loss_remain_strict(self):
        for field, value in (("authoritative", False), ("event_loss", True)):
            events = copy.deepcopy(self.events)
            self.phase(events, "seek_presented")["payload"][field] = value
            with self.assertRaises(analyze.InvalidData):
                self.digest(events)
        events = copy.deepcopy(self.events)
        recovery = copy.deepcopy(events[0])
        recovery["kind"] = "recovery_attempted"
        events.append(recovery)
        self.assertNotEqual(self.digest(), self.digest(events))
        for field in ("route_class", "status", "action_id", "generation", "outcome"):
            requests = copy.deepcopy(self.requests)
            container = requests[0] if field in requests[0] else requests[0]["payload"]
            container[field] = 500 if field == "status" else 99 if field == "generation" else "wrong"
            self.assertNotEqual(self.digest(), self.digest(requests=requests))

    def test_observed_settlement_must_stay_inside_production_frame_boundary(self):
        for kind, field in (("seeked", "actual_media_time_ms"),
                            ("seek_presented", "presented_media_time_ms")):
            for offset, valid in ((-1000 / 60 - .000999, True), (-1000 / 60, True),
                                  (0, True), (1000 / 60, True), (1000 / 60 + .000999, True),
                                  (-1000 / 60 - .002, False), (1000 / 60 + .002, False)):
                with self.subTest(kind=kind, offset=offset):
                    events = copy.deepcopy(self.events)
                    self.phase(events, "seeked")["payload"]["actual_media_time_ms"] = 1000
                    self.phase(events, "seek_presented")["payload"]["presented_media_time_ms"] = 1000
                    self.phase(events, kind)["payload"][field] += offset
                    if valid:
                        self.digest(events)
                    else:
                        with self.assertRaisesRegex(analyze.InvalidData, "off target"):
                            self.digest(events)

    def test_missing_or_invalid_verified_cadence_fails_through_analysis(self):
        for mutation in ("missing", "zero", "malformed", "negative", "undecoded", "ambiguous"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as root:
                builder = BundleBuilder(Path(root)).as_observer_control()
                fixture = builder.manifest["fixtures"][0]
                validation = fixture["media_validation"][0]
                video = validation["ffprobe"]["streams"][0]
                if mutation == "missing":
                    del fixture["media_validation"]
                elif mutation == "undecoded":
                    validation["decode_ok"] = False
                elif mutation == "ambiguous":
                    validation["ffprobe"]["streams"].append(copy.deepcopy(video))
                else:
                    video["avg_frame_rate"] = {"zero": "0/0", "malformed": "bad", "negative": "-60/1"}[mutation]
                with self.assertRaisesRegex(analyze.InvalidData, "verified fixture frame duration"):
                    analyze.analyze_many([builder.persist()])

    def test_fixture_id_alias_retains_verified_position_checks_through_analysis(self):
        for offset, valid in ((-1000 / 60, True), (1000 / 60, True),
                              (-1000 / 60 - .002, False), (1000 / 60 + .002, False)):
            with self.subTest(offset=offset), tempfile.TemporaryDirectory() as root:
                builder = BundleBuilder(Path(root)).as_observer_control()
                fixture = builder.manifest["fixtures"][0]
                fixture["fixture_id"] = fixture.pop("id")
                self.phase(builder.events, "seek_presented")["payload"]["presented_media_time_ms"] += offset
                if valid:
                    self.assertIn("semantic_outcome_sha256", analyze.analyze_many([builder.persist()])["trials"][0])
                else:
                    with self.assertRaisesRegex(analyze.InvalidData, "off target"):
                        analyze.analyze_many([builder.persist()])

    def test_control_gate_uses_versioned_semantics_and_requires_raw_reanalysis(self):
        with tempfile.TemporaryDirectory() as full_root, tempfile.TemporaryDirectory() as minimal_root:
            minimal, full = fixtures.ReplayAnalyzerTests.control_reports(
                Path(minimal_root), Path(full_root), full_presented_first=True,
            )
            self.assertNotEqual(minimal["trials"][0]["outcome_sha256"], full["trials"][0]["outcome_sha256"])
            self.assertEqual(minimal["trials"][0]["semantic_outcome_sha256"], full["trials"][0]["semantic_outcome_sha256"])
            self.assertTrue(analyze.validate_observer_control(minimal, full)["gate_passed"])
            del full["trials"][0]["semantic_outcome_sha256"]
            with self.assertRaisesRegex(analyze.InvalidData, "semantic_outcome"):
                analyze.validate_observer_control(minimal, full)


if __name__ == "__main__":
    unittest.main()
