# Replay and application benchmark protocol

This protocol is the evidence gate for `QB-REPLAY-008` and for later replay optimizations. It measures the optimized production Tauri/WebView application, its persistent native video element, the loopback range server, descendant WebView2 processes, and the current clip exporter. Browser preview, synthetic JavaScript timing, Cargo launches, and pre-existing build output do not substitute for a production run.

The first accepted report characterizes the current tree. It does not establish product latency budgets or justify replacing WebView playback, changing recorder GOP/fragment policy, persisting a ReplayIndex, or selecting an export strategy. Its sanitized evidence and canonical interpretation are retained under [`evidence/qb-replay-008-baseline-20260826-r1`](evidence/qb-replay-008-baseline-20260826-r1/README.md).

## Safety model

Every run is local-first and sentinel-owned:

- `sentinel_root` is an absolute directory whose leaf name is exactly `.chronobreak-replay-benchmark`;
- the manifest, library, config, app-data, results, scratch, Data Dragon cache, prepared fixtures, and exports resolve strictly below that root;
- roots and fixture files must be reparse-point-free and may not be a volume, profile, repository, or other broad directory;
- preparation copies explicitly selected files and directories; it never moves, links, modifies, or deletes the source;
- the app uses the normal initialization and retention code against the redirected library, with retention set to `0`, and never reads the user's app config or app-data roots;
- benchmark IPC disables save/delete/settings/cleanup/folder-opening/imported-file actions. Export may use only sentinel recordings and built-in/no music;
- the runner creates one new result root, never cleans it, starts exactly one production app process, and may terminate only that process and its descendants after a finite timeout;
- raw media and telemetry remain ignored beneath the sentinel root. Only analyzer-sanitized summaries may be committed.

Do not stage a real recording library. A user recording may be used only after the user explicitly authorizes a non-destructive copy into the sentinel root.

## Versioned launch contract

Schema v1 uses one immutable manifest for one production process, one scenario, and one trial. Repetitions and cold trials use separate manifests/result roots; they are never appended to or selectively replaced.

The exact activation form is:

```text
league-replay-app.exe --replay-benchmark-manifest C:\absolute\path\manifest.json
```

Any other argument form is rejected or starts normal production mode without benchmark privileges. The manifest declares `schema_version: 1`, a stable `run_id`, all scoped roots, `observer_profile` (`minimal` or `full`), fixed offline Data Dragon state, prepared fixtures with IDs/aliases/game IDs and hashes, and exactly one scenario with `id`, `trial_id`, `kind`, and `fixture_ids`.

`tools/replay_benchmark/schemas/manifest-v1.schema.json` and `prepare-v1.schema.json` are the machine-readable contracts. The examples beside them are templates, not accepted evidence.

The app records manifest parsing/validation as `harness_initialization_ms`, separate from product milestones. Product monotonic time begins at process entry and includes scoped config loading, retention, media-runtime resolution, built-in music setup, playback-server startup, and frontend/library work. “Cold” means a new isolated production-path process; it does not claim an empty Windows file cache.

## Corpus

An accepted baseline contains dedicated, identity-bound positive fixtures:

- short H.264/AAC: 2–5 minutes, continuously changing picture and deterministic audio/time signals;
- representative H.264/AAC: 20–35 minutes from the current canonical recorder path;
- long H.264/AAC: at least 45 minutes with an event-heavy bounded game log;
- current external-backend H.264 while that backend remains supported;
- HEVC coverage for open, play, seek, and rate capability only when the production WebView capability probe reports support. Preserve an unsupported capability result and omit positive HEVC arms rather than fabricating support.

Malformed, truncated, or missing-stream copies are negative fixtures and run only in isolated negative scenarios. No positive fixture is accepted until packaged `ffprobe` succeeds and packaged `ffmpeg` completes a full single-thread decode. Preparation records file SHA-256, size, write time, media facts, runtime identity, and copy receipt. Each launch validates prepared size/write-time identities without reading complete media before the timed observation and writes post hashes after the observation. The matrix verifier binds the final launch's exact full-corpus post hashes as the matrix-final integrity proof. These per-launch post-hash reads can warm the OS cache, so reports must retain that limitation and must never describe a trial as cold OS-cache evidence.

Prepare a fixture set from a reviewed spec:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/prepare.ps1 `
  -Spec C:\bench\prepare.json `
  -Manifest C:\bench\.chronobreak-replay-benchmark\manifests\run.json `
  -MediaRuntimeRoot (Resolve-Path build\media-runtime\windows-x86_64).Path `
  -PreflightOnly

powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/prepare.ps1 `
  -Spec C:\bench\prepare.json `
  -Manifest C:\bench\.chronobreak-replay-benchmark\manifests\run.json `
  -MediaRuntimeRoot (Resolve-Path build\media-runtime\windows-x86_64).Path
```

Preflight hashes explicit sources and validates destinations without copying. Preparation refuses an existing destination or manifest.

Build complete, anonymized bundles from reviewed generated recorder outputs when the source evidence contains media but not QueueBack metadata/game logs:

```powershell
python tools/replay_benchmark/build_corpus.py `
  --spec C:\bench\generated-corpus.json `
  --media-runtime-root C:\bench\media-runtime `
  --preflight-only

python tools/replay_benchmark/build_corpus.py `
  --spec C:\bench\generated-corpus.json `
  --media-runtime-root C:\bench\media-runtime
```

The corpus builder accepts only explicit media, opens sources read-only, verifies source hashes before and after construction, refuses an existing output, and generates bounded anonymized descriptive logs. `copy` preserves a generated recorder output with real AAC packets; `remux_audio` preserves native video packets while adding deterministic five-second AAC impulses when a graphics-only recorder fixture declared an empty audio track; `repeat` derives long scaling coverage through a finite concat-demuxer list, fragmented video stream copy, and the same deterministic audio; and `hevc` derives capability coverage with the packaged NVIDIA HEVC encoder. Derived fixtures are labelled as such and do not substitute for a genuinely independent long recorder run or hardware/backend validation. `prepare.ps1` remains the authority that copies the resulting bundles into the benchmark library and performs packaged full decode.

## Scenario matrix

Use deterministic seeds and retain individual trials. Fix target order, warmup, duration, action count, request rate, display state, input order, and cooldown in the manifest before the first accepted run.

1. `app_idle`: five new-process trials to useful library UI, then 60 seconds idle.
2. `cold_open`: five new-process trials per positive media class through payload, metadata, first authoritative presented frame, declared active-playback warmup, stable playback, and ready state.
3. `warm_open`: one initialized process, one initial mount, and five measured viewer disposal/remount cycles (`iterations: 6`); each cycle keeps real playback active for the declared warmup before its stable-playback assertion.
4. `play_pause`: five trials per media class, five-second warmup, 30-second steady 1x window, deterministic pause/resume.
5. `rate`: five-second warmup and 30 seconds each at 0.25x, 0.5x, 1x, 2x, 4x, and 8x, followed by five rapid 0.25x/8x transitions on representative H.264 and supported HEVC.
6. `seek`: at least 40 valid observations in each near/far × forward/backward stratum on representative and long fixtures. Separate manifests may set `seek_reason` to `event-jump` or `endpoint-edit`.
7. `scrub`: five deterministic bursts per relevant media/layout at the declared request rate, followed by authoritative presented-frame settle.
8. `layout`: repeated fullscreen enter/exit while playing, paused, and seeking; generation continuity proves that the decoder element was not replaced.
9. `lifecycle`: 20 open/close cycles. A separate `lifecycle` manifest with `iterations: 1` and `duration_seconds: 600` runs the bounded torture loop.
10. `export`: three independent manifests per request class covering horizontal/vertical/Discord, no/built-in music, unity/non-unity game gain, aligned/nonaligned endpoints, and Discord retry pressure.

A failed or invalid trial remains preserved. Replace it only with a complete fresh manifest/result root. Never omit it from operator notes or overwrite it.

## Event and time authority

Frontend records use the app session's monotonic clock offset. Requested preview time is never treated as authoritative media time. A seek action has one action ID and records, in order:

```text
seek_requested -> optional pending replacement/dedupe -> seek_dispatched
-> native seeking -> { seeked, first current-generation presented frame }
```

Native `seeked` and the first authoritative presented frame may arrive in either
order. Both must follow dispatch and both are required for a completed seek, as
already supported by the playback controller. The report retains
request-to-dispatch, request-to-`seeked`, request-to-presented, target error,
direction/distance class, reason, generation, and supersession. Timeout, recovery,
degraded state, media error, or stale-generation evidence invalidates the trial.

The full observer also records media readiness, native play/pause, applied/effective rate, fullscreen state, video quality, one-second diagnostics, export progress, and lifecycle. The minimal observer retains scenario/action boundaries, dispatch/native completion, the first authoritative frame, quality snapshots, and terminal state. Both frontend and Rust writers are bounded and expose capacity, high-water mark, and loss. Any required loss invalidates the trial.

## Server and Windows evidence

Normal production playback keeps its existing cheap atomics. Benchmark mode additionally records one bounded lifecycle item per completed/cancelled request, not per chunk: route class, method/status/range, declared/delivered bytes, start/first-byte/complete monotonic time, outcome, and active/peak streams. Game, clip, music, HEVC probe, and Data Dragon routes remain distinct.

The external collector creates the production app suspended, associates an I/O completion port with a non-breakaway, kill-on-close Windows Job Object, assigns the app, then resumes it. Exact `NEW_PROCESS` and exit notifications cover members—including short-lived WebView helpers—that can exist entirely between samples. One-second Toolhelp/direct-PInvoke snapshots supply live identities and counters without managed `Process.MainModule` enumeration, while cumulative Job counters retain CPU and I/O from exited members. The collector observes root exit before taking its final snapshot, then has a separate finite finalization window. Job total-process accounting must exactly match creation notifications, forced termination and collector timeout are invalid, post-hash identity must match the app terminal evidence, and collector finalization may not exceed 15 seconds. Core samples include per-process and aggregate CPU time, private/working memory, I/O bytes/operations, handles, threads, process appearance/disappearance, whole-system CPU, cadence gaps, and process-tree identity.

GPU engine/memory evidence is optional. Windows' in-process GPU performance-counter CIM provider has no runner-enforced finite deadline and has demonstrated multi-second stalls after a fast prelaunch query, so it is disabled in the core one-second sampler and every sample carries that explicit limitation. Missing GPU data is never emitted as zero. A future GPU collector must be independently bounded before it can join accepted full-observer evidence.

Before full-instrumentation baseline runs, collect four interleaved 60-second minimal/full observer pairs against the same representative fixture and deterministic actions. Warm normal playback first, then stay paused between repeated control seeks so continuous decoder read-ahead does not dominate observer CPU/byte repeatability. Each seek briefly resumes playback until its authoritative presented frame arrives and pauses again; playing-seek behavior remains covered by the baseline matrix itself. The observer-control analyzer gate requires:

- identical action/seek outcomes, settled request/range outcome classes, cancellation route/method classes, and error/recovery outcomes;
- raw request, completed-request, request-error, range, cancellation, and delivered-byte medians within `max(one unit or 1% of the minimal median, 3 × minimal MAD)`, with failure only when the increase also exceeds 5%;
- dropped-frame-rate medians within `max(one observed frame of resolution, 3 × minimal MAD)`, with failure only when the increase also exceeds 5%;
- zero required event/request loss;
- full-observer median seek-action latency increase no greater than 5%;
- seek-action p95 increase no greater than 10%;
- maximum process-tree private-memory increase no greater than 16 MiB;
- CPU-time increase no greater than the larger of 2% or one observed Windows accounting quantum per process.

Requested and completed range boundaries, cancelled partial delivery, byte/request counts, how many same-class ranges complete before cancellation, and frame-drop counts depend on WebView transport/decoder scheduling, so they remain first-class measured evidence but are not byte-for-byte semantic identity. Exact semantic identity retains the set of route, method, status, ranged-response, and terminal request-outcome classes plus deterministic action outcomes, errors, and recoveries. Separate count gates retain amplification/error sensitivity. The repeatability gates above detect material observer amplification without rejecting ordinary scheduling jitter. They are evidence-validity thresholds, not product performance budgets. If the control fails, reduce or redesign instrumentation and repeat four fresh pairs before collecting the baseline.

## Export evidence

The current strategy is explicitly `full_reencode`; copy and hybrid remain `not_implemented`. Each output records total time, encode time, thumbnail time, selected encoder, all failed/successful encoder attempts, retry count, output size, and progress stages. The runner validates every new sentinel clip with packaged `ffprobe` and a full decode, checks streams/duration and Discord's strict 10,000,000-byte limit, and confirms all source hashes after the trial.

## Run and analyze

Build the production executable first. Do not benchmark `cargo run` or a debug binary.

```powershell
npm run desktop:build --prefix app

powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 `
  -Manifest C:\bench\.chronobreak-replay-benchmark\manifests\run.json `
  -PreflightOnly

powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 `
  -Manifest C:\bench\.chronobreak-replay-benchmark\manifests\run.json
```

Preflight validates schema/root/runtime/binary/fixture identities, including the runtime manifest and exact ffmpeg/ffprobe hashes beneath `resources/media-runtime` beside the production app binary, but correctly reports production WebView, process sampling, post-hash, export decode, and terminal checks as deferred. A live run writes `manifest.json`, `events.jsonl`, `server_requests.jsonl`, `process_samples.jsonl`, `terminal.app.json`, finalized `terminal.json`, `collection-result.json`, environment/observer facts, Job completion notifications, post-hashes, and sanitized analyzer output under the immutable result root. `collection-result.json` is required analyzer evidence for finite startup, observation, shutdown, finalization, and source-integrity accounting; a terminal app record alone cannot turn an incomplete collector into a valid run.

Analyzer fixtures and direct use:

```powershell
python -m unittest discover -s tools/replay_benchmark/tests -v
python tools/replay_benchmark/analyze.py --input C:\absolute\run-root
```

Plan the complete matrix from one prepared template and a reviewed schema-v1 specification. Planning creates `matrix-plan.json` plus one immutable launch manifest per ordered trial and refuses an existing plan/result destination:

```powershell
python tools/replay_benchmark/matrix.py plan `
  --template C:\bench\.chronobreak-replay-benchmark\manifests\prepared-template.json `
  --spec C:\bench\.chronobreak-replay-benchmark\matrix-spec.json

python tools/replay_benchmark/matrix.py verify `
  --plan C:\bench\.chronobreak-replay-benchmark\matrix-plans\baseline\matrix-plan.json
```

The operator runs each generated launch manifest in declared order and observes the declared cooldown. The planner does not launch the app or sleep. After every immutable result exists, require the verifier to prove manifest/result identity, successful terminal evidence, start/completion order, cooldown, and final exact corpus integrity:

During every measured GUI scenario, do not click, type, switch applications, minimize QueueBack, or interact with its controls. The benchmark-only window remains always-on-top to prevent accidental occlusion. Focus loss is retained as required diagnostic evidence because Tauri/WebView launch can blur automatically; document hiding/minimization remains invalidating. Normal QueueBack windows are unchanged. This protects WebView frame, CPU, and dropped-frame measurements from operator/occlusion bias without rejecting automatic early blur.

```powershell
python tools/replay_benchmark/matrix.py verify `
  --plan C:\bench\.chronobreak-replay-benchmark\matrix-plans\baseline\matrix-plan.json `
  --require-results
```

Aggregate explicit immutable roots without copying evidence by repeating `--input`; multiple inputs require a separate output directory. Build the minimal observer report first, then pass it as the control for the matching full arm:

```powershell
python tools/replay_benchmark/analyze.py `
  --input C:\absolute\minimal-01 `
  --input C:\absolute\minimal-02 `
  --output-dir C:\absolute\reports\minimal

python tools/replay_benchmark/analyze.py `
  --input C:\absolute\full-01 `
  --input C:\absolute\full-02 `
  --output-dir C:\absolute\reports\full `
  --observer-control C:\absolute\reports\minimal\report.json
```

Only `requestVideoFrameCallback` events are authoritative presented-frame evidence. An animation-frame fallback may keep the UI observable, but it is marked non-authoritative and makes any authority-required benchmark arm invalid rather than silently weakening the protocol.

For a before/after comparison, provide at least five identity-matched trials per arm. P95/P99 comparisons require at least 40 observations in the exact stratum. The repeatability band is `max(resolution floor, 3 × baseline MAD)` with floors of 5 ms latency, 0.1 normalized CPU percentage point, 8 MiB memory, and 1% for byte/count metrics. An unfavorable change requires disposition only when it exceeds both the band and 5%; a new error, timeout, recovery, source mutation, event loss, or sustained-growth signal always requires disposition.

Reports preserve individual trials and sanitize paths/local identities. Missing optional GPU counters are a limitation, not fabricated zeroes. Missing core telemetry, incomplete terminal state, inconsistent fingerprints, malformed generation/action/request accounting, failed media validation, or changed source hashes makes a run invalid.

Every substantial accepted observation receives a canonical disposition in `feature-list.json`: an existing child owns it, a focused child is created, more evidence is required, or it is an accepted limitation with rationale. Absence of evidence is not an optimization result.
