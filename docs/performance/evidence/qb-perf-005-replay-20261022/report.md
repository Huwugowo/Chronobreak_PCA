# Standalone replay findings — 2026-10-22–23

QB-PERF-005 characterization is complete, including the final bounded settled-memory check. The fresh-index check exposed one startup search race, now fixed: the latest input waits for initial catalog admission, then runs while recording imports continue. No further production refactor is justified without a specific cause. Short post-reopen playback drops are recorded for focused follow-up under QB-REPLAY-009; no leak-free or soak claim is made.

The recorder was inactive. League tests, concurrent recording and Smite changes are excluded. Abandoned recorder/GPU supervision additions were archived and removed. This report supersedes the next-step recommendations in the historical [discovery report](../qb-perf-005-discovery-20261022/report.md), which remains unchanged as evidence.

## Conditions

One release process per new workload, on Windows 10 build 19045, i7-11800H (16 logical processors), RTX 3050 Ti / Intel UHD, WebView2 155.0.4283.62. CPU percentages are normalized to the entire machine. Both benchmark and ordinary release builds embed the current frontend through the Tauri CLI.

The disposable library contains two complete current preproduction telemetry bundles. The selected replay renders ten participants, 107 in-video events, 46 timeline markers and 30 decoded supported images. The offline Data Dragon cache has 168 files / 125 supported keys, version 16.21.1, fingerprint `974b5d6e8f63fc02344644ed3b2aa057906638803a5d699fb97a3dbe85603af8`. Existing unsupported evolved spell placeholders remain outside this work.

Original gameplay MP4s are unavailable. Only the disposable Swiftplay copy substitutes an existing synthetic moving native fixture, normalized using the existing corpus builder to 1920×1080 H.264/AAC, exact 60 FPS, 60,398 frames / 1006.633333s. Changing decoded frame hashes, strict timeline validation and full decode pass; original telemetry/source hashes are preserved. This is limited-complexity moving footage, with no gameplay or event-picture alignment claim. Combined playback uses a SQLite backup from a completed normal import into the same prepared library; the indexing diagnostic uses a fresh index. Neither condition is cold OS-cache evidence.

## Moving-video combined workflow

`motion-01` passed strict analysis, clean owned-process teardown and unchanged fixture hashes. Preserved benchmark binary SHA-256: `4c9724d85b84f7bf895105e03472dc69c0038ff9317b424cddf6de31aee6c916`. It ran eight actual typing/no-match/clear sequences, opened and verified the populated replay, played steadily for 30 seconds, completed 160 seeks, five eight-request scrub bursts and eight playing/eight paused native resizes, then returned to the two-game library.

| Observation | Result |
| --- | --- |
| App/WebView process-tree CPU | 4.85% mean; 14.82% maximum approximately one-second sample |
| Peak private / working memory | 563.75 / 668.80 MiB |
| Steady playback frame drops | 11 / 1,800 (0.61%) in 30.013s; media advanced 29.956s |
| Replay request → first authoritative frame | 517 ms; one opening |
| Near forward seek → presentation | 70.20 ms median / 184.53 ms p95; 40 observations |
| Near backward seek → presentation | 111.90 / 189.46 ms; 40 observations |
| Far forward seek → presentation | 82.10 / 192.11 ms; 40 observations |
| Far backward seek → presentation | 81.05 / 184.82 ms; 40 observations |
| Typing / no-match / clear → UI paint opportunity | 219.05 / 216.45 / 33.10 ms medians; eight each |
| Typing / no-match / clear backend-call duration | 7.85 / 5.15 / 6.60 ms medians; eight each |
| Playing resize → geometry / video presentation | 25.10 / 41.25 ms medians; eight actions |
| Paused resize → geometry / UI paint opportunity | 23.90 / 42.60 ms medians; eight actions |

Typing retains the normal 180ms debounce. The 88 inputs reconcile to 64 coalesced inputs and 24 completed/admitted/painted queries. UI paint means two revalidated animation frames; video presentation uses authoritative RVFC. arePercentiles describe actions within this one process, not across-process repeatability. Total combined frame drops include intentional seek/resize transitions and must not replace the steady-window value above.

## Fresh-index search diagnostic and minimal fix

`index-search-01` exposed a real readiness race: a query was dispatched before the backend catalog token existed and returned an error. Import subsequently completed, but this search trial is excluded. Its raw trace and disposition are preserved. The analyzer also incorrectly required settled-library readiness from an intentionally early diagnostic; the corrected gate instead requires bounded import-state completion and still rejects query errors.

The production fix holds/coalesces input while the initial catalog is pending and releases the latest query with its original action identity. Queries continue independently during subsequent per-recording imports. It changes neither SQL safety gates nor running-call cancellation semantics.

Fresh replacement `index-search-02`, binary SHA-256 `c3796c7b2ea1f11d289a56a38986876adf6ee98254797ff7c97bce923bca6e43`, passed strict analysis, clean exit and unchanged sources. The diagnostic took **8.677s**, observed indexing active and then complete, and submitted real search during indexing. Eleven inputs produced seven coalescences and four dispatched/completed queries: three admitted, one completed but superseded; one result painted and two admitted paints were superseded. There were **zero query errors**. No running query is described as cancelled. This two-recording test does not characterize a large catalog or fully indexed final-stat coverage.

## Reused evidence and exclusions

Accepted existing `search-01`, `resize-03` and `fix-scrub-02` were not rerun. Their counts and results are retained in [report.json](report.json). The static-video `combined-04` trace was accepted only after correcting stream-position membership for adjacent scrub bursts sharing a clock tick; its original analyzer rejection remains preserved. Different binaries/fixtures prevent treating old and new CPU or latency values as an improvement comparison.

Earlier failures remain excluded: unsupported-spell-label mismatch (`combined-01`), native maximized-state rejection before resize (`combined-02/03`, cause unproven), two undeclared warmup navigation seeks (`fix-scrub-01`), incomplete final native seek evidence (discovery scrub), and early generated-telemetry/empty-cache conditions. No source was overwritten, failure receipt removed, or action/error/timeout gate relaxed.

## Resource limits and disposition

GPU allocation is unavailable in all 108 moving-run samples; it is not zero. Private memory rose 245.68 MiB and working memory 262.29 MiB from startup to the last active-workload sample. The final resource sample precedes the settled library return, so this difference is not retained-memory evidence. The historical twenty-cycle static lifecycle trace also ends before its final cycle. Neither trace is reinterpreted as a leak test.

On 2026-10-23, supplementary `memory-01` passed strict analysis, unchanged fixture hashes and clean owned exit in 126.067s. Preserved release SHA-256: `e5da1c3f8431a91b64f3f8ec081cf3a5420aa910140206a9899cc37d1ffd4164`. It uses the same two-game moving fixture and populated assets, a SQLite backup of the completed normal index, and a fresh process/WebView profile. Six cycles each verify the populated roster/timeline/images, play for two seconds, release media and return to the library. Before playback and after every cycle, the mounted library settles for five seconds and is sampled for ten seconds.

The runner labels samples when it observes flushed window markers; frontend and runner clocks are not aligned. Two samples are discarded at each edge. All seven windows retain six or seven interior samples. These are within-window medians, not independent trials:

| Completed replay cycles | Private MiB | Working MiB | Handles | Interior samples |
| --- | ---: | ---: | ---: | ---: |
| 0 | 222.79 | 421.37 | 3287.5 | 6 |
| 1 | 337.29 | 572.76 | 3822 | 7 |
| 2 | 322.53 | 565.04 | 3876 | 6 |
| 3 | 318.12 | 558.94 | 3879 | 7 |
| 4 | 323.01 | 564.02 | 3877 | 6 |
| 5 | 319.79 | 562.20 | 3891.5 | 6 |
| 6 | 324.66 | 566.75 | 3894.5 | 6 |

Post-first-playback to final private memory decreased **12.63 MiB** and working memory decreased **6.01 MiB**. Later private windows fluctuate around 318–325 MiB instead of accumulating each cycle. Handles end 72.5 above the first playback median; most of that increase occurs by cycle two, and the sequence is not monotonic. This bounded observation supports retaining the current memory implementation; it does not prove the absence of a long-session leak or retention after every search/resize combination. The full combined resource trace and this isolated retention trace remain separate conditions.

The short immediate post-reopen playback windows report **155 dropped / 710 total frames (21.83%)**, with 3–40 drops per two-second window. These are native counter deltas, not lifetime counts. This is distinct from the earlier 30-second 11/1800 observation: no matched before/after regression, steady-state persistence or decoder/rendering cause is established. Record focused triage under existing playback owner **QB-REPLAY-009** before claiming smooth startup playback. No speculative fix or additional campaign is part of this characterization.

The memory run collected 124 process samples, including 124 explicitly unavailable GPU observations; Job creation accounting reconciles all 19 process notifications and ends with zero active processes, no forced termination, zero evidence loss and unchanged sources. Full per-window ranges, raw receipt hashes and short-window frame counts are retained in [report.json](report.json).

No query/HUD/decoder refactor is selected from this small synthetic-motion workload. The current implementation is retained. No arbitrary CPU target, gameplay/FPS claim or recorder compatibility claim is introduced. The startup-drop observation is a concrete follow-up question, not authorization for an open-ended campaign.

## Verification

Current frontend: **349 tests in 37 files pass**, TypeScript and production frontend build pass. Replay tooling: **128 checks: 127 pass, one skipped**. App Rust: **136 ordinary / 150 benchmark tests pass, four ignored in each**; formatting and all-target/all-feature Clippy with warnings denied pass, reused because the final memory seam changes no Rust interface. Both Tauri release builds pass with the final frontend. The actual PowerShell phase reader passes baseline/completion/next-cycle checks. Focused review found no actionable memory-seam defect with the frozen manifest (which omits `duration_seconds`); earlier catalog/action-ownership and diagnostic-gate reviews have no findings. All three new accepted live runs have clean teardown, zero required evidence loss and unchanged fixture hashes. Exact sanitized distributions, identities and receipt hashes are in [report.json](report.json); raw local runs remain ignored under `build/perf/p5motion/`.
