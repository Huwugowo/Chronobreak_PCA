import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import analyze
from opening_attribution import open_cycles


def cycle(index):
    start = index * 1000
    generation = index + 1
    def event(kind, offset, **payload):
        source = "app" if kind in {"playback_payload_requested", "playback_payload_backend_ready"} else "frontend"
        return dict(kind=kind, source=source, monotonic_ms=start + offset,
                    generation=generation - 1 if kind == "viewer_mounted" else generation, payload=payload)
    return [
        event("replay_requested" if index == 0 else "viewer_reopen_requested", 0),
        event("playback_payload_requested", 101),
        event("playback_payload_backend_ready", 102, elapsed_ms=1),
        event("playback_payload_ready", 103),
        event("viewer_mounted", 104),
        event("media_loadstart", 110),
        event("metadata_ready", 160),
        event("media_canplay", 180),
        event("first_presented_frame", 190, authoritative=True),
    ]


class OpeningAttributionTests(unittest.TestCase):
    def test_warm_initial_mount_is_separate_from_five_remounts(self):
        events = [event for index in range(6) for event in cycle(index)]
        rows = open_cycles(list(reversed(events)), "warm_open", 6)
        self.assertEqual([row["role"] for row in rows], ["initial_mount"] + ["warm_remount"] * 5)
        self.assertEqual(rows[1]["request_to_first_presented_frame_ms"], 190)
        self.assertEqual(rows[1]["mount_to_first_frame_ms"], 86)
        self.assertEqual(rows[1]["loadstart_to_metadata_ms"], 50)
        self.assertTrue(rows[1]["request_includes_remount_timer"])
        self.assertFalse(rows[0]["request_includes_remount_timer"])

    def test_missing_or_ambiguous_phases_are_rejected(self):
        for kind in ("playback_payload_ready",
                     "viewer_mounted", "media_loadstart", "metadata_ready", "media_canplay", "first_presented_frame"):
            with self.subTest(kind=kind), self.assertRaises(analyze.InvalidData):
                open_cycles([event for event in cycle(0) if event["kind"] != kind], "cold_open", 1)
        with self.assertRaises(analyze.InvalidData):
            open_cycles(cycle(0) + [cycle(0)[4]], "cold_open", 1)

    def test_stale_generation_and_non_authoritative_frame_are_rejected(self):
        for mutation in ("stale", "non_authoritative", "early"):
            events = copy.deepcopy(cycle(0))
            if mutation == "stale": events[-1]["generation"] = 99
            elif mutation == "non_authoritative": events[-1]["payload"]["authoritative"] = False
            else: events[-1]["monotonic_ms"] = 102
            with self.subTest(mutation=mutation), self.assertRaises(analyze.InvalidData):
                open_cycles(events, "cold_open", 1)

    def test_reused_generation_and_wrong_cycle_count_are_rejected(self):
        events = cycle(0) + cycle(1)
        with self.assertRaises(analyze.InvalidData):
            open_cycles(events, "warm_open", 6)
        for event in events: event["generation"] = 1
        with self.assertRaises(analyze.InvalidData):
            open_cycles(events, "warm_open", 2)

    def test_native_clock_offset_cannot_assign_payload_to_a_frontend_cycle(self):
        events = cycle(0) + cycle(1)
        for event in events:
            if event["source"] == "app":
                event["monotonic_ms"] -= 500
        rows = open_cycles(events, "warm_open", 2)
        self.assertEqual([row["generation"] for row in rows], [1, 2])
        self.assertNotIn("backend_payload_ms", rows[0])


if __name__ == "__main__":
    unittest.main()
