"""Physical native resize evidence has separate paused and playing authorities."""
import copy
import unittest
from test_analyze import analyze


def events():
    result = []
    def emit(kind, at, action=None, **payload):
        result.append(dict(kind=kind, monotonic_ms=at, source="frontend", generation=3, action_id=action,
                           payload=dict(scale_factor=1.25, **payload)))
    for kind in ("library_requested", "library_useful", "replay_requested", "playback_payload_ready", "viewer_mounted",
                 "media_loadstart", "metadata_ready", "media_canplay", "first_presented_frame"):
        emit(kind, 0)
    emit("desktop_resize_started", 1, original_width=1800, original_height=1125,
         target_sizes=[dict(width=1350, height=850), dict(width=1800, height=1125)])
    for index in range(16):
        mode = "playing" if index < 8 else "paused"
        size = dict(width=1350, height=850) if index % 2 == 0 else dict(width=1800, height=1125)
        for kind, offset in (("requested", 0), ("geometry", 10), ("completed", 30)):
            payload = dict(mode=mode, **size)
            if kind == "completed":
                payload.update(authority="rvfc" if mode == "playing" else "two-animation-frame-paint-opportunity",
                               media_tick=300 if mode == "playing" else None, same_load=True, same_element=True)
            emit("desktop_resize_" + kind, 100 + index * 300 + offset, f"resize-{index}", **payload)
    emit("desktop_resize_restored", 5000, width=1800, height=1125)
    emit("scenario_completed", 5010)
    return result


class ResizeEvidenceTests(unittest.TestCase):
    def test_accepts_distinct_authorities_and_reports_separate_metrics(self):
        records = events()
        analyze._validate_scenario_evidence(analyze.TrialKey("resize", "1"), dict(kind="desktop_resize"), records)
        metrics = analyze._trial_metrics(analyze.TrialKey("resize", "1"), records, [], [])
        self.assertEqual(metrics["desktop_resize_playing_request_to_presented_ms"], [30] * 8)
        self.assertEqual(metrics["desktop_resize_paused_request_to_painted_ms"], [30] * 8)

    def test_missing_wrong_generation_geometry_authority_pacing_and_restore_reject(self):
        mutations = [
            lambda records: records.remove(next(e for e in records if e["kind"] == "desktop_resize_geometry")),
            lambda records: records.append(copy.deepcopy(next(e for e in records if e["kind"] == "desktop_resize_completed"))),
            lambda records: next(e for e in records if e["kind"] == "desktop_resize_completed").update(generation=4),
            lambda records: next(e for e in records if e["kind"] == "desktop_resize_completed")["payload"].update(width=999),
            lambda records: next(e for e in records if e["kind"] == "desktop_resize_completed")["payload"].update(authority="approximate"),
            lambda records: next(e for e in records if e["kind"] == "desktop_resize_completed" and e["payload"]["mode"] == "paused")["payload"].update(media_tick=300),
            lambda records: next(e for e in records if e["action_id"] == "resize-1" and e["kind"] == "desktop_resize_requested").update(monotonic_ms=200),
            lambda records: next(e for e in records if e["kind"] == "desktop_resize_restored")["payload"].update(width=999),
            lambda records: [e["payload"].update(width=1350, height=850) for e in records if e["kind"] in {"desktop_resize_requested", "desktop_resize_geometry", "desktop_resize_completed"}],
            lambda records: next(e for e in records if e["action_id"] == "resize-0" and e["kind"] == "desktop_resize_completed").update(monotonic_ms=450),
        ]
        for index, mutate in enumerate(mutations):
            records = events(); mutate(records)
            with self.subTest(case=index), self.assertRaises(analyze.InvalidData):
                analyze._validate_scenario_evidence(analyze.TrialKey("resize", "1"), dict(kind="desktop_resize"), records)
