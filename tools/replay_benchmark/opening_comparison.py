"""Strict offline replay-opening-comparison-v1 measurement of validated bundles."""
from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path
from typing import Any

import analyze

CONTRACT = "replay-opening-comparison-v1"
CYCLE_METRICS = dict.fromkeys((
    "request_to_viewer_mounted_ms", "request_to_media_loadstart_ms",
    "request_to_metadata_ready_ms", "request_to_media_canplay_ms",
    "request_to_first_presented_frame_ms", "request_to_playback_payload_ready_ms",
    "mount_to_loadstart_ms", "mount_to_metadata_ms", "mount_to_canplay_ms",
    "mount_to_first_frame_ms", "loadstart_to_metadata_ms", "metadata_to_canplay_ms",
    "full_payload_ready_minus_mount_ms",
), "ms")
PROCESS_METRICS = {
    "library_request_to_games_usable_ms": "ms",
    "library_request_to_useful_ms": "ms",
    "process_tree_cpu_percent_mean": "percentage_point",
    "process_tree_cpu_percent_max": "percentage_point",
    "process_tree_cpu_time_100ns_delta": "count",
    "whole_system_cpu_percent_mean": "percentage_point",
    "whole_system_cpu_percent_max": "percentage_point",
    **{f"{name}_{suffix}": unit
       for name, unit in (("process_tree_private_bytes", "bytes"), ("working_set_bytes", "bytes"),
                          ("handles", "count"), ("threads", "count"))
       for suffix, unit in (("max", unit), ("growth", unit), ("sustained_monotonic_growth_flag", "count"))},
    **{f"io_{kind}_{unit}_delta": "bytes" if unit == "bytes" else "count"
       for kind in ("read", "write", "other") for unit in ("bytes", "operations")},
    **{name: "count" for name in ("server_requests_count", "server_completed_count",
                                  "server_cancelled_count", "server_error_count",
                                  "server_ranges_count", "server_peak_active_streams")},
    "server_delivered_bytes": "bytes",
}
RELIABILITY_TOKENS = {
    "reliability_error_events_count": ("error", "failed", "timeout", "exhausted"),
    "reliability_recovery_events_count": ("recovery", "degraded"),
    "reliability_stale_events_count": ("stale", "wrong_result"),
}
PROCESS_METRICS.update(dict.fromkeys(RELIABILITY_TOKENS, "count"))
REQUIRED_METRICS = {**CYCLE_METRICS, **PROCESS_METRICS}
LIMITATIONS = [
    "Cold means fresh process, not cold OS file cache; post-hash reads can warm that cache.",
    "Warm comparison uses five process medians; the 25 remounts are not independent processes.",
    "Warm request intervals include the existing 100 ms disposal/remount timer.",
    "Full payload timing is boundary-local, not generation-correlated. An indistinguishable lone stale payload cannot be detected.",
    "Accepted keyed-controller and native correctness evidence supplies payload identity; this telemetry does not prove it.",
    "Native/backend/server clocks are not causally joined to frontend cycles.",
    "Job I/O is aggregate process I/O, not physical disk I/O.",
    "HTTP cancellation does not establish child-process failure.",
    "Optional GPU counters are unavailable, not zero.",
    "Installed WebView bytes are checked before launch; live telemetry proves matching descendant versions, not executable-byte identity.",
    "CPU quantum is independently sampled by each runner; exact identity mismatch rejects evidence without tolerance or retry.",
    "Five workload cells cover axes, not all long-media and large-library combinations.",
    "Derived R/L media is not independent long-recorder evidence; six mounts do not prove indefinite leak freedom.",
]


def open_cycles(events: list[dict[str, Any]], kind: str, iterations: int) -> list[dict[str, Any]]:
    expected = {"cold_open": 1, "warm_open": 6}.get(kind)
    if expected is None or isinstance(iterations, bool) or iterations != expected:
        raise analyze.InvalidData("opening contract requires one cold or six warm cycles")
    # Stable sort + index slices preserve JSONL order for equal-time ties.
    ordered = sorted((event for event in events if event.get("source") == "frontend"),
                     key=lambda event: analyze._nonnegative_number(event.get("monotonic_ms"), "frontend time"))
    boundaries = [i for i, event in enumerate(ordered)
                  if event["kind"] in {"replay_requested", "viewer_reopen_requested"}]
    if len(boundaries) != expected or ordered[boundaries[0]]["kind"] != "replay_requested":
        raise analyze.InvalidData("missing or ambiguous opening boundaries")
    if any(ordered[i]["kind"] != "viewer_reopen_requested" for i in boundaries[1:]):
        raise analyze.InvalidData("repeated initial replay request")
    observed = {"viewer_mounted", "media_loadstart", "metadata_ready", "media_canplay",
                "first_presented_frame", "playback_payload_ready", "viewer_cycle_completed", "scenario_completed"}
    if any(event["kind"] in observed for event in ordered[:boundaries[0]]):
        raise analyze.InvalidData("opening evidence precedes initial request")
    rows, generations = [], set()
    for ordinal, begin in enumerate(boundaries):
        end = boundaries[ordinal + 1] if ordinal + 1 < expected else len(ordered)
        section = ordered[begin:end]
        boundary = section[0]

        def matches(name):
            return [event for event in section if event["kind"] == name]

        def one(name):
            found = matches(name)
            if len(found) != 1:
                raise analyze.InvalidData(f"cycle {ordinal} requires exactly one {name}")
            return found[0]

        mounted, loadstart = one("viewer_mounted"), one("media_loadstart")
        payload, metadata, frame = (one(name) for name in
                                    ("playback_payload_ready", "metadata_ready", "first_presented_frame"))
        canplays = matches("media_canplay")
        if not canplays:
            raise analyze.InvalidData("opening lacks canplay")
        canplay = canplays[0]
        generation = analyze._field(loadstart, "generation")
        if isinstance(generation, bool) or not isinstance(generation, int) or generation < 0 or generation in generations:
            raise analyze.InvalidData("missing or reused opening generation")
        generations.add(generation)
        for event in section:
            if event["kind"] in {"media_loadstart", "metadata_ready", "media_canplay",
                                 "first_presented_frame", "viewer_cycle_completed"}:
                if analyze._field(event, "generation") != generation:
                    raise analyze.InvalidData("stale media generation in opening")
        if analyze._field(frame, "authoritative") is not True:
            raise analyze.InvalidData("opening requires authoritative RVFC frame")

        def delta(last, first):
            value = float(last["monotonic_ms"]) - float(first["monotonic_ms"])
            if value < 0 or section.index(last) < section.index(first):
                raise analyze.InvalidData("opening phase precedes its start")
            return round(value, 6)

        terminal = one("viewer_cycle_completed") if kind == "warm_open" else one("scenario_completed")
        if kind == "cold_open" and matches("viewer_cycle_completed"):
            raise analyze.InvalidData("unexpected cold cycle terminal")
        if kind == "warm_open":
            if ordinal < 5 and matches("scenario_completed"):
                raise analyze.InvalidData("premature final terminal")
            if ordinal == 5:
                delta(one("scenario_completed"), terminal)
        for event in (mounted, loadstart, payload, metadata, frame, canplay):
            delta(event, boundary)
            delta(terminal, event)
        row = {"ordinal": ordinal, "role": "initial_mount" if ordinal == 0 else "warm_remount",
               "generation": generation, "request_includes_remount_timer": ordinal > 0}
        for name, event in (("viewer_mounted", mounted), ("media_loadstart", loadstart),
                            ("metadata_ready", metadata), ("media_canplay", canplay),
                            ("first_presented_frame", frame), ("playback_payload_ready", payload)):
            row[f"request_to_{name}_ms"] = delta(event, boundary)
        for name, event in (("loadstart", loadstart), ("metadata", metadata),
                            ("canplay", canplay), ("first_frame", frame)):
            row[f"mount_to_{name}_ms"] = delta(event, mounted)
        row["loadstart_to_metadata_ms"] = delta(metadata, loadstart)
        row["metadata_to_canplay_ms"] = delta(canplay, metadata)
        delta(frame, loadstart)
        row["full_payload_ready_minus_mount_ms"] = round(float(payload["monotonic_ms"]) - float(mounted["monotonic_ms"]), 6)
        rows.append(row)
    return rows


def process_metrics(bundle) -> dict[str, float]:
    key = next(iter(bundle.expected))
    raw = analyze._trial_metrics(key, bundle.events, bundle.requests, bundle.samples)
    # Legacy analyzer does not derive deltas for every Job counter. Read these
    # required counters directly, preserving their whole-process scope.
    for kind in ("read", "write", "other"):
        for unit in ("bytes", "operations"):
            field = f"io_{kind}_{unit}"
            values = [analyze._nonnegative_number(analyze._field(sample, field), field)
                      for sample in bundle.samples]
            if len(values) < 3 or any(b < a for a, b in zip(values, values[1:])):
                raise analyze.InvalidData("missing or decreasing Job counters")
            raw[field + "_delta"] = [values[-1] - values[0]]
    for name, tokens in RELIABILITY_TOKENS.items():
        raw[name] = [sum(any(token in str(event["kind"]).lower() for token in tokens)
                         for event in bundle.events)]
    result = {}
    for name in PROCESS_METRICS:
        values = raw.get(name)
        if not values:
            raise analyze.InvalidData(f"required process metric missing: {name}")
        result[name] = statistics.median([analyze._finite_number(v, name) for v in values])
    return result


def extract(bundle, cell: str, mode: str, arm: str, sequence: int) -> dict[str, Any]:
    if cell not in "SRLNE" or len(cell) != 1 or mode not in {"cold", "warm"} or arm not in {"reference", "candidate"}:
        raise analyze.InvalidData("invalid campaign alias")
    if len(bundle.expected) != 1:
        raise analyze.InvalidData("opening requires one scenario per process")
    key, scenario = next(iter(bundle.expected.items()))
    if scenario["kind"] != mode + "_open" or bundle.manifest["observer_profile"] != "minimal":
        raise analyze.InvalidData("campaign scenario/profile mismatch")
    cycles = open_cycles(bundle.events, scenario["kind"], scenario.get("iterations", 6 if mode == "warm" else 1))
    measured = cycles[1:] if mode == "warm" else cycles
    metrics = {name: statistics.median(row[name] for row in measured) for name in CYCLE_METRICS}
    metrics.update(process_metrics(bundle))
    return {"run_alias": f"run-{sequence:03d}", "cell": cell, "mode": mode, "arm": arm,
            "fingerprint_sha256": bundle.identity_by_scenario[key.scenario_id],
            "subject_sha256": bundle.subject_by_scenario[key.scenario_id],
            "cycles": cycles, "process_values": metrics}


def compare_group(reference: list[dict], candidate: list[dict], cell: str, mode: str) -> dict:
    if len(reference) != 5 or len(candidate) != 5:
        raise analyze.InvalidData("comparison requires five independent processes per arm")
    all_rows = reference + candidate
    if len({row["run_alias"] for row in all_rows}) != 10:
        raise analyze.InvalidData("duplicate process observation")
    if len({row["fingerprint_sha256"] for row in all_rows}) != 1:
        raise analyze.InvalidData("comparison fingerprints differ")
    for arm_rows, arm in ((reference, "reference"), (candidate, "candidate")):
        if len({row["subject_sha256"] for row in arm_rows}) != 1:
            raise analyze.InvalidData("subject changes within arm")
        for row in arm_rows:
            if (row["cell"], row["mode"], row["arm"]) != (cell, mode, arm):
                raise analyze.InvalidData("unmatched comparison membership")
            if set(row["process_values"]) != set(REQUIRED_METRICS):
                raise analyze.InvalidData("asymmetric or missing required metric inventory")
            for name, value in row["process_values"].items():
                analyze._finite_number(value, name)
    scenario = f"qb010-library-v2-replay-opening-{cell.lower()}-{mode}"

    def report(rows):
        summaries = []
        for name, unit in REQUIRED_METRICS.items():
            values = [row["process_values"][name] for row in rows]
            median = statistics.median(values)
            summaries.append({
                "scenario_id": scenario, "metric": name, "trial_statistic": "median", "unit": unit,
                "direction": "neutral" if name == "full_payload_ready_minus_mount_ms" else "lower",
                "fingerprint_sha256": rows[0]["fingerprint_sha256"],
                "subject_sha256": rows[0]["subject_sha256"], "eligible_trials": 5,
                "observation_count": 25 if mode == "warm" and name in CYCLE_METRICS else 5,
                "distribution": {"count": 5, "median": median,
                                 "mad": statistics.median(abs(v - median) for v in values)},
            })
        return {"schema_version": analyze.SCHEMA_VERSION, "status": "valid", "summaries": summaries}

    before, after = report(reference), report(candidate)
    comparison = analyze.compare_reports(before, after)
    if len(comparison["metrics"]) != len(REQUIRED_METRICS) or any(not m["eligible"] for m in comparison["metrics"]):
        raise analyze.InvalidData("insufficient symmetric metric eligibility")
    for item, summary in zip(comparison["metrics"], sorted(before["summaries"], key=lambda s: s["metric"])):
        item["reference_mad"] = summary["distribution"]["mad"]
        item["unit"] = summary["unit"]
        item["process_values_per_arm"] = 5
        item["improvement_beyond_band_and_five_percent"] = (
            item["direction"] == "lower" and -item["delta"] > item["repeatability_band"]
            and -item["delta"] > abs(item["baseline_median"]) * .05)
    # Individual growth/error signals cannot disappear in a five-process median.
    signals = [{"run_alias": row["run_alias"], "arm": row["arm"], "metric": name, "value": value}
               for row in all_rows for name, value in row["process_values"].items()
               if (name.endswith("_sustained_monotonic_growth_flag") or name == "server_error_count"
                   or name in RELIABILITY_TOKENS) and value > 0]
    return {"cell": cell, "mode": mode, "comparison": comparison, "individual_signals": signals,
            "disposition_required": comparison["disposition_required"] or bool(signals),
            "reference_process_summaries": before["summaries"], "candidate_process_summaries": after["summaries"]}


def compare_campaign(path: Path) -> dict:
    import opening_campaign
    verified = opening_campaign.verify_campaign(path, require_results=True)
    plan = verified["plan"]
    runs = []
    for entry in plan["launches"]:
        bundle = analyze.load_bundle(Path(entry["result_root"]), f"run-{entry['sequence']:03d}")
        runs.append(extract(bundle, entry["cell"], entry["mode"], entry["arm"], entry["sequence"]))
    groups = [compare_group(
        [row for row in runs if (row["cell"], row["mode"], row["arm"]) == (cell, mode, "reference")],
        [row for row in runs if (row["cell"], row["mode"], row["arm"]) == (cell, mode, "candidate")],
        cell, mode) for cell in "SRLNE" for mode in ("cold", "warm")]
    return {"contract": CONTRACT, "status": "compared", "campaign_sha256": verified["campaign_sha256"],
            "required_metric_inventory": REQUIRED_METRICS,
            "primary_metrics": ["request_to_first_presented_frame_ms", "library_request_to_games_usable_ms"],
            "disposition_required": any(group["disposition_required"] for group in groups),
            "runs": runs, "groups": groups, "limitations": LIMITATIONS}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--campaign", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = compare_campaign(args.campaign)
        with args.output.open("x", encoding="utf-8") as output:
            json.dump(result, output, indent=2, allow_nan=False)
            output.write("\n")
    except (ValueError, OSError) as error:
        parser.exit(2, f"opening comparison failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
