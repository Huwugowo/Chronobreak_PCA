# QB-REPLAY-010 opening attribution

The material library delay is attributable to synchronous clip-duration probes.
This supports a focused library implementation design; it is not an improvement
claim or completion of QB-REPLAY-010.

## Coverage and identity

The frozen production Tauri/WebView benchmark subject is revision
`3a750e2c003d6e3605b2f907ff19ad7898d3dbd2`, executable SHA-256
`f1e3390cb29b3c9465eee3898a46af5d2c3c69894aeb9f7a5ad5c09a1ff75b49`, packaged
media runtime r6. The machine is a Dell G15 5511, i7-11800H, 16 GB RAM,
Intel UHD / RTX 3050 Ti, Windows 10 build 19045, WebView2 152.0.4191.66.
[Provenance](provenance.json) retains runtime hashes, environment, all 30 ordered
launch identities, immutable manifest hashes, and raw-artifact hashes. Raw inputs
and results remain in the ignored root named there. Seed: 20260915; five-second
active warmup/cooldown. Cold means a fresh process; OS file caches were warm.

All 30 planned launches were attempted. **28 are individually valid; overall
planned coverage is incomplete.** S/R/E have five valid cold processes each;
N/L have four each. Each cell also has one valid warm process with an initial
mount plus five remounts. S/R/E required-result matrix checks passed. L/N failed:

- Launch 14, L cold 3: collector/session interruption, missing terminal,
  collection and post-hash receipts. Preserved and excluded.
- Launch 18, N cold 4: scenario finished, source hashes matched, but resource
  sample 2 had Job total processes 96 versus 95 creation notifications. The next
  sample reconciled 118/118; strict all-sample telemetry completeness still
  fails. Preserved and excluded, including its otherwise healthy app timings.

No replacement standard launches exceeded the 30-launch ceiling. No complete
matrix, before/after, p95/p99, or resource non-regression pass is claimed.

## Fixtures

S/R/L share one three-game, zero-clip library, selecting 240 / 1,800 / 3,600 seconds
respectively. S is generated current schema-v2 native media; R/L are stream-copy
repeat transformations of S, not independent long recordings or League evidence.
The exact video grids contain 14,400 / 108,000 / 216,000 frames at 60 FPS.

N/E each have 50 games and 50 dedicated ten-second H.264/AAC clips. Corresponding
media, metadata, identities and counts are identical. N has 7 events / 8 snapshots
per game; E has 259 / 240 (30-second versus one-second generator intervals).
All 203 staged media files passed packaged probing and full single-thread decode.
The corpus occupied 2.198 GiB against an 8 GiB bound before result collection.
These are synthetic scaling libraries, not a representative-user claim.

## Cold observations

Milliseconds, median [minimum–maximum]; MAD is median absolute deviation.
Backend command durations overlap and must not be added into wall time.

| Cell | Valid cold n | Library useful | Clip command | Games command | Storage command |
| --- | ---: | ---: | ---: | ---: | ---: |
| S | 5 | 92.4 [88.8–96.7] | 7.4 [7.1–10.3] | 7.3 | 7.4 |
| R | 5 | 97.8 [89.9–331.7] | 8.9 [7.6–9.4] | 8.3 | 8.6 |
| L | 4 | 100.0 [85.0–126.8] | 7.7 [6.0–8.1] | 7.7 | 7.6 |
| N | 4 | 2121.6 [2013.2–2287.7] | 1992.6 [1890.1–2174.5] | 26.4 | 27.3 |
| E | 5 | 2312.6 [2260.2–2355.7] | 2196.4 [2138.4–2240.8] | 324.4 | 320.1 |

Useful MAD S/R/L/N/E: 3.2 / 7.9 / 9.1 / 89.8 / 43.1 ms.
Clip-command MAD: 0.31 / 0.56 / 0.20 / 83.26 / 44.37 ms.
The current useful event waits for games, clips, storage, settings, HEVC
capability, and two animation frames; actual library rendering waits for the
first four resources. Clip command completion gates both paths here.

| Cell | Backend payload median | Viewer mounted median | Metadata median [min–max] | First authoritative frame median [min–max] |
| --- | ---: | ---: | ---: | ---: |
| S | 0.66 | 14.3 | 68.1 [66.3–70.8] | 190.4 [188.0–196.5] |
| R | 2.26 | 19.8 | 263.3 [236.1–556.3] | 376.4 [360.8–676.0] |
| L | 4.35 | 25.0 | 439.7 [429.0–445.7] | 562.8 [561.4–568.1] |
| N | 0.72 | 16.4 | 68.8 [58.7–86.3] | 205.8 [196.3–219.4] |
| E | 7.96 | 33.1 | 93.3 [67.1–390.5] | 221.7 [215.6–512.0] |

All frontend intervals start at its replay request. Native payload elapsed uses
its own clock. Duration-associated delay is predominantly after payload/mount.
This does not distinguish container parsing, delivery, or native WebView readiness;
it does not justify a payload split, container change, or playback backend change.
L remains undersampled. The schema-v1 008 historical baseline is not a comparator.

## One narrower diagnostic follow-up

The whole clip command was insufficient to distinguish game enrichment from
probes. Existing Job records confirmed 118 process creations in N/E versus 18 in
S/R/L, but lack executable identities and child lifetimes. They cannot assign
elapsed time or CPU to ffprobe.

The plan's one narrower follow-up directly timed the actual `library::list_clips`
work in one optimized native test process. The [retained patch](clip-diagnostic.patch)
adds only test-and-benchmark-gated aggregate timers and an ignored fixture test.
There were three calls per cell, order N/E/E/N/N/E, using the prepared libraries
and packaged ffprobe. No Tauri launch or replacement trial was added. Timed work
did no extra media reads or probes for instrumentation. The source file was
restored byte-for-byte afterward; no instrumentation remains in the application.
The [post-diagnostic source check](source-preservation.json) matched all 400
prepared N/E files against their preparation hashes and sizes.

| Cell | Source game enrichment | 50 duration probes | Whole clip command | Probe share |
| --- | ---: | ---: | ---: | ---: |
| N | 19.0–24.2 ms | 1673.6–1772.1 ms | 1700.2–1805.3 ms | 98.2–98.4% |
| E | 287.7–306.3 ms | 1665.4–1817.3 ms | 1965.9–2132.1 ms | 84.3–85.2% |

[Exact six observations](clip-work.json) include count reconciliation, source
identity and command. Every call returned 50 clips with positive durations.
Probe subtotal includes synchronous child creation, execution, waiting and result
parsing; it does not isolate those subparts. Native diagnostic timings are not
substitutes for Tauri user-visible latency or collector resource evidence.

This resolves the responsible work: one synchronous probe per clip delays the
required clip list. The measured probe subtotal comfortably exceeds E's useful
decision thresholds of 115.6 ms (5%) and 129.3 ms (three MAD). It establishes a
plausible material improvement, not an observed speedup. Sparse/dense parsing
also adds roughly 300 ms to game/storage commands, but the N/E useful difference
alone does not exceed the sparse reference's 269.5 ms repeatability band and N is
undersampled. Do not claim that contrast alone proves a useful-time improvement.

## Warm characterization

Five disposal/remount observations **within one process per cell**, initial mount
excluded. Request intervals include the existing 100 ms remount timer.

| Cell | Request to first frame median [min–max] | Mount to first frame median |
| --- | ---: | ---: |
| S | 156.3 [136.3–165.6] | 41.6 |
| R | 328.8 [310.8–356.5] | 205.9 |
| L | 520.9 [485.1–829.1] | 393.8 |
| N | 180.1 [173.8–188.2] | 38.0 |
| E | 200.9 [174.0–529.7] | 42.1 |

[Opening rows](opening.json) preserve initial mounts separately. The helper
partitions only frontend clocks and uses loadstart's new media generation;
`viewer_mounted` occurs before generation advances. Native payload durations and
server requests are whole-process records with no guessed per-cycle joins.
Missing phases, stale/reused media generations and non-authoritative frames fail
extraction. Five synthetic helper tests and all 28 valid real records passed.

## Resources and limitations

Cold median server requests/ranges are 1/1 for S/N/E and 2/2 for R/L. Delivered
bytes are 7,973,362 for S/N/E, 68,455,047 for R and 127,464,798 for L. R/L median
cancelled streams: one. Every included cold trial has zero server errors. Request
shape does not identify metadata/container versus delivery cost.

Median aggregate Job write bytes S/R/L/N/E: 3,686,618 / 3,692,530 / 3,008,081.5 /
5,378,309 / 5,707,929. Median normalized process-tree mean CPU: 7.15 / 7.08 / 7.24 /
9.09 / 7.78%. These are aggregate accounting observations, not disk attribution.
[Trial data](trials.json) and [summaries](summary.json) retain CPU, memory, handles,
threads, I/O, server, frame and error observations with min/median/max and MAD.

Sustained thread-growth flags occurred in 5/5, 3/5, 4/4, 4/4, 3/5 cold trials;
working-set flags in 4/5, 1/5, 0/4, 2/4, 1/5 respectively. No included cold trial
had sustained handle/private-memory growth. These short-run watchpoints require
comparison/disposition if a later candidate changes them; they do not establish
either a leak or a non-regression pass. Warm resource records cover whole processes.
Preserve 011's unresolved aggregate I/O attribution and 009's deferred human A/V
gate. Neither is converted into passing evidence here.

## Disposition

Attributed material library issue: select a coherent per-refresh filesystem scan
for games, clip identities and storage totals, with clip durations optional and
bounded off the critical games path. Finalize a reviewed versioned successor
ExecPlan before implementation. Keep replay-descriptor staging and unexplained
post-payload delay open; they are not authorized by this library attribution.
QB-REPLAY-010 remains non-done, with its original acceptance criteria intact.
