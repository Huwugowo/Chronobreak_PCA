"""The composed workload retains each real phase and rejects stale/partial evidence."""
import copy
import unittest
from test_analyze import analyze
from test_resize import events as resize_events


def events():
    result = []
    def emit(kind, at, action=None, **payload):
        result.append(dict(kind=kind, monotonic_ms=at, source="frontend", generation=3, action_id=action, payload=payload))
    emit("search_preconditioning_ready", 1, catalog_count=2, iterations=8)
    emit("search_workload_completed", 10, search_sequences=24)
    emit("replay_requested", 11)
    render = dict(participant_count=10, event_count=107, source_epoch=0, source_token="fixture-source", media_tick=52_433 * 48_000)
    emit("seek_requested", 12, "seek-pre-render", reason="render-preconditioning", raw_requested_ms=52_433)
    emit("replay_render_admitted", 13, **render)
    emit("replay_render_ready", 15, **render, rendered_marker_count=50, decoded_image_count=30, decoded_url_count=20,
         authority="dom-and-image-decode-after-two-animation-frames")
    emit("steady_playback", 30_020, duration_ms=30_000, media_advance_ms=30_000)
    emit("seek_requested", 30_030, "seek-pre-strata", reason="seek-preconditioning", raw_requested_ms=0)
    for index in range(160):
        emit("seek_requested", 30_050 + index * 300, f"seek-{index}", reason="benchmark")
        emit("seek_dispatched", 30_051 + index * 300, f"seek-{index}", reason="benchmark", distance_ms=[5000, 400000, -5000, -400000][index % 4])
    for burst in range(5):
        at = 80_000 + burst * 1000
        emit("scrub_started", at, request_count=8)
        for index in range(8):
            emit("seek_requested", at + 10 + index * 17, f"scrub-{burst}-{index}", reason="benchmark-scrub")
        emit("scrub_settled", at + 300, request_count=8)
    for event in resize_events():
        if event["kind"].startswith("desktop_resize_"):
            event["monotonic_ms"] += 86_000
            result.append(event)
    emit("viewer_cycle_completed", 92_000, playback_rate=1)
    emit("viewer_close_requested", 92_010)
    emit("library_return_ready", 92_050, catalog_count=2, authority="two-animation-frame-paint-opportunity")
    emit("scenario_completed", 92_060, viewer_cycles=1)
    return sorted(result, key=lambda event: event["monotonic_ms"])


class WholeAppTests(unittest.TestCase):
    def test_accepts_complete_composed_workflow(self):
        analyze._validate_whole_app_evidence(events())

    def test_adjacent_bursts_with_equal_clock_ticks_keep_stream_membership(self):
        records = events()
        starts = [event for event in records if event['kind'] == 'scrub_started']
        settles = [event for event in records if event['kind'] == 'scrub_settled']
        starts[1]['monotonic_ms'] = settles[0]['monotonic_ms']
        next(event for event in records if event.get('action_id') == 'scrub-1-0')['monotonic_ms'] = settles[0]['monotonic_ms']
        analyze._validate_whole_app_evidence(records)

    def test_missing_stale_reordered_short_and_wrong_counts_reject(self):
        mutations = [
            lambda es: es.remove(next(e for e in es if e["kind"] == "library_return_ready")),
            lambda es: next(e for e in es if e["kind"] == "replay_render_ready")["payload"].pop("source_epoch"),
            lambda es: next(e for e in es if e["kind"] == "replay_render_ready")["payload"].update(source_token="stale"),
            lambda es: next(e for e in es if e["kind"] == "replay_render_ready")["payload"].update(decoded_image_count=0),
            lambda es: next(e for e in es if e["kind"] == "replay_render_ready")["payload"].update(media_tick=0),
            lambda es: next(e for e in es if e["kind"] == "replay_render_admitted").update(generation=2),
            lambda es: next(e for e in es if e["kind"] == "search_preconditioning_ready")["payload"].update(catalog_count=1),
            lambda es: next(e for e in es if e["kind"] == "steady_playback")["payload"].update(duration_ms=1000),
            lambda es: next(e for e in es if e["kind"] == "steady_playback")["payload"].update(media_advance_ms=1000),
            lambda es: next(e for e in es if e["kind"] == "viewer_cycle_completed")["payload"].update(playback_rate=2),
            lambda es: next(e for e in es if e["kind"] == "library_return_ready").update(monotonic_ms=0),
            lambda es: es.remove(next(e for e in es if e["kind"] == "seek_requested" and e["action_id"] == "seek-1")),
            lambda es: next(e for e in es if e["kind"] == "seek_dispatched")["payload"].update(distance_ms=400000),
            lambda es: next(e for e in es if e["kind"] == "scrub_settled")["payload"].update(request_count=7),
            lambda es: next(e for e in es if e["kind"] == "seek_requested" and e["action_id"] == "scrub-0-0").update(monotonic_ms=0),
        ]
        for index, mutate in enumerate(mutations):
            records = copy.deepcopy(events()); mutate(records)
            with self.subTest(case=index), self.assertRaises(analyze.InvalidData):
                analyze._validate_whole_app_evidence(records)
