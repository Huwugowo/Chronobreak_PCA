"""Bracket exact-window WGC with capture-off intervals on a D3D11 renderer."""

import argparse
from collections import Counter
import csv
import json
from pathlib import Path
import re
import statistics
import subprocess
import time


RESOURCE_SAMPLER = r"""
$ErrorActionPreference = 'Stop'
while ($true) {
    $before = [Diagnostics.Stopwatch]::GetTimestamp()
    $processes = @(Get-Process -Name dwm,d3d11_renderer,native_wgc_source_probe -ErrorAction SilentlyContinue)
    $ids = $processes.Id
    $gpu = @(
        Get-CimInstance Win32_PerfRawData_GPUPerformanceCounters_GPUEngine |
            Where-Object { $_.Name -match '^pid_(\d+)_' -and [int]$Matches[1] -in $ids } |
            Select-Object Name,RunningTime
    )
    @{
        qpc = ($before + [Diagnostics.Stopwatch]::GetTimestamp()) / 2
        processes = @($processes | Select-Object Id,ProcessName,CPU,PrivateMemorySize64)
        engines = $gpu
    } | ConvertTo-Json -Compress -Depth 4
    Start-Sleep -Seconds 4
}
"""


def qpc_ms():
    return time.perf_counter_ns() / 1_000_000


def percentiles(values):
    if not values:
        return None
    ordered = sorted(values)
    cuts = statistics.quantiles(ordered, n=100, method="inclusive") if len(values) > 1 else ordered * 99
    return {
        "count": len(ordered), "median": cuts[49], "p95": cuts[94],
        "p99": cuts[98], "max": ordered[-1],
    }


def read_frames(path, frequency):
    scale = 1000 / frequency
    samples = []
    with path.open(newline="") as handle:
        rows = csv.DictReader(handle)
        first = next(rows, None)
        if first is None:
            raise ValueError("renderer frame trace is empty")
        previous = int(first["present_qpc"])
        for row in rows:
            now = int(row["present_qpc"])
            samples.append((
                now * scale,
                (now - previous) * scale,
                (int(row["return_qpc"]) - now) * scale,
                (now - int(row["start_qpc"])) * scale,
                row["foreground"] == "1",
                float(row["gpu_ms"]),
            ))
            previous = now
    return samples


def read_source(path, luid, size, hwnd):
    text = path.read_text(encoding="utf-8")
    width, height = size.split("x")
    ready = f"WGC_TIMING_READY adapter_luid={luid} size=({width}, {height}) hwnd={hwnd.lower()}"
    if ready not in text.splitlines():
        raise ValueError("source adapter, pool size, or HWND differs from renderer")
    telemetry = re.search(
        r"consumed=(\d+).*arrivals: (\d+), admitted: (\d+), handoff_drops: (\d+).*callback_errors: (\d+)",
        text,
    )
    if not telemetry:
        raise ValueError("missing terminal capture telemetry")
    consumed, arrived, admitted, dropped, errors = map(int, telemetry.groups())
    if errors or arrived != admitted + dropped or not 0 <= admitted - consumed <= 1:
        raise ValueError("capture error or unreconciled frame accounting")
    rows = []
    for line in text.splitlines():
        if not line.startswith("WGC_TIMING index="):
            continue
        fields = dict(re.findall(r"(\w+)=(-?\d+)", line))
        source = int(fields["qpc_100ns"])
        lead = int(fields["source_100ns"]) - int(fields["arrival_100ns"])
        if fields["width"] != width or fields["height"] != height:
            raise ValueError("unexpected capture frame dimensions")
        rows.append((source / 10_000, (source - lead) / 10_000, lead / 10_000,
                     int(fields["close_ns"]) / 1_000_000))
    if len(rows) != consumed or len(rows) < 2:
        raise ValueError("capture trace does not match consumed frame count")
    return rows, {"arrived": arrived, "admitted": admitted, "consumed": consumed,
                  "handoff_drops": dropped}


def summarize(frames, source, presents, start, end, *, foreground):
    rows = [row for row in frames if start <= row[0] < end]
    if not rows or any(row[4] != foreground for row in rows):
        raise ValueError("renderer produced no frames or changed activation during measurement")
    captures = [row for row in source if start <= row[1] < end]
    stamps = [row[0] for row in captures]
    return {
        "duration_s": (end - start) / 1000,
        "renderer_fps": 1000 * len(rows) / (end - start),
        "frame_ms": percentiles([row[1] for row in rows]),
        "present_call_ms": percentiles([row[2] for row in rows]),
        "renderer_submit_ms": percentiles([row[3] for row in rows]),
        "renderer_gpu_ms": percentiles([row[5] for row in rows if row[5] >= 0]),
        "frames_over_16_67ms": sum(row[1] > 1000 / 60 for row in rows),
        "present_modes": dict(Counter(mode for timestamp, mode in presents if start <= timestamp < end)),
        "wgc_unique_fps": 1000 * len(set(stamps)) / (end - start),
        "wgc_nonadvancing": sum(b <= a for a, b in zip(stamps, stamps[1:])),
        "wgc_source_gap_ms": percentiles([b - a for a, b in zip(stamps, stamps[1:])]),
        "wgc_source_lead_ms": percentiles([row[2] for row in captures]),
        "wgc_close_ms": percentiles([row[3] for row in captures]),
    }


def resource_rates(resources, frequency, luid, start, end):
    rows = [row for row in resources if start <= row["qpc"] * 1000 / frequency < end]
    if len(rows) < 2:
        return None
    first, last = rows[0], rows[-1]
    seconds = (last["qpc"] - first["qpc"]) / frequency
    process_starts = {row["Id"]: row for row in first["processes"]}
    engine_starts = {row["Name"]: int(row["RunningTime"]) for row in first["engines"]}
    result = {"duration_s": seconds, "processes": {}}
    for row in last["processes"]:
        if row["Id"] not in process_starts:
            continue
        initial = process_starts[row["Id"]]
        name = row["ProcessName"]
        rates = {"cpu_ms_per_s": (row["CPU"] - initial["CPU"]) * 1000 / seconds
                 if row["CPU"] is not None and initial["CPU"] is not None else None,
                 "private_mib_start": initial["PrivateMemorySize64"] / 2**20,
                 "private_mib_end": row["PrivateMemorySize64"] / 2**20,
                 "gpu_engine_ms_per_s": {}}
        prefix = f"pid_{row['Id']}_luid_0x{luid[:8]}_0x{luid[8:]}_".lower()
        for engine in last["engines"]:
            key = engine["Name"]
            if not key.lower().startswith(prefix) or key not in engine_starts:
                continue
            kind = key.split("_engtype_")[-1]
            delta = int(engine["RunningTime"]) - engine_starts[key]
            if delta >= 0:
                totals = rates["gpu_engine_ms_per_s"]
                totals[kind] = totals.get(kind, 0) + delta / 10_000 / seconds
        result["processes"][name] = rates
    return result


def wait_renderer_ready(renderer, path):
    deadline = time.monotonic() + 15
    while renderer.poll() is None and time.monotonic() < deadline:
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.startswith("RENDERER_READY "):
                return line
        time.sleep(0.05)
    raise RuntimeError(f"renderer failed to start, see {path}")


def run(args):
    durations = args.arm_seconds or [args.seconds] * len(args.arms)
    total = sum(durations) + 12
    if len(durations) != len(args.arms) or any(not 8 <= value <= 600 for value in durations) or total > 1800:
        raise ValueError("each arm must be 8–600s and total renderer time at most 1800s")
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    renderer_csv = out / "renderer.csv"
    renderer_log = out / "renderer.log"
    present_csv = out / "presentmon.csv"
    resource_path = out / "resources.jsonl"
    phases = []
    children = []
    with renderer_log.open("w", encoding="utf-8") as log, \
            (out / "presentmon.log").open("w", encoding="utf-8") as pm_log, \
            resource_path.open("w", encoding="utf-8") as resource_log, \
            (out / "resources.log").open("w", encoding="utf-8") as resource_errors:
        try:
            if args.resources:
                children.append(subprocess.Popen([
                    "powershell.exe", "-NoProfile", "-NonInteractive", "-Command", RESOURCE_SAMPLER,
                ], stdout=resource_log, stderr=resource_errors, creationflags=subprocess.CREATE_NO_WINDOW))
            command = [args.renderer, str(total), str(args.iterations), renderer_csv]
            if args.display or args.no_activate:
                command.append(args.display or "-")
            if args.no_activate:
                command.append("no-activate")
            renderer = subprocess.Popen(
                command, stdout=log, stderr=subprocess.STDOUT,
                creationflags=subprocess.CREATE_NO_WINDOW,
            )
            children.append(renderer)
            ready = wait_renderer_ready(renderer, renderer_log)
            print(ready, flush=True)
            fields = dict(re.findall(r"(\w+)=(\S+)", ready))
            if args.presentmon:
                pm = subprocess.Popen([
                    args.presentmon, "--process_id", str(renderer.pid), "--v2_metrics",
                    "--qpc_time_ms", "--no_console_stats", "--terminate_on_proc_exit",
                    "--output_file", present_csv,
                ], stdout=pm_log, stderr=subprocess.STDOUT, creationflags=subprocess.CREATE_NO_WINDOW)
                children.append(pm)
            time.sleep(3)
            for index, (arm, duration) in enumerate(zip(args.arms, durations, strict=True), 1):
                if renderer.poll() is not None:
                    raise RuntimeError("renderer exited before the matrix ended")
                print(f"Arm {index} {arm} for {duration}s", flush=True)
                start = qpc_ms()
                path = out / f"{index:02d}-{arm}.log"
                if arm != "off":
                    with path.open("w", encoding="utf-8") as capture_log:
                        capture = subprocess.Popen([
                            args.capture, "--pid", str(renderer.pid), "--timing-only",
                            "--duration-seconds", str(duration),
                        ], stdout=capture_log,
                            stderr=subprocess.STDOUT, creationflags=subprocess.CREATE_NO_WINDOW)
                        children.append(capture)
                        if capture.wait(duration + 8) != 0:
                            raise RuntimeError(f"source-only capture failed, see {path}")
                else:
                    time.sleep(duration)
                phases.append((arm, start, start + duration * 1000, path))
            if renderer.wait(20) != 0:
                raise RuntimeError(f"renderer failed, see {out / 'renderer.log'}")
            if args.presentmon and pm.poll() is None:
                try:
                    pm.wait(5)
                except subprocess.TimeoutExpired:
                    print("PresentMon did not exit after renderer; see presentmon.log", flush=True)
        finally:
            for child in reversed(children):
                if child.poll() is None:
                    child.kill()
                    child.wait()
    frequency = int(fields["freq"])
    (out / "phases.json").write_text(json.dumps([
        {"condition": arm, "start_qpc_ms": start, "end_qpc_ms": end, "trace": path.name}
        for arm, start, end, path in phases
    ], indent=2) + "\n", encoding="utf-8")
    frames = read_frames(renderer_csv, frequency)
    resources = [json.loads(line) for line in resource_path.read_text(encoding="utf-8").splitlines()]
    presents = []
    if present_csv.is_file():
        with present_csv.open(newline="") as handle:
            presents = [(float(row["CPUStartQPCTime"]), row["PresentMode"])
                        for row in csv.DictReader(handle) if "CPUStartQPCTime" in row]
    if args.presentmon and not presents:
        print("No PresentMon events available. See presentmon.log.", flush=True)
    result = {"renderer": ready, "shader_iterations": args.iterations,
              "foreground": not args.no_activate, "arms": []}
    for arm, start, end, path in phases:
        source, accounting = [], None
        start, end = start + 2000, end - 2000
        if arm == "on":
            source, accounting = read_source(path, fields["luid"], fields["size"], fields["hwnd"])
            if source[0][1] > start or source[-1][1] < end:
                raise ValueError("WGC did not cover the fixed measurement interval")
        summary = summarize(frames, source, presents, start, end, foreground=not args.no_activate)
        windows = []
        for offset in range(0, int(end - start), 15_000):
            lo, hi = start + offset, min(start + offset + 15_000, end)
            if hi - lo >= 5000:
                windows.append({"arm_offset_s": offset / 1000,
                                **summarize(frames, source, presents, lo, hi,
                                            foreground=not args.no_activate)})
        result["arms"].append({"condition": arm, "start_qpc_ms": start, "end_qpc_ms": end,
                               "accounting": accounting, **summary, "windows": windows,
                               "resources": resource_rates(resources, frequency, fields["luid"], start, end)})
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    for arm in result["arms"]:
        print(arm["condition"], arm["frame_ms"], "wgc_unique_fps", arm["wgc_unique_fps"],
              "present_modes", arm["present_modes"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--renderer", type=Path, required=True)
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--presentmon", type=Path)
    parser.add_argument("--resources", action="store_true")
    parser.add_argument("--display")
    parser.add_argument("--no-activate", action="store_true")
    parser.add_argument("--seconds", type=int, default=25)
    parser.add_argument("--arm-seconds", type=int, nargs="+")
    parser.add_argument("--iterations", type=int, default=96)
    parser.add_argument("--arms", nargs="+", choices=["off", "on"],
                        default=["off", "on", "off"])
    run(parser.parse_args())
