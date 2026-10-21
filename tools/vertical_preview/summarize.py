"""Finalize the predefined QB-CLIP-010 five-pair acceptance gate."""
import json
import statistics
from pathlib import Path

root = Path("build/perf/qb-clip-010")
rows = []
for number in range(1, 6):
    pair = []
    for arm in ("a", "b"):
        item = json.loads((root / "trials" / f"{arm}-{number}" / "result.json").read_text(encoding="utf-8"))
        assert item["valid"] and item["cpu"]["valid"] and item["media_valid"], f"Invalid {arm}-{number}"
        foreground = [v for v in item["videos"] if v["role"] == "foreground"]
        assert len(foreground) == 1
        video = foreground[0]
        frames = video["end"]["frames"] - video["start"]["frames"]
        decoded = video["end"]["decoded"] - video["start"]["decoded"]
        dropped = video["end"]["dropped"] - video["start"]["dropped"]
        assert frames > 0 and decoded > 0 and dropped >= 0
        assert len(item["videos"]) == (2 if arm == "a" else 1)
        assert item["canvas_count"] == (0 if arm == "a" else 1)
        pair.append({"label": item["label"], "cpu": item["cpu"]["logical_cores"], "frames": frames, "decoded": decoded, "dropped_fraction": dropped / frames})
    rows.append({"a": pair[0], "b": pair[1], "cpu_delta": pair[1]["cpu"]-pair[0]["cpu"], "drop_delta": pair[1]["dropped_fraction"]-pair[0]["dropped_fraction"]})
baseline = statistics.median(p["a"]["cpu"] for p in rows)
delta = statistics.median(p["cpu_delta"] for p in rows)
allowed = max(baseline*.15, .05)
# Each pair must satisfy the foreground drop bound; this also bounds its median.
drops_pass = all(p["drop_delta"] <= .01 for p in rows)
result = {"pairs":rows,"baseline_median_cores":baseline,"paired_median_cpu_delta":delta,"allowed_cpu_increase":allowed,"cpu_pass":delta<=allowed,"drops_pass":drops_pass,"passed":delta<=allowed and drops_pass}
(root / "paired-summary.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
print(json.dumps(result,indent=2))
assert result["passed"], "Predefined acceptance gate failed"
