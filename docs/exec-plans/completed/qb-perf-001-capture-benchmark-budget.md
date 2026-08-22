# QB-PERF-001 — Capture Benchmark and Performance Budget

Status: completed on 2026-08-12 by explicit product acceptance of the full single-monitor matrix as diagnostic pre-change characterization. The analyzer's INVALID label and underlying validity rules remain unchanged.

## Purpose

Create a repeatable, auditable baseline-versus-recording benchmark for QueueBack capture. The result must show game frame delivery and system/encoder resource usage, enforce an explicit performance budget, and produce sanitized evidence that can support or reject a negligible-impact claim. This feature establishes measurement and gates; it does not tune recorder behavior.

## Relevant current architecture

- `recorder/src/main.rs` exposes diagnostics plus tray and `--headless` service modes.
- `recorder/src/config.rs` selects configuration through `LEAGUE_REPLAY_CONFIG` and logs through `LEAGUE_REPLAY_LOG_DIR`. Recorder output comes from `[storage].output_path`; despite existing verification prose, `LEAGUE_REPLAY_OUTPUT_PATH` is not implemented.
- `recorder/src/service.rs` watches `League of Legends.exe`, starts one ffmpeg child, starts the Live Client poller, and writes the final bundle.
- `recorder/src/encoder.rs` selects NVENC/AMF/QSV, captures a window-sized desktop region through ffmpeg, and records fragmented MP4. The concrete codec, encoder, profile, resolution, and FPS flow into metadata through `service.rs` and `poller.rs`.
- `recorder/src/poller.rs` writes finalized recording details to `metadata.json`; its detailed polling/error counters exist only in the final structured `Live Client poller diagnostics` log event, so isolated logs and graceful shutdown are required.
- `docs/development/VERIFICATION.md` is the canonical command index. It currently has no reproducible game benchmark entry point.
- `feature-list.json` is the canonical work state. `QB-CAP-001`, the sole dependency, is done.

## Scope / non-goals

In scope:

- a guided Windows PowerShell collector using an operator-supplied PresentMon 2.x console executable;
- deterministic, standard-library Python analysis and report generation;
- Windows CPU, GPU-engine, GPU-memory, RAM, process I/O, output-growth, recorder-diagnostic, and media-integrity telemetry;
- fixtures/tests for calculations, validity rules, aggregation, and exact gates;
- durable protocol, counter semantics, budgets, result-storage rules, and verification entry points;
- one completed manual matrix on the target Ryzen 5 5600X / RTX 4060 / 1920×1080 NVENC configuration before completion.

Out of scope:

- changing capture, encoder, poller, or lifecycle behavior to improve a result;
- automatically launching, terminating, controlling, or editing League;
- modifying or deleting a user's recordings or League graphics configuration;
- claiming AMF or QSV validation;
- hiding individual runs behind aggregate values.

## Exploration findings

- PresentMon's official console interface supports exact-PID targeting, timed collection, 2.x metrics, QPC time in milliseconds, and separate video-engine tracking. The collector requires 2.1.1 or newer within major version 2 and never excludes dropped presents.
- PresentMon records per-present frame data, not whole-system/process resource telemetry. Locale-independent raw Windows CIM counters are therefore collected separately at a stable one-second cadence and cooked deterministically in Python.
- Windows process CPU counters may use one logical processor as 100%; the analyzer must divide raw values by the logical processor count and retain both values.
- GPU Engine instances encode PID, physical adapter, engine index, and engine type in their instance names. Whole-adapter engine utilization is derived by summing contexts on the same physical engine, clamping at 100%, and taking the busiest matching engine. Per-process activity uses the busiest matching engine for that process. 3D and Video Encode remain separate.
- Per-process GPU activity and process GPU-memory counters vary by Windows/driver and can be unreliable. The report must name unavailable optional fields and their limitations; missing core frame/system counters invalidates the dataset.
- GPU Process Memory has known platform limitations, so adapter totals are authoritative for the resource summary while per-process values are diagnostic.
- Starting a recorder against a dedicated library requires an isolated TOML config selected by `LEAGUE_REPLAY_CONFIG`; there is no output-path environment override.
- A finalized capture cannot be media-validated while ffmpeg is still writing it. Collection and capture finalization are separate guided phases; neither phase controls League or the recorder.

## Chosen design

Add `tools/capture_benchmark/collect.ps1` and `tools/capture_benchmark/analyze.py`.

The collector identifies a run by condition (`baseline` or `capture`), frame mode (`capped` or `uncapped`), and run number under an explicit result root. It performs read-only preflight, gives the operator a five-second League-refocus countdown, waits through the warmup, starts a timed PresentMon session for the exact discovered League PID, and samples raw Windows performance data once per second without burst catch-up. Capture runs require a sentinel-marked dedicated library, a `[storage]`-scoped and verified isolated recorder config, exactly one prepared recorder plus its path/hash-verified ffmpeg child, and a later finalization invocation. The launcher pins the League watcher name and production `info` logging; final logs bind the recorder target PID and audio source to the measured run. The finalization invocation runs ffprobe plus full ffmpeg decode, reads only that run's recording metadata and isolated final log diagnostics, and records encoder errors or output stalls. The script never launches, terminates, or edits League and never deletes a library.

Raw run data lives under ignored `build/perf/` by convention:

```text
<result-root>/
  capped/baseline/run-1/
  capped/capture/run-1/
  ...
  uncapped/baseline/run-1/
  uncapped/capture/run-1/
```

Each run contains `manifest.json`, `presentmon.csv`, raw system/process/GPU `telemetry.ndjson`, and collector logs. Paths recorded in manifests are relative artifact identifiers. Capture finalization adds ffprobe/decode and typed recorder diagnostics. Large media and raw telemetry remain ignored.

The analyzer validates schemas and protocol identity, selects the dominant League swap chain, derives frame and resource metrics, compares paired/aggregate conditions, and writes sanitized JSON and Markdown. It never embeds the result root, user profile, config/library absolute paths, command lines containing paths, or raw process arguments. A committed manual report belongs under `docs/performance/results/`.

Frame definitions:

- average FPS is `1000 / mean(MsBetweenPresents)` over valid positive primary-swap-chain presents; Present intervals may differ from CPU-start deltas per frame, but their count and total duration must agree with primary-chain CPU-start coverage. Forward-looking `MsBetweenAppStart` is aligned row *i* to CPU starts *i*→*i+1* when it is the available 2.x metric;
- 1%-low FPS is `1000 / mean(slowest 1% of frame intervals)`, with at least one interval;
- p50/p95/p99 use deterministic type-7 interpolation;
- a dropped frame is an explicit dropped indicator when available, otherwise a present whose `DisplayedTime` is unavailable; dropped rate is dropped presents divided by all primary-swap-chain presents.

The capped condition aggregates each metric with the median of three runs per condition. It remains invalid if a required run is missing/malformed, identity/config/fingerprint/order differs, the wrong process or frame-mode confirmation is present, required counters have less than 95% of the protocol's expected samples or a gap over three seconds, frame/telemetry coverage is under 178 seconds, media validation fails, a capture is not NVENC for this validation target, or variability `(max - min) / median` exceeds 3% for average FPS or 10% for p99 frametime. Required counters are system CPU/RAM, League CPU/private memory, target-adapter 3D/Video Encode and adapter memory, plus capture recorder/ffmpeg CPU/private memory/I/O/output size; per-process GPU engine/memory is optional-with-limitation. Because Windows GPU Engine counters enumerate active contexts, an absent baseline Video Encode context is explicitly inferred as 0% only while the query and target-adapter 3D interval are valid; capture requires observed target-adapter Video Encode intervals. The uncapped pair is diagnostic and never changes capped gates, but a missing/invalid pair invalidates the complete matrix.

The 144-FPS frame gate passes only when all are true:

- average-FPS loss ≤ 2%;
- 1%-low FPS loss ≤ 5%;
- p95 frametime increase ≤ 5%;
- p99 frametime increase ≤ 8%;
- additional dropped-frame rate ≤ 0.1 percentage point.

Resource deltas are always reported. The resource-safety gate passes only when:

- capture increases sample time at total CPU ≥ 90% by no more than 5 percentage points;
- capture increases sample time at GPU 3D ≥ 95% by no more than 5 percentage points;
- Video Encode is reported separately and never counted as 3D saturation;
- recorder/ffmpeg combined `PrivateBytes` (private commit) does not show sustained growth (last-versus-first window growth at least 64 MiB and 20%, with a positive slope of at least 0.5 MiB/s);
- the capture output grows, has no measured stall of 15 seconds or longer starting at measurement time zero, and no recorder/ffmpeg disappearance, recorded encoder error, or poller lifecycle/final-flush error occurs;
- ffprobe finds valid expected audio/video streams and full ffmpeg decoding succeeds.

Other CPU/GPU/RAM/VRAM/I/O increases are findings, not automatic failures. Substantial resource findings are evaluated for both capped and uncapped comparisons; an uncapped frame delta at a capped-budget boundary is also a disposition-requiring diagnostic flag, never an uncapped gate. A substantial measured regression becomes a targeted planned child of `EPIC-CAP-PERF`; tuning is not performed here.

## Milestones

1. Mark `QB-PERF-001` in progress, save this plan path, and keep the restart pointer current.
2. Implement result layout, safety sentinel, preflight, manifest capture, timed PresentMon collection, Windows resource sampling, and capture finalization.
3. Implement deterministic parsing, validation, metric summaries, variability checks, gates, sanitized JSON/Markdown, and exit statuses.
4. Add compact fixtures and unit/integration tests for calculations, normalization, GPU aggregation, saturation, boundaries, invalid inputs, variability, resource safety, and diagnostic uncapped behavior.
5. Document the exact Practice Tool protocol, counter semantics/limitations, storage rules, collector/finalizer/analyzer commands, and budgets; link it from canonical verification.
6. Run automated checks and collector preflight without League, recording actual outcomes.
7. Run the interleaved capped matrix and uncapped diagnostic pair on Ryzen 5 5600X / RTX 4060 at 1920×1080 with NVENC, finalize/validate media, commit sanitized reports, and create a measured optimization child if a gate fails.

## Verification

- `python -m unittest discover -s tools/capture_benchmark/tests -v`
- `python tools/capture_benchmark/analyze.py --help`
- `powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture_benchmark/collect.ps1 -PresentMonPath <path> -Condition baseline -FrameMode capped -RunNumber 1 -ResultRoot build/perf/preflight -PreflightOnly`
- Run `B1/C1, B2/C2, B3/C3` at 144 FPS and one uncapped `B1/C1` diagnostic pair using the documented protocol.
- Finalize each capture run with ffprobe/full decode and run the analyzer against the complete matrix.
- Run all applicable repository commands in `docs/development/VERIFICATION.md`, canonical feature-list validation, and `git diff --check`.

Acceptance mapping:

- documented protocol plus manifest fingerprints proves repeatability;
- raw PresentMon and Windows telemetry plus per-run summaries prove selected metrics are recorded;
- paired and median aggregate comparisons prove baseline-versus-capture analysis;
- explicit analyzer constants, report gate results, and protocol documentation prove the budget existed before later optimization completion.

## Performance / reliability

The collector itself is identical across baseline and capture and samples resource counters at one-second cadence to limit observer overhead; its version/hash and collection cadence are recorded. PresentMon captures only the League process. No destructive operation targets a recording library. Capture libraries must be dedicated and sentinel-marked; tools refuse ambiguous/non-dedicated locations and never clean them automatically.

Every capture is invalid until the exact file observed growing during measurement is finalized and both structural probe and full decode pass. Output growth, ffmpeg/recorder presence, poller diagnostics/lifecycle errors, and encoder errors remain visible even when frame gates pass. The final report records hardware, OS/driver, League version, recorder revision/dirty state, display/config fingerprint, telemetry limitations, and individual run variability so regressions cannot be hidden by medians.

## Progress

- [x] Resolve dependency, mark feature active, and save ExecPlan.
- [x] Implement collector and finalization.
- [x] Implement analyzer and reports.
- [x] Add fixtures/tests.
- [x] Document protocol and verification.
- [x] Pass 56 automated analyzer/collector tests, Python/PowerShell syntax checks, analyzer CLI smoke, and actionable missing-PresentMon preflight coverage.
- [x] Pass applicable project checks, recorder release build, canonical feature-list validation, and `git diff --check`.
- [x] Pass operator preflight in an interactive Windows shell: PresentMon 2.5.1 and raw CPU/process/GPU counters reported `QB-PERF-PREFLIGHT-OK`.
- [x] Complete and finalize an initial eight-run target NVENC matrix; all four capture media/resource-safety checks passed, but the dataset is invalid because three runs contain 5-6 second telemetry gaps and the capped frame runs contain excessive focus-related variability.
- [x] Complete the target NVENC manual matrix and media validation; the product owner accepted the focus/telemetry-contaminated result as diagnostic evidence instead of requesting a clean single-monitor rerun.
- [x] Record final evidence, create QB-PERF-002 for the measured capture bottleneck, retain AMF/QSV validation follow-ups, and complete the feature without relabeling the report valid.

## Deviations / surprises

- Existing verification prose mentions `LEAGUE_REPLAY_OUTPUT_PATH`, but the recorder does not read it. The benchmark uses an isolated TOML configuration and corrects that prose.
- Deep review found and fixed initially green-test validity holes involving sparse PresentMon streams, GPU-family zero filling, role/PID attribution, interval integration, media binding, output stalls from time zero, and PowerShell nullable handling. Regression fixtures now cover those paths.
- The initial sandbox had no PresentMon executable or CIM access. An operator interactive shell subsequently passed preflight with PresentMon 2.5.1; the League matrix remains external interactive work.
- Windows withheld the live League process image path even from an elevated shell. The collector now records the version from the exact game executable adjacent to the operator-supplied `game.cfg`, labels this source in the manifest/report, and keeps PID/config/frame validation intact.
- The first complete matrix on 2026-08-12 cannot be used for the formal gate: capped baseline run 3, capped capture run 1, and uncapped capture run 1 each have one 5-6 second telemetry gap, while capped baseline runs 1/3 and capped capture run 1 show focus-contaminated frame variability. The operator attributed this to necessary Alt-Tab checks on a single display. The two clean capped capture runs nevertheless agree closely and, against the clean capped baseline run, show a repeatable frame-pacing regression and large CPU/copy cost. Source inspection and process attribution point to Windows `gdigrab` plus system-memory pixel conversion/upload as the likely bottleneck; NVENC Video Encode and total GPU 3D are not saturated, and recorder orchestration is effectively idle. On 2026-08-12 the product owner explicitly accepted this diagnostic disposition, waived a clean QB-PERF-001 rerun, and retained the INVALID label.

## Decision log

- 2026-08-11: Use the user-supplied protocol and hybrid gates without recorder tuning in this feature.
- 2026-08-11: Keep collection and capture finalization separate so no tool controls League or kills a live encoder.
- 2026-08-11: Treat adapter GPU memory as authoritative and per-process GPU memory as diagnostic because Windows documents counter limitations.
- 2026-08-11: Keep raw data under ignored build output and generate path-sanitized summaries for version control.
- 2026-08-11: Use a `1e-12` percent/percentage-point comparison tolerance only for binary-float representation noise at inclusive boundaries; reported measurements remain unrounded and epsilon-over-budget fixtures still fail.
- 2026-08-12: Accept the completed single-monitor matrix as sufficient diagnostic pre-change characterization because the bottleneck disposition is clear. Do not reinterpret it as a formal gate pass, a negligible-impact claim, or a waiver of QB-PERF-002's valid post-change matrix.

## Completion

Completed. The collector, analyzer, protocol, tests, preflight, all eight manual runs, capture finalization, ffprobe/full-decode checks, diagnostic analysis, and canonical optimization disposition are complete. The report remains formally INVALID because of single-monitor focus changes and telemetry gaps; by explicit product decision this is accepted as sufficient pre-change characterization and no clean QB-PERF-001 rerun remains. This completion does not establish negligible impact. QB-PERF-002 must produce its own valid post-change matrix, and AMF/QSV remain explicitly unvalidated under QB-PERF-003/QB-PERF-004.
