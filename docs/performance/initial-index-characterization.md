# Real-journal initial indexing characterization

Measured on Windows on 2026-10-21 for QB-LIB-015. This is local characterization,
not a performance budget or a disk-cold benchmark. No production indexing code
changed for this measurement.

## Reproduction and boundaries

Run from the repository root:

```powershell
cargo test --release --manifest-path app/src-tauri/Cargo.toml real_preprod_index_measurements -- --ignored --nocapture
```

The opt-in test makes three independent temporary libraries and SQLite caches
from the unchanged `fixtures/preprod-library/library/games` bundles. It calls the
production `Index::recording` path, with its four-journal-commit bound, until ready;
then explicitly rebuilds each recording, closes/reopens the cache and checks reuse.
SHA-256 checks prove both source fixtures and copied inputs remain unchanged.
Projection counts and scopes must match before/after rebuild. The existing
`real_preprod_sources_import_query_and_rebuild_without_rewriting_evidence` test
separately checks actual query values and source preservation.

Fixture copy plus hashing, directory scan, open and reconcile are timed separately.
The three runs share an OS and filesystem cache: “fresh” means a fresh SQLite
cache, not cold storage. No compiler/test workload ran alongside the timed trials.
Journal commits are not SQLite transaction counts. Cache byte counts include the
shared database, WAL and SHM and are cumulative, not attributable to one recording.

## Backend results

All durations below are milliseconds, with repetitions in execution order.

| Operation | Practice, 65 commits / 17 calls | Swiftplay, 887 commits / 222 calls |
| --- | --- | --- |
| Fresh cache import | 136.250 / 187.717 / 173.645 | 4,622.090 / 4,506.006 / 4,380.943 |
| Explicit rebuild | 136.138 / 162.469 / 184.756 | 4,005.382 / 5,026.473 / 4,679.279 |
| Reopened cache, recording check | 2.071 / 1.850 / 2.183 | 6.282 / 4.475 / 4.622 |

Every reopened recording check returned ready with zero imported batches.
Swiftplay fresh-call p95 was 24.241 / 29.060 / 28.646 ms; maxima were
874.900 / 126.143 / 62.630 ms. Rebuild p95 was 26.537 / 32.451 / 29.397 ms.
These are per-run empirical order statistics, not a tail-latency guarantee.

The source journals contain 196,997 and 4,736,710 bytes respectively. All copied
fixture files total 13,399,459 bytes. Copy plus hashing took 23.122–25.777 ms,
scan 5.218–6.223 ms, initial open 11.235–13.118 ms and initial reconcile
0.455–0.919 ms. Reopen took 1.893–4.457 ms and reconciliation 3.280–5.492 ms.
After Swiftplay, the retained shared cache was 16,904,552 bytes on fresh import
and 19,549,280 bytes after rebuild.

## Actual production WebView

Three rebuilds used the freshly built Tauri production executable and the
isolated `build/preprod` library/config/app-data. They exercised Settings →
Rebuild League data → Back with the document visible. A DOM mutation observer
measured from immediately before reset to the disappearance of “Updating League
data.” in the recorded-matches view: **35,531.2 / 37,546.3 / 35,973.8 ms**.
The third trial additionally observed the final “Updating matches.” disappearing
at **35,979.1 ms**. The first two figures do not claim final query/render completion.
These runs include reset, refresh, navigation, IPC, query and rendering work; they
are not directly equivalent to the isolated backend loop.

Diagnostic CDP request accounting was incomplete (preflights in trial one and
missing completion events later). It is excluded from latency attribution. The
DOM measurements above are independent of that accounting. Diagnostic launch
used a temporary WebView debugging port; normal startup does not require it.

## Disposition

`HistoryController.schedule` waits 100 ms before every import continuation.
For one recording, the deterministic waits between calls alone are 1,600 ms for
Practice and 22,100 ms for Swiftplay. These are calculated minimum scheduled
delays, not measured wall-time attribution; browser timer delays may be longer.
The real UI results support examining this avoidable pacing first.

The next bounded optimization should shorten the continuation timer while
retaining a task yield, one active import, the four-commit backend bound,
independent queries, stale-context rejection and close cancellation. Measure the
same production WebView rebuild and responsiveness afterwards. These results do
not justify changing schema, transaction integrity, admission, concurrency or
increasing the import batch bound. Backend serialization/SQL costs remain a
separate profiling question.

Raw local artifacts: ignored `build/perf/qb-lib-015/backend-release.txt` and
`webview-trial-{1,2,3}.json`; all durable conclusions are retained above.

## Verification

The release measurement passed all three repetitions. App formatting and
all-target Clippy passed. App tests passed 136 tests with four opt-in tests
ignored using normal Windows child-process access. An initial sandbox run
failed the child-ready cancellation fixture (135 passed); the unchanged fixture
passed in the normal-access run. The real-fixture query/rebuild regression passed
in both runs. Canonical validation and diff whitespace checks passed.

## Short continuation yield (QB-LIB-016)

The subsequent production change only shortens the history import timer from
100 ms to 4 ms. It retains a timer task yield, one active importer, independent
queries, four-commit worker calls, snapshot checks and navigation/close cancellation.
Replay detail import pacing is unchanged. This direct change does not establish
a background capture or resource budget.

The freshly rebuilt production Tauri executable, same isolated library and same
observer procedure produced these results:

| Repetition | Indexing finished (ms) | Final query settled (ms) | rAF gap p95 / maximum (ms) |
| --- | --- | --- | --- |
| 1 | 8,613.2 | 8,619.0 | 16.8 / 17.4 |
| 2 | 10,357.7 | 10,364.4 | 16.8 / 17.0 |
| 3 | 9,115.9 | 9,122.3 | 16.8 / 17.1 |

The indexing median decreased from 35,973.8 to 9,115.9 ms (74.7% less elapsed
time, approximately 3.9 times as fast). This is three sequential local trials,
not a cold-disk or cross-machine guarantee. The 517/621/548 animation-frame
callbacks indicate the renderer kept receiving frames while visible; they are
not physical presentation, CPU/disk utilization or capture-overhead measurements.

A separate active-index interaction trial filtered to Master Yi in 8 ms from the
automation action to the single visible card while indexing remained active.
That is local filter response, not a semantic-query latency claim. Clearing the
filter and opening the recording loaded playable media; keyboard seek reached
5 seconds without an alert. Returning to Match History resumed/completed the
remaining work and restored both cards without an alert.

Controller tests verify task yielding, no overlapping import while a worker is
held, independent search queries, close cancellation and leaving Games/stale
context behavior. All 279 frontend tests across 31 files passed; TypeScript,
Vite and the Tauri production build passed. Raw post-change timings and the
interaction receipt are under ignored `build/perf/qb-lib-016/`.
