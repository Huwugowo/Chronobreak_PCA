# Timeline marker reactivity — QB-REPLAY-023

Measured on 2026-10-21, Windows, installed Edge 155.0.4283.62 in headless mode,
1280×800. This is an isolated development-renderer measurement, without video,
capture or decoding. It does not establish production playback FPS or a media
performance budget.

## Fixture and reproduction

`app/timeline-benchmark.html` mounts the actual ReplayTimeline with 512 visible
event anchors, spaced two seconds apart over a 1,200-second viewport. Each trial
updates the playhead on 240 animation frames, from one to five seconds. Three
fresh browser contexts were measured before editing the production expression,
then three after the change. Both versions end with three elapsed anchors.

Start the Vite development server on port 5173, then run from the repository root:

```powershell
node tools/timeline_benchmark/measure.mjs <absolute-playwright-module-path> <absolute-chromium-executable-path> after
```

Use `before` only against the original implementation. The label does not switch
implementations. Reports are written under ignored `build/perf/qb-replay-023/`.
The fixture is opt-in and is not imported by the production application entry.

## Results

| Metric | Before | After |
| --- | --- | --- |
| Playhead getter reads, each 240-frame trial | 124,080 | 1,440 |
| Total synchronous update time, trials (ms) | 282.4 / 296.0 / 284.4 | 38.6 / 26.7 / 28.4 |
| Median total synchronous update time (ms) | 284.4 | 28.4 |
| Per-update p95, trials (ms) | 1.8 / 1.9 / 1.9 | 0.3 / 0.2 / 0.2 |
| Animation-frame gap p95, trials (ms) | 16.9 / 16.9 / 16.8 | 16.9 / 16.8 / 16.9 |
| Maximum frame gap, trials (ms) | 83.4 / 17.1 / 16.9 | 83.4 / 17.0 / 17.0 |

Getter reads fall 98.84%; the median accumulated synchronous update time falls
about 90% in this fixture. Getter instrumentation counts reactive reads, not CPU
instructions or DOM mutations. Solid already avoids unchanged attribute writes.
Timing includes the instrumentation and development build overhead; these are
local observations rather than stable production latency guarantees. Frame-gap
p95 is effectively unchanged, and the first trial has the same large gap on both
versions. No frame-scheduling improvement is established.

## Implementation and correctness

The sorted visible anchor list supplies an inclusive upper bound by binary
search. A memo publishes its count only when that boundary changes. A selector
keyed by reactive anchor index updates elapsed bindings that cross the boundary.
Ordinary frames perform logarithmic boundary lookup rather than reevaluating 512
marker bindings. On a changed boundary the selector still compares registered
keys; this is not a claim of constant work for seeks or dense event crossings.

Head, fill, overview and accessible position retain continuous frame updates.
The change adds no timer, fetching, persistent state or media authority.

Verification: all 281 frontend tests in 31 files passed; the strengthened focused
timeline suite passed all 28 tests. New coverage checks exact equality, repeated
and duplicate ticks, forward/backward seeks, unsorted input, event replacement,
viewport replacement, empty lists and per-frame head/fill geometry. Existing
tests cover pointer seeking, pan/zoom, selection and clip endpoint interactions.
In the real browser fixture, ArrowRight moved 1→2 seconds with two elapsed
anchors, End moved to 1,200 with 512, and Home moved to zero with one. TypeScript
and production Vite build passed. No user media was touched.
