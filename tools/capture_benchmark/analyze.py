#!/usr/bin/env python3
"""Deterministic analyzer for the QueueBack capture benchmark.

Raw collection is intentionally separate from analysis.  The collector writes
PresentMon CSV plus locale-independent raw Windows counter snapshots.  This
module cooks those counters, validates the complete protocol, and emits only a
small typed/sanitized report suitable for version control.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import re
import statistics
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from pathlib import Path
from typing import Any, Iterable, Sequence


REPORT_SCHEMA_VERSION = "2"
SUPPORTED_SCHEMA_VERSIONS = {"1", "2"}
EXPECTED_WARMUP_SECONDS = 60.0
EXPECTED_MEASUREMENT_SECONDS = 180.0
MIN_FRAME_COVERAGE_SECONDS = 178.0
MIN_TELEMETRY_COVERAGE_SECONDS = 178.0
MAX_TELEMETRY_GAP_SECONDS = 3.0
MIN_REQUIRED_SAMPLE_COVERAGE = 0.95

FRAME_BUDGET = {
    "average_fps_loss_percent": Decimal("2"),
    "one_percent_low_fps_loss_percent": Decimal("5"),
    "p95_frametime_increase_percent": Decimal("5"),
    "p99_frametime_increase_percent": Decimal("8"),
    "dropped_rate_delta_percentage_points": Decimal("0.1"),
}
RESOURCE_BUDGET = {
    "cpu_saturation_delta_percentage_points": Decimal("5"),
    "gpu_3d_saturation_delta_percentage_points": Decimal("5"),
}
# JSON and Python expose computed ratios as binary floats. This tolerance is
# only for inclusive gate comparisons; reported values remain unrounded.
GATE_NUMERIC_TOLERANCE = Decimal("0.000000000001")

CPU_SATURATION_PERCENT = 90.0
GPU_3D_SATURATION_PERCENT = 95.0
OUTPUT_STALL_SECONDS = 15.0
MEMORY_GROWTH_MIN_BYTES = 64 * 1024 * 1024
MEMORY_GROWTH_MIN_FRACTION = 0.20
MEMORY_GROWTH_MIN_BYTES_PER_SECOND = 0.5 * 1024 * 1024

EXPECTED_RUNS_V1 = (
    ("capped", "baseline", 1),
    ("capped", "capture", 1),
    ("capped", "baseline", 2),
    ("capped", "capture", 2),
    ("capped", "baseline", 3),
    ("capped", "capture", 3),
    ("uncapped", "baseline", 1),
    ("uncapped", "capture", 1),
)
EXPECTED_RUNS_V2 = (
    ("capped", "baseline", 1),
    ("capped", "capture", 1),
    ("uncapped", "baseline", 1),
    ("uncapped", "capture", 1),
)
# Historical tests and callers may still build the immutable schema-v1 matrix.
EXPECTED_RUNS = EXPECTED_RUNS_V1

REQUIRED_QUERY_SOURCES = (
    "system_cpu",
    "system_memory",
    "processes",
    "gpu_engine",
    "gpu_adapter_memory",
)

WINDOWS_PATH_RE = re.compile(
    r"(?i)(?:(?<![a-z0-9+.-])[a-z]:[\\/][^\r\n\t\"'<>|]*|"
    r"\\\\[^\\/\s]+[\\/][^\\/\s\"'<>|]+(?:[\\/][^\r\n\t\"'<>|]*)?)"
)
ABSOLUTE_POSIX_PATH_RE = re.compile(
    r"(?<![A-Za-z0-9_.:/-])/(?:[^\s\"'<>|/]+(?:/[^\s\"'<>|/]*)*)"
)
GPU_INSTANCE_RE = re.compile(
    r"pid_(?P<pid>\d+).*?luid_(?P<luid>.+?)_phys_(?P<phys>\d+)_eng_(?P<engine>\d+)_engtype_(?P<type>.+)$",
    re.IGNORECASE,
)

EXPECTED_INTEROP = {
    "nvenc": "d3d11-nvenc-direct",
    "amf": "d3d11-amf-direct",
    "qsv": "d3d11-qsv-direct-map",
}


class InvalidData(ValueError):
    """A deterministic protocol or input validity failure."""


def _normalized_model_name(value: Any) -> str:
    return " ".join(str(value).strip().split())


def _is_target_cpu(value: Any) -> bool:
    return re.fullmatch(
        r"AMD Ryzen 5 5600X(?: 6-Core Processor)?",
        _normalized_model_name(value),
        re.IGNORECASE,
    ) is not None


def _is_target_gpu(value: Any) -> bool:
    return re.fullmatch(
        r"(?:NVIDIA )?(?:GeForce )?RTX 4060",
        _normalized_model_name(value),
        re.IGNORECASE,
    ) is not None


@dataclass(frozen=True)
class RunKey:
    frame_mode: str
    condition: str
    run_number: int

    @property
    def run_id(self) -> str:
        return f"{self.frame_mode}-{self.condition}-{self.run_number}"

    @property
    def relative_directory(self) -> Path:
        return Path(self.frame_mode) / self.condition / f"run-{self.run_number}"


def finite_number(value: Any, label: str) -> float:
    try:
        result = float(value)
    except (TypeError, ValueError) as error:
        raise InvalidData(f"{label} is not numeric") from error
    if not math.isfinite(result):
        raise InvalidData(f"{label} is not finite")
    return result


def integer(value: Any, label: str) -> int:
    if isinstance(value, bool):
        raise InvalidData(f"{label} is not an integer")
    if isinstance(value, int):
        return value
    if isinstance(value, str) and re.fullmatch(r"[+-]?\d+", value.strip()):
        return int(value)
    raise InvalidData(f"{label} is not an integer")


def percentile_type7(values: Sequence[float], quantile: float) -> float:
    if not values:
        raise InvalidData("cannot calculate a percentile from an empty series")
    if not 0.0 <= quantile <= 1.0:
        raise ValueError("quantile must be between zero and one")
    ordered = sorted(float(value) for value in values)
    position = (len(ordered) - 1) * quantile
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    fraction = position - lower
    return ordered[lower] + (ordered[upper] - ordered[lower]) * fraction


def summarize(values: Sequence[float]) -> dict[str, float]:
    if not values:
        raise InvalidData("cannot summarize an empty series")
    checked = [finite_number(value, "series value") for value in values]
    return {
        "median": percentile_type7(checked, 0.50),
        "p95": percentile_type7(checked, 0.95),
        "maximum": max(checked),
    }


def one_percent_low_fps(frame_intervals_ms: Sequence[float]) -> float:
    if not frame_intervals_ms:
        raise InvalidData("no frame intervals are available")
    count = max(1, math.ceil(len(frame_intervals_ms) * 0.01))
    slowest = sorted(frame_intervals_ms, reverse=True)[:count]
    return 1000.0 / statistics.fmean(slowest)


def calculate_frame_metrics(
    frame_intervals_ms: Sequence[float], dropped_count: int, present_count: int
) -> dict[str, float | int]:
    if not frame_intervals_ms:
        raise InvalidData("the primary swap chain has no usable frame intervals")
    intervals = [finite_number(value, "frame interval") for value in frame_intervals_ms]
    if any(value <= 0 for value in intervals):
        raise InvalidData("frame intervals must be positive")
    if present_count <= 0 or dropped_count < 0 or dropped_count > present_count:
        raise InvalidData("dropped-frame counts are inconsistent")
    return {
        "average_fps": 1000.0 / statistics.fmean(intervals),
        "one_percent_low_fps": one_percent_low_fps(intervals),
        "p50_frametime_ms": percentile_type7(intervals, 0.50),
        "p95_frametime_ms": percentile_type7(intervals, 0.95),
        "p99_frametime_ms": percentile_type7(intervals, 0.99),
        "dropped_frames": dropped_count,
        "present_count": present_count,
        "dropped_rate": dropped_count / present_count,
        "interval_count": len(intervals),
    }


def _na(value: str | None) -> bool:
    return value is None or value.strip().upper() in {"", "NA", "N/A", "NULL"}


def _is_dropped(row: dict[str, str], dropped_column: str | None) -> bool:
    if dropped_column is not None:
        raw_value = row.get(dropped_column)
        if raw_value is None:
            raise InvalidData(f"{dropped_column} is missing from a PresentMon row")
        value = raw_value.strip().lower()
        if value in {"1", "true", "yes", "dropped"}:
            return True
        if value in {"0", "false", "no", "displayed"}:
            return False
        raise InvalidData(f"invalid {dropped_column} value {value!r}")
    displayed = row.get("DisplayedTime")
    if _na(displayed):
        return True
    numeric = finite_number(displayed, "DisplayedTime")
    if numeric < 0:
        raise InvalidData("DisplayedTime cannot be negative")
    return numeric == 0


def parse_presentmon(
    path: Path, expected_pid: int, expected_process_name: str, measurement_seconds: float
) -> dict[str, Any]:
    try:
        with path.open("r", encoding="utf-8-sig", newline="") as handle:
            reader = csv.DictReader(handle)
            headers = reader.fieldnames or []
            rows = list(reader)
    except (OSError, UnicodeError, csv.Error) as error:
        raise InvalidData(f"PresentMon CSV cannot be read: {error}") from error

    required = {"Application", "ProcessID"}
    missing = sorted(required.difference(headers))
    if missing:
        raise InvalidData(f"PresentMon CSV is missing columns: {', '.join(missing)}")

    time_column = next(
        (name for name in ("CPUStartQPCTime", "CPUStartTime", "TimeInSeconds") if name in headers),
        None,
    )
    if time_column is None:
        raise InvalidData("PresentMon CSV has no supported CPU start-time column")
    time_scale = 1000.0 if time_column == "TimeInSeconds" else 1.0

    explicit_interval = next(
        (name for name in ("MsBetweenPresents", "MsBetweenAppStart") if name in headers),
        None,
    )

    dropped_column = next(
        (name for name in ("Dropped", "DroppedFrame", "WasDropped") if name in headers),
        None,
    )
    if dropped_column is None and "DisplayedTime" not in headers:
        raise InvalidData("PresentMon CSV has no dropped-frame indicator or DisplayedTime")

    matching: list[tuple[dict[str, str], float]] = []
    process_name = expected_process_name.casefold()
    seen_target_name = False
    for row_index, row in enumerate(rows, start=2):
        application = (row.get("Application") or "").casefold()
        if application == process_name:
            seen_target_name = True
        try:
            pid = integer(row.get("ProcessID"), f"PresentMon row {row_index} ProcessID")
        except InvalidData as error:
            raise InvalidData(f"PresentMon row {row_index} has a malformed ProcessID") from error
        if application != process_name or pid != expected_pid:
            continue
        timestamp = finite_number(row.get(time_column), f"PresentMon row {row_index} {time_column}")
        matching.append((row, timestamp * time_scale))

    if not matching:
        detail = "wrong PID" if seen_target_name else "target process is absent"
        raise InvalidData(f"PresentMon has no frames for the manifest League process ({detail})")

    if "SwapChainAddress" in headers:
        counts = Counter((row.get("SwapChainAddress") or "<missing>") for row, _ in matching)
        primary_chain, primary_count = counts.most_common(1)[0]
        tied = [chain for chain, count in counts.items() if count == primary_count]
        if len(tied) > 1:
            raise InvalidData("PresentMon has ambiguous dominant League swap chains")
        selected = [(row, timestamp) for row, timestamp in matching if (row.get("SwapChainAddress") or "<missing>") == primary_chain]
    else:
        counts = Counter({"<not-reported>": len(matching)})
        primary_chain = "<not-reported>"
        selected = matching

    timestamps = [timestamp for _, timestamp in selected]
    if any(current <= previous for previous, current in zip(timestamps, timestamps[1:])):
        raise InvalidData("PresentMon CPU start times are duplicate or out of order")
    coverage_seconds = (timestamps[-1] - timestamps[0]) / 1000.0 if len(timestamps) > 1 else 0.0
    required_coverage = min(MIN_FRAME_COVERAGE_SECONDS, measurement_seconds - 2.0)
    if coverage_seconds < required_coverage:
        raise InvalidData(
            f"PresentMon frame coverage is short ({coverage_seconds:.3f}s; need {required_coverage:.3f}s)"
        )

    derived_intervals = [current - previous for previous, current in zip(timestamps, timestamps[1:])]
    if any(value <= 0 for value in derived_intervals):
        raise InvalidData("derived CPU start-time frame intervals must be positive")
    intervals = derived_intervals
    if explicit_interval == "MsBetweenPresents":
        explicit_values = []
        # The first row's present interval begins before the timed trace and is
        # intentionally excluded. Present intervals need not equal CPU-start
        # intervals per row, but their total must describe the same trace span.
        for row, _ in selected[1:]:
            value = row.get(explicit_interval)
            if _na(value):
                raise InvalidData(f"{explicit_interval} is unavailable inside the primary frame stream")
            interval = finite_number(value, explicit_interval)
            if interval <= 0:
                raise InvalidData(f"{explicit_interval} must be positive")
            explicit_values.append(interval)
        tolerance = max(500.0, coverage_seconds * 1000.0 * 0.02)
        if abs(sum(explicit_values) - (timestamps[-1] - timestamps[0])) > tolerance:
            raise InvalidData(f"{explicit_interval} is temporally inconsistent with CPU-start coverage")
        intervals = explicit_values
    elif explicit_interval == "MsBetweenAppStart":
        explicit_values = []
        # This v2 metric is forward-looking: row i describes CPU start i to
        # CPU start i+1, so the final row is intentionally excluded.
        for row, _ in selected[:-1]:
            value = row.get(explicit_interval)
            if _na(value):
                raise InvalidData(f"{explicit_interval} is unavailable inside the primary frame stream")
            interval = finite_number(value, explicit_interval)
            if interval <= 0:
                raise InvalidData(f"{explicit_interval} must be positive")
            explicit_values.append(interval)
        for explicit, derived in zip(explicit_values, derived_intervals):
            tolerance = max(0.25, abs(derived) * 0.05)
            if abs(explicit - derived) > tolerance:
                raise InvalidData(f"{explicit_interval} is temporally inconsistent with CPU-start timestamps")
        intervals = explicit_values

    dropped_count = sum(_is_dropped(row, dropped_column) for row, _ in selected)
    metrics = calculate_frame_metrics(intervals, dropped_count, len(selected))
    metrics.update(
        {
            "coverage_seconds": coverage_seconds,
            "primary_swap_chain": primary_chain,
            "primary_swap_chain_share": len(selected) / len(matching),
            "other_swap_chain_count": max(0, len(counts) - 1),
            "interval_source": explicit_interval or f"derived:{time_column}",
        }
    )
    return metrics


def parse_gpu_instance_name(name: str) -> dict[str, Any]:
    match = GPU_INSTANCE_RE.search(name)
    if not match:
        raise InvalidData(f"unrecognized GPU Engine instance name {name!r}")
    return {
        "pid": int(match.group("pid")),
        "adapter": match.group("luid").casefold(),
        "physical_adapter": int(match.group("phys")),
        "engine_index": int(match.group("engine")),
        "engine_type": match.group("type").casefold().replace(" ", "").replace("_", ""),
    }


def cook_active_percent(previous: dict[str, Any], current: dict[str, Any], label: str) -> float:
    before = integer(previous["raw"], f"{label} previous raw")
    after = integer(current["raw"], f"{label} current raw")
    before_time = integer(previous["timestamp_100ns"], f"{label} previous timestamp")
    after_time = integer(current["timestamp_100ns"], f"{label} current timestamp")
    numerator = after - before
    denominator = after_time - before_time
    if numerator < 0:
        raise InvalidData(f"{label} raw counter reset")
    if denominator <= 0:
        raise InvalidData(f"{label} counter timestamp did not advance")
    return 100.0 * numerator / denominator


def normalize_process_cpu(raw_percent: float, logical_processors: int) -> float:
    if logical_processors <= 0:
        raise InvalidData("logical processor count must be positive")
    value = finite_number(raw_percent, "raw process CPU")
    if value < 0:
        raise InvalidData("raw process CPU cannot be negative")
    return value / logical_processors


def cook_io_rate(previous: dict[str, Any], current: dict[str, Any], field: str, label: str) -> float:
    before = integer(previous[field], f"{label} previous {field}")
    after = integer(current[field], f"{label} current {field}")
    before_time = integer(previous["timestamp_perf"], f"{label} previous perf timestamp")
    after_time = integer(current["timestamp_perf"], f"{label} current perf timestamp")
    frequency = integer(current["frequency_perf"], f"{label} perf frequency")
    delta = after - before
    elapsed_ticks = after_time - before_time
    if delta < 0:
        raise InvalidData(f"{label} {field} counter reset")
    if elapsed_ticks <= 0 or frequency <= 0:
        raise InvalidData(f"{label} I/O timing is invalid")
    return delta * frequency / elapsed_ticks


def gpu_engine_family(engine_type: str) -> str | None:
    normalized = engine_type.casefold().replace(" ", "").replace("_", "")
    if normalized == "3d" or normalized.endswith("3d"):
        return "3d"
    if "videoencode" in normalized or normalized == "encode":
        return "video_encode"
    return None


def aggregate_gpu_engine_values(
    cooked_rows: Sequence[dict[str, Any]], target_adapter: str | None = None
) -> dict[str, Any]:
    node_totals: dict[tuple[str, int, int, str], float] = defaultdict(float)
    process_nodes: dict[tuple[int, str, int, int, str], float] = defaultdict(float)
    for row in cooked_rows:
        parsed = parse_gpu_instance_name(str(row["name"]))
        if target_adapter is not None and parsed["adapter"] != target_adapter:
            continue
        utilization = finite_number(row["utilization_percent"], "GPU utilization")
        if utilization < 0 or utilization > 105:
            raise InvalidData("GPU Engine utilization is outside the supported 0-105% jitter range")
        utilization = min(100.0, utilization)
        node_key = (
            parsed["adapter"],
            parsed["physical_adapter"],
            parsed["engine_index"],
            parsed["engine_type"],
        )
        process_key = (parsed["pid"],) + node_key
        node_totals[node_key] += utilization
        process_nodes[process_key] += utilization

    total: dict[str, float] = {"3d": 0.0, "video_encode": 0.0}
    observed: set[str] = set()
    process_observed: dict[int, set[str]] = defaultdict(set)
    unclamped_node_maximum = 0.0
    adapters: dict[str, dict[str, float]] = defaultdict(lambda: {"3d": 0.0, "video_encode": 0.0})
    for (adapter, _physical, _engine, engine_type), value in node_totals.items():
        family = gpu_engine_family(engine_type)
        if family is None:
            continue
        unclamped_node_maximum = max(unclamped_node_maximum, value)
        cooked = min(100.0, value)
        observed.add(family)
        adapters[adapter][family] = max(adapters[adapter][family], cooked)
        total[family] = max(total[family], cooked)

    processes: dict[int, dict[str, float]] = defaultdict(lambda: {"3d": 0.0, "video_encode": 0.0})
    for (pid, _adapter, _physical, _engine, engine_type), value in process_nodes.items():
        family = gpu_engine_family(engine_type)
        if family is not None:
            process_observed[pid].add(family)
            processes[pid][family] = max(processes[pid][family], min(100.0, value))
    return {
        "total": total,
        "observed_families": sorted(observed),
        "process_observed_families": {
            str(key): sorted(value) for key, value in sorted(process_observed.items())
        },
        "unclamped_node_maximum_percent": unclamped_node_maximum,
        "adapters": {key: value for key, value in sorted(adapters.items())},
        "processes": {str(key): value for key, value in sorted(processes.items())},
    }


def saturation_summary(
    interval_starts: Sequence[float],
    interval_ends: Sequence[float],
    values: Sequence[float],
    duration_seconds: float,
    threshold: float,
) -> dict[str, float]:
    if (
        not interval_starts
        or len(interval_starts) != len(interval_ends)
        or len(interval_starts) != len(values)
    ):
        raise InvalidData("saturation intervals and values are inconsistent")
    if duration_seconds <= 0:
        raise InvalidData("measurement duration must be positive")
    seconds = 0.0
    covered = 0.0
    previous_end = -math.inf
    for start, end, value in zip(interval_starts, interval_ends, values):
        start = max(0.0, finite_number(start, "saturation interval start"))
        end = finite_number(end, "saturation interval end")
        end = min(duration_seconds, end)
        if end <= start or start < previous_end:
            raise InvalidData("saturation intervals overlap or do not advance")
        weight = end - start
        covered += weight
        if value >= threshold:
            seconds += weight
        previous_end = end
    if covered <= 0:
        raise InvalidData("saturation series covers no measurement time")
    sample_proportion = sum(value >= threshold for value in values) / len(values)
    saturated_samples = sum(value >= threshold for value in values)
    return {
        "threshold_percent": threshold,
        "sample_proportion": sample_proportion,
        "saturated_samples": saturated_samples,
        "sample_count": len(values),
        "seconds": seconds,
        "covered_seconds": covered,
    }


def linear_slope(timestamps: Sequence[float], values: Sequence[float]) -> float:
    if len(timestamps) != len(values) or len(values) < 2:
        raise InvalidData("at least two memory samples are required")
    x_mean = statistics.fmean(timestamps)
    y_mean = statistics.fmean(values)
    denominator = sum((value - x_mean) ** 2 for value in timestamps)
    if denominator <= 0:
        raise InvalidData("memory sample timestamps do not vary")
    return sum((x - x_mean) * (y - y_mean) for x, y in zip(timestamps, values)) / denominator


def memory_growth_assessment(timestamps: Sequence[float], values: Sequence[float]) -> dict[str, Any]:
    if len(values) < 10:
        raise InvalidData("memory growth assessment needs at least ten samples")
    window = max(2, math.ceil(len(values) * 0.20))
    first = percentile_type7(list(values[:window]), 0.50)
    last = percentile_type7(list(values[-window:]), 0.50)
    growth = last - first
    fraction = growth / first if first > 0 else math.inf if growth > 0 else 0.0
    slope = linear_slope(timestamps, values)
    sustained = memory_growth_is_sustained(growth, fraction, slope)
    return {
        "first_window_median_bytes": first,
        "last_window_median_bytes": last,
        "growth_bytes": growth,
        "growth_fraction": fraction,
        "slope_bytes_per_second": slope,
        "sustained_growth": sustained,
        "heuristic": {
            "minimum_growth_bytes": MEMORY_GROWTH_MIN_BYTES,
            "minimum_growth_fraction": MEMORY_GROWTH_MIN_FRACTION,
            "minimum_slope_bytes_per_second": MEMORY_GROWTH_MIN_BYTES_PER_SECOND,
        },
    }


def memory_growth_is_sustained(growth_bytes: float, growth_fraction: float, slope_bytes_per_second: float) -> bool:
    """Apply the finite-window proxy for otherwise-unprovable unbounded growth."""
    return (
        growth_bytes >= MEMORY_GROWTH_MIN_BYTES
        and growth_fraction >= MEMORY_GROWTH_MIN_FRACTION
        and slope_bytes_per_second >= MEMORY_GROWTH_MIN_BYTES_PER_SECOND
    )


def output_growth_assessment(timestamps: Sequence[float], sizes: Sequence[float]) -> dict[str, Any]:
    if not timestamps or len(timestamps) != len(sizes):
        raise InvalidData("output growth samples are inconsistent")
    if any(size < 0 for size in sizes):
        raise InvalidData("output size cannot be negative")
    if any(current < previous for previous, current in zip(sizes, sizes[1:])):
        raise InvalidData("recording output size decreased during measurement")
    if not any(current > previous for previous, current in zip(sizes, sizes[1:])):
        return {
            "grew": False,
            "growth_bytes": sizes[-1] - sizes[0],
            "longest_stall_seconds": timestamps[-1] - timestamps[0],
            "stalled": True,
        }
    last_growth_time = timestamps[0]
    longest = 0.0
    previous_size = sizes[0]
    for timestamp, size in zip(timestamps[1:], sizes[1:]):
        if size > previous_size:
            longest = max(longest, timestamp - last_growth_time)
            last_growth_time = timestamp
        previous_size = size
    longest = max(longest, timestamps[-1] - last_growth_time)
    return {
        "grew": True,
        "growth_bytes": sizes[-1] - sizes[0],
        "longest_stall_seconds": longest,
        "stalled": longest >= OUTPUT_STALL_SECONDS,
    }


def read_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise InvalidData(f"{label} cannot be read: {error}") from error
    if not isinstance(value, dict):
        raise InvalidData(f"{label} must contain a JSON object")
    return value


def read_ndjson(path: Path) -> list[dict[str, Any]]:
    samples: list[dict[str, Any]] = []
    try:
        with path.open("r", encoding="utf-8-sig") as handle:
            for line_number, line in enumerate(handle, start=1):
                if not line.strip():
                    continue
                value = json.loads(line)
                if not isinstance(value, dict):
                    raise InvalidData(f"telemetry line {line_number} is not an object")
                samples.append(value)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise InvalidData(f"telemetry cannot be read: {error}") from error
    if not samples:
        raise InvalidData("telemetry is empty")
    return samples


def _artifact_path(run_directory: Path, manifest: dict[str, Any], key: str) -> Path:
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, dict) or not isinstance(artifacts.get(key), str):
        raise InvalidData(f"manifest is missing artifact {key}")
    relative = Path(artifacts[key])
    if relative.is_absolute() or ".." in relative.parts:
        raise InvalidData(f"manifest artifact {key} is not a safe relative path")
    path = run_directory / relative
    resolved_run = run_directory.resolve()
    resolved_path = path.resolve()
    if resolved_run != resolved_path and resolved_run not in resolved_path.parents:
        raise InvalidData(f"manifest artifact {key} escapes the run directory")
    return path


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError as error:
        raise InvalidData(f"artifact cannot be hashed: {error}") from error
    return digest.hexdigest()


def _verify_artifact_hash(path: Path, manifest: dict[str, Any], key: str) -> str:
    hashes = manifest.get("artifact_sha256")
    if not isinstance(hashes, dict) or not isinstance(hashes.get(key), str):
        raise InvalidData(f"manifest is missing SHA-256 for artifact {key}")
    digest = _sha256_file(path)
    if digest.casefold() != hashes[key].casefold():
        raise InvalidData(f"artifact {key} SHA-256 does not match the manifest")
    return digest


def _parse_utc(value: Any, label: str) -> datetime:
    if not isinstance(value, str):
        raise InvalidData(f"{label} must be an ISO-8601 timestamp")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise InvalidData(f"{label} is not a valid ISO-8601 timestamp") from error
    if parsed.tzinfo is None:
        raise InvalidData(f"{label} must include a timezone")
    return parsed


def _manifest_process(manifest: dict[str, Any], role: str) -> int | None:
    processes = manifest.get("processes")
    if not isinstance(processes, dict):
        raise InvalidData("manifest processes must be an object")
    value = processes.get(role)
    if value is None:
        return None
    if isinstance(value, list):
        if len(value) != 1:
            raise InvalidData(f"manifest must identify exactly one {role} PID")
        value = value[0]
    return integer(value, f"manifest {role} PID")


def _validate_manifest(manifest: dict[str, Any], key: RunKey) -> None:
    schema_version = str(manifest.get("schema_version"))
    if schema_version not in SUPPORTED_SCHEMA_VERSIONS:
        raise InvalidData(f"unsupported manifest schema version {manifest.get('schema_version')!r}")
    if schema_version == "2":
        contract = manifest.get("benchmark_contract")
        if not isinstance(contract, dict):
            raise InvalidData("schema-v2 manifest is missing its benchmark contract")
        if integer(contract.get("required_pair_count"), "required pair count") != 1:
            raise InvalidData("schema-v2 requires exactly one capped and one uncapped pair")
        target_encoder = str(contract.get("target_encoder", "")).casefold()
        if target_encoder not in EXPECTED_INTEROP:
            raise InvalidData("schema-v2 benchmark target encoder is unsupported")
        if contract.get("capture_backend") != "windows_graphics_capture_d3d11":
            raise InvalidData("schema-v2 benchmark requires the optimized WGC/D3D11 backend")
        if integer(contract.get("diagnostics_abi"), "capture diagnostics ABI") != 1:
            raise InvalidData("schema-v2 benchmark requires QueueBack capture diagnostics ABI 1")
        if contract.get("encoder_interop") != EXPECTED_INTEROP[target_encoder]:
            raise InvalidData("schema-v2 benchmark encoder interop does not match its target")
    if manifest.get("run_id") != key.run_id:
        raise InvalidData(f"manifest run_id does not match {key.run_id}")
    if manifest.get("frame_mode") != key.frame_mode or manifest.get("condition") != key.condition:
        raise InvalidData("manifest condition/frame mode does not match its directory")
    if integer(manifest.get("run_number"), "manifest run number") != key.run_number:
        raise InvalidData("manifest run number does not match its directory")
    timing = manifest.get("timing")
    if not isinstance(timing, dict):
        raise InvalidData("manifest timing must be an object")
    warmup = finite_number(timing.get("warmup_seconds"), "warmup seconds")
    measurement = finite_number(timing.get("measurement_seconds"), "measurement seconds")
    elapsed = finite_number(timing.get("measurement_elapsed_seconds"), "measurement elapsed seconds")
    sample_interval = finite_number(timing.get("sample_interval_seconds"), "sample interval seconds")
    if warmup != EXPECTED_WARMUP_SECONDS or measurement != EXPECTED_MEASUREMENT_SECONDS:
        raise InvalidData("production benchmark timing must be exactly 60s warmup and 180s measurement")
    if elapsed < MIN_TELEMETRY_COVERAGE_SECONDS or elapsed > EXPECTED_MEASUREMENT_SECONDS + 5:
        raise InvalidData(f"measurement elapsed time is outside the 178-185s validity window ({elapsed:.3f}s)")
    if sample_interval != 1:
        raise InvalidData("production telemetry sample interval must be exactly one second")
    started = _parse_utc(timing.get("measurement_started_utc"), "measurement start")
    ended = _parse_utc(timing.get("measurement_ended_utc"), "measurement end")
    wall_elapsed = (ended - started).total_seconds()
    if wall_elapsed <= 0 or abs(wall_elapsed - elapsed) > 5.0:
        raise InvalidData("measurement start/end timestamps are inconsistent with elapsed time")

    target = manifest.get("target")
    if not isinstance(target, dict):
        raise InvalidData("manifest target must be an object")
    if target.get("process_name") != "League of Legends.exe":
        raise InvalidData("manifest target process is not League of Legends.exe")
    if target.get("protocol_attested") is not True:
        raise InvalidData("operator did not attest the fixed Practice Tool protocol")
    if key.frame_mode == "capped":
        if finite_number(target.get("configured_fps_limit"), "configured FPS limit") != 144:
            raise InvalidData("capped run is not attested at 144 FPS")
    elif target.get("configured_fps_limit") is not None:
        raise InvalidData("uncapped run must record a null FPS limit")

    configuration = manifest.get("configuration")
    if not isinstance(configuration, dict):
        raise InvalidData("manifest configuration must be an object")
    required_configuration = {
        "league_config_sha256": str,
        "width": int,
        "height": int,
        "display_mode": str,
        "vsync": bool,
    }
    for name, expected_type in required_configuration.items():
        if not isinstance(configuration.get(name), expected_type):
            raise InvalidData(f"manifest configuration {name} is missing or malformed")
    if (
        configuration["width"] != 1920
        or configuration["height"] != 1080
        or configuration["display_mode"] != "borderless"
        or configuration["vsync"] is not False
    ):
        raise InvalidData("run is not attested as 1920x1080 borderless with VSync off")
    after_hash = configuration.get("league_config_sha256_after")
    if not isinstance(after_hash, str):
        raise InvalidData("post-measurement League configuration fingerprint is missing")
    if after_hash != configuration["league_config_sha256"]:
        raise InvalidData("League configuration fingerprint changed during the run")

    environment = manifest.get("environment")
    if not isinstance(environment, dict):
        raise InvalidData("manifest environment must be an object")
    if not isinstance(environment.get("system_fingerprint_sha256"), str):
        raise InvalidData("manifest system fingerprint is missing")
    os_identity = environment.get("os")
    if not isinstance(os_identity, dict) or not str(os_identity.get("version", "")).strip():
        raise InvalidData("OS identity is missing")
    cpu = environment.get("cpu")
    if not isinstance(cpu, dict) or not str(cpu.get("name", "")).strip():
        raise InvalidData("benchmark CPU identity is missing")
    if schema_version == "1" and not _is_target_cpu(cpu.get("name")):
        raise InvalidData("benchmark environment is not the required Ryzen 5 5600X target")
    if integer(cpu.get("logical_processors"), "logical processor count") <= 0:
        raise InvalidData("logical processor count must be positive")
    gpus = environment.get("gpus")
    if not isinstance(gpus, list) or not gpus:
        raise InvalidData("benchmark GPU identity is missing")
    if schema_version == "1" and not any(
        _is_target_gpu(gpu.get("name")) for gpu in gpus if isinstance(gpu, dict)
    ):
        raise InvalidData("benchmark environment is not the required RTX 4060 target")
    if not all(
        isinstance(gpu, dict) and str(gpu.get("name", "")).strip() and str(gpu.get("driver_version", "")).strip()
        for gpu in gpus
    ):
        raise InvalidData("GPU name/driver identity is missing")
    display = environment.get("display")
    if not isinstance(display, dict) or display.get("width") != 1920 or display.get("height") != 1080:
        raise InvalidData("display identity is missing or not 1920x1080")
    if finite_number(display.get("refresh_hz"), "display refresh rate") <= 0 or not str(
        display.get("adapter_name", "")
    ).strip():
        raise InvalidData("display adapter/refresh identity is missing")
    if schema_version == "1" and not _is_target_gpu(display.get("adapter_name")):
        raise InvalidData("benchmark display is not attached to the required RTX 4060 target")
    if schema_version == "2" and _normalized_model_name(display.get("adapter_name")) not in {
        _normalized_model_name(gpu.get("name")) for gpu in gpus
    }:
        raise InvalidData("benchmark display adapter is not one of the enumerated GPUs")
    if not str(display.get("video_mode", "")).strip():
        raise InvalidData("display video-mode identity is missing")
    if not str(environment.get("league_version", "")).strip():
        raise InvalidData("League version is missing")
    presentmon = environment.get("presentmon")
    if not isinstance(presentmon, dict) or not isinstance(presentmon.get("sha256"), str):
        raise InvalidData("PresentMon identity is missing")
    try:
        presentmon_parts = tuple(int(part) for part in str(presentmon.get("version", "")).split("."))
    except ValueError as error:
        raise InvalidData("PresentMon version is malformed") from error
    if not presentmon_parts or presentmon_parts[0] != 2 or presentmon_parts < (2, 1, 1):
        raise InvalidData("PresentMon must be major version 2 and at least 2.1.1")
    recorder = environment.get("recorder")
    if (
        not isinstance(recorder, dict)
        or not isinstance(recorder.get("revision"), str)
        or not isinstance(recorder.get("dirty"), bool)
    ):
        raise InvalidData("recorder revision/dirty state is missing")
    collector = environment.get("collector")
    if not isinstance(collector, dict) or not isinstance(collector.get("sha256"), str):
        raise InvalidData("collector identity is missing")

    telemetry = manifest.get("telemetry")
    if not isinstance(telemetry, dict) or telemetry.get("format") != "raw-cim-ndjson-v1":
        raise InvalidData("telemetry format identity is missing or unsupported")
    if telemetry.get("process_cpu_normalization") != "raw_percent / logical_processors":
        raise InvalidData("process CPU normalization semantics are missing or unsupported")
    required_sources = telemetry.get("required_sources")
    if (
        not isinstance(required_sources, list)
        or any(not isinstance(source, str) for source in required_sources)
        or not set(REQUIRED_QUERY_SOURCES).issubset(set(required_sources))
    ):
        raise InvalidData("manifest does not declare every required telemetry source")
    capture = manifest.get("capture")
    if key.condition == "capture" and not isinstance(capture, dict):
        raise InvalidData("capture manifest section must be an object")
    if key.condition == "baseline" and capture is not None:
        raise InvalidData("baseline manifest must not contain capture state")


def _telemetry_query_coverage(samples: Sequence[dict[str, Any]], source: str) -> float:
    successful = 0
    for sample in samples:
        status = sample.get("query_status")
        if isinstance(status, dict) and status.get(source) is True:
            successful += 1
    return successful / len(samples)


def _sample_timestamps(
    samples: Sequence[dict[str, Any]], measurement_seconds: float, schema_version: str
) -> list[float]:
    timestamps: list[float] = []
    utc_values: list[datetime] = []
    known_sources = set(REQUIRED_QUERY_SOURCES) | {"gpu_process_memory"}
    for index, sample in enumerate(samples):
        if str(sample.get("schema_version")) != schema_version:
            raise InvalidData(f"telemetry sample {index} has an unsupported schema version")
        if integer(sample.get("sequence"), f"telemetry sample {index} sequence") != index:
            raise InvalidData("telemetry sequence is missing, duplicated, or out of order")
        timestamps.append(finite_number(sample.get("elapsed_seconds"), "telemetry elapsed_seconds"))
        utc_values.append(_parse_utc(sample.get("utc"), f"telemetry sample {index} UTC timestamp"))
        status = sample.get("query_status")
        if not isinstance(status, dict) or any(not isinstance(status.get(source), bool) for source in known_sources):
            raise InvalidData(f"telemetry sample {index} has malformed query status")
        errors = sample.get("query_errors")
        if not isinstance(errors, list) or any(not isinstance(error, str) for error in errors):
            raise InvalidData(f"telemetry sample {index} has malformed query errors")
        for source in known_sources:
            matching_errors = [error for error in errors if error.startswith(f"{source}:")]
            if status[source] is False and not matching_errors:
                raise InvalidData(f"telemetry sample {index} lacks an error for failed source {source}")
            if status[source] is True and matching_errors:
                raise InvalidData(f"telemetry sample {index} reports both success and failure for {source}")
        if not isinstance(sample.get("system"), dict):
            raise InvalidData(f"telemetry sample {index} has malformed system counters")
        for field in ("processes", "gpu_engines", "gpu_adapter_memory", "gpu_process_memory"):
            rows = sample.get(field)
            if not isinstance(rows, list) or any(not isinstance(row, dict) for row in rows):
                raise InvalidData(f"telemetry sample {index} has malformed {field}")
        if sample.get("output") is not None and not isinstance(sample.get("output"), dict):
            raise InvalidData(f"telemetry sample {index} has malformed output telemetry")
    if any(current <= previous for previous, current in zip(timestamps, timestamps[1:])):
        raise InvalidData("telemetry timestamps are duplicate or out of order")
    if any(current <= previous for previous, current in zip(utc_values, utc_values[1:])):
        raise InvalidData("telemetry UTC timestamps are duplicate or out of order")
    if timestamps[0] > 1.5:
        raise InvalidData("telemetry did not start near measurement time zero")
    if timestamps[-1] < min(MIN_TELEMETRY_COVERAGE_SECONDS, measurement_seconds - 2.0):
        raise InvalidData("telemetry coverage is short")
    if any(current - previous > MAX_TELEMETRY_GAP_SECONDS for previous, current in zip(timestamps, timestamps[1:])):
        raise InvalidData("telemetry has a gap greater than three seconds")
    return timestamps


def _process_by_role(sample: dict[str, Any], role: str) -> dict[str, Any] | None:
    processes = sample.get("processes")
    if not isinstance(processes, list):
        return None
    matches = [value for value in processes if isinstance(value, dict) and value.get("role") == role]
    if len(matches) > 1:
        raise InvalidData(f"telemetry has multiple {role} process rows in one sample")
    return matches[0] if matches else None


def _query_succeeded(sample: dict[str, Any], source: str) -> bool:
    status = sample.get("query_status")
    return isinstance(status, dict) and status.get(source) is True


def _validate_cooked_coverage(timestamps: Sequence[float], expected_count: int, label: str) -> None:
    if expected_count <= 0 or len(timestamps) / expected_count < MIN_REQUIRED_SAMPLE_COVERAGE:
        coverage = len(timestamps) / expected_count if expected_count > 0 else 0.0
        raise InvalidData(f"required {label} cooked data has only {coverage:.1%} coverage")
    if not timestamps or timestamps[-1] - timestamps[0] < MIN_TELEMETRY_COVERAGE_SECONDS:
        raise InvalidData(f"required {label} cooked data does not span 178 seconds")
    if any(current - previous > MAX_TELEMETRY_GAP_SECONDS for previous, current in zip(timestamps, timestamps[1:])):
        raise InvalidData(f"required {label} cooked data has a gap greater than three seconds")


def _cook_telemetry(manifest: dict[str, Any], samples: Sequence[dict[str, Any]]) -> dict[str, Any]:
    timing = manifest["timing"]
    duration = finite_number(timing["measurement_seconds"], "measurement seconds")
    sample_interval = finite_number(timing["sample_interval_seconds"], "sample interval seconds")
    expected_intervals = round(duration / sample_interval)
    expected_samples = expected_intervals + 1
    if len(samples) / expected_samples < MIN_REQUIRED_SAMPLE_COVERAGE:
        raise InvalidData(
            f"raw telemetry has only {len(samples) / expected_samples:.1%} of the protocol sample count"
        )
    telemetry_manifest = manifest.get("telemetry", {})
    sample_overruns = integer(telemetry_manifest.get("sample_overruns"), "telemetry sample overruns")
    if sample_overruns < 0 or len(samples) + sample_overruns != expected_samples:
        raise InvalidData("telemetry sample count and declared overruns are inconsistent")
    raw_timestamps = _sample_timestamps(samples, duration, str(manifest.get("schema_version")))
    logical_processors = integer(manifest.get("environment", {}).get("cpu", {}).get("logical_processors"), "logical processor count")

    for source in REQUIRED_QUERY_SOURCES:
        coverage = _telemetry_query_coverage(samples, source)
        if coverage < MIN_REQUIRED_SAMPLE_COVERAGE:
            raise InvalidData(f"required telemetry source {source} has only {coverage:.1%} coverage")

    condition = manifest["condition"]
    required_roles = ["league"] + (["recorder", "ffmpeg"] if condition == "capture" else [])
    expected_role_pids = {role: _manifest_process(manifest, role) for role in required_roles}
    for sample in samples:
        for role, expected_pid in expected_role_pids.items():
            row = _process_by_role(sample, role)
            if row is not None and integer(row.get("pid"), f"{role} telemetry PID") != expected_pid:
                raise InvalidData(f"{role} telemetry PID does not match the manifest")
    for role in required_roles:
        coverage = sum(_process_by_role(sample, role) is not None for sample in samples) / len(samples)
        if coverage < MIN_REQUIRED_SAMPLE_COVERAGE:
            raise InvalidData(f"required {role} process telemetry has only {coverage:.1%} coverage")
    if condition == "baseline":
        if any(_process_by_role(sample, role) is not None for sample in samples for role in ("recorder", "ffmpeg")):
            raise InvalidData("baseline telemetry is contaminated by recorder or ffmpeg")

    cooked_interval_starts: list[float] = []
    cooked_timestamps: list[float] = []
    system_cpu: list[float] = []
    system_ram: list[float] = []
    system_ram_timestamps: list[float] = []
    process_series: dict[str, dict[str, list[float]]] = {
        role: defaultdict(list) for role in ("league", "recorder", "ffmpeg", "collector")
    }
    process_metric_timestamps: dict[str, dict[str, list[float]]] = {
        role: defaultdict(list) for role in process_series
    }
    process_private_samples: dict[str, dict[float, float]] = {
        role: {} for role in process_series
    }
    gpu_memory_snapshots: list[dict[str, float]] = []
    process_gpu_memory: dict[str, dict[str, list[float]]] = {
        role: {"gpu_dedicated_memory_bytes": [], "gpu_shared_memory_bytes": []}
        for role in ("league", "recorder", "ffmpeg")
    }
    gpu_process_memory_snapshots: list[dict[str, Any]] = []
    output_timestamps: list[float] = []
    output_sizes: list[float] = []

    schema_v2 = str(manifest.get("schema_version")) == "2"
    if condition == "capture" and not schema_v2:
        capture_contract = manifest.get("capture", {})
        expected_relative_path = (
            capture_contract.get("measurement_output_relative_path")
            if str(manifest.get("schema_version")) == "2"
            else capture_contract.get("recording_relative_path")
        )
        if not isinstance(expected_relative_path, str):
            raise InvalidData("capture measurement output path identity is missing")
        normalized_expected = expected_relative_path.replace("\\", "/").casefold()
        for sample in samples:
            output = sample.get("output")
            if not isinstance(output, dict) or output.get("size_bytes") is None:
                continue
            relative_path = output.get("relative_path")
            if not isinstance(relative_path, str):
                raise InvalidData("capture output path identity is missing")
            if relative_path.replace("\\", "/").casefold() != normalized_expected:
                raise InvalidData("telemetry output path does not match the measured FFmpeg output")
            output_timestamps.append(
                finite_number(sample.get("elapsed_seconds"), "recording output timestamp")
            )
            output_sizes.append(finite_number(output["size_bytes"], "recording output size"))

    target_adapter_scores: dict[str, float] = defaultdict(float)
    provisional_gpu: list[tuple[float, float, list[dict[str, Any]]]] = []

    for previous, current in zip(samples, samples[1:]):
        interval_start = finite_number(previous["elapsed_seconds"], "telemetry interval start")
        timestamp = finite_number(current["elapsed_seconds"], "telemetry elapsed_seconds")
        previous_system = previous.get("system")
        current_system = current.get("system")
        if (
            isinstance(previous_system, dict)
            and isinstance(current_system, dict)
            and _query_succeeded(previous, "system_cpu")
            and _query_succeeded(current, "system_cpu")
        ):
            system_active = cook_active_percent(
                {"raw": previous_system.get("processor_idle_raw"), "timestamp_100ns": previous_system.get("processor_timestamp_100ns")},
                {"raw": current_system.get("processor_idle_raw"), "timestamp_100ns": current_system.get("processor_timestamp_100ns")},
                "system CPU idle",
            )
            total_cpu = 100.0 - system_active
            if total_cpu < -0.5 or total_cpu > 100.5:
                raise InvalidData("cooked system CPU is outside 0-100%")
            cooked_interval_starts.append(interval_start)
            cooked_timestamps.append(timestamp)
            system_cpu.append(min(100.0, max(0.0, total_cpu)))
        if (
            isinstance(current_system, dict)
            and _query_succeeded(current, "system_memory")
        ):
            available = finite_number(current_system.get("available_memory_bytes"), "available memory")
            total_memory = finite_number(current_system.get("total_memory_bytes"), "total memory")
            if available < 0 or total_memory <= 0 or available > total_memory:
                raise InvalidData("system memory counters are inconsistent")
            system_ram.append(total_memory - available)
            system_ram_timestamps.append(timestamp)

        for role in process_series:
            before = _process_by_role(previous, role)
            after = _process_by_role(current, role)
            if before is None or after is None:
                continue
            if integer(before.get("pid"), f"{role} previous PID") != integer(after.get("pid"), f"{role} PID"):
                raise InvalidData(f"{role} PID changed during measurement")
            raw_cpu = cook_active_percent(
                {"raw": before.get("processor_time_raw"), "timestamp_100ns": before.get("timestamp_100ns")},
                {"raw": after.get("processor_time_raw"), "timestamp_100ns": after.get("timestamp_100ns")},
                f"{role} CPU",
            )
            process_series[role]["cpu_raw_percent"].append(raw_cpu)
            process_metric_timestamps[role]["cpu_raw_percent"].append(timestamp)
            normalized_cpu = normalize_process_cpu(raw_cpu, logical_processors)
            if normalized_cpu > 100.5:
                raise InvalidData(f"{role} machine-normalized CPU exceeds the supported 100.5% jitter bound")
            process_series[role]["cpu_normalized_percent"].append(min(100.0, normalized_cpu))
            process_metric_timestamps[role]["cpu_normalized_percent"].append(timestamp)
            for destination, source in (
                ("working_set_bytes", "working_set_bytes"),
                ("private_bytes", "private_bytes"),
            ):
                value = finite_number(after.get(source), f"{role} {source}")
                if value < 0:
                    raise InvalidData(f"{role} {source} cannot be negative")
                process_series[role][destination].append(value)
                process_metric_timestamps[role][destination].append(timestamp)
                if destination == "private_bytes":
                    process_private_samples[role][timestamp] = value
            for destination, source in (
                ("io_read_bytes_per_second", "io_read_bytes_raw"),
                ("io_write_bytes_per_second", "io_write_bytes_raw"),
                ("io_other_bytes_per_second", "io_other_bytes_raw"),
            ):
                process_series[role][destination].append(
                    cook_io_rate(before, after, source, role)
                )
                process_metric_timestamps[role][destination].append(timestamp)

        previous_rows = {
            str(row.get("name")): {
                "raw": row.get("utilization_raw"),
                "timestamp_100ns": row.get("timestamp_100ns"),
            }
            for row in previous.get("gpu_engines", [])
            if isinstance(row, dict) and row.get("name") is not None
        }
        cooked_gpu_rows: list[dict[str, Any]] = []
        current_gpu_rows = current.get("gpu_engines", []) if _query_succeeded(current, "gpu_engine") else []
        for row in current_gpu_rows:
            if not isinstance(row, dict) or row.get("name") is None:
                continue
            name = str(row["name"])
            before = previous_rows.get(name)
            after = {"raw": row.get("utilization_raw"), "timestamp_100ns": row.get("timestamp_100ns")}
            if before is not None:
                utilization = cook_active_percent(before, after, f"GPU Engine {name}")
                cooked_gpu_rows.append({"name": name, "utilization_percent": utilization})
                parsed = parse_gpu_instance_name(name)
                if parsed["pid"] == _manifest_process(manifest, "league") and parsed["engine_type"] == "3d":
                    target_adapter_scores[parsed["adapter"]] += utilization
        if cooked_gpu_rows:
            provisional_gpu.append((interval_start, timestamp, cooked_gpu_rows))

        adapter_memory = current.get("gpu_adapter_memory") if _query_succeeded(current, "gpu_adapter_memory") else []
        if not isinstance(adapter_memory, list):
            adapter_memory = []
        memory_by_adapter: dict[str, dict[str, float]] = defaultdict(lambda: {"dedicated": 0.0, "shared": 0.0})
        for row in adapter_memory:
            if not isinstance(row, dict):
                continue
            name = str(row.get("name", "")).casefold()
            adapter_match = re.search(r"luid_(.+?)(?:_phys_|$)", name)
            adapter = adapter_match.group(1) if adapter_match else name
            dedicated = finite_number(row.get("dedicated_bytes"), "GPU dedicated memory")
            shared = finite_number(row.get("shared_bytes"), "GPU shared memory")
            if dedicated < 0 or shared < 0:
                raise InvalidData("GPU adapter memory cannot be negative")
            memory_by_adapter[adapter]["dedicated"] += dedicated
            memory_by_adapter[adapter]["shared"] += shared
        if _query_succeeded(current, "gpu_adapter_memory"):
            gpu_memory_snapshots.append({
                "timestamp": timestamp,
                "by_adapter": dict(memory_by_adapter),
            })

        gpu_process_rows = current.get("gpu_process_memory") if _query_succeeded(current, "gpu_process_memory") else []
        if not isinstance(gpu_process_rows, list):
            gpu_process_rows = []
        memory_by_pid_adapter: dict[tuple[int, str], dict[str, float]] = defaultdict(
            lambda: {"dedicated": 0.0, "shared": 0.0}
        )
        for row in gpu_process_rows:
            if not isinstance(row, dict) or row.get("pid") is None:
                continue
            pid = integer(row.get("pid"), "GPU process-memory PID")
            name = str(row.get("name", "")).casefold()
            adapter_match = re.search(r"luid_(.+?)(?:_phys_|$)", name)
            if adapter_match is None:
                continue
            adapter = adapter_match.group(1)
            dedicated = finite_number(row.get("dedicated_bytes"), "GPU process dedicated memory")
            shared = finite_number(row.get("shared_bytes"), "GPU process shared memory")
            if dedicated < 0 or shared < 0:
                raise InvalidData("GPU process memory cannot be negative")
            memory_by_pid_adapter[(pid, adapter)]["dedicated"] += dedicated
            memory_by_pid_adapter[(pid, adapter)]["shared"] += shared

        if _query_succeeded(current, "gpu_process_memory"):
            gpu_process_memory_snapshots.append(
                {"timestamp": timestamp, "by_pid_adapter": dict(memory_by_pid_adapter)}
            )

    if not system_cpu:
        raise InvalidData("no cooked system telemetry intervals are available")
    _validate_cooked_coverage(cooked_timestamps, expected_intervals, "system CPU")
    _validate_cooked_coverage(system_ram_timestamps, expected_intervals, "system RAM")
    for role in required_roles:
        for metric in (
            "cpu_raw_percent",
            "cpu_normalized_percent",
            "private_bytes",
            "working_set_bytes",
            "io_read_bytes_per_second",
            "io_write_bytes_per_second",
            "io_other_bytes_per_second",
        ):
            _validate_cooked_coverage(
                process_metric_timestamps[role].get(metric, []), expected_intervals, f"{role} {metric}"
            )

    adapters_seen = {
        parsed["adapter"]
        for _, _, rows in provisional_gpu
        for row in rows
        for parsed in [parse_gpu_instance_name(row["name"])]
    }
    if target_adapter_scores:
        target_adapter = max(sorted(target_adapter_scores), key=target_adapter_scores.get)
    elif len(adapters_seen) == 1:
        target_adapter = next(iter(adapters_seen))
    else:
        raise InvalidData("target GPU adapter cannot be identified from League 3D activity")

    role_pids = {
        role: _manifest_process(manifest, role)
        for role in ("league", "recorder", "ffmpeg")
    }
    gpu_total_3d: list[float] = []
    gpu_total_encode: list[float] = []
    gpu_family_starts: dict[str, list[float]] = {"3d": [], "video_encode": []}
    gpu_family_ends: dict[str, list[float]] = {"3d": [], "video_encode": []}
    process_gpu: dict[str, dict[str, list[float]]] = {
        role: {"gpu_3d_percent": [], "gpu_video_encode_percent": []}
        for role in role_pids
    }
    process_gpu_timestamps: dict[str, dict[str, list[float]]] = {
        role: {"gpu_3d_percent": [], "gpu_video_encode_percent": []}
        for role in role_pids
    }
    gpu_unclamped_node_maximum = 0.0
    baseline_encode_inferred = False
    for interval_start, timestamp, rows in provisional_gpu:
        aggregate = aggregate_gpu_engine_values(rows, target_adapter)
        gpu_unclamped_node_maximum = max(
            gpu_unclamped_node_maximum, aggregate["unclamped_node_maximum_percent"]
        )
        observed_families = set(aggregate["observed_families"])
        for family, destination in (("3d", gpu_total_3d), ("video_encode", gpu_total_encode)):
            inferred_idle_encode = (
                condition == "baseline" and family == "video_encode" and "3d" in observed_families
            )
            if inferred_idle_encode and family not in observed_families:
                baseline_encode_inferred = True
            if family in observed_families or inferred_idle_encode:
                gpu_family_starts[family].append(interval_start)
                gpu_family_ends[family].append(timestamp)
                destination.append(aggregate["total"][family] if family in observed_families else 0.0)
        for role, pid in role_pids.items():
            if pid is None:
                continue
            pid_key = str(pid)
            values = aggregate["processes"].get(pid_key, {})
            observed_for_process = set(aggregate["process_observed_families"].get(pid_key, []))
            for family, metric in (("3d", "gpu_3d_percent"), ("video_encode", "gpu_video_encode_percent")):
                if family in observed_for_process:
                    process_gpu[role][metric].append(values[family])
                    process_gpu_timestamps[role][metric].append(timestamp)

    _validate_cooked_coverage(gpu_family_ends["3d"], expected_intervals, "target-adapter GPU 3D")
    _validate_cooked_coverage(
        gpu_family_ends["video_encode"], expected_intervals, "target-adapter GPU Video Encode"
    )

    gpu_dedicated: list[float] = []
    gpu_shared: list[float] = []
    gpu_memory_value_timestamps: list[float] = []
    for snapshot in gpu_memory_snapshots:
        by_adapter = snapshot["by_adapter"]
        value = by_adapter.get(target_adapter)
        if value is not None:
            gpu_dedicated.append(value["dedicated"])
            gpu_shared.append(value["shared"])
            gpu_memory_value_timestamps.append(snapshot["timestamp"])
    _validate_cooked_coverage(
        gpu_memory_value_timestamps, expected_intervals, "target-adapter GPU memory"
    )

    for snapshot in gpu_process_memory_snapshots:
        by_pid_adapter = snapshot["by_pid_adapter"]
        for role in process_gpu_memory:
            pid = role_pids.get(role)
            value = by_pid_adapter.get((pid, target_adapter)) if pid is not None else None
            if value is not None:
                process_gpu_memory[role]["gpu_dedicated_memory_bytes"].append(value["dedicated"])
                process_gpu_memory[role]["gpu_shared_memory_bytes"].append(value["shared"])

    processes_report: dict[str, Any] = {}
    for role, metrics in process_series.items():
        if not metrics:
            if role in required_roles:
                raise InvalidData(f"no cooked {role} process counters are available")
            continue
        role_report = {name: summarize(values) for name, values in sorted(metrics.items())}
        for name, values in process_gpu.get(role, {}).items():
            timestamps = process_gpu_timestamps.get(role, {}).get(name, [])
            if len(values) / expected_intervals >= MIN_REQUIRED_SAMPLE_COVERAGE:
                _validate_cooked_coverage(timestamps, expected_intervals, f"optional {role} {name}")
                role_report[name] = summarize(values)
        for name, values in process_gpu_memory.get(role, {}).items():
            if len(values) / expected_intervals >= MIN_REQUIRED_SAMPLE_COVERAGE:
                role_report[name] = summarize(values)
        processes_report[role] = role_report

    resources: dict[str, Any] = {
        "system_cpu_percent": {
            **summarize(system_cpu),
            "saturation": saturation_summary(
                cooked_interval_starts,
                cooked_timestamps,
                system_cpu,
                duration,
                CPU_SATURATION_PERCENT,
            ),
        },
        "system_ram_used_bytes": summarize(system_ram),
        "processes": processes_report,
        "gpu": {
            "adapter_id": target_adapter,
            "three_d_percent": {
                **summarize(gpu_total_3d),
                "saturation": saturation_summary(
                    gpu_family_starts["3d"],
                    gpu_family_ends["3d"],
                    gpu_total_3d,
                    duration,
                    GPU_3D_SATURATION_PERCENT,
                ),
            },
            "video_encode_percent": summarize(gpu_total_encode),
            "unclamped_physical_engine_maximum_percent": gpu_unclamped_node_maximum,
            "dedicated_memory_bytes": summarize(gpu_dedicated),
            "shared_memory_bytes": summarize(gpu_shared),
        },
    }

    safety: dict[str, Any] = {}
    if condition == "capture":
        if schema_v2:
            output = {
                "source": "recorder_in_process_mux_watchdog",
                "grew": None,
                "stalled": None,
                "maximum_stall_seconds": None,
            }
        else:
            _validate_cooked_coverage(output_timestamps, expected_samples, "capture output size")
            output = output_growth_assessment(output_timestamps, output_sizes)
        resources["output"] = {
            **output,
            "write_bytes_per_second": (
                None
                if schema_v2
                else summarize(
                    [
                        (current_size - previous_size) / (current_time - previous_time)
                        for previous_time, current_time, previous_size, current_size in zip(
                            output_timestamps, output_timestamps[1:], output_sizes, output_sizes[1:]
                        )
                        if current_time > previous_time
                    ]
                )
            ),
        }
        memory_timestamps = sorted(
            set(process_private_samples["recorder"]).intersection(process_private_samples["ffmpeg"])
        )
        _validate_cooked_coverage(memory_timestamps, expected_intervals, "combined recorder/ffmpeg memory")
        combined = [
            process_private_samples["recorder"][timestamp]
            + process_private_samples["ffmpeg"][timestamp]
            for timestamp in memory_timestamps
        ]
        memory = memory_growth_assessment(memory_timestamps, combined)
        safety = {"output": output, "memory": memory}

    optional_gpu_coverage = _telemetry_query_coverage(samples, "gpu_process_memory")
    limitations: list[str] = [
        "Per-process GPU memory is diagnostic only because Windows can over-report it; adapter memory is authoritative."
    ]
    if baseline_encode_inferred:
        limitations.append(
            "Baseline target-adapter Video Encode samples with no exposed context were inferred as 0% only while GPU Engine queries and target-adapter 3D visibility were valid."
        )
    if optional_gpu_coverage < MIN_REQUIRED_SAMPLE_COVERAGE:
        limitations.append("Per-process GPU memory was unavailable or incomplete and is not used for gating.")
    for role in required_roles:
        role_gpu_coverage = max(
            (
                len(timestamps) / expected_intervals
                for timestamps in process_gpu_timestamps.get(role, {}).values()
            ),
            default=0.0,
        )
        if role_gpu_coverage < MIN_REQUIRED_SAMPLE_COVERAGE:
            limitations.append(
                f"Per-process GPU Engine activity for {role} was unavailable or incomplete; adapter engine data remains authoritative."
            )
        role_memory = process_gpu_memory.get(role, {})
        if not role_memory or len(role_memory["gpu_dedicated_memory_bytes"]) / expected_intervals < MIN_REQUIRED_SAMPLE_COVERAGE:
            limitations.append(
                f"Per-process GPU memory for {role} was unavailable or incomplete; adapter memory is authoritative."
            )
    if not target_adapter_scores:
        limitations.append("Target adapter was inferred from a single exposed adapter rather than League per-process GPU activity.")

    return {"resources": resources, "safety": safety, "limitations": limitations}


def _parse_frame_rate(value: Any) -> float:
    if not isinstance(value, str) or "/" not in value:
        raise InvalidData("ffprobe video frame rate is missing or malformed")
    numerator_text, denominator_text = value.split("/", 1)
    numerator = finite_number(numerator_text, "ffprobe frame-rate numerator")
    denominator = finite_number(denominator_text, "ffprobe frame-rate denominator")
    if numerator <= 0 or denominator <= 0:
        raise InvalidData("ffprobe video frame rate must be positive")
    return numerator / denominator


def _canonical_luid(value: Any, label: str) -> str:
    text = str(value).strip().casefold()
    if re.fullmatch(r"[0-9a-f]{16}", text) is None:
        raise InvalidData(f"{label} is not a 16-digit DXGI LUID")
    return text


def _adapter_id_luid(value: Any) -> str | None:
    match = re.search(
        r"luid_0x(?P<low>[0-9a-f]+)_0x(?P<high>[0-9a-f]+)_phys_\d+",
        str(value),
        re.IGNORECASE,
    )
    if match is None:
        return None
    low = int(match.group("low"), 16)
    high = int(match.group("high"), 16)
    if low > 0xFFFFFFFF or high > 0xFFFFFFFF:
        raise InvalidData("telemetry target adapter contains an invalid DXGI LUID")
    return f"{high:08x}{low:08x}"


def _capture_validation(
    manifest: dict[str, Any],
    telemetry_safety: dict[str, Any],
    target_adapter_id: Any,
    recording_digest: str,
    ffprobe_path: Path,
    recorder_diagnostics_path: Path,
    target_encoder: str = "nvenc",
) -> dict[str, Any]:
    capture = manifest.get("capture")
    if not isinstance(capture, dict) or capture.get("finalized") is not True:
        raise InvalidData("capture run has not been finalized")
    media_hash = capture.get("media_sha256")
    if not isinstance(media_hash, str) or re.fullmatch(r"[0-9a-fA-F]{64}", media_hash) is None:
        raise InvalidData("capture media SHA-256 is missing or malformed")
    if integer(capture.get("media_size_bytes"), "capture media size") <= 0:
        raise InvalidData("capture media size must be positive")
    if recording_digest.casefold() != media_hash.casefold():
        raise InvalidData("capture media SHA-256 does not match the finalized recording")
    media = capture.get("media_validation")
    if not isinstance(media, dict):
        raise InvalidData("capture run has no media validation")
    required_media = {
        "ffprobe_ok": True,
        "decode_ok": True,
    }
    for name, expected in required_media.items():
        if media.get(name) is not expected:
            raise InvalidData(f"capture media validation failed: {name}")
    probe = read_json(ffprobe_path, "ffprobe artifact")
    streams = probe.get("streams")
    if not isinstance(streams, list) or any(not isinstance(stream, dict) for stream in streams):
        raise InvalidData("ffprobe artifact has malformed streams")
    video_streams = [stream for stream in streams if stream.get("codec_type") == "video"]
    audio_streams = [stream for stream in streams if stream.get("codec_type") == "audio"]
    if len(video_streams) != 1:
        raise InvalidData("capture media must have exactly one video stream")
    if len(audio_streams) < 1:
        raise InvalidData("capture media has no audio stream")
    if integer(media.get("video_streams"), "video stream count") != len(video_streams):
        raise InvalidData("manifest video-stream count disagrees with ffprobe")
    if integer(media.get("audio_streams"), "audio stream count") != len(audio_streams):
        raise InvalidData("manifest audio-stream count disagrees with ffprobe")
    probe_format = probe.get("format")
    if not isinstance(probe_format, dict):
        raise InvalidData("ffprobe artifact has no format metadata")
    duration = finite_number(probe_format.get("duration"), "ffprobe media duration")
    if abs(duration - finite_number(media.get("duration_seconds"), "manifest media duration")) > 0.01:
        raise InvalidData("manifest media duration disagrees with ffprobe")
    minimum_media_duration = EXPECTED_WARMUP_SECONDS + EXPECTED_MEASUREMENT_SECONDS - 2
    if duration < minimum_media_duration:
        raise InvalidData("capture media does not cover the warmup plus benchmark measurement")

    metadata = capture.get("recording_metadata")
    if not isinstance(metadata, dict):
        raise InvalidData("capture recording metadata is missing")
    if str(metadata.get("encoder_used", "")).casefold() != target_encoder:
        raise InvalidData(f"target validation capture did not use {target_encoder.upper()}")
    if integer(metadata.get("width"), "capture width") != 1920 or integer(metadata.get("height"), "capture height") != 1080:
        raise InvalidData("capture metadata is not 1920x1080")
    capture_fps = finite_number(metadata.get("fps"), "capture FPS")
    if capture_fps <= 0 or capture_fps > 240:
        raise InvalidData("capture metadata reports an invalid configured capture FPS")
    if not str(metadata.get("codec", "")).strip() or not str(metadata.get("profile", "")).strip():
        raise InvalidData("capture metadata is missing codec or profile")
    metadata_duration = finite_number(metadata.get("duration_ms"), "capture metadata duration") / 1000.0
    if abs(metadata_duration - duration) > 2.0:
        raise InvalidData("ffprobe duration and recorder metadata duration differ by more than two seconds")

    if str(manifest.get("schema_version")) == "2":
        contract = manifest["benchmark_contract"]
        if str(contract.get("target_encoder", "")).casefold() != target_encoder:
            raise InvalidData("schema-v2 target encoder disagrees with the analyzer target")
        capture_metadata = metadata.get("capture")
        if not isinstance(capture_metadata, dict):
            raise InvalidData("schema-v2 recording metadata has no capture contract")
        if integer(capture_metadata.get("schema_version"), "capture metadata schema") != 1:
            raise InvalidData("capture metadata schema is unsupported")
        if capture_metadata.get("backend") != contract.get("capture_backend"):
            raise InvalidData("actual capture backend disagrees with the benchmark contract")
        if integer(capture_metadata.get("diagnostics_abi"), "capture diagnostics ABI") != integer(
            contract.get("diagnostics_abi"), "benchmark diagnostics ABI"
        ):
            raise InvalidData("actual capture diagnostics ABI disagrees with the benchmark contract")
        if str(capture_metadata.get("encoder_backend", "")).casefold() != target_encoder:
            raise InvalidData("capture metadata encoder backend disagrees with the benchmark target")
        if capture_metadata.get("encoder_interop") != EXPECTED_INTEROP[target_encoder]:
            raise InvalidData("capture metadata does not prove the expected direct encoder interop")
        support_label = capture_metadata.get("support_label")
        allowed_support_labels = contract.get("support_labels")
        if (
            not isinstance(allowed_support_labels, list)
            or support_label not in allowed_support_labels
        ):
            raise InvalidData("capture support label is missing or incompatible")
        capture_luid = _canonical_luid(
            capture_metadata.get("capture_adapter_luid"), "capture adapter LUID"
        )
        encoder_luid = _canonical_luid(
            capture_metadata.get("encoder_adapter_luid"), "encoder adapter LUID"
        )
        if capture_luid != encoder_luid:
            raise InvalidData("capture and encoder adapters do not match")
        telemetry_luid = _adapter_id_luid(target_adapter_id)
        if telemetry_luid is not None and telemetry_luid != capture_luid:
            raise InvalidData("capture adapter LUID disagrees with the measured display adapter")
        if capture_metadata.get("host_readback") is not False:
            raise InvalidData("optimized capture metadata does not prove host-readback is disabled")
        if capture_metadata.get("source_format") != "d3d11_bgra" or capture_metadata.get(
            "converted_format"
        ) != "d3d11_nv12":
            raise InvalidData("capture metadata does not identify the GPU-resident BGRA-to-NV12 path")
        stages = capture_metadata.get("gpu_stages")
        if (
            not isinstance(stages, list)
            or len(stages) < 3
            or any(not isinstance(stage, str) or not stage for stage in stages)
            or any("hwdownload" in stage.casefold() for stage in stages)
        ):
            raise InvalidData("capture GPU-stage metadata is missing or includes host download")
        expected_depth = 16 if target_encoder == "nvenc" else 4
        exact_bounds = {
            "frame_pool_capacity": 2,
            "capture_output_pool_capacity": 8,
            "filter_buffered_frame_limit": 32,
            "encoder_depth": expected_depth,
            "progress_stall_timeout_seconds": 15,
        }
        for field, expected in exact_bounds.items():
            if integer(capture_metadata.get(field), f"capture {field}") != expected:
                raise InvalidData(f"capture {field} does not match the audited finite bound")
        if integer(capture_metadata.get("maximum_texture_bytes"), "maximum texture bytes") <= 0:
            raise InvalidData("capture texture budget is missing or empty")
        for field in (
            "source_frames_surfaced",
            "source_frames_superseded",
            "encoded_frames",
            "muxed_bytes",
            "cfr_duplicates",
            "cfr_discards",
            "pool_recreations",
        ):
            if integer(capture_metadata.get(field), f"capture {field}") < 0:
                raise InvalidData(f"capture {field} cannot be negative")
        if integer(capture_metadata.get("source_frames_surfaced"), "source frames surfaced") <= 0:
            raise InvalidData("capture reported no surfaced source frames")
        if integer(capture_metadata.get("encoded_frames"), "encoded frames") <= 0:
            raise InvalidData("capture reported no encoded frames")
        if integer(capture_metadata.get("muxed_bytes"), "muxed bytes") <= 0:
            raise InvalidData("capture reported no muxed output")
        first_qpc = integer(capture_metadata.get("first_qpc_100ns"), "first capture QPC")
        latest_qpc = integer(capture_metadata.get("latest_qpc_100ns"), "latest capture QPC")
        if first_qpc <= 0 or latest_qpc < first_qpc:
            raise InvalidData("capture QPC timestamps are missing or inconsistent")
        if capture_metadata.get("terminal_progress") is not True:
            raise InvalidData("capture did not report both terminal source and mux progress")
        if not str(capture_metadata.get("media_runtime_id", "")).strip():
            raise InvalidData("capture media runtime identity is missing")

    video = video_streams[0]
    actual_width = integer(video.get("width"), "ffprobe video width")
    actual_height = integer(video.get("height"), "ffprobe video height")
    actual_codec = str(video.get("codec_name", "")).casefold()
    actual_fps = _parse_frame_rate(video.get("avg_frame_rate"))
    if actual_width != integer(metadata.get("width"), "capture metadata width") or actual_height != integer(
        metadata.get("height"), "capture metadata height"
    ):
        raise InvalidData("ffprobe resolution disagrees with recorder metadata")
    if actual_codec != str(metadata.get("codec", "")).casefold():
        raise InvalidData("ffprobe codec disagrees with recorder metadata")
    if abs(actual_fps - capture_fps) > 0.5:
        raise InvalidData("ffprobe frame rate disagrees with recorder metadata")
    expected_probe_summary = {
        "codec": actual_codec,
        "codec_profile": str(video.get("profile", "")),
        "width": actual_width,
        "height": actual_height,
        "average_frame_rate": actual_fps,
    }
    for field, actual in expected_probe_summary.items():
        if field in media:
            declared = media[field]
            if isinstance(actual, float):
                if abs(finite_number(declared, f"manifest media {field}") - actual) > 0.01:
                    raise InvalidData(f"manifest media {field} disagrees with ffprobe")
            elif declared != actual:
                raise InvalidData(f"manifest media {field} disagrees with ffprobe")

    diagnostics = capture.get("diagnostics")
    if not isinstance(diagnostics, dict):
        raise InvalidData("capture diagnostics are missing")
    encoder_errors = diagnostics.get("encoder_errors")
    if not isinstance(encoder_errors, list) or any(not isinstance(value, str) for value in encoder_errors):
        raise InvalidData("capture encoder diagnostics are malformed")
    poller_errors = diagnostics.get("poller_errors")
    if not isinstance(poller_errors, list) or any(not isinstance(value, str) for value in poller_errors):
        raise InvalidData("capture poller-error diagnostics are malformed")
    poller = diagnostics.get("poller")
    if not isinstance(poller, dict):
        raise InvalidData("capture poller diagnostics were not collected from the isolated recorder log")
    required_poller_fields = {
        "calibration_requests",
        "event_requests",
        "snapshot_requests",
        "successful_responses",
        "average_response_latency_ms",
        "maximum_response_latency_ms",
        "captured_events",
        "captured_snapshots",
        "game_log_bytes",
        "json_writes",
        "slowest_json_write_ms",
        "event_failure_reason",
        "snapshot_failure_reason",
    }
    if not required_poller_fields.issubset(poller):
        raise InvalidData("capture poller diagnostics are incomplete")
    for field in required_poller_fields - {"event_failure_reason", "snapshot_failure_reason"}:
        value = finite_number(poller.get(field), f"poller diagnostic {field}")
        if value < 0:
            raise InvalidData(f"poller diagnostic {field} cannot be negative")
    for field in ("snapshot_requests", "successful_responses", "captured_snapshots", "game_log_bytes", "json_writes"):
        if finite_number(poller.get(field), f"poller diagnostic {field}") <= 0:
            raise InvalidData(f"poller diagnostic {field} shows an inactive recorder poller")
    if any(str(poller.get(field, "")).casefold() != "none" for field in ("event_failure_reason", "snapshot_failure_reason")):
        raise InvalidData("recorder poller reported a persistent endpoint failure")
    if diagnostics.get("capture_target_fallback") is not False:
        raise InvalidData("capture used or may have used the primary-display fallback instead of the League window region")
    for field in ("recorder_disappeared", "ffmpeg_disappeared"):
        if not isinstance(diagnostics.get(field), bool):
            raise InvalidData(f"capture diagnostic {field} is missing or malformed")
    league_pid = _manifest_process(manifest, "league")
    if integer(diagnostics.get("recording_pid"), "recorder target PID") != league_pid:
        raise InvalidData("recorder log target PID does not match the measured League PID")
    if not str(diagnostics.get("audio_source", "")).strip():
        raise InvalidData("recorder audio-source diagnostics are missing")

    capture_progress_safety: dict[str, Any] | None = None
    if str(manifest.get("schema_version")) == "2":
        progress = diagnostics.get("capture_progress")
        if not isinstance(progress, list) or len(progress) < 2:
            raise InvalidData("schema-v2 capture progress diagnostics are missing")
        progress_fields = (
            "elapsed_ms",
            "source_frames_surfaced",
            "source_frames_superseded",
            "encoded_frames",
            "muxed_bytes",
            "latest_qpc_100ns",
            "cfr_duplicates",
            "cfr_discards",
            "pool_recreations",
        )
        normalized_progress: list[dict[str, Any]] = []
        for index, point in enumerate(progress):
            if not isinstance(point, dict) or not isinstance(point.get("terminal"), bool):
                raise InvalidData("capture progress diagnostics contain a malformed point")
            normalized: dict[str, Any] = {"terminal": point["terminal"]}
            for field in progress_fields:
                value = integer(point.get(field), f"capture progress {field}")
                if value < 0:
                    raise InvalidData(f"capture progress {field} cannot be negative")
                normalized[field] = value
            if index > 0:
                previous = normalized_progress[-1]
                if normalized["elapsed_ms"] <= previous["elapsed_ms"]:
                    raise InvalidData("capture progress elapsed time did not advance")
                if normalized["elapsed_ms"] - previous["elapsed_ms"] > 15_000:
                    raise InvalidData("capture progress reporting has a gap over 15 seconds")
                for field in progress_fields[1:]:
                    if normalized[field] < previous[field]:
                        raise InvalidData(f"capture progress {field} regressed")
                # Periodic points must prove source, encode, and mux advancement.
                # A terminal point may immediately follow the last periodic point.
                if not normalized["terminal"]:
                    for field in (
                        "source_frames_surfaced",
                        "encoded_frames",
                        "muxed_bytes",
                        "latest_qpc_100ns",
                    ):
                        if normalized[field] <= previous[field]:
                            raise InvalidData(f"capture progress {field} stopped advancing")
            normalized_progress.append(normalized)
        if normalized_progress[0]["elapsed_ms"] > 15_000:
            raise InvalidData("capture progress did not start within the watchdog window")
        if any(point["terminal"] for point in normalized_progress[:-1]) or not normalized_progress[-1]["terminal"]:
            raise InvalidData("capture progress terminal evidence is not the final point")
        final_point = normalized_progress[-1]
        minimum_span_ms = (
            integer(manifest["timing"].get("warmup_seconds"), "warmup seconds")
            + integer(manifest["timing"].get("measurement_seconds"), "measurement seconds")
        ) * 1000 - 15_000
        if final_point["elapsed_ms"] < minimum_span_ms:
            raise InvalidData("capture progress does not span warmup plus measurement")
        final_counter_fields = {
            "source_frames_surfaced": "source_frames_surfaced",
            "source_frames_superseded": "source_frames_superseded",
            "encoded_frames": "encoded_frames",
            "muxed_bytes": "muxed_bytes",
            "latest_qpc_100ns": "latest_qpc_100ns",
            "cfr_duplicates": "cfr_duplicates",
            "cfr_discards": "cfr_discards",
            "pool_recreations": "pool_recreations",
        }
        for progress_field, metadata_field in final_counter_fields.items():
            if final_point[progress_field] != integer(
                capture_metadata.get(metadata_field), f"capture metadata {metadata_field}"
            ):
                raise InvalidData(
                    f"final capture progress {progress_field} disagrees with recording metadata"
                )
        capture_progress_safety = {
            "source": "recorder_in_process_mux_watchdog",
            "watchdog_seconds": 15,
            "points": len(normalized_progress),
            "first_elapsed_ms": normalized_progress[0]["elapsed_ms"],
            "last_elapsed_ms": final_point["elapsed_ms"],
            "muxed_bytes_first": normalized_progress[0]["muxed_bytes"],
            "muxed_bytes_final": final_point["muxed_bytes"],
            "grew": final_point["muxed_bytes"] > normalized_progress[0]["muxed_bytes"],
            "stalled": False,
        }

    raw_diagnostics = read_json(recorder_diagnostics_path, "recorder diagnostics artifact")
    for field in (
        "encoder_errors",
        "poller_errors",
        "poller",
        "capture_target_fallback",
        "recording_pid",
        "audio_source",
        *(("capture_progress",) if str(manifest.get("schema_version")) == "2" else ()),
    ):
        if raw_diagnostics.get(field) != diagnostics.get(field):
            raise InvalidData(f"manifest capture diagnostic {field} disagrees with the recorder log artifact")

    output_safety = capture_progress_safety or telemetry_safety["output"]
    checks = {
        "output_progressed": output_safety["grew"] is True,
        "no_output_stall": output_safety["stalled"] is False,
        "bounded_memory": telemetry_safety["memory"]["sustained_growth"] is False,
        "recorder_present": diagnostics.get("recorder_disappeared") is False,
        "ffmpeg_present": diagnostics.get("ffmpeg_disappeared") is False,
        "no_encoder_errors": not encoder_errors,
        "no_poller_errors": not poller_errors,
        "media_valid": True,
    }
    return {
        "checks": checks,
        "passed": all(checks.values()),
        "output_progress": output_safety,
        "media": {**expected_probe_summary, "duration_seconds": duration},
    }


def analyze_run(
    run_directory: Path, key: RunKey, target_encoder: str = "nvenc"
) -> dict[str, Any]:
    manifest_path = run_directory / "manifest.json"
    manifest = read_json(manifest_path, f"{key.run_id} manifest")
    _validate_manifest(manifest, key)
    presentmon_path = _artifact_path(run_directory, manifest, "presentmon")
    telemetry_path = _artifact_path(run_directory, manifest, "telemetry")
    _verify_artifact_hash(presentmon_path, manifest, "presentmon")
    _verify_artifact_hash(telemetry_path, manifest, "telemetry")
    recording_path: Path | None = None
    recording_digest: str | None = None
    ffprobe_path: Path | None = None
    recorder_diagnostics_path: Path | None = None
    if key.condition == "capture":
        capture = manifest["capture"]
        for artifact_key in ("recording", "ffprobe", "decode_log", "recorder_diagnostics"):
            artifact_path = _artifact_path(run_directory, manifest, artifact_key)
            artifact_digest = _verify_artifact_hash(artifact_path, manifest, artifact_key)
            if artifact_key == "recording":
                recording_path = artifact_path
                recording_digest = artifact_digest
                if artifact_path.stat().st_size != integer(
                    capture.get("media_size_bytes"), "capture media size"
                ):
                    raise InvalidData("capture media size does not match the finalized artifact")
                capture_relative = str(capture.get("recording_relative_path", ""))
                artifact_relative = str(manifest.get("artifacts", {}).get("recording", ""))
                if capture_relative.replace("\\", "/").casefold() != artifact_relative.replace("\\", "/").casefold():
                    raise InvalidData("capture recording path does not match the recording artifact")
                declared_media_hash = str(capture.get("media_sha256", ""))
                artifact_hash = str(manifest.get("artifact_sha256", {}).get("recording", ""))
                if declared_media_hash.casefold() != artifact_hash.casefold():
                    raise InvalidData("capture media SHA-256 disagrees with the recording artifact hash")
            elif artifact_key == "ffprobe":
                ffprobe_path = artifact_path
            elif artifact_key == "recorder_diagnostics":
                recorder_diagnostics_path = artifact_path
    league_pid = _manifest_process(manifest, "league")
    if league_pid is None:
        raise InvalidData("manifest does not identify the League PID")
    frame = parse_presentmon(
        presentmon_path,
        league_pid,
        manifest["target"]["process_name"],
        finite_number(manifest["timing"]["measurement_seconds"], "measurement seconds"),
    )
    if frame["primary_swap_chain_share"] < 0.90:
        raise InvalidData("the dominant League swap chain contains less than 90% of target presents")
    if key.frame_mode == "capped" and frame["average_fps"] > 155:
        raise InvalidData("capped run frame rate is inconsistent with the attested 144-FPS cap")
    telemetry_samples = read_ndjson(telemetry_path)
    telemetry = _cook_telemetry(manifest, telemetry_samples)
    capture_safety = None
    if key.condition == "capture":
        if recording_path is None or recording_digest is None or ffprobe_path is None or recorder_diagnostics_path is None:
            raise InvalidData("capture finalization artifacts are incomplete")
        capture_safety = _capture_validation(
            manifest,
            telemetry["safety"],
            telemetry["resources"]["gpu"].get("adapter_id"),
            recording_digest,
            ffprobe_path,
            recorder_diagnostics_path,
            target_encoder,
        )

    environment = manifest.get("environment", {})
    configuration = manifest.get("configuration", {})
    capture_manifest = manifest.get("capture") if isinstance(manifest.get("capture"), dict) else {}
    return {
        "run_id": key.run_id,
        "frame_mode": key.frame_mode,
        "condition": key.condition,
        "run_number": key.run_number,
        "measurement_started_utc": manifest["timing"]["measurement_started_utc"],
        "measurement_ended_utc": manifest["timing"]["measurement_ended_utc"],
        "identity": {
            "league_config_sha256": configuration.get("league_config_sha256"),
            "system_fingerprint_sha256": environment.get("system_fingerprint_sha256"),
            "league_version": environment.get("league_version"),
            "league_version_source": environment.get("league_version_source"),
            "recorder_revision": environment.get("recorder", {}).get("revision"),
            "recorder_dirty": environment.get("recorder", {}).get("dirty"),
            "presentmon_version": environment.get("presentmon", {}).get("version"),
            "presentmon_sha256": environment.get("presentmon", {}).get("sha256"),
            "collector_sha256": environment.get("collector", {}).get("sha256"),
            "display": environment.get("display"),
            "target_adapter_id": telemetry["resources"]["gpu"].get("adapter_id"),
            "recorder_source_config_sha256": capture_manifest.get("source_config_sha256"),
            "recorder_config_sha256": capture_manifest.get("recorder_config_sha256"),
            "recorder_binary_sha256": capture_manifest.get("recorder_binary_sha256"),
            "ffmpeg_binary_sha256": capture_manifest.get("ffmpeg_binary_sha256"),
            "audio_source": capture_manifest.get("diagnostics", {}).get("audio_source"),
        },
        "environment": {
            "os": environment.get("os"),
            "cpu": environment.get("cpu"),
            "gpus": environment.get("gpus"),
            "display": environment.get("display"),
            "league_version": environment.get("league_version"),
            "league_version_source": environment.get("league_version_source"),
            "recorder": environment.get("recorder"),
            "presentmon": environment.get("presentmon"),
            "collector": environment.get("collector"),
        },
        "configuration": {
            "league_config_sha256": configuration.get("league_config_sha256"),
            "width": configuration.get("width"),
            "height": configuration.get("height"),
            "display_mode": configuration.get("display_mode"),
            "vsync": configuration.get("vsync"),
            "configured_fps_limit": manifest["target"].get("configured_fps_limit"),
        },
        "capture_configuration": (
            {
                "encoder": manifest.get("capture", {}).get("recording_metadata", {}).get("encoder_used"),
                "codec": manifest.get("capture", {}).get("recording_metadata", {}).get("codec"),
                "profile": manifest.get("capture", {}).get("recording_metadata", {}).get("profile"),
                "width": manifest.get("capture", {}).get("recording_metadata", {}).get("width"),
                "height": manifest.get("capture", {}).get("recording_metadata", {}).get("height"),
                "fps": manifest.get("capture", {}).get("recording_metadata", {}).get("fps"),
                "capture_backend": manifest.get("capture", {})
                .get("recording_metadata", {})
                .get("capture", {})
                .get("backend"),
                "diagnostics_abi": manifest.get("capture", {})
                .get("recording_metadata", {})
                .get("capture", {})
                .get("diagnostics_abi"),
                "capture_adapter_luid": manifest.get("capture", {})
                .get("recording_metadata", {})
                .get("capture", {})
                .get("capture_adapter_luid"),
                "encoder_adapter_luid": manifest.get("capture", {})
                .get("recording_metadata", {})
                .get("capture", {})
                .get("encoder_adapter_luid"),
                "encoder_interop": manifest.get("capture", {})
                .get("recording_metadata", {})
                .get("capture", {})
                .get("encoder_interop"),
                "media_runtime_id": manifest.get("capture", {})
                .get("recording_metadata", {})
                .get("capture", {})
                .get("media_runtime_id"),
                "support_label": manifest.get("capture", {})
                .get("recording_metadata", {})
                .get("capture", {})
                .get("support_label"),
                "host_readback": manifest.get("capture", {})
                .get("recording_metadata", {})
                .get("capture", {})
                .get("host_readback"),
            }
            if key.condition == "capture"
            else None
        ),
        "capture_validation": (
            {
                "media": {
                    **(manifest.get("capture", {}).get("media_validation") or {}),
                    "sha256": manifest.get("capture", {}).get("media_sha256"),
                    "size_bytes": manifest.get("capture", {}).get("media_size_bytes"),
                },
                "diagnostics": manifest.get("capture", {}).get("diagnostics"),
                "flow": manifest.get("capture", {})
                .get("recording_metadata", {})
                .get("capture"),
            }
            if key.condition == "capture"
            else None
        ),
        "frame": frame,
        "resources": telemetry["resources"],
        "resource_safety": capture_safety,
        "limitations": (
            telemetry["limitations"]
            + (
                [
                    "League version was read from the executable adjacent to the operator-supplied game.cfg because Windows did not expose the target process image path."
                ]
                if environment.get("league_version_source") == "configured_installation_executable"
                else []
            )
        ),
    }


def median_tree(values: Sequence[Any]) -> Any:
    if not values:
        return None
    if all(isinstance(value, bool) for value in values):
        return all(values)
    if all(isinstance(value, (int, float)) and not isinstance(value, bool) for value in values):
        return percentile_type7([float(value) for value in values], 0.50)
    if all(isinstance(value, dict) for value in values):
        common = set(values[0])
        for value in values[1:]:
            common.intersection_update(value)
        return {key: median_tree([value[key] for value in values]) for key in sorted(common)}
    return values[0] if all(value == values[0] for value in values) else None


def aggregate_condition(runs: Sequence[dict[str, Any]]) -> dict[str, Any]:
    if not runs:
        raise InvalidData("cannot aggregate an empty condition")
    return {
        "run_count": len(runs),
        "frame": median_tree([run["frame"] for run in runs]),
        "resources": median_tree([run["resources"] for run in runs]),
    }


def percent_delta(baseline: float, capture: float, loss: bool = False) -> Decimal:
    base = Decimal(str(baseline))
    current = Decimal(str(capture))
    if base == 0:
        raise InvalidData("a percentage delta has a zero baseline denominator")
    numerator = base - current if loss else current - base
    return numerator / base * Decimal(100)


def evaluate_frame_gates(baseline: dict[str, Any], capture: dict[str, Any]) -> dict[str, Any]:
    measurements = {
        "average_fps_loss_percent": percent_delta(baseline["average_fps"], capture["average_fps"], loss=True),
        "one_percent_low_fps_loss_percent": percent_delta(
            baseline["one_percent_low_fps"], capture["one_percent_low_fps"], loss=True
        ),
        "p95_frametime_increase_percent": percent_delta(
            baseline["p95_frametime_ms"], capture["p95_frametime_ms"]
        ),
        "p99_frametime_increase_percent": percent_delta(
            baseline["p99_frametime_ms"], capture["p99_frametime_ms"]
        ),
        "dropped_rate_delta_percentage_points": (
            Decimal(str(capture["dropped_rate"])) - Decimal(str(baseline["dropped_rate"]))
        ) * Decimal(100),
    }
    gates = {
        name: {
            "value": float(value),
            "limit": float(FRAME_BUDGET[name]),
            "passed": value <= FRAME_BUDGET[name] + GATE_NUMERIC_TOLERANCE,
        }
        for name, value in measurements.items()
    }
    return {"passed": all(value["passed"] for value in gates.values()), "gates": gates}


def evaluate_resource_gates(
    baseline_resources: dict[str, Any], capture_resources: dict[str, Any], capture_runs: Sequence[dict[str, Any]]
) -> dict[str, Any]:
    cpu_baseline = baseline_resources["system_cpu_percent"]["saturation"]
    cpu_capture = capture_resources["system_cpu_percent"]["saturation"]
    gpu_baseline = baseline_resources["gpu"]["three_d_percent"]["saturation"]
    gpu_capture = capture_resources["gpu"]["three_d_percent"]["saturation"]
    cpu_delta = (
        Decimal(str(cpu_capture["sample_proportion"]))
        - Decimal(str(cpu_baseline["sample_proportion"]))
    ) * Decimal(100)
    gpu_delta = (
        Decimal(str(gpu_capture["sample_proportion"]))
        - Decimal(str(gpu_baseline["sample_proportion"]))
    ) * Decimal(100)
    checks: dict[str, Any] = {
        "cpu_saturation_delta_percentage_points": {
            "value": float(cpu_delta),
            "limit": float(RESOURCE_BUDGET["cpu_saturation_delta_percentage_points"]),
            "passed": cpu_delta
            <= RESOURCE_BUDGET["cpu_saturation_delta_percentage_points"] + GATE_NUMERIC_TOLERANCE,
        },
        "gpu_3d_saturation_delta_percentage_points": {
            "value": float(gpu_delta),
            "limit": float(RESOURCE_BUDGET["gpu_3d_saturation_delta_percentage_points"]),
            "passed": gpu_delta
            <= RESOURCE_BUDGET["gpu_3d_saturation_delta_percentage_points"] + GATE_NUMERIC_TOLERANCE,
        },
    }
    for run in capture_runs:
        safety = run.get("resource_safety")
        if not isinstance(safety, dict):
            raise InvalidData(f"capture run {run['run_id']} has no resource-safety result")
        checks[f"{run['run_id']}_safety"] = {
            "passed": safety["passed"],
            "checks": safety["checks"],
        }
    return {"passed": all(value["passed"] for value in checks.values()), "gates": checks}


def variability_percent(values: Sequence[float]) -> float:
    median = percentile_type7([float(value) for value in values], 0.50)
    if median == 0:
        raise InvalidData("variability has a zero median denominator")
    return (max(values) - min(values)) / median * 100.0


def validate_capped_variability(runs: Sequence[dict[str, Any]], condition: str) -> dict[str, Any]:
    average = variability_percent([run["frame"]["average_fps"] for run in runs])
    p99 = variability_percent([run["frame"]["p99_frametime_ms"] for run in runs])
    result = {
        "average_fps_range_over_median_percent": average,
        "average_fps_limit_percent": 3.0,
        "average_fps_valid": average <= 3.0 + float(GATE_NUMERIC_TOLERANCE),
        "p99_frametime_range_over_median_percent": p99,
        "p99_frametime_limit_percent": 10.0,
        "p99_frametime_valid": p99 <= 10.0 + float(GATE_NUMERIC_TOLERANCE),
    }
    if not result["average_fps_valid"] or not result["p99_frametime_valid"]:
        raise InvalidData(f"{condition} capped-run variability exceeds the protocol limit")
    return result


def _identity_issues(runs: Sequence[dict[str, Any]]) -> list[str]:
    issues: list[str] = []
    for field in (
        "system_fingerprint_sha256",
        "league_version",
        "league_version_source",
        "recorder_revision",
        "recorder_dirty",
        "presentmon_version",
        "presentmon_sha256",
        "collector_sha256",
        "display",
        "target_adapter_id",
    ):
        values = {json.dumps(run["identity"].get(field), sort_keys=True) for run in runs}
        if len(values) != 1:
            issues.append(f"run identity field {field} is inconsistent")
    for frame_mode in ("capped", "uncapped"):
        values = {
            json.dumps(run["identity"].get("league_config_sha256"), sort_keys=True)
            for run in runs
            if run["frame_mode"] == frame_mode
        }
        if len(values) != 1:
            issues.append(f"{frame_mode} League configuration fingerprint is inconsistent")
    capture_configurations = {
        json.dumps(run.get("capture_configuration"), sort_keys=True)
        for run in runs
        if run["condition"] == "capture"
    }
    if len(capture_configurations) != 1:
        issues.append("capture encoder/codec/profile/resolution/FPS metadata is inconsistent")
    for field in (
        "recorder_source_config_sha256",
        "recorder_binary_sha256",
        "ffmpeg_binary_sha256",
        "audio_source",
    ):
        values = {
            json.dumps(run["identity"].get(field), sort_keys=True)
            for run in runs
            if run["condition"] == "capture"
        }
        if None in {run["identity"].get(field) for run in runs if run["condition"] == "capture"}:
            issues.append(f"capture identity field {field} is missing")
        elif len(values) != 1:
            issues.append(f"capture identity field {field} is inconsistent")
    return issues


def _resource_deltas(baseline: dict[str, Any], capture: dict[str, Any]) -> dict[str, Any]:
    def stat_delta(base: dict[str, Any], current: dict[str, Any]) -> dict[str, float]:
        return {
            key: float(current[key]) - float(base[key])
            for key in ("median", "p95", "maximum")
            if key in base and key in current
        }

    deltas: dict[str, Any] = {
        "system_cpu_percentage_points": stat_delta(
            baseline["system_cpu_percent"], capture["system_cpu_percent"]
        ),
        "system_ram_bytes": stat_delta(
            baseline["system_ram_used_bytes"], capture["system_ram_used_bytes"]
        ),
        "gpu_3d_percentage_points": stat_delta(
            baseline["gpu"]["three_d_percent"], capture["gpu"]["three_d_percent"]
        ),
        "gpu_video_encode_percentage_points": stat_delta(
            baseline["gpu"]["video_encode_percent"], capture["gpu"]["video_encode_percent"]
        ),
        "gpu_dedicated_memory_bytes": stat_delta(
            baseline["gpu"]["dedicated_memory_bytes"], capture["gpu"]["dedicated_memory_bytes"]
        ),
        "gpu_shared_memory_bytes": stat_delta(
            baseline["gpu"]["shared_memory_bytes"], capture["gpu"]["shared_memory_bytes"]
        ),
        "processes": {},
    }
    roles = sorted(set(baseline.get("processes", {})) | set(capture.get("processes", {})))
    for role in roles:
        base_role = baseline.get("processes", {}).get(role, {})
        current_role = capture.get("processes", {}).get(role, {})
        role_delta: dict[str, Any] = {}
        for metric in sorted(set(base_role) | set(current_role)):
            base_metric = base_role.get(metric, {"median": 0.0, "p95": 0.0, "maximum": 0.0})
            current_metric = current_role.get(metric, {"median": 0.0, "p95": 0.0, "maximum": 0.0})
            if isinstance(base_metric, dict) and isinstance(current_metric, dict):
                role_delta[metric] = stat_delta(base_metric, current_metric)
        if role_delta:
            deltas["processes"][role] = role_delta
    return deltas


def _paired_frame_deltas(baseline_runs: Sequence[dict[str, Any]], capture_runs: Sequence[dict[str, Any]]) -> list[dict[str, Any]]:
    pairs: list[dict[str, Any]] = []
    for baseline, capture in zip(baseline_runs, capture_runs):
        result = evaluate_frame_gates(baseline["frame"], capture["frame"])
        pairs.append(
            {
                "pair": baseline["run_number"],
                "baseline_run_id": baseline["run_id"],
                "capture_run_id": capture["run_id"],
                "deltas": {key: value["value"] for key, value in result["gates"].items()},
            }
        )
    return pairs


def diagnostic_resource_findings(resource_deltas: dict[str, Any]) -> list[dict[str, Any]]:
    """Flag non-gating increases that require an explicit optimization disposition."""
    candidates: list[tuple[str, float, float, str]] = [
        (
            "system_cpu_median",
            float(resource_deltas["system_cpu_percentage_points"]["median"]),
            2.0,
            "percentage_points",
        ),
        (
            "gpu_3d_median",
            float(resource_deltas["gpu_3d_percentage_points"]["median"]),
            5.0,
            "percentage_points",
        ),
        (
            "system_ram_median",
            float(resource_deltas["system_ram_bytes"]["median"]),
            256.0 * 1024 * 1024,
            "bytes",
        ),
        (
            "gpu_dedicated_memory_median",
            float(resource_deltas["gpu_dedicated_memory_bytes"]["median"]),
            256.0 * 1024 * 1024,
            "bytes",
        ),
    ]
    league = resource_deltas.get("processes", {}).get("league", {})
    if "cpu_normalized_percent" in league:
        candidates.append(
            (
                "league_cpu_median",
                float(league["cpu_normalized_percent"]["median"]),
                2.0,
                "percentage_points",
            )
        )
    if "private_bytes" in league:
        candidates.append(
            (
                "league_private_memory_median",
                float(league["private_bytes"]["median"]),
                128.0 * 1024 * 1024,
                "bytes",
            )
        )
    return [
        {
            "metric": name,
            "increase": increase,
            "substantial_threshold": threshold,
            "unit": unit,
            "requires_optimization_disposition": True,
        }
        for name, increase, threshold, unit in candidates
        if increase >= threshold
    ]


def diagnostic_frame_findings(frame_deltas: dict[str, float]) -> list[dict[str, Any]]:
    """Classify substantial uncapped frame regressions without turning them into gates."""
    return [
        {
            "metric": name,
            "increase": float(frame_deltas[name]),
            "substantial_threshold": float(limit),
            "unit": "percentage_points" if name == "dropped_rate_delta_percentage_points" else "percent",
            "requires_optimization_disposition": True,
        }
        for name, limit in FRAME_BUDGET.items()
        if Decimal(str(frame_deltas[name])) >= limit
    ]


def _sanitize_string(value: str) -> str:
    value = WINDOWS_PATH_RE.sub("<absolute-path>", value)
    value = ABSOLUTE_POSIX_PATH_RE.sub("<absolute-path>", value)
    return value


def sanitize(value: Any) -> Any:
    if isinstance(value, str):
        return _sanitize_string(value)
    if isinstance(value, dict):
        sanitized: dict[str, Any] = {}
        for index, (key, item) in enumerate(value.items()):
            cleaned_key = _sanitize_string(str(key))
            if cleaned_key in sanitized:
                cleaned_key = f"{cleaned_key}#redacted-{index}"
            sanitized[cleaned_key] = sanitize(item)
        return sanitized
    if isinstance(value, list):
        return [sanitize(item) for item in value]
    if isinstance(value, float) and not math.isfinite(value):
        return None
    return value


def _dataset_schema_version(input_root: Path) -> str:
    sentinel = input_root / ".queueback-perf-results.json"
    if sentinel.is_file():
        try:
            value = json.loads(sentinel.read_text(encoding="utf-8-sig"))
        except (OSError, UnicodeError, json.JSONDecodeError) as error:
            raise InvalidData(f"benchmark result-root contract cannot be read: {error}") from error
        if not isinstance(value, dict):
            raise InvalidData("benchmark result-root contract must be an object")
        return str(value.get("schema_version"))
    first_manifest = input_root / "capped" / "baseline" / "run-1" / "manifest.json"
    if first_manifest.is_file():
        return str(read_json(first_manifest, "first benchmark manifest").get("schema_version"))
    return REPORT_SCHEMA_VERSION


def analyze_dataset(input_root: Path, target_encoder: str = "nvenc") -> dict[str, Any]:
    target_encoder = target_encoder.casefold()
    if target_encoder not in EXPECTED_INTEROP:
        raise InvalidData(f"unsupported target encoder {target_encoder!r}")
    dataset_schema = _dataset_schema_version(input_root)
    expected_runs = EXPECTED_RUNS_V1 if dataset_schema == "1" else EXPECTED_RUNS_V2
    runs: list[dict[str, Any]] = []
    issues: list[str] = []
    if dataset_schema not in SUPPORTED_SCHEMA_VERSIONS:
        issues.append(f"unsupported dataset schema version {dataset_schema!r}")
    for frame_mode, condition, run_number in expected_runs:
        key = RunKey(frame_mode, condition, run_number)
        directory = input_root / key.relative_directory
        if not directory.is_dir():
            issues.append(f"missing run directory {key.relative_directory.as_posix()}")
            continue
        try:
            runs.append(analyze_run(directory, key, target_encoder))
        except InvalidData as error:
            issues.append(f"{key.run_id}: {error}")
        except (AttributeError, IndexError, KeyError, TypeError, ZeroDivisionError) as error:
            issues.append(
                f"{key.run_id}: malformed data structure ({type(error).__name__})"
            )

    if runs:
        issues.extend(_identity_issues(runs))
        ordered = sorted(runs, key=lambda run: _parse_utc(run["measurement_started_utc"], "run start"))
        actual_order = [run["run_id"] for run in ordered]
        expected_order = [RunKey(*values).run_id for values in expected_runs]
        if len(runs) == len(expected_runs) and actual_order != expected_order:
            order_description = (
                "capped B1/C1 then uncapped B1/C1"
                if dataset_schema == "2"
                else "B1/C1, B2/C2, B3/C3, then uncapped B1/C1"
            )
            issues.append(f"run timestamps do not follow {order_description}")
        for previous, current in zip(ordered, ordered[1:]):
            previous_end = _parse_utc(previous["measurement_ended_utc"], "run end")
            current_start = _parse_utc(current["measurement_started_utc"], "run start")
            if current_start < previous_end:
                issues.append("run measurement windows overlap or do not advance strictly")

    base_report: dict[str, Any] = {
        "schema_version": dataset_schema,
        "analyzer_schema_version": REPORT_SCHEMA_VERSION,
        "feature_id": "QB-PERF-002" if dataset_schema == "2" else "QB-PERF-001",
        "status": "invalid" if issues else "pass",
        "validation_target": f"{target_encoder.upper()} / 1920x1080 / configured capture FPS",
        "unvalidated_encoders": [
            encoder.upper() for encoder in EXPECTED_INTEROP if encoder != target_encoder
        ],
        "issues": sorted(set(issues)),
        "runs": runs,
    }
    if issues:
        return sanitize(base_report)

    capped_baseline = [run for run in runs if run["frame_mode"] == "capped" and run["condition"] == "baseline"]
    capped_capture = [run for run in runs if run["frame_mode"] == "capped" and run["condition"] == "capture"]
    uncapped_baseline = [run for run in runs if run["frame_mode"] == "uncapped" and run["condition"] == "baseline"]
    uncapped_capture = [run for run in runs if run["frame_mode"] == "uncapped" and run["condition"] == "capture"]

    variability = None
    if dataset_schema == "1":
        try:
            variability = {
                "baseline": validate_capped_variability(capped_baseline, "baseline"),
                "capture": validate_capped_variability(capped_capture, "capture"),
            }
        except InvalidData as error:
            base_report["status"] = "invalid"
            base_report["issues"] = [str(error)]
            return sanitize(base_report)

    capped_baseline_aggregate = aggregate_condition(capped_baseline)
    capped_capture_aggregate = aggregate_condition(capped_capture)
    frame_gates = evaluate_frame_gates(
        capped_baseline_aggregate["frame"], capped_capture_aggregate["frame"]
    )
    resource_gates = evaluate_resource_gates(
        capped_baseline_aggregate["resources"],
        capped_capture_aggregate["resources"],
        capped_capture,
    )
    uncapped_baseline_aggregate = aggregate_condition(uncapped_baseline)
    uncapped_capture_aggregate = aggregate_condition(uncapped_capture)
    uncapped_comparison = evaluate_frame_gates(
        uncapped_baseline_aggregate["frame"], uncapped_capture_aggregate["frame"]
    )
    uncapped_frame_deltas = {
        key: value["value"] for key, value in uncapped_comparison["gates"].items()
    }
    uncapped_resource_deltas = _resource_deltas(
        uncapped_baseline_aggregate["resources"], uncapped_capture_aggregate["resources"]
    )
    capped_resource_deltas = _resource_deltas(
        capped_baseline_aggregate["resources"], capped_capture_aggregate["resources"]
    )

    base_report.update(
        {
            "status": "pass" if frame_gates["passed"] and resource_gates["passed"] else "fail",
            "environment": runs[0]["environment"],
            "configuration": runs[0]["configuration"],
            "capture_configuration": capped_capture[0]["capture_configuration"],
            "capped": {
                "status": "pass" if frame_gates["passed"] and resource_gates["passed"] else "fail",
                "baseline": capped_baseline_aggregate,
                "capture": capped_capture_aggregate,
                "repeat_policy": (
                    "fresh_pair_on_invalid_or_doubtful_evidence"
                    if dataset_schema == "2"
                    else "median_of_three_with_variability_gate"
                ),
                **({"variability": variability} if variability is not None else {}),
                "paired_frame_deltas": _paired_frame_deltas(capped_baseline, capped_capture),
                "frame_gates": frame_gates,
                "resource_safety_gates": resource_gates,
                "resource_deltas": capped_resource_deltas,
                "diagnostic_findings": diagnostic_resource_findings(capped_resource_deltas),
            },
            "uncapped": {
                "status": "diagnostic",
                "baseline": uncapped_baseline_aggregate,
                "capture": uncapped_capture_aggregate,
                "frame_deltas": uncapped_frame_deltas,
                "resource_deltas": uncapped_resource_deltas,
                "diagnostic_findings": (
                    diagnostic_frame_findings(uncapped_frame_deltas)
                    + diagnostic_resource_findings(uncapped_resource_deltas)
                ),
            },
            "limitations": sorted(
                {
                    limitation
                    for run in runs
                    for limitation in run.get("limitations", [])
                }
            ),
        }
    )
    return sanitize(base_report)


def _fmt(value: Any, digits: int = 3) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, bool):
        return "yes" if value else "no"
    if isinstance(value, (int, float)):
        return f"{value:.{digits}f}"
    return str(value)


def render_markdown(report: dict[str, Any]) -> str:
    validation_target = str(report.get("validation_target", "configured capture target"))
    unvalidated = ", ".join(report.get("unvalidated_encoders", [])) or "none"
    target_statement = (
        f"Validation target only (dataset invalid): {validation_target}. "
        f"Unvalidated encoders: {unvalidated}."
        if report.get("status") == "invalid"
        else f"Validated configuration: {validation_target}. Unvalidated encoders: {unvalidated}."
    )
    lines = [
        f"# {report.get('feature_id', 'QueueBack')} Capture Benchmark",
        "",
        f"Status: **{str(report['status']).upper()}**",
        "",
        target_statement,
        "",
    ]
    issues = report.get("issues", [])
    if issues:
        lines.extend(["## Invalidity issues", ""])
        lines.extend(f"- {issue}" for issue in issues)
        lines.append("")
    if report.get("status") != "invalid":
        frame_gates = report["capped"]["frame_gates"]["gates"]
        lines.extend(
            [
                "## Capped 144-FPS gates",
                "",
                "| Gate | Measured | Limit | Pass |",
                "|---|---:|---:|:---:|",
            ]
        )
        for name, gate in frame_gates.items():
            lines.append(
                f"| {name.replace('_', ' ')} | {_fmt(gate['value'])} | {_fmt(gate['limit'])} | {'yes' if gate['passed'] else 'no'} |"
            )
        resource_gates = report["capped"]["resource_safety_gates"]["gates"]
        for name in (
            "cpu_saturation_delta_percentage_points",
            "gpu_3d_saturation_delta_percentage_points",
        ):
            gate = resource_gates[name]
            lines.append(
                f"| {name.replace('_', ' ')} | {_fmt(gate['value'])} | {_fmt(gate['limit'])} | {'yes' if gate['passed'] else 'no'} |"
            )
        lines.append("")
        if "variability" in report["capped"]:
            lines.extend(
                [
                    "### Capped-run variability",
                    "",
                    "| Condition | Avg FPS range/median % | Limit % | p99 range/median % | Limit % |",
                    "|---|---:|---:|---:|---:|",
                ]
            )
            for condition in ("baseline", "capture"):
                variability = report["capped"]["variability"][condition]
                lines.append(
                    f"| {condition} | {_fmt(variability['average_fps_range_over_median_percent'])} | {_fmt(variability['average_fps_limit_percent'])} | {_fmt(variability['p99_frametime_range_over_median_percent'])} | {_fmt(variability['p99_frametime_limit_percent'])} |"
                )
            lines.append("")
        else:
            lines.extend(
                [
                    "Capped repeat policy: one controlled baseline/capture pair. Rerun the complete pair if evidence is invalid, noisy, or contradictory.",
                    "",
                ]
            )
        lines.extend(
            [
                "### Capture resource-safety checks",
                "",
                "| Run | Check | Pass |",
                "|---|---|:---:|",
            ]
        )
        for run in report["runs"]:
            if run["frame_mode"] != "capped" or run["condition"] != "capture":
                continue
            for name, passed in sorted(run["resource_safety"]["checks"].items()):
                lines.append(f"| {run['run_id']} | {name.replace('_', ' ')} | {'yes' if passed else 'no'} |")
        lines.append("")
        lines.extend(
            [
                "## Aggregate resource results",
                "",
                "| Metric | Baseline median | Capture median | Delta |",
                "|---|---:|---:|---:|",
            ]
        )
        base_resources = report["capped"]["baseline"]["resources"]
        capture_resources = report["capped"]["capture"]["resources"]
        resource_rows = [
            (
                "System CPU %",
                base_resources["system_cpu_percent"]["median"],
                capture_resources["system_cpu_percent"]["median"],
                "percentage points",
                1.0,
            ),
            (
                "GPU 3D %",
                base_resources["gpu"]["three_d_percent"]["median"],
                capture_resources["gpu"]["three_d_percent"]["median"],
                "percentage points",
                1.0,
            ),
            (
                "GPU Video Encode %",
                base_resources["gpu"]["video_encode_percent"]["median"],
                capture_resources["gpu"]["video_encode_percent"]["median"],
                "percentage points",
                1.0,
            ),
            (
                "System RAM MiB",
                base_resources["system_ram_used_bytes"]["median"],
                capture_resources["system_ram_used_bytes"]["median"],
                "MiB",
                1024.0 * 1024.0,
            ),
            (
                "Dedicated GPU memory MiB",
                base_resources["gpu"]["dedicated_memory_bytes"]["median"],
                capture_resources["gpu"]["dedicated_memory_bytes"]["median"],
                "MiB",
                1024.0 * 1024.0,
            ),
        ]
        for label, baseline_value, capture_value, _unit, scale in resource_rows:
            baseline_scaled = baseline_value / scale
            capture_scaled = capture_value / scale
            lines.append(
                f"| {label} | {_fmt(baseline_scaled)} | {_fmt(capture_scaled)} | {_fmt(capture_scaled - baseline_scaled)} |"
            )
        lines.append("")
        findings = report["capped"].get("diagnostic_findings", [])
        if findings:
            lines.extend(["### Substantial diagnostic increases", ""])
            lines.extend(
                f"- {item['metric']}: {_fmt(item['increase'])} {item['unit']} (threshold {_fmt(item['substantial_threshold'])})."
                for item in findings
            )
            lines.append("")
        lines.extend(
            [
                "## Aggregate frame results",
                "",
                "| Condition | Avg FPS | 1% low FPS | p50 ms | p95 ms | p99 ms | Dropped % |",
                "|---|---:|---:|---:|---:|---:|---:|",
            ]
        )
        for condition in ("baseline", "capture"):
            frame = report["capped"][condition]["frame"]
            lines.append(
                "| {} | {} | {} | {} | {} | {} | {} |".format(
                    condition,
                    _fmt(frame["average_fps"]),
                    _fmt(frame["one_percent_low_fps"]),
                    _fmt(frame["p50_frametime_ms"]),
                    _fmt(frame["p95_frametime_ms"]),
                    _fmt(frame["p99_frametime_ms"]),
                    _fmt(frame["dropped_rate"] * 100),
                )
            )
        lines.append("")
        lines.extend(
            [
                "## Individual capped runs",
                "",
                "| Run | Avg FPS | 1% low FPS | p99 ms | Dropped % |",
                "|---|---:|---:|---:|---:|",
            ]
        )
        for run in report["runs"]:
            if run["frame_mode"] != "capped":
                continue
            frame = run["frame"]
            lines.append(
                f"| {run['run_id']} | {_fmt(frame['average_fps'])} | {_fmt(frame['one_percent_low_fps'])} | {_fmt(frame['p99_frametime_ms'])} | {_fmt(frame['dropped_rate'] * 100)} |"
            )
        lines.extend(["", "## Uncapped diagnostic", ""])
        lines.append(
            "The uncapped pair is reported as diagnostic and does not affect the capped pass/fail gates."
        )
        lines.extend(
            [
                "",
                "| Condition | Avg FPS | 1% low FPS | p95 ms | p99 ms | Dropped % |",
                "|---|---:|---:|---:|---:|---:|",
            ]
        )
        for condition in ("baseline", "capture"):
            frame = report["uncapped"][condition]["frame"]
            lines.append(
                f"| {condition} | {_fmt(frame['average_fps'])} | {_fmt(frame['one_percent_low_fps'])} | {_fmt(frame['p95_frametime_ms'])} | {_fmt(frame['p99_frametime_ms'])} | {_fmt(frame['dropped_rate'] * 100)} |"
            )
        lines.extend(["", "Frame deltas: "])
        lines.extend(
            f"- {name.replace('_', ' ')}: {_fmt(value)}"
            for name, value in sorted(report["uncapped"]["frame_deltas"].items())
        )
        lines.append("")
        uncapped_findings = report["uncapped"].get("diagnostic_findings", [])
        if uncapped_findings:
            lines.extend(["### Uncapped findings requiring disposition", ""])
            lines.extend(
                f"- {item['metric']}: {_fmt(item['increase'])} {item['unit']} (substantial threshold {_fmt(item['substantial_threshold'])})."
                for item in uncapped_findings
            )
            lines.append("")
    limitations = report.get("limitations", [])
    if limitations:
        lines.extend(["## Limitations", ""])
        lines.extend(f"- {item}" for item in limitations)
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def write_report(report: dict[str, Any], json_path: Path, markdown_path: Path) -> None:
    json_path.parent.mkdir(parents=True, exist_ok=True)
    markdown_path.parent.mkdir(parents=True, exist_ok=True)
    json_text = json.dumps(report, indent=2, sort_keys=True, ensure_ascii=False, allow_nan=False) + "\n"
    json_path.write_text(json_text, encoding="utf-8", newline="\n")
    markdown_path.write_text(render_markdown(report), encoding="utf-8", newline="\n")


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path, help="raw benchmark result root")
    parser.add_argument("--output-json", required=True, type=Path, help="sanitized JSON report")
    parser.add_argument("--output-markdown", required=True, type=Path, help="sanitized Markdown report")
    parser.add_argument(
        "--target-encoder",
        choices=tuple(EXPECTED_INTEROP),
        default="nvenc",
        help="hardware encoder identity expected for schema-v2 capture runs",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    arguments = parse_args(argv)
    try:
        report = analyze_dataset(arguments.input, arguments.target_encoder)
        write_report(report, arguments.output_json, arguments.output_markdown)
    except (InvalidData, OSError) as error:
        print(f"QB-PERF-ANALYZE-ERROR: {error}", file=sys.stderr)
        return 2
    print(f"{report['feature_id']} benchmark status: {report['status']}")
    if report["status"] == "invalid":
        return 2
    if report["status"] == "fail":
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
