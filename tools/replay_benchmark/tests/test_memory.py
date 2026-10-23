"""Comparable quiet windows must not rely on cross-process clock alignment."""
import copy
import unittest

from test_analyze import analyze


def evidence():
    events, samples = [], []
    def emit(at, kind, **payload):
        events.append({"source": "frontend", "kind": kind, "monotonic_ms": at, "payload": payload})
    for cycle in range(7):
        at = 1_000_000 + cycle * 20000  # Deliberately unrelated to runner time.
        if cycle:
            emit(at - 4000, "replay_render_ready", participant_count=10, rendered_marker_count=46,
                 decoded_image_count=30, authority="dom-and-image-decode-after-two-animation-frames")
            emit(at - 1900, "steady_playback", duration_ms=2001, media_advance_ms=1990)
            emit(at - 1800, "viewer_cycle_completed", playback_rate=1)
        emit(at, "memory_window_started", cycle=cycle, settle_ms=5000, sample_ms=10000)
        emit(at + 10001, "memory_window_completed", cycle=cycle)
        for index in range(10):
            samples.append({"monotonic_ms": cycle * 20000 + index * 1000, "memory_window_cycle": cycle,
                            "process_tree_private_bytes": cycle * 100 + (9999 if index in (0, 1, 8, 9) else index),
                            "working_set_bytes": 200, "handles": 30})
    return events, samples


class MemoryTests(unittest.TestCase):
    def test_uses_runner_labels_and_discards_boundary_samples(self):
        events, samples = evidence()
        windows = analyze.settled_memory_windows(events, samples)
        self.assertEqual(len(windows), 7)
        self.assertEqual(windows[1]["interior_samples"], 6)
        self.assertEqual(windows[1]["process_tree_private_bytes"]["median"], 104.5)

    def test_rejects_incomplete_or_misordered_evidence(self):
        for failure in ("missing-window", "short-window", "missing-cycle", "unpopulated", "short-playback", "missing-samples", "bad-label", "reordered-label"):
            with self.subTest(failure=failure):
                events, samples = copy.deepcopy(evidence())
                if failure == "missing-window": events.pop()
                if failure == "short-window": events[1]["monotonic_ms"] -= 10
                if failure == "missing-cycle": events.pop(4)
                if failure == "unpopulated": events[2]["payload"]["decoded_image_count"] = 0
                if failure == "short-playback": events[3]["payload"]["media_advance_ms"] = 0
                if failure == "missing-samples": samples = samples[:-2]
                if failure == "bad-label": samples[0]["memory_window_cycle"] = True
                if failure == "reordered-label": samples[0]["memory_window_cycle"] = 1
                with self.assertRaises(analyze.InvalidData):
                    analyze.settled_memory_windows(events, samples)


if __name__ == "__main__":
    unittest.main()
