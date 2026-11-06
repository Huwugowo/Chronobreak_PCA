"""Reconcile real packet QPC coverage without claiming media synchronization."""
from __future__ import annotations

import argparse
from fractions import Fraction
import hashlib
import json
from pathlib import Path


def analyze_group(group: dict) -> dict:
    rows = group["packet_ledger"]
    if not rows or len(rows) != group["packets"]:
        raise ValueError("Missing or inconsistent packet ledger")
    cumulative = 0
    previous_end = None
    gaps = []
    overlaps = []
    timestamp_errors = 0
    discontinuities = 0
    total_delta = Fraction(0)
    for index, row in enumerate(rows):
        if row["cumulative_start_frame"] != cumulative or row["frames"] <= 0:
            raise ValueError("Packet sample accounting is inconsistent")
        start = Fraction(int(row["qpc_100ns"]))
        if start <= 0:
            raise ValueError("Packet QPC is unavailable")
        if previous_end is not None:
            delta = start - previous_end
            total_delta += delta
            item = {"packet": index, "duration_ms": float(delta / 10000),
                    "previous_peak": rows[index - 1]["peak"], "next_peak": row["peak"],
                    "discontinuity_flag": bool(row["flags"] & 1)}
            if delta > 0:
                gaps.append(item)
            elif delta < 0:
                overlaps.append(item)
        timestamp_errors += bool(row["flags"] & 4)
        discontinuities += index > 0 and bool(row["flags"] & 1)
        previous_end = start + Fraction(row["frames"] * 10000000, 48000)
        cumulative += row["frames"]
    if cumulative != group["captured_frames"]:
        raise ValueError("Final sample count differs from packet ledger")
    span = previous_end - int(rows[0]["qpc_100ns"])
    samples_duration = Fraction(cumulative * 10000000, 48000)
    if span - samples_duration != total_delta:
        raise ValueError("QPC span did not reconcile exactly")
    return {
        "packets": len(rows), "captured_frames": cumulative,
        "qpc_span_ms": float(span / 10000), "sample_duration_ms": float(samples_duration / 10000),
        "gap_total_ms": float(sum((Fraction(str(item["duration_ms"])) for item in gaps), Fraction(0))),
        "gap_count": len(gaps), "gaps": gaps,
        "overlap_count": len(overlaps), "overlaps": overlaps,
        "timestamp_errors": timestamp_errors,
        "post_start_discontinuities": discontinuities,
        "device_positions_all_zero": all(int(row["device_frame"]) == 0 for row in rows),
        "span_minus_samples_equals_sum_of_packet_deltas_exactly": True,
        "arithmetic_reconciliation_passed": True,
        "contiguous_valid_timestamp_coverage": not gaps and not overlaps and not timestamp_errors and not discontinuities,
        "capture_video_alignment_proven": False,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    raw = args.report.read_bytes()
    source = json.loads(raw.decode("utf-8-sig"))
    report = {
        "schema_version": 1, "feature": "QB-AUDIO-001",
        "scope": "packet timestamp/sample accounting; no WGC/video or decoded-media comparison",
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "include": analyze_group(source["observation"]["include"]),
        "exclude": analyze_group(source["observation"]["exclude"]),
        "source_separation_passed": source["capture_separation_proven"],
        "capture_video_alignment_proven": False, "replay_synchronization_proven": False,
        "interpretation": "Contiguous sample concatenation discards observed timestamp gaps. Raw QPC gaps require explicit capture-time coverage policy; they are not a measured A/V drift or permission to compensate in playback.",
    }
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    print(json.dumps({name: {key: group[key] for key in (
        "packets", "gap_count", "gap_total_ms", "overlap_count", "arithmetic_reconciliation_passed", "contiguous_valid_timestamp_coverage"
    )} for name, group in (("include", report["include"]), ("exclude", report["exclude"]))}))


if __name__ == "__main__":
    main()
