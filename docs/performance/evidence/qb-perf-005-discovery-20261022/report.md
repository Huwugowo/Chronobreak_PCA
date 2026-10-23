# Current-release application discovery — 2026-10-22

Six existing release-app workloads passed analysis with current telemetry and populated Data Dragon assets. Scrubbing was rejected for incomplete final-seek evidence. This is a descriptive planning baseline; QB-PERF-005 is not complete.

Subject: `7c341937bf67a8de36828fd42f576ce426ea4c7d`, production frontend in the feature-gated release benchmark build. Intel i7-11800H, 16 logical processors, Windows 10 build 19045. One process per workload; persisted ready index and warm OS cache. CPU is the whole-run app/WebView process-tree mean normalized to total machine capacity.

| Workload | CPU mean | Private peak | Successful icon requests |
| --- | ---: | ---: | ---: |
| idle | 0.55% | 319.1 MiB | 20 |
| play | 6.96% | 572.5 MiB | 17 |
| open | 8.48% | 434.6 MiB | 17 |
| seek | 4.75% | 552.5 MiB | 17 |
| layout | 9.37% | 517.4 MiB | 27 |
| lifecycle | 9.01% | 512.3 MiB | 223 |
| scrub | Rejected | Final native seeked observation missing | — |

Opening to first video frame: 179.6 ms; selected event-window readiness: 298.1 ms. Seek request to presented frame: median 146.8 ms, p95 378.8 ms over 160 requests in one process. These are within-run descriptive values, not replicated tail estimates.

Playback quality counter: 36/2162 dropped/total frames over the complete opening/play-pause sequence. Startup and transitions are included; this does not isolate steady playback.

The fixture has 107 in-video events and ten indexed participants. The roster begins at 24.574 s; the one-time opening marker legitimately reports zero players at 0 s. Offline cache 16.21.1 contains 125 supported fixture asset keys; all logged icon responses in these runs completed with HTTP 200. Three unsupported evolved spell labels retain existing placeholders (QB-LIB-013); Smite remains excluded.

The video is a static synthetic 1080p60 H.264/AAC title card paired with current recorded telemetry. Results cover this populated UI workload, not high-motion gameplay decoding. The current bridge proves the overlay-ready admission gate, not physical rendering of roster content. Layout is frontend fullscreen, not desktop resizing.

Earlier obsolete-journal and empty-cache trials are excluded and preserved. The first cold-index viewer attempt hit the existing 30 s readiness deadline; its result is separate from these settled-index measurements. No old/new optimization comparison is made.

Next: add real search, native window resizing, verified active recorder overlap and bounded GPU allocation collection through the finalized QB-PERF-005 design. Run repeated paired conditions before selecting an optimization. League testing is excluded.

Scrubbing presented its final requested frame, but the benchmark released the viewer before recording native seeked. The strict analyzer rejected it. Preserve the failed trace, add a bounded benchmark-only wait for both observations, then use a fresh declared trial. No production playback failure or complete seven-run campaign is claimed.

Detailed sanitized metrics, exact identities, limitations and dispositions are in [report.json](report.json). Raw immutable launches and traces remain under `build/perf/p5p2/.chronobreak-replay-benchmark`; static matrix verification passes and the six accepted individual results pass the existing analyzer. The complete-result matrix gate fails on the preserved scrub result.
