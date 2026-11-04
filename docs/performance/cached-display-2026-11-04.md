# Cached desktop display, 2026-11-04

The 8.552–8.847 seconds reported for QB-LIB-028 is full reconstruction from an
empty SQLite preparation cache. It is not the time required to display an already
prepared library. This check measures the latter in the actual production WebView.

## Change

Data Dragon initialization previously awaited its online version request before
publishing an existing local manifest cache. A slow request could withhold cached
artwork for the request's 12-second timeout, followed by the frontend's status poll.
Initialization now publishes the same validated local-cache status before awaiting
the optional refresh. Refresh success and offline fallback retain their existing
behavior. Journal preparation, source validation and media loading are unchanged.

## Measurement

`tools/library_benchmark/warm.mjs` starts the staged production executable against
the eight-recording preprod library with retention disabled, the prepared SQLite
cache and existing local artwork. It disables recorder supervision only in its
isolated measurement child and uses a separate, persistent WebView profile.

The process timestamp precedes spawn. The library endpoint requires eight cards,
settled history, completed image loads and decoded artwork, then waits through a
paint opportunity. It includes all 140 available card images, including cards
outside the initial viewport. Three unavailable recorded spell variants retain
their existing placeholders; these are counted separately.

Replay timing starts at the actual enabled recording-button handler and ends only
after the production details state is ready, the timeline exists, visible artwork
has decoded and a paint opportunity has passed. Each of the eight recordings is
opened. Optional hidden full-scoreboard hydration and video decode are separate
from this cached-data endpoint. Observation can attach after the app has started;
that makes the launch-to-display result a conservative upper bound, not an
artificially shortened frontend-only duration.

Example command, from the repository root:

```powershell
node tools/library_benchmark/warm.mjs C:/Users/Hugo/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright build/release/warm-display-20261104/league-replay-app.exe build/perf/<new-artifact-directory> build/perf/warm-display-profile --assert-budget
```

## Results and limits

| Complete cached run | Launch to cards and artwork | Selection to replay details, all eight recordings |
| --- | --- | --- |
| after-3 | 907.1 ms | 222.1–605.2 ms |
| after-4 | 922.5 ms | 209.4–356.3 ms |
| after-5 | 892.9 ms | 225.9–385.0 ms |

All three complete runs passed the explicit 1,000 ms assertions for the library
and every replay. Raw reports are
`build/perf/warm-display-after-20261104-{3,4,5}/summary.json`.

The initial launch of the new staged build measured **1,271.6 ms** for the library;
the observer attached at 928 ms, versus roughly 540 ms in the repeated runs. Its
navigation check subsequently failed because the harness used the wrong accessible
Back-button name. That does not invalidate or erase its startup measurement,
retained in `build/perf/warm-display-after-20261104-1/failure.json`.
A second exploratory run measured 909.0 ms for the library, then exposed another
harness issue: the next recording was clicked before its control became enabled.
Both harness issues were corrected before the three complete runs above.

The original build's final-method library-only measurement was 2,032.6 ms
(`build/perf/warm-display-before-20261104-4/summary.json`). Internet and startup
variability mean this single baseline is not a fixed speedup ratio.

Repeated cached operation meets the measured budget. The first updated-build
launch exceeded it, so these results do **not** establish an unconditional
subsecond process-start ceiling. No filesystem cache flush was performed and no
claim is made for arbitrary library sizes, uncached remote icons or other machines.

## Validation and open build

- Held-refresh regression: cached manifests become ready while the online check
  is still pending; deliberately failing that check leaves them usable.
- Backend suite: 189 passed, 10 opt-in tests ignored;
  `.codex/logs/command-20261104-124700.log`.
- Viewer suite: 50 passed; `.codex/logs/command-20261104-124139.log`.
- Strict all-target backend Clippy, Rust formatting, frontend type check/build,
  production desktop build and packaged runtime verification passed.
- Staged executable: `build/release/warm-display-20261104/league-replay-app.exe`,
  SHA-256 `11121ee15db959b3c76d3a5193ee659d172380775bbcde0ae01548de6d3c8f0e`.
- The GUI, recorder and windowless preparation helper were opened from that same
  stage after the old recorder reported idle and no League game was running.

QB-LIB-029 remains in progress because the initial launch exceeded the full
process-start budget. The separate capture-watchdog design remains deferred.
