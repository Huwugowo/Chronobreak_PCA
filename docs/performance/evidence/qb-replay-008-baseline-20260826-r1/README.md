# QB-REPLAY-008 accepted current-tree baseline

Status: accepted characterization evidence on 2026-08-26. This is not a product performance budget or a claim that WebView playback, recorder media settings, or export strategy should be replaced.

The committed `report.json` and `report.md` aggregate 67 valid immutable production-Tauri/WebView runs and preserve 67 individual trials. Raw media, process samples, request traces, launch manifests, failed runs, and exports remain below the ignored sentinel-owned `build/perf/.chronobreak-replay-benchmark` root.

## Exact subject and evidence sets

All 67 accepted trials used:

- production app binary SHA-256 `5d07f9ec6282eb05cba6efe9a0fbd34a665262f76029d5da2956e6be3c23c873`;
- repository revision `2105b9dbb4b68f0af486ebc6e9ad68010d2b8375` with the benchmark implementation recorded as a dirty-tree binary, so the binary hash—not the base revision alone—is the exact executable identity;
- sanitized analyzer subject SHA-256 `fabbbc17d6953f492636a35f8b0ecc99cce5fd2127974d0b9f89ac07348bac2c`;
- WebView2 `151.0.4129.101`;
- packaged runtime `queueback-ffmpeg-8.1.2-windows-x86_64-r6`, FFmpeg SHA-256 `1dc19648acdcaa7ac2497689837feb32788d404e7705f756e331a2f535bfe015`, and ffprobe SHA-256 `639da6703f05c4e0f436d8602b39f44070cbe8233c5dd942775e00f25004615e`.

The accepted roots are exactly:

- sequences 1–49 from `baseline-current-tree-20260825-r2/matrix-plan.json`;
- sequences 1–3 from `baseline-current-tree-post-rate-20260826-r3/matrix-plan.json`;
- sequences 1–15 from `baseline-current-tree-final-continuation-20260826-r1/matrix-plan.json`.

The first set covers five app-idle trials, five cold opens for each H.264 media class, five in-process warm reopen cycles per media class, and five play/pause runs per media class. The latter sets cover deterministic seek, event-jump, endpoint-edit, scrub, layout, reopen, ten-minute torture, and nine export arms. Each plan was verified against its immutable results, declared order, cooldown, terminal state, and final corpus hashes with `matrix.py verify --require-results`; the analyzer was then invoked with those 67 explicit result roots and this directory as `--output-dir`.

The identity-bound corpus contains short native H.264/AAC (241.34 s), representative native H.264/AAC (1800.02 s), long derived native H.264/AAC (3600.02 s), external-backend H.264/AAC (240.79 s), and a derived HEVC/AAC capability fixture (180 s). Preparation used packaged ffprobe plus full single-thread decode and preserved all source hashes. The production WebView capability marker reported HEVC unsupported, so the positive matrix omitted HEVC arms instead of mislabelling them as supported.

## Selected observations

- A five-game/two-clip isolated library became useful in a median 192.4 ms (214.0 ms across-trial p95). The clip query itself took a median 102.9 ms; the games and storage queries were each about 10 ms. This small corpus does not justify catalog/index restructuring by itself.
- Cold replay request-to-first-presented-frame medians were 204.2 ms for short native H.264, 538.5 ms for external H.264, 1681.9 ms for representative native H.264, and 3075.3 ms for long native H.264. Across-trial p95 values were 271.9, 775.9, 2231.7, and 4975.7 ms respectively.
- Playback-payload backend medians for those same classes were only 1.1, 1.1, 4.3, and 8.0 ms. The measured opening delay appears after payload readiness and is dominated by media metadata/readiness and first presentation. `QB-REPLAY-010` owns measured opening work; it must not assume the payload/catalog path is the bottleneck.
- Representative and long deterministic seeks recorded 160 authoritative observations each. Request-to-presented medians/p95s were 261.1/332.2 ms and 282.8/322.2 ms. Five scrub bursts settled in 450.3/573.2 ms and 486.5/553.3 ms median/p95 respectively, while bounded latest-wins dispatch coalesced the request bursts without server errors.
- The ten-minute torture arm completed 60 seeks, 60 rate changes, and 20 fullscreen transitions with zero server errors and no sustained private-memory, working-set, handle, or thread growth flag. The dedicated layout arm did produce short-window monotonic resource-growth flags; this is retained as a comparison/review target for `QB-REPLAY-009`, not promoted to a leak claim from one transition-heavy trial.
- Current export is `full_reencode`. Horizontal arms took 1.767–4.480 s, vertical arms 2.542–5.449 s, Discord arms 3.916–4.850 s, and the near-limit Discord arm 13.591 s. Encode time dominated; all outputs validated, had zero retries, remained inside their size contract, and preserved sources. `QB-CLIP-004` owns any compatible copy/remux decision.

## Capability findings and limitations

The preserved representative-H.264 rate capability run applied and closely observed 0.25×, 0.5×, 1×, 2×, and 4×. Applying 8× succeeded at the media property but effective advancement fell to `0.0029579112626615784×`; the following 0.25× transition then failed with `rate window ended without advancing current media`. This is the current production-WebView capability result and is assigned to `QB-REPLAY-009`; it is not retried into a pass.

Optional Windows GPU-engine/memory collection remained unavailable because the CIM provider cannot be bounded inside the one-second collector cadence. The report therefore does not treat generic GPU activity as proof of hardware video decoding and makes no hardware-decoder normal-path claim. Explicit decoder-path validation remains owned by `QB-REPLAY-009` before its before/after performance claims.

Three 2026-08-26 launches under a restricted desktop token failed before frontend startup because Chromium's GPU process exited with Windows access denied (`0xC0000022`). Launching the exact production binary through the normal desktop token succeeded. Those preserved roots diagnose the tool sandbox, not Chronobreak product startup.

The first endpoint-edit result was application-valid but exposed an analyzer defect: automatic `clip-loop` seeks were counted as planned `endpoint-edit` seeks. The analyzer now scopes requested seeks by `seek_reason`, has a regression test, accepted the preserved result under the corrected rule, and accepted a clean replacement arm.
