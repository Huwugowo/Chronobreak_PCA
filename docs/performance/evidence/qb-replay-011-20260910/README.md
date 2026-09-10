# QB-REPLAY-011 delivery verification

The hardened production Windows WebView passed the route probe and ten matched
post-change trials against ten valid pre-change trials on 2026-09-10. The
QB-REPLAY-008 comparison has one accepted aggregate I/O disposition, retained
without altering the analyzer result or thresholds. No latency, cancellation,
CPU/memory or new sustained-growth gate requires a product change.

## Retained evidence

- [Before report](before.json): five paused-control seek and five playing scrub
  trials, valid and identity-matched to the post-change report.
- [After report and complete comparison](after.json): all ten trials valid,
  zero required event/request loss and no server errors.
- [Production route observations](route-probe.json): exact
  `http://tauri.localhost` origin; HEAD 200 and GET range 206 on game, clip,
  thumbnail, music, import, HEVC probe and cached Data Dragon routes. Native loads
  succeeded except the explicitly unsupported HEVC probe. Source hashes matched.
- [Range delivery extraction](range-delivery.json): existing game GET 206 records,
  including first-body-read timing, delivered bytes and cancellations per trial.
- [I/O disposition](io-disposition.json): exact flagged values, retained per-trial
  counter evidence, unresolved attribution and acceptance rationale.

The execution record at [QB-REPLAY-011](../../../execution/qb-replay-011.md)
owns commands, test results, failure diagnoses and ignored raw artifact roots.

## Measured delivery result

These are medians across five trials per phase, using the same generated
240-second 1080p60 H.264/AAC fixture, seed, observer, actions and cooldowns.

| Metric | Before | After |
| --- | ---: | ---: |
| Seek request to presented frame, median | 102.65 ms | 93.20 ms |
| Seek request to presented frame, p95 | 295.825 ms | 288.43 ms |
| Scrub request to presented frame, median | 149.35 ms | 138.15 ms |
| Game-range first body read, seek median | 0.2852 ms | 0.47695 ms |
| Game-range first body read, scrub median | 0.34515 ms | 0.40065 ms |
| Game-range cancellations, seek | 4 | 4 |
| Game-range cancellations, scrub | 2 | 1 |
| Process-tree normalized CPU, seek mean | 1.3517% | 1.3199% |
| Process-tree normalized CPU, scrub mean | 7.6198% | 7.5504% |

The first-byte increases remain below the unchanged 5 ms resolution floor. The
range timing starts inside the delivery handler, so user-visible seek timing is
the complementary check covering outer admission and native presentation.

## Accepted I/O disposition

Seek job-write bytes increased from 28,897,294 to 38,468,533 (+9,571,239, 33.12%),
above the 3,171,576-byte repeatability band. This flag is valid and remains in
`after.json`. The collector retains aggregate Windows Job `WriteTransferCount`;
that counter covers current and exited job processes. Its retained records do
not identify the writer or distinguish disk, IPC and network destinations.
See [Microsoft's IO_COUNTERS contract](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-io_counters).

Accept the measured aggregate increase and attribution limitation for this
feature. All seek trials retain ten requests and 18,179,774 declared bytes;
delivered-byte median changes only 339,968 bytes (3.08%) and write-operation
median changes 0.83%. User latency, cancellation, CPU/memory, source integrity
and new sustained-growth gates pass. The byte difference is not attributed to
hardening, cache or sockets without evidence. Adding process/destination tracing
or changing delivery to suppress this aggregate counter would exceed the justified
scope of foundation hardening. No new performance budget or disk-I/O claim follows.

## Limits

- Scrub presentation p95 and game-range first-byte tails lack forty observations
  in their exact strata and are ineligible; their descriptive values are not tail
  acceptance claims.
- Short scrub trials already had thread/working-set growth flags before this
  change; they remain unchanged. No new sustained-growth flag appeared. This is
  not a long-recording resource soak.
- Optional GPU counters are unavailable. These results make no new decoder,
  capture, League FPS or multi-gigabyte scaling claim.
- HEVC HTTP delivery passed; native HEVC load was unavailable in the probe.
  Audio metadata loading is not audible/A-V observation. QB-REPLAY-009's manual
  observation remains explicitly deferred and non-blocking.
