"""Extract QB-REPLAY-010 open phases without changing benchmark event meanings.

Inputs must pass the existing complete-run validator. Warm cycle zero remains
separate from measured remounts. Server records have no media-generation key:
they remain whole-process observations and are never joined by timing guesses.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import analyze


def open_cycles(events: list[dict[str, Any]], kind: str, iterations: int) -> list[dict[str, Any]]:
    if kind not in {"cold_open", "warm_open"}:
        raise analyze.InvalidData("opening attribution requires cold_open or warm_open")
    # Native/front-end calibration is not an exact causal join. Partition only
    # front-end events; native payload spans remain whole-process observations.
    ordered = sorted((event for event in events if event.get("source") == "frontend"),
                     key=lambda event: float(event["monotonic_ms"]))
    boundaries = [event for event in ordered if event["kind"] in {
        "replay_requested", "viewer_reopen_requested"
    }]
    expected = iterations if kind == "warm_open" else 1
    if len(boundaries) != expected or not boundaries or boundaries[0]["kind"] != "replay_requested":
        raise analyze.InvalidData("missing or ambiguous opening boundaries")
    if any(event["kind"] != "viewer_reopen_requested" for event in boundaries[1:]):
        raise analyze.InvalidData("repeated initial replay request")
    rows = []
    generations = set()
    for ordinal, boundary in enumerate(boundaries):
        start = float(boundary["monotonic_ms"])
        end = float(boundaries[ordinal + 1]["monotonic_ms"]) if ordinal + 1 < len(boundaries) else float("inf")
        if start >= end:
            raise analyze.InvalidData("opening boundaries are not strictly ordered")
        section = [event for event in ordered if start <= float(event["monotonic_ms"]) < end]

        def one(name: str) -> dict[str, Any]:
            matching = [event for event in section if event["kind"] == name]
            if len(matching) != 1:
                raise analyze.InvalidData(f"cycle {ordinal} requires exactly one {name}")
            return matching[0]

        mounted = one("viewer_mounted")
        loadstart = one("media_loadstart")
        # viewer_mounted precedes controller.open(), which advances generation.
        generation = analyze._field(loadstart, "generation")
        if generation is None or generation in generations:
            raise analyze.InvalidData("missing or reused opening generation")
        generations.add(generation)
        required = {name: one(name) for name in (
            "playback_payload_ready", "media_loadstart", "metadata_ready", "first_presented_frame"
        )}
        canplay = [event for event in section if event["kind"] == "media_canplay"]
        if not canplay:
            raise analyze.InvalidData("opening lacks canplay")
        required["media_canplay"] = canplay[0]
        for name in ("media_loadstart", "metadata_ready", "first_presented_frame", "media_canplay"):
            if any(analyze._field(event, "generation") != generation
                   for event in section if event["kind"] == name):
                raise analyze.InvalidData("stale or ambiguous media generation in opening")
        frame = required["first_presented_frame"]
        if analyze._field(frame, "authoritative") is not True:
            raise analyze.InvalidData("opening requires authoritative presented video")

        def delta(last: dict[str, Any], first: dict[str, Any]) -> float:
            value = float(last["monotonic_ms"]) - float(first["monotonic_ms"])
            if value < 0:
                raise analyze.InvalidData("opening phase precedes its start")
            return round(value, 6)

        # Same-producer deltas; backend command timing uses its own elapsed field.
        row = {"ordinal": ordinal, "role": "initial_mount" if ordinal == 0 else "warm_remount",
               "generation": generation, "request_includes_remount_timer": ordinal > 0}
        for name in ("playback_payload_ready", "media_loadstart", "metadata_ready", "media_canplay", "first_presented_frame"):
            row[f"request_to_{name}_ms"] = delta(required[name], boundary)
        row["request_to_viewer_mounted_ms"] = delta(mounted, boundary)
        row["payload_to_mount_ms"] = delta(mounted, required["playback_payload_ready"])
        row["mount_to_loadstart_ms"] = delta(loadstart, mounted)
        row["mount_to_metadata_ms"] = delta(required["metadata_ready"], mounted)
        row["loadstart_to_metadata_ms"] = delta(required["metadata_ready"], required["media_loadstart"])
        row["metadata_to_canplay_ms"] = delta(required["media_canplay"], required["metadata_ready"])
        row["mount_to_first_frame_ms"] = delta(frame, mounted)
        rows.append(row)
    return rows


def extract(path: Path, alias: str) -> dict[str, Any]:
    bundle = analyze.load_bundle(path, alias)
    if len(bundle.expected) != 1:
        raise analyze.InvalidData("opening extraction requires one scenario per process")
    key, scenario = next(iter(bundle.expected.items()))
    rows = open_cycles(bundle.events, scenario["kind"], scenario.get("iterations", 6))
    backend = [analyze._field(event, "elapsed_ms") for event in bundle.events
               if event["kind"] == "playback_payload_backend_ready" and event.get("source") == "app"]
    requests_count = sum(event["kind"] == "playback_payload_requested" and event.get("source") == "app"
                         for event in bundle.events)
    if len(backend) != len(rows) or requests_count != len(rows) or any(
        isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0 for value in backend
    ):
        raise analyze.InvalidData("opening lacks complete native payload spans")
    requests = []
    for record in bundle.requests:
        if record.get("route_class") != "game_video":
            continue
        requests.append({name: record.get(name) for name in (
            "request_id", "method", "status", "range_start", "range_end", "declared_bytes",
            "delivered_bytes", "started_ms", "first_byte_ms", "completed_ms", "outcome"
        )})
    return {"run_alias": alias, "scenario_id": key.scenario_id, "cycles": rows,
            "backend_payload_elapsed_ms_whole_process": backend,
            "backend_to_cycle_join": "unavailable; no exact cycle key; native durations are not assigned to front-end cycles",
            "game_video_requests_whole_process": requests,
            "server_to_cycle_join": "unavailable; no generation key; no timing-based join"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    paths = [path.resolve() for path in args.input]
    if len(set(paths)) != len(paths):
        parser.error("duplicate input")
    try:
        result = {"feature": "QB-REPLAY-010", "runs": [
            extract(path, f"run-{index:03d}") for index, path in enumerate(paths, 1)
        ], "limitations": [
            "Cold means fresh process, not cold OS file cache.",
            "Warm remounts are within-process observations; initial mounts are separate.",
            "Warm request intervals include the existing 100 ms disposal/remount timer.",
            "Server request lifecycle does not distinguish admission from first-read cost.",
            "No exact server request to media generation join is available."
        ]}
        with args.output.open("x", encoding="utf-8") as output:
            json.dump(result, output, indent=2, allow_nan=False)
            output.write("\n")
    except (analyze.InvalidData, OSError, ValueError) as error:
        parser.exit(2, f"opening attribution failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
