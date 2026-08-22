# Capture benchmark protocol and performance budget

This is the durable baseline-versus-recording protocol shared by `QB-PERF-001` and `QB-PERF-002`. It measures QueueBack's incremental cost by comparing League with no QueueBack recorder to the same scenario with the recorder and its single FFmpeg child active. Recorder diagnostics and the synthetic WGC fixture are useful integration checks, but neither substitutes for the League comparison.

The collector never launches, terminates, controls, or edits League. It never deletes a result directory or recording library. Capture runs use a new sentinel-marked library and an isolated copy of the recorder config; a missing or ambiguous sentinel is a hard error.

## Validation targets and claims

The first required validation target is:

- AMD Ryzen 5 5600X;
- NVIDIA RTX 4060;
- 1920×1080 borderless League;
- NVENC as proven by finalized recorder metadata.

The codec, profile, output resolution, and configured capture FPS come from the current recorder configuration and must remain identical across capture runs. Schema v2 records the Windows capture backend, diagnostics ABI, capture and encoder adapter LUIDs, direct-interoperability mode, packaged-runtime identity, support label, finite resource limits, and final capture counters. AMF and QSV use the same budgets but remain performance-unvalidated until their separate physical-hardware reports pass.

## Prerequisites

- Windows 10 or newer and PowerShell 5.1 or newer.
- A prebuilt release recorder (`cargo build --release --manifest-path recorder/Cargo.toml`). Do not benchmark through `cargo run`; compiler/Cargo processes contaminate resource data.
- The current recorder TOML and the exact ffmpeg/ffprobe executables used by the recorder.
- An operator-supplied [PresentMon console application](https://github.com/GameTechDev/PresentMon/blob/main/README-ConsoleApplication.md), major version 2 and at least 2.1.1. The collector targets the exact League PID, keeps dropped presents, requests QPC milliseconds and Video Encode tracking, and uses `--v2_metrics` only when that version exposes the switch.
- Permission to read Windows performance data. PresentMon and CIM queries may require membership in Performance Log Users or an elevated interactive shell.
- Enough free space for the selected immutable matrix: eight runs/four captures for historical schema v1, or four runs/two captures for schema v2.

Do not use a real QueueBack library. `LEAGUE_REPLAY_OUTPUT_PATH` is not a recorder setting. The recorder reads `[storage].output_path` from the TOML selected by `LEAGUE_REPLAY_CONFIG`; `-PrepareCapture` locates that key specifically inside the single `[storage]` table, replaces it with the run's sentinel-owned library, and verifies the generated value before exposing a launcher. It also isolates `LEAGUE_REPLAY_LOG_DIR`, pins `LEAGUE_REPLAY_PROCESS_NAME=League of Legends.exe`, uses the production `info` log level, and preserves the selected recording/app choices. An existing `LEAGUE_REPLAY_AUDIO_DEVICE` choice is pinned into the launcher; otherwise it is explicitly unset so normal auto-detection applies.

## League scenario

Use League Practice Tool on Summoner's Rift with default-skin Garen.

1. Use the user's normal graphics settings except for the required measurement state: 1920×1080 borderless, VSync off, and either the 144-FPS cap or uncapped mode named by the run.
2. Do not let the collector edit `game.cfg`. Pass its path so the collector records a SHA-256 fingerprint before warmup and again after measurement. Capped runs must share one fingerprint; uncapped runs may share a second because the cap changes.
   If Windows withholds the live League process image path, the collector reads the version from `Game\League of Legends.exe` adjacent to that operator-supplied `game.cfg`, records the fallback source, and surfaces it as a report limitation. PID, target name, configuration fingerprint, and frame telemetry validation remain unchanged.
3. Place the camera at the fixed mid-lane position used for every run. Keep League focused, visible, and unobscured during every warmup and measurement so PresentMon conditions remain comparable. The optimized recorder itself targets the exact League HWND and has no primary-display or GDI fallback.
4. Supply no input during the 60-second warmup or 180-second measurement. Close avoidable background workloads, overlays, updates, and monitoring tools, and keep power/thermal conditions stable.
5. Repeat any run whose exact process, cap attestation, configuration fingerprint, or target window is wrong.

The collector's `-ProtocolAttestation` means the operator has verified default-skin Garen, the fixed camera/no-input procedure, 1920×1080 borderless, VSync off, and the requested cap. It is not an automatic League configuration check.

## Run matrix and lifecycle

Historical schema v1 (`QB-PERF-001`) retains this exact order and median-of-three capped analysis:

```text
capped B1, capped C1, capped B2, capped C2, capped B3, capped C3,
uncapped B1, uncapped C1
```

Each collection gives the operator a five-second countdown to refocus League, then performs a 60-second warmup followed by a 180-second measurement. While that no-input interval runs, the collector requests Windows' continuous display/system-required execution state without synthesizing input and restores the prior policy afterward; this prevents an idle monitor timeout from suspending WGC. At measurement completion it prints an explicit safe-to-Alt-Tab message and plays a three-tone cue when the console supports it; do not Alt-Tab merely to inspect the timer before that cue. Recorder isolation is rechecked after warmup and throughout measurement; any recorder appearing in a baseline or any recorder identity change during capture aborts the run without terminating a process. The capped conditions are aggregated using the median of three run-level values. The uncapped pair is diagnostic: it has no pass/fail gate, but a missing or invalid uncapped run still makes the complete matrix invalid.

Schema v2 (`QB-PERF-002`) intentionally uses exactly four runs:

```text
capped B1, capped C1, uncapped B1, uncapped C1
```

The single capped pair keeps every existing per-run validity check and every frame/resource budget. The uncapped pair remains required and diagnostic. There is no schema-v2 three-run variability or median gate. If a pair is invalid, noisy, or genuinely doubtful, preserve its immutable root and rerun that complete pair in a fresh root; never hide uncertainty by appending selective runs.

For the current four-run proof, run `& .\tools\capture_benchmark\run_qb_perf_002.ps1` from the repository root. It opts into schema v2, defaults to the staged packaged runtime under `build/media-runtime/windows-x86_64`, and accepts `-TargetEncoder nvenc|amf|qsv`. The historical `run_qb_perf_001.ps1` remains schema v1 by default. Both runners create a fresh timestamped root, print one operator action at a time, and never launch, terminate, or edit League. A failed or interrupted root is immutable: preserve it for diagnosis and use a new root.

Start with static preflight (League may be closed):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture_benchmark/collect.ps1 `
  -PresentMonPath C:\Tools\PresentMon-2.4.1-x64.exe `
  -Condition baseline -FrameMode capped -RunNumber 1 `
  -ResultRoot build/perf/qb-perf-001 -PreflightOnly
```

Preflight validates PresentMon identity/flags, raw Windows counter classes, PowerShell/Windows, and result-root safety. It prints `LIVE-CHECKS-DEFERRED` because League PID/config, recorder/ffmpeg identity, output growth, and media can only be checked during the live run. Stable error codes such as `QB-PERF-PRESENTMON_MISSING`, `QB-PERF-CIM_ACCESS_DENIED`, and `QB-PERF-LEAGUE_NOT_RUNNING` identify actionable failures.

For a baseline, confirm no `recorder.exe` is running, position League, and run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture_benchmark/collect.ps1 `
  -PresentMonPath C:\Tools\PresentMon-2.4.1-x64.exe `
  -Condition baseline -FrameMode capped -RunNumber 1 `
  -ResultRoot build/perf/qb-perf-001 `
  -LeagueConfigPath C:\path\to\game.cfg -ProtocolAttestation
```

For each capture condition, first prepare an immutable run directory and isolated recorder environment:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture_benchmark/collect.ps1 `
  -PresentMonPath C:\Tools\PresentMon-2.4.1-x64.exe `
  -Condition capture -FrameMode capped -RunNumber 1 `
  -ResultRoot build/perf/qb-perf-001 -PrepareCapture `
  -RecorderSourceConfig C:\path\to\current-config.toml `
  -RecorderPath recorder\target\release\recorder.exe `
  -FfmpegPath C:\path\to\ffmpeg.exe
```

Run the generated `start-recorder.ps1` in a separate PowerShell window. Wait for the red tray state and obtain the single `recorder.exe` PID, then collect. During candidate startup the private measured output may be named `video-candidate-N.mp4`; the collector binds it to the exact FFmpeg command line and records that identity in telemetry. Graceful finalization publishes the successful candidate as canonical `video.mp4`, and the finalizer verifies that canonical path.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture_benchmark/collect.ps1 `
  -PresentMonPath C:\Tools\PresentMon-2.4.1-x64.exe `
  -Condition capture -FrameMode capped -RunNumber 1 `
  -ResultRoot build/perf/qb-perf-001 -RecorderPid 1234 `
  -LeagueConfigPath C:\path\to\game.cfg -ProtocolAttestation
```

After measurement, use the prepared recorder's tray **Quit** action so FFmpeg, metadata, poller diagnostics, and logs flush. Ctrl+C is supported only when the recorder was started with `--headless`; the tray build uses its tray Quit command. Do not close the terminal or use `Stop-Process` for a normal benchmark capture. League may remain open. Then finalize:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture_benchmark/collect.ps1 `
  -PresentMonPath C:\Tools\PresentMon-2.4.1-x64.exe `
  -Condition capture -FrameMode capped -RunNumber 1 `
  -ResultRoot build/perf/qb-perf-001 -FinalizeCapture `
  -FfprobePath C:\path\to\ffprobe.exe
```

Finalization reads only the sentinel-owned run library, requires recorder/ffmpeg to have stopped, verifies `metadata.json`, binds the log's recorded League PID and audio source to the measured run, requires the complete final poller-diagnostics event, detects encoder/fallback errors plus poller stop/panic/final-flush errors, cross-checks ffprobe codec/resolution/frame rate/duration against recorder metadata, and performs a full ffmpeg decode. It never deletes the media.

Repeat the commands with run numbers/modes in matrix order. For uncapped runs use `-FrameMode uncapped -RunNumber 1` and attest an uncapped League frame limit.

## Raw data and manifest

Raw output belongs under ignored `build/perf/` and is never committed. Every run has:

- a schema-versioned `manifest.json`;
- PresentMon frame CSV and stdout/stderr;
- one-second raw CIM counter snapshots in `telemetry.ndjson`;
- hashes for collected artifacts;
- hardware, OS, GPU driver, display, League version, recorder revision/dirty state, PresentMon version/hash, config fingerprint, exact PIDs, telemetry sources, timing, and relative artifact names;
- for capture, the isolated TOML/log/library plus finalized media, ffprobe/decode, concrete encoder/codec/profile/resolution/FPS, and recorder diagnostics.

Raw files may contain absolute local paths and logs. The analyzer constructs public output from selected typed fields, applies defense-in-depth recursive path redaction to those fields, and never copies raw manifests or logs. Commit only the sanitized JSON/Markdown under `docs/performance/results/`.

## Frame calculations

The analyzer selects the exact manifest League PID and its dominant swap chain. A tie, less than 90% dominance, wrong process, or a capped average inconsistent with a 144-FPS cap invalidates the run.

- Frame interval uses `MsBetweenPresents` when available, then forward-looking `MsBetweenAppStart`, otherwise successive QPC-millisecond CPU start times. `MsBetweenPresents` may legitimately differ from CPU-start deltas per frame, but its interval count and total duration must agree with the primary-chain CPU-start coverage; `MsBetweenAppStart` row *i* is aligned to CPU starts *i*→*i+1*. These checks prevent sparse/truncated rows from claiming an unrelated high FPS.
- Average FPS is `1000 / mean(frame interval ms)`, never the mean of instantaneous FPS.
- 1%-low FPS is `1000 / mean(slowest ceil(frame count × 1%) intervals)`.
- p50, p95, and p99 frametime use type-7 linear interpolation.
- A present is dropped when an explicit dropped flag says so or `DisplayedTime` is `NA`/zero. Dropped rate is dropped primary-chain presents divided by all primary-chain presents.
- Gate calculations use unrounded values. A `1e-12` percent/percentage-point comparison tolerance exists only to neutralize binary-float representation noise at inclusive boundaries; it is not report rounding or budget slack. Reports retain each run and each capped B/C pair.

## Windows resource-counter semantics

Collection uses locale-independent `Win32_PerfRawData_*` CIM classes and cooks deltas in Python. The same queries run in baseline and capture to keep observer overhead symmetric.

- Whole-system CPU is the inverse `_Total` processor timer, constrained to 0–100%.
- A process CPU raw timer may reach logical-processor-count × 100%. Machine-normalized process CPU is raw percent divided by logical processor count; both are reported.
- Process I/O byte rates are cooked from raw byte counters with their performance timestamps/frequency.
- System and process RAM are gauges. Recorder plus ffmpeg private bytes feed only the sustained-growth safety heuristic.
- GPU Engine instance names provide PID, adapter LUID, physical part, engine index, and engine type. Contexts are summed per physical engine, small per-context counter jitter is clamped, physical-engine totals are capped at 100% after retaining the unclamped diagnostic maximum, and the busiest target-adapter engine represents 3D or Video Encode. Video Encode is never counted as 3D saturation. These counters enumerate active contexts: absent baseline Video Encode intervals are inferred as 0% only while the query and target-adapter 3D interval are valid, whereas a capture run must expose Video Encode intervals for active NVENC.
- Adapter dedicated/shared memory is authoritative. Per-process GPU Engine and GPU-memory values are diagnostic where exposed. Windows documents that [GPU Process Memory can over-report apparent growth](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/gpu-process-memory-counters-report-wrong-value), so it never drives the memory-growth gate.

Required data must cover at least 95% of the 180 expected one-second intervals (not merely 95% of whatever samples were emitted), span at least 178 seconds, and contain no telemetry gap over three seconds. Per-line schema, sequence, UTC time, query status/error pairing, declared overruns, role PID, and counter progression are validated. Required sources are system CPU/RAM, League CPU/private memory, target-adapter 3D and Video Encode, adapter dedicated/shared memory, and—for capture—recorder/ffmpeg CPU/private memory/I/O plus output size. Missing per-process GPU Engine/memory is a reported limitation, not automatic invalidity.

For output-liveness specifically, schema v1 treats the observable open-file length as the progress source. Windows may buffer fragmented-MP4 writes without updating that length while FFmpeg still owns the file, so schema v2 instead uses the recorder's in-process monotonic mux-byte counter; it retains the sampled file length only as a diagnostic.

## Validity and budgets

A dataset is invalid before gates are evaluated when any run is missing/malformed/short, order or fingerprints differ (including the inferred target-adapter LUID), required coverage is absent, the process/cap/target is wrong, PresentMon/artifact hashes do not match, capture metadata does not match the declared target encoder/backend/adapter/direct-interoperability contract, or media validation fails. Schema-v1 capped variability also invalidates when:

- `(max - min) / median` > 3% for average FPS within either condition;
- `(max - min) / median` > 10% for p99 frametime within either condition.

Exact boundaries are valid.

Schema v2 has one capped pair and therefore no variability statistic. Doubtful evidence is dispositioned by rerunning a fresh complete pair, not by weakening validity checks.

The capped frame budget passes only when:

- average-FPS loss `(B - C) / B` ≤ 2%;
- 1%-low FPS loss ≤ 5%;
- p95 frametime increase `(C - B) / B` ≤ 5%;
- p99 frametime increase ≤ 8%;
- capture dropped-rate minus baseline dropped-rate ≤ 0.1 percentage point.

The resource-safety budget passes only when:

- capture minus baseline sample proportion at total CPU ≥90% is ≤5 percentage points;
- capture minus baseline sample proportion at target-adapter GPU 3D ≥95% is ≤5 percentage points;
- every capture has positive advancing output evidence and no 15-second-or-longer stall beginning at measurement time zero, using schema-v1 file length or schema-v2 in-process mux-byte progress;
- recorder/ffmpeg stay present through measurement and no encoder or poller lifecycle/final-flush error is logged;
- finalized media passes structural probe and full decode;
- recorder plus ffmpeg private memory does not meet all three sustained-growth conditions: last-versus-first 20%-window median growth ≥64 MiB, growth ≥20%, and least-squares slope ≥0.5 MiB/s.

A finite run cannot prove mathematically unbounded growth; the last rule is an explicit repeatable proxy. Saturation seconds, proportions, median/p95/max, raw/normalized CPU, RAM/VRAM, process GPU/I/O, output throughput, and poller/error diagnostics are reported whether gates pass or fail.

Non-gating resource increases are flagged as substantial when median system/League CPU rises by at least 2 percentage points, median GPU 3D rises by at least 5 points, median system/dedicated GPU memory rises by at least 256 MiB, or League private memory rises by at least 128 MiB. These checks apply to both capped and uncapped comparisons. An uncapped frame delta reaching the corresponding capped frame-budget boundary is also flagged as substantial for disposition, but remains diagnostic and never becomes an uncapped gate. Each flag requires an explicit optimization disposition in canonical evidence.

## Analyze and publish

```powershell
python -m unittest discover -s tools/capture_benchmark/tests -v

python tools/capture_benchmark/analyze.py `
  --input build/perf/qb-perf-002 `
  --target-encoder nvenc `
  --output-json docs/performance/results/qb-perf-002-nvenc.json `
  --output-markdown docs/performance/results/qb-perf-002-nvenc.md
```

Exit status is 0 for `pass` (with nested uncapped status `diagnostic`), 1 for a valid gated `fail`, and 2 for `invalid`. A failed or invalid `QB-PERF-002` report is preserved as evidence but cannot complete the feature; fix or disposition the concrete issue and collect a fresh pair when required.
