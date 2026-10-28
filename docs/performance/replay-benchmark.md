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
11. `library_search`: settled imported history, then eight typing/no-match/clear sequences per process by default (`iterations: 1..100`). Typing uses the declared fixture's champion, one input every 60ms through the real search field and existing debounce. No-match and clear use the same production query path. Wait for indexing and its final refresh query to finish before measured input. Record lengths and action/query-generation identities without query text. Report last-input-to-paint, frontend-observed backend-call duration and admission-to-paint separately for each stratum; reconcile coalesced input, queued replacement, completed superseded queries and admitted results. Two animation frames with current generation/origin revalidation establish a UI paint opportunity, not physical screen presentation. Fewer than 40 observations per stratum remain descriptive; a single process cannot establish across-process repeatability.
12. `desktop_resize`: eight alternating native window sizes while playing, then eight while paused, at least 250ms apart and without overlapping unsettled actions. Sizes are physical pixels derived from display scale, app minimum and current monitor work area. Require a matching native resize event/geometry and the same video element/load; playing settlement requires a current-generation RVFC observation, paused settlement requires two revalidated animation frames. Report geometry and presentation/paint latencies separately. Native resizing is enabled only by the benchmark feature and active session, without expanding production window permissions. Drain an issued request before restoring original size on success, failure or cancellation. Missing evidence, changed display/load or failed restoration invalidates the run.
13. `whole_app`: a frozen two-game library with completed normal import and populated offline assets; eight actual typing/no-match/clear search sequences, open the selected replay, verify ten rendered participants/events and supported image decode at a populated timestamp, then 30 seconds steady 1x playback, 40 completed seeks per near/far × forward/backward stratum, five eight-request scrub bursts, eight playing/eight paused native resizes, and close back to the two-game library. All actions retain the same media generation. Report steady-window frame deltas separately from combined totals that include intentional seeks/resizes. One process is descriptive, with no before/after claim. Synthetic moving media must be labelled and cannot establish gameplay decode cost or event-picture alignment.

The `library_search` scenario ID `cold-index-search` is a separate fresh-index diagnostic, fixed at one typing/no-match/clear sequence and bounded to 120 seconds (110-second action deadline). It starts after the accepted filesystem scan, before settled-index readiness, and records actual indexing-state/count changes through normal import completion. Queries queued before initial catalog admission preserve the latest input/action; searches can run during subsequent recording imports. Replaced queued queries and completed-but-superseded results are distinct terminals; query errors still invalidate the diagnostic. Its import-state completion evidence replaces the settled `library_useful` prerequisite, while normal search retains that prerequisite. If import finishes too quickly to overlap, report that fact without inflating the library. Fresh app/index state is not cold OS-cache evidence.

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

## Replay-opening contract v1

`qb010-opening-v1-*` scenarios declare `replay-opening-v1`. They support H.264
`cold_open`, six-cycle `warm_open`, and `seek` with `observer_control: true`.
Unknown opening versions or malformed workloads fail closed. Historical scenarios
keep their existing readiness and compatibility-duration-drain semantics.

For this contract, `opening_library_usable` follows two animation frames and
revalidation of the current Games navigation, library origin and selected available
recording. The candidate gate is its admitted actionable snapshot; the reference
gate is its actual eager games/clips/usage/settings readiness. Optional clip-duration,
Data Dragon and diagnostic HEVC results do not delay this H.264 entry. The normal
HEVC capability check still runs and its eventual result is retained.

Each selected intent starts a numbered cycle with a declared fixture ID. Bounded
frontend milestones are `opening_requested`, candidate-only
`opening_descriptor_requested` / `opening_descriptor_ready`,
`opening_media_open_requested`, `opening_media_open_bound`, and
`opening_details_ready`. The bound event records the controller's actual generation
after `open`; mount generations and predictions cannot substitute. Existing
loadstart, metadata, canplay, authoritative RVFC, play/pause/steady-playback and
cycle-terminal records carry the same contract/cycle/fixture and load generation.
Repeated canplay is allowed within its cycle; its first occurrence supplies latency.
No per-frame stream is added.

The candidate can start real playback before optional full details finish, in
either legal details/first-frame order. Every positive cycle still requires one
successful details admission within the readiness deadline. Failure, disposal or
stale delivery cannot produce a positive terminal. The reference retains its real
full-probe-before-media dependency. Its minimal three-file observation patch and
parity tests are under [`tools/replay_benchmark/reference/opening`](../../tools/replay_benchmark/reference/opening/README.md).

`opening_acceptance.py` consumes the existing fully validated raw bundles. It
requires a frozen `opening-campaign.schema.json` specification with explicit roles,
source revisions/patch hashes, binary hashes, declared-input fingerprints,
configuration hashes, exact ordered manifests and all expected matrix slots.
Each subject includes a hashed successful build receipt binding `source_revision`,
`source_patch_sha256` and `binary_sha256`; the runner verifies that binary for each
raw run. Launch manifests carry no synthetic analyzer fingerprint fields. The
generic runner's `source` metadata identifies the runner checkout; it is not the
separately built application's source identity. Source/clock/fixture/cycle/generation mismatch, missing or duplicate
required events and non-authoritative presentation invalidate acceptance.
All artifact/result paths must remain relative, contained and reparse-free beneath
the campaign directory. Declared-input fingerprints bind the manifest's scenario,
fixture, packaged runtime and exact configuration before collection. Full measured
subject/comparison fingerprints also bind observed WebView/device/collector facts;
they must agree across corresponding runs after observation. A prelaunch spec does
not predict a measured accounting quantum or fabricate observed environment data.

All opening latencies subtract frontend timestamps only: library request to Games
paint, and selected intent to media-open, loadstart, metadata, canplay, first RVFC
and full details. Candidate descriptor intervals are separate decomposition.
Native processing and collector/Job counters remain in the generic reports with
their own clock authority. The existing generic report schema remains version 1;
the additional `opening-report.schema.json` is version 1 of this separate contract.

QB-REPLAY-010 freezes five S/R/L/N/E cells for both cold and warm opening, five
independent processes per role/cell/kind: 100 main launches. Each warm process
retains its initial mount and takes the median of its five remounts before the
five-process comparison. Both roles first require four minimal/full observer
control pairs on R: 16 control launches. The runner owns frozen order, adjacent
pairs, alternating leaders, cooldown, receipts and stop-on-first-invalid behavior;
the acceptance tool does not select or replace runs.

```powershell
python tools/replay_benchmark/opening_acceptance.py `
  --campaign C:\bench\.chronobreak-replay-benchmark\opening-campaign.json `
  --output-dir C:\bench\.chronobreak-replay-benchmark\opening-report
```

The output retains per-cycle values, independent process medians, all 70 shared
comparisons and the generic resource/control reports. Missing primary cells cannot
disappear silently. N/E Games improvement must exceed both 5% and
`max(5 ms, 3 * reference MAD)` for cold and warm; other material unfavorable
latencies require disposition. Five process values do not support p95/p99 claims.
A valid report or passing latency gate does not close resource or normal-build
native acceptance; those dispositions remain explicit feature-completion gates.

## Server and Windows evidence

Normal production playback keeps its existing cheap atomics. Benchmark mode additionally records one bounded lifecycle item per completed/cancelled request, not per chunk: route class, method/status/range, declared/delivered bytes, start/first-byte/complete monotonic time, outcome, and active/peak streams. Game, clip, music, HEVC probe, and Data Dragon routes remain distinct.

The external collector creates the production app suspended, associates an I/O completion port with a non-breakaway, kill-on-close Windows Job Object, assigns the app, then resumes it. Exact `NEW_PROCESS` and exit notifications cover members—including short-lived WebView helpers—that can exist entirely between samples. One-second Toolhelp/direct-PInvoke snapshots supply live identities and counters without managed `Process.MainModule` enumeration, while cumulative Job counters retain CPU and I/O from exited members. The collector observes root exit before taking its final snapshot, then has a separate finite finalization window. Job total-process accounting must exactly match creation notifications, forced termination and collector timeout are invalid, post-hash identity must match the app terminal evidence, and collector finalization may not exceed 15 seconds. Core samples include per-process and aggregate CPU time, private/working memory, I/O bytes/operations, handles, threads, process appearance/disappearance, whole-system CPU, cadence gaps, and process-tree identity.

GPU engine/memory evidence is optional. Windows' in-process GPU performance-counter CIM provider has no runner-enforced finite deadline and has demonstrated multi-second stalls after a fast prelaunch query, so it is disabled in the core one-second sampler and every sample carries that explicit limitation. Missing GPU data is never emitted as zero. A future GPU collector must be independently bounded before it can join accepted full-observer evidence.

Matched processes must begin with identical application state. Set `isolate_launch_roots: true` in a matrix specification to derive separate app-data and scratch children for every process; prepare each child with the same fingerprinted offline `ddragon` cache before preflight. Start other app-data empty, including the capability marker and WebView profile, and reject unexpected prelaunch contents. The planner only declares these paths; the launch procedure must verify and retain the initial-state receipt rather than infer empty state from the option. All six warm cycles within one process retain that process's state. A shared app-data root lets the first control populate the HEVC capability cache and changes later controls' request classes; this correctly fails semantic equality. The omitted/default option preserves existing matrix expansion.

When an approved comparison explicitly requires a normally settled index, declare
that condition for both roles before collecting controls. After normal indexing
finishes, use SQLite's backup API to freeze one complete, library-root-bound
snapshot per role; never copy a live database/WAL pair or share a mutable database
between processes. Seed identical copies into each isolated launch, retaining the
database hash, catalog/root identity, integrity check and every recording's
`ready,facts_current=1` admission in an initial-state receipt. Verify the exact
prelaunch tree and hashes against that receipt immediately before preflight;
allow only the declared offline assets and index snapshot, with empty scratch,
capability and WebView state. The two roles must match fixture, schema and admitted
index facts; root-only database/config differences remain explicit. This is a
declared settled-index condition, not empty-cache opening evidence or permission
to extend readiness deadlines. Unexpected or partially imported state invalidates
the launch. A cache-only failure remains retained separately.

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

Observer outcome comparison uses `attributed-seek-pair-v1`. Explicitly declared
`observer_control: true` trials receive this contract; other trials retain their
existing analysis and cannot establish observer-control validity. Reports retain the
original `outcome_sha256` (including raw arrival order) and add a separately
versioned `semantic_outcome_sha256`. Only adjacent native `seeked` and authoritative
RVFC `seek_presented` observations for the same requested action may commute in
the semantic behavior sequence; unrelated behavior and action order stay fixed.
Each completed seek requires exactly one request, dispatch, native completion and
presentation, with dispatch before both observations. Action ID, generation,
producer source, fixture, cycle, measurement contract, reason and any media/source
epoch attribution must agree. Generic completion aliases do not receive this
ordering exception. Requested and dispatched targets remain comparison
identities; both completion targets must match dispatch. Missing, invalid,
undecoded or ambiguous fixture cadence fails closed. Verified fixture cadence
also bounds observed native/presented positions to the production controller's
one-frame plus one-microsecond tolerance. Within that existing qualification,
observed positions and latency remain measurements rather than deterministic
identities. Missing, duplicate, stale, non-authoritative, off-target or
misattributed evidence cannot become equal by sorting. Request source/action/
generation classes, errors, recoveries, loss checks and every overhead threshold
above remain enforced. Raw records are never reordered or rewritten.

Old report hashes cannot establish this stronger contract: reanalyze their
preserved raw bundles into new output directories. Reports without the exact
semantic contract/hash are ineligible for observer-control comparison; no fallback
to the original digest is permitted. Reanalysis does not repair inconsistent
source identities, failed collection, or a failed overhead gate.

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

Preflight reserves the atomic terminal temporary filename and rejects a result path requiring 260 or more characters, before any app launch. Use shorter result-root/run names to remain within Windows PowerShell 5.1 path limits. Preflight validates schema/root/runtime/binary/fixture identities, including the runtime manifest and exact ffmpeg/ffprobe hashes beneath `resources/media-runtime` beside the production app binary, but correctly reports production WebView, process sampling, post-hash, export decode, and terminal checks as deferred. A live run writes `manifest.json`, `events.jsonl`, `server_requests.jsonl`, `process_samples.jsonl`, `terminal.app.json`, finalized `terminal.json`, `collection-result.json`, environment/observer facts, Job completion notifications, post-hashes, and sanitized analyzer output under the immutable result root. `collection-result.json` is required analyzer evidence for finite startup, observation, shutdown, finalization, and source-integrity accounting; a terminal app record alone cannot turn an incomplete collector into a valid run.

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

### Bounded settled-memory diagnostic

The named `settled-memory` lifecycle scenario supplements the combined workflow's active-resource measurements with comparable library windows. Declare exactly six iterations and omit `duration_seconds`. Use the same populated telemetry, moving media and supported offline assets, a completed normal index snapshot, and a fresh release process/WebView profile. Before opening and after each cycle, require the mounted two-game library with its stable admitted page, empty search input and no video element. Wait five seconds, then sample ten seconds. Each replay cycle seeks to the populated timestamp, verifies roster/timeline/image decoding, and performs two seconds of authoritative advancing 1x playback before releasing media and returning.

Frontend `memory_window_started/completed` markers are flushed. The runner labels its own process samples with the latest observed open window's cycle; it never compares frontend timestamps to runner timestamps. Analysis requires all seven ordered complete windows and six rendered playback cycles, discards two labelled samples at each window edge, and requires at least five remaining samples per window. Report private/working memory and handle medians/ranges, comparing post-first-playback windows with later cycles separately from initial warmup.

This is one bounded same-process retention observation, not a soak, a zero-growth budget or a proof against every leak. It does not turn the earlier combined run's startup-to-peak difference into retained memory or establish retention for an arbitrarily large catalog. A continuing material accumulation requires disposition; an observed plateau does not justify speculative memory refactoring.
