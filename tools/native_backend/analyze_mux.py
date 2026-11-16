"""Summarize production mux traces without conflating pipe traffic with file writes."""

import argparse
import json
from pathlib import Path


def quantile(values, probability):
    ordered = sorted(values)
    position = (len(ordered) - 1) * probability
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def distribution(values):
    return {"count": len(values), "median": quantile(values, .5),
            "p95": quantile(values, .95), "p99": quantile(values, .99), "max": max(values)}


def window(resources, frames, start, end):
    rows = [row for row in resources if row["active_elapsed"] is not None
            and start <= row["active_elapsed"] < end]
    selected = [row for row in frames if start <= row["elapsed_ns"] / 1e9 < end]
    if len(rows) < 2 or not selected:
        raise ValueError("window has insufficient samples")
    first, last = rows[0], rows[-1]
    elapsed = last["elapsed"] - first["elapsed"]
    result = {"start_seconds": start, "end_seconds": end, "sample_span_seconds": elapsed,
              "output_length_start": first["output_file_length"],
              "output_length_end": last["output_file_length"],
              "writer_ms": distribution([row["write_ns"] / 1e6 for row in selected]),
              "feed_lateness_ms": distribution([row["late_ns"] / 1e6 for row in selected]),
              "writer_over_frame_budget": sum(row["write_ns"] > 1e9 / 60 for row in selected)}
    for label in ("recorder", "ffmpeg"):
        before, after = first[label], last[label]
        result[label] = {
            "cpu_ms_per_second": (after["cpu_100ns"] - before["cpu_100ns"]) / 10000 / elapsed,
            "private_start_mib": before["private"] / 2**20,
            "private_end_mib": after["private"] / 2**20,
            "private_peak_mib": max(row[label]["private"] for row in rows) / 2**20,
            "handles_min": min(row[label]["handles"] for row in rows),
            "handles_max": max(row[label]["handles"] for row in rows),
            **{name + "_per_second": (after[name] - before[name]) / elapsed
               for name in ("read_bytes", "write_bytes", "read_ops", "write_ops")},
        }
        if "cpu_cycles" in before:
            result[label]["cpu_cycles_per_second"] = (after["cpu_cycles"] - before["cpu_cycles"]) / elapsed
    # During the measured interval the recorder reads only child progress/stderr.
    # FFmpeg writes the MP4 and those same pipes. This accounts logical file bytes,
    # while filesystem caching and physical device traffic remain outside these counters.
    child_write = last["ffmpeg"]["write_bytes"] - first["ffmpeg"]["write_bytes"]
    parent_read = last["recorder"]["read_bytes"] - first["recorder"]["read_bytes"]
    file_growth = last["output_file_length"] - first["output_file_length"]
    residual = child_write - parent_read
    result["ffmpeg_file_write_residual_estimate_bytes_per_second"] = residual / elapsed
    result["observed_file_growth_bytes_per_second"] = file_growth / elapsed
    result["file_write_residual_minus_growth_bytes"] = residual - file_growth
    result["progress_and_stderr_pipe_bytes_per_second"] = parent_read / elapsed
    result["input_pipes_write_minus_read_bytes"] = (
        last["recorder"]["write_bytes"] - first["recorder"]["write_bytes"]
        - last["ffmpeg"]["read_bytes"] + first["ffmpeg"]["read_bytes"]
    )
    return result


def analyze(directory):
    trace = json.loads((directory / "writer.json").read_text())
    resources = json.loads((directory / "resources.json").read_text())
    media = json.loads((directory / "media.json").read_text())
    video = [stream for stream in media["streams"] if stream["codec_type"] == "video"]
    frames = trace["frames"]
    assert len(video) == 1 and video[0]["codec_name"] == "h264"
    assert video[0]["profile"] == "High" and video[0]["width"] == 1920 and video[0]["height"] == 1080
    assert video[0]["avg_frame_rate"] == "60/1" and int(video[0]["nb_read_frames"]) == len(frames)
    assert trace["validated_timeline"]["video"]["exact_cfr"]
    assert trace["terminal"]["reader_errors"] == 0 and trace["terminal"]["progress_end"]
    audio = trace["audio_summary"]
    if audio is not None:
        expected = len(frames) * 800
        assert audio["last_error"] is None and audio["outcome"] == "captured"
        assert audio["output"] == "captured_aac"
        assert expected <= audio["written_frames"] <= expected + 24000
        assert audio["written_frames"] == audio["valid_written_frames"] == audio["non_silent_written_frames"]
        assert audio["observed_frames"] >= audio["written_frames"]
        assert audio["tail_overhang_frames"] == max(0, audio["written_frames"] - expected)
        assert audio["maximum_lead_frames"] <= 24000
        for counter in ("discarded_frames", "late_packets", "late_valid_frames", "unavailable_fill_frames", "inferred_idle_frames"):
            assert audio[counter] == 0, counter
    duration = len(frames) / 60
    windows = [(0, min(6, duration)), (6, duration)]
    if duration >= 60:
        windows.extend([(6, duration / 3), (duration / 3, duration * 2 / 3),
                        (duration * 2 / 3, duration)])
    result = {
        "run": directory.name, "seconds": duration, "runtime": trace["runtime_id"],
        "bytes": trace["terminal"]["bytes"], "drain_ms": trace["drain_ns"] / 1e6,
        "preparation_ms": trace.get("preparation_ns", 0) / 1e6,
        "audio_summary": audio,
        "terminal": trace["terminal"],
        "mux_observation_ms": distribution([row["observation_ns"] / 1e6 for row in trace["progress"]]),
        "windows": [window(resources, frames, start, end) for start, end in windows],
        "slowest_writes": sorted(frames, key=lambda row: row["write_ns"], reverse=True)[:12],
    }
    (directory / "summary.json").write_text(json.dumps(result, indent=2))
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, nargs="+")
    args = parser.parse_args()
    for directory in args.directory:
        result = analyze(directory)
        steady = result["windows"][1]
        print(json.dumps({"run": result["run"], "bytes": result["bytes"],
                          "drain_ms": result["drain_ms"], "steady": steady}))
