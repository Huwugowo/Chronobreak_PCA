# QB-REPLAY-008 — Reproducible Replay Performance Baseline

Status: complete as of 2026-08-26; implementation, generated corpus, observer-control, supported production-WebView matrix, sanitized report, canonical result dispositions, and final non-GUI closure verification all passed.

## Purpose

Create a repeatable, auditable baseline for the real Chronobreak desktop application and replay path before changing playback architecture, seek policy, library loading, recorder media, or export strategy.

The user-visible outcome is not an optimization by itself. It is a trustworthy answer to: how long does the app and a replay take to become useful; how well does the current WebView play, seek, scrub, and change rate; what I/O does that cause; how stable are app and WebView resources; and where does current export spend time? Later replay work must use this evidence to select concrete changes and to prove it did not make the experience or resource envelope worse.

## Relevant current architecture

- `app/src/App.tsx` starts games, clips, storage, settings, HEVC, and Data Dragon resources and owns screen navigation. Current cold startup waits for the main library resources rather than exposing a benchmarkable useful-first milestone.
- `app/src/api.ts` is the frontend/Tauri boundary. Browser mock mode supplies semantic data but no media URL, so it cannot validate real decoding, seeking, buffering, byte ranges, or WebView resource behavior.
- `app/src/types.ts::PlaybackProbe` is one monolithic replay DTO containing the video URL, summary, participants, events, and player/KDA timelines.
- `app/src/components/ViewerScreen.tsx::PlaybackSurface` owns one persistent `<video preload="auto">`. It prefers `requestVideoFrameCallback`, keeps media time viewer-local, serializes authoritative seeks with at most one active request plus the latest pending request, limits dispatch to 10 Hz, uses a half-frame dedupe tolerance, and has finite timeout/recovery/degraded behavior.
- `PlaybackSurface::seekTo` optimistically updates the visible time before a native seek completes. The same signal later receives `seeked`, current-time, or presented-frame values. Benchmark evidence must distinguish requested preview time, dispatched seek time, `seeked`, and the first authoritative presented frame instead of treating this signal as one universal clock.
- `app/src/viewerUtils.ts` contains pure timeline lookup and frame-alignment helpers. It has no performance/scenario harness or exact rational media-time model.
- `app/src-tauri/src/library.rs::playback_probe` synchronously reads metadata and the complete game log, derives roster/timelines, sorts events, and only then returns the video URL. Cold app startup also scans games repeatedly through games, clips, and storage queries; clip duration can invoke one synchronous ffprobe per clip. These are candidates to measure, not authorization to redesign yet.
- `app/src-tauri/src/playback_server.rs` provides the correct WebView-first foundation: loopback-only GET/HEAD and single byte ranges streamed through Tokio without loading whole files. Its five atomic counters are process-lifetime totals shared by game video, clips, music, the HEVC probe, and Data Dragon; they cannot attribute one benchmark scenario.
- `app/src-tauri/src/clip_export.rs` stages outputs and preserves sources, but every shipped preset filters and re-encodes video/audio. It exposes total elapsed time and bytes, not strategy, actual encoder, attempts, per-stage time, fallback reason, real-time factor, or post-export validation.
- `app/package.json` has type-check/build scripts but no frontend unit-test or replay-benchmark command.
- `docs/development/VERIFICATION.md` is the project verification authority. It has a manual replay checklist but no reproducible replay protocol.
- `QB-REPLAY-006`, `QB-LIB-008`, `QB-CLIP-007`, and `QB-DIST-001` are complete dependencies. `feature-list.json` is the canonical work state.

## Scope / non-goals

In scope:

- a sentinel-only benchmark mode in the production Tauri/WebView application;
- a versioned manifest, structured frontend/Tauri event stream, Windows app/WebView process-tree collector, deterministic analyzer, fixtures, and sanitized reports;
- safe preparation and validation of short, representative, and long replay fixtures;
- cold/warm app and replay opening, play/pause, all required playback-rate capabilities, seeks, scrubbing, event jumps, clip endpoint edits, fullscreen transitions, repeated open/close, and bounded torture scenarios;
- route/session-attributable replay-server evidence;
- current full-reencode export timing and media-validation evidence sufficient to refine `QB-CLIP-004` later;
- explicit dispositions for measured problems and limitations.

Out of scope:

- changing the 100 ms seek interval, preload policy, playback controller, library query structure, media server security policy, recorder GOP/fragment/timestamp behavior, or export encoding strategy;
- implementing thumbnails, ReplayIndex, separate audio stems, stream copy/hybrid export, smart cut, active-session segmentation, or a new playback backend;
- setting product performance budgets from intuition or from a single first run;
- automating or controlling League;
- reading, deleting, rewriting, truncating, or exporting from a real user recording library.

## Exploration findings

- The WebView-first foundation is already sensible: one persistent native video element, native decode/buffering, byte-range delivery, presented-frame callbacks, and bounded seek recovery. Replacement is not a default conclusion.
- Current diagnostics are useful for live debugging but not evidence. They retain only 50 string events, one latest seek latency, one-second snapshots, and process-global server totals. The rAF fallback's displayed “FPS” counts animation callbacks, not necessarily presented video frames.
- A seek has at least four meaningful times: user request/preview, native dispatch, `seeked`, and first presented frame from the current generation. Request-to-presented latency and target error are the user-facing measures; dispatch-to-`seeked` alone is insufficient.
- Current server totals are polluted by unrelated routes and cannot relate requests to a scrub burst. Scenario start/end snapshots help, but bounded route/request timing evidence is needed to explain cancellation, bytes, and active streams.
- App-only memory and JavaScript heap miss WebView2 child processes and GPU/disk cost. A Windows external sampler must follow the app's descendant process tree while frontend events provide high-resolution media timing.
- The benchmark must run an optimized production Tauri executable. Browser preview/mock mode cannot substitute for the Windows WebView, media server, codec stack, or process topology.
- No representative recording is committed. Large/raw media should remain ignored. The protocol therefore needs versioned, sentinel-owned fixture preparation and identity manifests rather than embedding personal recordings in the repository.
- There is no accepted cross-process policy for background thumbnail/index/export work while the separate recorder is active. The benchmark can measure controlled concurrency later, but this feature must not invent or depend on the rolled-back QB-CAP-002 recording-state/classification machinery.
- Current source facts differ by backend. The native path normally produces H.264 CFR, two-second IDRs, no B-frames, AAC, and fragmented MP4; the external path and HEVC must be probed rather than inferred from encoder metadata.
- Current export always re-encodes. Baseline reports may identify requests that appear compatible, but must label copy and hybrid strategies `not implemented` rather than simulate their performance.

## Chosen design

### 1. Sentinel-only benchmark mode

Add a narrowly activated desktop mode selected by an exact command-line form:

```text
league-replay-app.exe --replay-benchmark-manifest <absolute-manifest-path>
```

Normal startup remains unchanged. Benchmark startup first parses and validates a versioned manifest. Record that harness-initialization interval separately and exclude it from product cold-start milestones. The manifest, dedicated library, benchmark-scoped config/app-data root, result root, and scratch/export root must all resolve below one explicit root containing a repository-defined sentinel such as `.chronobreak-replay-benchmark`. Reject relative, missing, broad, user-configured, symlink/reparse-escaping, or non-sentinel roots.

After validation, benchmark mode enters the same production initialization pipeline with a normal `Config` redirected to the benchmark-scoped config, app-data, media library, clips, retention, runtime, music, playback-server, and resource state. It does not load or write the user's configuration or roots, and it disables user-visible destructive mutations after initialization, but it does not skip production startup work merely to improve the result. Run Data Dragon in a predeclared cached or offline condition with a fixed cache fingerprint and report that condition; do not leave uncontrolled network variance inside the matrix. Label the result `isolated-production-path cold process`, not cold OS file cache. The runner never cleans the benchmark root automatically and creates a new immutable run directory that must not already exist.

Expose only the commands needed to read the validated benchmark session, emit bounded structured events, snapshot route metrics, record terminal results, and request a clean benchmark exit. The scenario must traverse the production `App`/library/viewer components and persistent video element; it may use an explicitly named benchmark hook to drive the existing closures, but it must not render a synthetic alternate player.

### 2. Fixture corpus and identity

Create `tools/replay_benchmark/prepare.ps1` and a versioned input-manifest schema. Preparation copies only explicitly supplied or newly generated inputs into the sentinel library. It does not hard-link, symlink, move, or modify source files. Preflight records SHA-256, size, timestamps, bundle ID, metadata/log schema facts, and packaged-runtime ffprobe facts. Full-file hashing and decode happen before a declared cooldown and outside every timed trial; `cold` never claims an empty OS page cache. Each immutable launch currently records out-of-window post hashes, and the matrix verifier requires the final launch to carry exact full-corpus post-hash evidence within that launch's lifetime. A mismatch invalidates the matrix. Because those per-launch reads may warm the OS cache, accepted evidence must retain that limitation and must not claim a cold OS page cache.

The accepted baseline uses at least:

- a short 2–5 minute changing H.264/AAC recording with visual timecode and audio impulses;
- a representative 20–35 minute current canonical recording/bundle;
- a long recording of at least 45 minutes with a bounded but event-heavy game log;
- a current external-backend H.264 recording while that backend remains supported;
- supported HEVC coverage for open/play/seek/rate behavior;
- explicit malformed/truncated/missing-stream copies only in isolated negative fixtures.

Fresh recorder outputs may be generated with existing sentinel-owned native/external fixture tools. Real user recordings are not benchmark inputs unless the user separately authorizes a non-destructive copy; completion evidence should prefer dedicated generated/current canonical fixtures. Each accepted report uses sanitized aliases rather than paths, summoner names, or local identifiers.

All positive inputs pass ffprobe and full single-thread decode once before the matrix. The visual timecode/audio impulses provide seek, thumbnail-future, duration, and A/V checks without relying on League content.

### 3. Structured replay events

Add a schema-versioned event model with monotonic timestamps and a stable run/scenario/media/generation/action identity. Events cover:

- process/app start, library requested/useful, replay requested, payload ready, viewer mounted;
- `loadstart`, metadata/data/can-play readiness, first authoritative presented frame, steady playback, pause, end, media error;
- requested preview, pending replacement, dedupe, dispatch, native `seeking`, `seeked`, first current-generation presented frame, timeout, recovery, degraded state, and settle;
- rate requested/applied, observed effective media-time rate, audio state, frame counts/drops, and rapid transitions;
- fullscreen/layout transitions and repeated viewer disposal/reopen;
- export probe/start/attempt/fallback/progress/thumbnail/validation/finalization/complete/failure.

Keep requested/preview time separate from authoritative presented media time. For every dispatched seek, record target, source/target distance class, request/dispatch/seeked/presented monotonic times, presented media time, target error, generation, reason, and whether a newer request superseded it. A seek is not user-visibly settled until a frame from the current generation is presented or a documented fallback limitation prevents that observation.

Production mode retains only its current cheap diagnostics unless a small metric is independently useful. Benchmark-only event buffers are bounded and streamed/appended outside the per-frame hot path. Do not serialize a growing full event vector on each frame or range chunk.

Every bounded event queue/ring exposes capacity, high-water mark, dropped/overwritten count, and terminal reconciliation. Any lost event needed for action or request accounting invalidates the trial. Add a minimal-observer profile that records only scenario/action boundaries, native seek completion, first presented frame, video-quality snapshots, and terminal state. Interleave it with the full profile in dedicated observer-control arms so the harness measures its own cost before the baseline is accepted.

Add Vitest as a locked frontend development dependency and an `npm run test` command. Unit tests cover event reduction, generation matching, percentile inputs, scenario scripts, bounded buffers, and sanitization. Actual HTML media behavior remains a real WebView check.

### 4. Replay-server attribution

Preserve the current streamed single-range response and cheap production atomics. Add benchmark-session route classes and bounded request lifecycle evidence containing request sequence, method, route class, status, requested byte interval, declared/delivered bytes, start/first-byte/complete times, completed/cancelled/error outcome, and active/peak stream counts.

Do not attach unbounded URL/path strings or per-chunk event records. The benchmark exposes scenario start/end snapshots and a bounded append-only request stream; the analyzer correlates by monotonic time windows because the browser does not supply a custom seek header. HEVC, music, Data Dragon, clip, and game-video traffic remain separate route classes.

### 5. Windows process-tree collector

Add `tools/replay_benchmark/run.ps1`. It validates the manifest/sentinel/runtime/app binary, launches exactly one production desktop process with a hidden console where applicable, follows its descendant WebView2 process tree, and samples at a documented one-second cadence:

- process and whole-system CPU;
- private bytes and working set;
- process I/O bytes/operations;
- handles, threads, process appearance/disappearance;
- adapter and attributable process-tree GPU engines/memory where Windows exposes reliable counters;
- collection gaps and process-tree changes.

The app emits high-resolution UI/media events; the external collector does not busy-poll to approximate them. Record OS, WebView2, CPU/GPU/display, media-runtime ID, app binary hash/revision/dirty state, logical processor count, sampler version, and optional-counter limitations. Reuse the capture benchmark's documented CPU normalization and GPU-engine identity semantics where applicable, but keep replay schemas and validity rules separate so capture reports remain backward compatible.

Before the main matrix, run four interleaved 60-second minimal-observer/full-observer pairs against the same representative media and deterministic seek/scrub actions. Warm production playback normally, then remain paused between repeated control seeks so uncontrolled continuous decode/read-ahead does not dominate CPU and byte repeatability. Briefly resume for each seek until its authoritative presented frame arrives, then pause again; the full baseline matrix separately covers continuous playing seeks. Full instrumentation is valid only when it preserves exact action/seek, settled request/range outcome class, cancellation route/method class, error, and recovery outcomes; loses zero required events; changes median seek-action latency by no more than 5% and seek-action p95 by no more than 10%; adds no more than 16 MiB maximum process-tree private memory; and changes aggregate process-tree CPU time by no more than the larger of 2% or one observed Windows accounting quantum per process. Requested/completed range boundaries, partial delivery, raw request/completed/range/cancellation/delivered-byte counts, and dropped-frame rate are schedule-sensitive evidence: compare their arm medians with the baseline repeatability rule `max(resolution floor, 3 × minimal MAD)` and fail only when an unfavorable increase exceeds both that band and 5%. Count/byte floors are the larger of one unit or 1% of the minimal median; dropped-frame rate uses the coarsest one-frame resolution observed across the paired trials. Exact request semantics retain route, method, status, ranged-response class, and terminal outcome. These control thresholds validate the observer rather than defining product budgets. If this gate fails, reduce/redesign instrumentation and repeat fresh control pairs before collecting the baseline.

The runner never launches, terminates, or configures League. It may terminate only the benchmark app process it created after the app reports terminal completion or an explicit finite timeout. It preserves failed run roots.

### 6. Scenario matrix

Use deterministic seeded targets and preserve individual trials. The initial protocol includes:

1. App idle: five independent new-process trials to useful library UI, then a 60-second idle window. Exclude manifest validation/harness setup and label the result isolated-production-path process cold, not cold OS cache.
2. Cold replay open: five independent new-process trials to replay request, payload, metadata, first frame, and playback ready for each positive media class.
3. Warm replay open: five clean viewer disposal/reopen trials per positive media class within an already initialized process.
4. Play/pause: five 30-second steady 1x windows per positive media class with a fixed five-second warmup and deterministic pause/resume transitions.
5. Rate capability: one five-second warmup plus a 30-second measured window at each of `0.25x`, `0.5x`, `1x`, `2x`, `4x`, and `8x`, followed by five rapid `0.25x ↔ 8x` transition cycles on representative H.264 and supported HEVC media. Benchmark mode may set the native video rate directly before QB-REPLAY-009 exists, but must report this as capability evidence rather than shipped UI.
6. Seeking: at least 40 valid observations in each near/far and forward/backward stratum on the representative and long recordings, plus repeated event jumps and clip endpoint edits. Record request-to-seeked and request-to-presented distributions separately; a percentile is eligible only with at least 40 observations in its exact reported stratum.
7. Rapid scrub: five deterministic fixed-rate pointer-equivalent request bursts per relevant media/layout followed by release/settle. Record requests, actual native seeks, ranges, delivered bytes, cancellations, and settle latency.
8. Layout: enter/exit fullscreen repeatedly while playing, paused, seeking, and in clip mode; prove the decoder node/generation remains continuous.
9. Lifecycle: 20 repeated open/close cycles across different recordings and a 10-minute long-recording torture loop; report memory/resource windows and slopes without declaring a product leak budget before evidence.
10. Export: three independent trials for each current horizontal/vertical/Discord request class covering music/no music, unity/non-unity gain, keyframe-aligned/nonaligned endpoints, and Discord near-limit retry. Every output remains in the sentinel clips root and passes structural/full-decode/source-hash checks outside the timed encode interval.

The constants above plus rate-arm duration, warmup, input order, cooldown, and random seed are recorded in the manifest. Use balanced/interleaved order where conditions are compared. A failed or invalid trial is preserved and replaced only by a fresh complete trial; it is never overwritten or selectively omitted.

### 7. Analyzer and reports

Add `tools/replay_benchmark/analyze.py` with standard-library-first deterministic analysis and `tools/replay_benchmark/tests/` fixtures. It validates schema, sentinel-relative artifact identities, app/media/runtime/environment fingerprints, expected scenario/trial completeness, monotonic clocks, event-generation reconciliation, media integrity, collection coverage, and terminal status.

Invalid conditions include missing actions/trials, wrong binary/media identity, timestamp regression, impossible generation ordering, unaccounted process disappearance, excessive telemetry gaps, media error/recovery exhaustion, source-hash change, invalid export, or incomplete terminal evidence. Optional Windows GPU fields remain explicit limitations rather than automatic invalidation when the core process/UI/server data is complete.

Reports retain every individual result and compute documented type-7 percentile/median summaries for:

- open milestones;
- request-to-dispatch, dispatch-to-seeked, request-to-seeked, and request-to-first-presented latency;
- target error and settle latency by reason/distance/media/rate;
- requested/coalesced/deduped/actual seek ratios;
- presented/dropped frames and effective media-time rate;
- route requests/ranges/bytes/cancellations/active streams;
- process-tree and system CPU/memory/GPU/I/O/resource windows;
- export stage times, attempts, real-time factor, output facts, and bytes.

The analyzer also publishes a versioned comparison contract for downstream before/after work. For a metric, define the baseline repeatability band as `max(resolution_floor, 3 × MAD)` over eligible independent baseline trials, with documented floors of 5 ms for latency, 0.1 percentage point for normalized CPU, 8 MiB for memory, and 1% for byte/count-like metrics. A downstream delta is disposition-requiring only when it exceeds both that absolute band and 5% in the unfavorable direction; any new media error, timeout, recovery exhaustion, stale result, source mutation, event loss, or sustained monotonic resource growth is disposition-requiring regardless of numeric noise. Use at least five balanced before and five after trials with identical fingerprints and at least 40 action observations for reported p95/p99 strata. These rules identify evidence that needs review; they are not user-facing product budgets or automatic proof that a change is good enough.

Write raw results under ignored `build/perf/replay/`. Accepted summaries go under `docs/performance/results/` as path-sanitized JSON and Markdown. They contain no absolute paths, usernames, summoner/player names, capability tokens, imported-music paths, raw command lines, or personal recording identifiers.

The first report establishes observations, natural run variation, and the comparison contract above. It does not invent user-facing pass/fail latency or resource budgets. For each disposition-requiring or contradictory result, update `feature-list.json` with one of: existing child owns it; create a focused child; gather more evidence; accepted limitation with rationale; or no material problem. This plan already identifies likely owners (`QB-REPLAY-009` through `QB-REPLAY-014`, `QB-REPLAY-003`, `QB-AUDIO-001`, `QB-CLIP-004`) but results may close or narrow them.

### Rejected alternatives

- Replace WebView playback immediately: rejected because the current native-video/range/RVFC design is sound and no reproducible failure selects MSE, libmpv, WebCodecs, or native playback.
- Refactor into the playback controller while adding instrumentation: rejected for this feature because it would erase the pre-change behavior being measured. Add the smallest benchmark seam first; `QB-REPLAY-009` performs the controller extraction afterward.
- Commit large representative recordings: rejected because of repository size, privacy, and recoverability. Commit schemas/generators/sanitized reports; keep media and raw telemetry in sentinel-owned ignored roots.
- Use browser preview or synthetic unit timing as the baseline: rejected because it lacks real media URLs, WebView2, loopback ranges, codec behavior, and process topology.
- Reuse the capture analyzer/report schema directly: rejected because the subject, processes, timings, and validity model differ. Reuse documented counter semantics where useful without breaking historical capture evidence.
- Reset process-global counters between actions: rejected as a production coordination primitive. Use session/route snapshots and bounded benchmark request evidence; production counters remain monotonic.

## Milestones

1. Add benchmark manifest/event/result schemas, sentinel/root validation, CLI parsing, benchmark-only app state, and tests proving normal startup cannot activate benchmark behavior accidentally.
2. Add safe fixture preparation, packaged-runtime media inspection, source hashing, deterministic synthetic bundle/log generation, and short/representative/long corpus documentation.
3. Add structured viewer/app/export instrumentation around the existing behavior without refactoring the playback controller or changing seek/rate/preload/media policy.
4. Add session/route server metrics and bounded request lifecycle evidence while preserving the streamed single-range contract and production overhead.
5. Add the Windows process-tree collector, preflight, run orchestration, finite timeouts, failure preservation, and environment/runtime identity.
6. Add deterministic analyzer, sanitization, report generation, invalidity/resource/event fixtures, Vitest setup, and exact verification commands.
7. Run automated checks and preflight; generate fresh sentinel-owned canonical media and validate the complete corpus.
8. Run the full production-WebView scenario matrix, preserve raw roots, analyze results, rerun only fresh complete invalid trials, and commit accepted sanitized summaries.
9. Record every material finding's canonical disposition, update replay/performance verification documentation and restart pointers, and complete the feature only after all acceptance criteria and applicable project checks pass.

## Verification

Primary commands expected after implementation:

```powershell
python -m unittest discover -s tools/replay_benchmark/tests -v
python tools/replay_benchmark/analyze.py --help
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
npm run desktop:build --prefix app
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <sentinel-benchmark-manifest> -PreflightOnly
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <sentinel-benchmark-manifest>
```

Run all other applicable commands from `docs/development/VERIFICATION.md`, the canonical feature-list validation block, packaged-runtime verification needed by media/export scenarios, and `git diff --check`.

Acceptance mapping:

- manifest/root/runtime/media validation and source before/after hashes prove corpus identity and source safety;
- structured app/viewer events prove open, rate, seek, scrub, recovery, fullscreen, and lifecycle behavior;
- route-scoped request evidence proves random-read and cancellation cost without whole-file loading;
- Windows process-tree telemetry plus WebView metrics prove the measured resource envelope and observer limitations;
- export events, ffprobe/full decode, size/duration/A-V facts, and source hashes prove the current export baseline;
- analyzer fixtures and rejected invalid runs prove reports cannot silently accept incomplete or mismatched evidence;
- committed sanitized summaries plus feature dispositions prove measurement was used without inventing unsupported optimization claims.

## Performance / reliability

The benchmark must not become the performance problem it measures. Normal production mode retains bounded atomics and existing lightweight UI diagnostics. High-volume event/request evidence exists only in explicit benchmark mode, uses fixed capacities or streaming append, performs no full-vector clone per frame/request, and never blocks `requestVideoFrameCallback`, media events, server body streaming, or an async runtime worker on filesystem/process work. Minimal/full observer pairs, the predeclared overhead gate, and zero-loss accounting are mandatory validity checks, not optional diagnostics.

The external sampler uses a stable documented cadence and identical logic across compared trials. Its process-tree membership, gaps, and overhead are recorded. It never injects into the WebView or game, and it does not infer presented frames from OS process counters.

All destructive/corruption/export tests use the sentinel library and scratch roots. Normal user configuration, recordings, clips, settings, caches, credentials, and media paths are outside the accepted root and unreachable from benchmark mutations. Source files are copied, identity-checked before launch, opened read-only by the replay path, and checked out of the timed observation window; the final launch's exact corpus hash set is the matrix-final integrity proof. Failed/partial evidence is preserved rather than cleaned automatically.

Benchmark startup and exit are finite. A media, scenario, collector, or app timeout produces an invalid/failed result with the exact last events and process state; it must not be converted into a pass or hidden by a later trial. Missing optional GPU counters are reported truthfully. A first baseline does not authorize performance claims, playback-backend removal, or recorder changes.

## Progress

- [x] Resolve the broad request to `EPIC-REPLAY` and decompose it into bounded canonical children.
- [x] Audit the replay/app/loopback/export architecture and archived advanced replay/recorder documents.
- [x] Challenge scope, time domains, fixture safety, process attribution, derived-artifact lifecycle, security, and verification gaps.
- [x] Save this self-contained ExecPlan and link it from `feature-list.json`.
- [x] Implement benchmark mode, schemas, safety gates, structured events, and tests.
- [x] Implement fixture preparation and current-media validation.
- [x] Implement route attribution, Windows collector, matrix planner/verifier, analyzer, and reports.
- [x] Pass automated and applicable project verification; build the production executable and verify the packaged media runtime.
- [x] Build, prepare, fully decode, and identity-bind the dedicated short/representative/long/external/HEVC generated corpus.
- [x] Run a bounded production-WebView cold-open rehearsal and have the runner, analyzer, and immutable matrix verifier accept it.
- [x] Isolate benchmark WebView2 state below the sentinel root, add bounded frontend-startup evidence, and verify the packaged production custom-protocol executable.
- [x] Define and test semantic observer equivalence separately from schedule-sensitive request/frame repeatability.
- [x] Run and pass four fresh interleaved minimal/full observer-control pairs.
- [x] Run and validate the complete supported production-WebView baseline matrix; preserve unsupported capability outcomes rather than retrying them into passes.
- [x] Commit the sanitized aggregate report and record canonical dispositions for measured opening, playback/rate/resource, decoder-proof, and export findings.
- [x] Pass final current-tree non-GUI project verification, record exact results, and reconcile canonical state and restart pointers.

## Deviations / surprises

- The Windows collector creates the app suspended, associates a completion port with a kill-on-close Job Object, assigns the app, and resumes it only after assignment. Exact `NEW_PROCESS`/exit notifications prove membership for helpers that can start and stop between samples; Toolhelp/.NET/PInvoke snapshots provide live identities/counters, while cumulative Job accounting retains exited-member CPU and I/O. The first polling-only rehearsals correctly exposed ten short-lived WebView helpers that snapshots alone could not identify.
- Windows GPU performance-counter CIM calls are optional and are not run inside the one-second core sampler. A rehearsal observed an otherwise successful live query block for 5.18 seconds despite a fast prelaunch probe, and the provider offers no runner-enforced finite deadline. Accepted runs therefore carry an explicit GPU-unavailable limitation until a separately bounded collector exists; core Job/process/system CPU, memory, I/O, identity, and cadence evidence remain mandatory.
- The generated corpus uses explicit, previously generated QueueBack media only. Native graphic fixtures declared AAC streams with zero packets, so the builder preserves their H.264 video packets and adds deterministic five-second AAC impulses; the external fixture preserves its real AAC stream, the long fixture uses finite concat stream copy, and HEVC is a labelled NVENC-derived capability fixture. These derived fixtures are scaling/capability evidence, not independent backend or hardware validation.
- The production binary initially lacked its sibling `resources/media-runtime`; runner preflight now requires the packaged runtime ID and exact ffmpeg/ffprobe hashes to match preparation. Open scenarios also computed but ignored `warmup_seconds`; they now keep real playback active for that bounded warmup before the stable-playback assertion so one-second resource telemetry can be complete.
- Frontend and Rust writers are independent monotonic producers. The analyzer validates every envelope and then stable-sorts cross-producer frontend/server records by the shared monotonic authority; it still rejects append-time regression from the single process-sample producer.
- `requestVideoFrameCallback` is the only authoritative presented-frame source. The animation-frame fallback is explicitly labelled non-authoritative, and the analyzer rejects it for scenarios that require presented-frame proof.
- One prepared manifest is expanded by `matrix.py` into immutable, single-process launch manifests. The verifier proves exact expansion, declared order/cooldown, successful result identities, and final full-corpus post-hash coverage; it intentionally does not launch the app or wait for the operator.
- The runner currently writes out-of-window post hashes for each launch instead of hashing only once after the complete matrix. The final launch is bound as the matrix-final proof, but earlier reads can warm the OS cache. This must remain an explicit baseline limitation unless the live rehearsal shows the orchestration should be revised before acceptance.
- Export scenarios drive the production backend command directly and report `full_reencode`; copy/hybrid and UI-included export timing remain explicitly unimplemented rather than simulated.
- A sandbox-restricted desktop token made WebView2 helpers exit before frontend initialization with `SBOX_ERROR_CANNOT_CREATE_RESTRICTED_TOKEN`/Windows error 87. Live WebView evidence therefore runs through the normal approved desktop token; runner preflight and all non-GUI analysis remain sandboxed. Failed profiles and result roots were preserved.
- Raw `cargo build --release` produced a release-optimized Tauri development-URL executable because it did not enable Tauri's `custom-protocol` feature. Accepted GUI evidence uses `npm run desktop:build --prefix app`; page-load telemetry must show the packaged `http://tauri.localhost/` origin rather than the development server URL.
- Tauri's default window used the normal application WebView2 profile before setup could redirect benchmark state. Benchmark mode now removes only the generated default window from the runtime context and recreates the same configured window with an absolute sentinel-owned `webview2-user-data` directory; normal startup is unchanged.
- Early observer-control runs exposed multiple invalidity classes. One run contained a manual navigation seek and play/pause interaction and was preserved as operator-contaminated evidence. Untouched diagnostics then showed cancellation ranges/counts, dropped-frame snapshots, and even the completed tail-range start can vary with WebView scheduling while the ranged response class and all deterministic actions match. The analyzer now hashes exact semantic outcome classes while gating raw schedule-sensitive counts/rates with the predeclared repeatability rule; all accepted pairs must be freshly collected after that rule was fixed.
- The first complete four-pair semantic control (`r9`) passed matrix identity, order, cooldown, integrity, loss, semantic, CPU, memory, p95, request-count, range-count, cancellation-count, completion-count, and dropped-frame gates, but failed the mixed-action median-latency and delivered-byte gates. Full observation emitted only one additional frontend event per trial (`media_loadeddata`). The latency failure came from combining two sub-3 ms play/pause actions with only three bimodal 250–415 ms seeks, which made the mixed median select a different distribution rank even though the full-observer seek-only median was lower. Observer latency now scopes explicitly to seek actions, and the replacement workload contains repeated deterministic seeks to stabilize both latency and byte evidence; thresholds remain unchanged.
- The strengthened 40-seek `r10` control stabilized semantics, seek latency, delivered bytes, memory, requests, ranges, cancellations, completions, and dropped frames, but failed CPU because continuous playing read-ahead produced 45–78% one-core-equivalent process-tree CPU variance across otherwise valid trials. The full profile still emitted only one extra event. The next fresh control warms playback and then pauses during its identical 40-seek sequence; this isolates observer/seek work from uncontrolled continuous decoder work without weakening the CPU gate or replacing the playing-seek baseline scenarios.
- The first paused prototype (`r11`) correctly failed rather than accepting native `seeked` as presentation: WebView2 did not issue an authoritative frame callback while permanently paused. The replacement remains paused between actions but briefly plays each requested seek through its current-generation presented frame before pausing again.
- The pulse-seek `r12` prototype proved authoritative presentation and tightly balanced CPU/bytes, but pair 2 completed three same-class HTTP ranges in one arm and one in the other while all 40 actions and request outcome classes matched. Settled-range multiplicity is therefore schedule-sensitive like cancellation multiplicity. The semantic digest now retains the unique route/method/status/ranged-response/outcome classes, while total/completed/error/range/cancellation/byte gates retain amplification and failure sensitivity.
- The clean semantic `r13` matrix still failed CPU and dropped-frame gates while the full profile retained only one extra event. WebView scheduling is focus/occlusion-sensitive, and reading progress in another application can itself change these outcomes. Initial focus guards then showed Tauri/WebView can blur automatically 0.5 seconds after scenario start while the document remains visible, so focus loss alone is diagnostic rather than invalidating. The benchmark-only window is always-on-top to prevent occlusion, document hiding/minimization remains invalidating, and normal windows remain unchanged.
- The fresh always-on-top `r16` matrix first passed all eleven observer gates, but the first warm-open driver rehearsal then exposed component-local action IDs and media generations restarting across viewer remounts; the analyzer correctly rejected duplicate `pause-2` request identities. Action and media identities now live in the session-scoped observer, and a regression test proves uniqueness across remounts. Because that changed the app binary, `r16` was superseded rather than reused.
- The authoritative corrected-binary `r18` matrix completed all eight immutable launches in declared order with the required cooldown, unchanged corpus hashes, zero required-event/request loss, no focus/hidden/not-interactive diagnostics, exact observer outcomes, and accepted per-run analyzers. Its aggregate four-pair gate passed all eleven checks. Full-observer CPU time was 111,250 ms versus 118,156.25 ms minimal, maximum private memory was 492,806,144 bytes versus 495,816,704 bytes minimal, median seek latency was 300.15 ms in both arms, and p95 was 461.575 ms versus 442.94 ms, inside the predeclared 10% gate. Full instrumentation is valid for the remount-corrected baseline binary; `r17` is retained as earlier passing evidence but is not the final authority.
- The first long cold-open matrix arm exposed collector shutdown overhead dominated by managed process-module enumeration after the app had already exited. The runner now uses Toolhelp/direct PInvoke, observes root exit before the final snapshot, writes an authoritative `collection-result.json`, and the analyzer rejects startup/collection timeout, forced termination, mismatched post hashes, incomplete Job accounting, or collector finalization above 15 seconds. `collector-shutdown-rehearsal-20260825-r1` completed without forced termination and finalized in 995.18 ms.
- The complete accepted baseline is the explicit union of sequences 1–49 from `baseline-current-tree-20260825-r2`, sequences 1–3 from `baseline-current-tree-post-rate-20260826-r3`, and sequences 1–15 from `baseline-current-tree-final-continuation-20260826-r1`: 67 valid immutable production-WebView runs/trials. The final-continuation matrix passed `verify --require-results` 15/15, and the aggregate analyzer accepted all 67 roots. Sanitized `report.json`, `report.md`, and the canonical interpretation are committed under `docs/performance/evidence/qb-replay-008-baseline-20260826-r1`.
- The representative H.264 rate run is preserved as a real capability failure, not discarded evidence. Effective advancement tracked 0.25× through 4×, but setting 8× yielded about 0.00296× effective advancement and the following transition failed because media no longer advanced. `QB-REPLAY-009` owns the required speed-ladder behavior and truthful unsupported/degraded handling.
- The production WebView HEVC probe recorded `supported: false`; the prepared HEVC fixture remains valid capability input, but positive HEVC arms were omitted under the predeclared supported-only rule. Optional GPU CIM counters also remained unavailable because they cannot be bounded inside the core cadence. The baseline therefore does not use generic GPU activity as decoder-path proof; explicit decoder-path validation remains a `QB-REPLAY-009` obligation before normal-path performance claims.
- Three 2026-08-26 desktop launches made through a restricted tool token failed before frontend startup because Chromium's GPU process exited with Windows access denied (`0xC0000022`). The exact binary launched normally through the desktop token. Those roots remain diagnostic environment failures and do not replace accepted product-path trials.
- One application-valid endpoint-edit result exposed an analyzer attribution defect: automatic `clip-loop` seeks were counted as planned `endpoint-edit` seeks. Requested-seek validation now filters by the scenario's `seek_reason`, a regression test covers auxiliary clip-loop seeks, the preserved result passes the corrected analyzer, and a clean replacement endpoint arm is included in the 67-run report.

## Decision log

- 2026-08-24: Preserve WebView-first playback as the measured baseline; no alternative backend is selected without a reproducible demonstrated failure.
- 2026-08-24: Add instrumentation before extracting the playback controller so pre-change behavior remains measurable.
- 2026-08-24: Treat requested preview, native seek dispatch, `seeked`, and first presented frame as distinct events and timestamp authorities.
- 2026-08-24: Use explicit sentinel-owned copied/generated media and ignored raw outputs; never benchmark destructively against a user library.
- 2026-08-24: Attribute resources to the app plus descendant WebView processes, not the Rust host or JavaScript heap alone.
- 2026-08-24: Reuse capture-counter semantics where applicable but keep replay schemas and historical capture reports independent.
- 2026-08-24: Do not set product latency/resource gates from the first baseline. Preserve individual results and create evidence-backed dispositions.
- 2026-08-24: Keep local-media security, preview artifacts, canonical time, ReplayIndex, audio stems, fast export, and recorder-media experiments as separate planned features.
- 2026-08-24: Measure manifest/harness setup separately, then run the normal initialization pipeline against benchmark-scoped config/app-data/library roots; call this isolated-production-path process cold rather than cold OS cache.
- 2026-08-24: Keep full-file hashing outside timed observation windows and require a final exact-corpus integrity proof; the implemented per-launch post-hash deviation and its cache limitation are recorded above.
- 2026-08-24: Require interleaved minimal/full observer-control arms, zero-loss accounting, and an explicit overhead validity gate before accepting full-instrumentation results.
- 2026-08-24: Define deterministic repeatability/disposition rules and minimum repetitions/action counts without treating them as user-facing performance budgets.
- 2026-08-24: Complete QB-REPLAY-012's canonical time contract before QB-REPLAY-009 extracts a typed playback boundary.
- 2026-08-25: Launch the production process suspended and place it in a non-breakaway, kill-on-close Windows Job Object before any app or WebView child can escape process accounting.
- 2026-08-25: Treat only current-generation `requestVideoFrameCallback` evidence as an authoritative presented frame; preserve fallback observations as limitations, never as accepted seek/open proof.
- 2026-08-25: Expand one reviewed template into immutable one-trial manifests and verify result order, cooldown, identity, success, and final corpus integrity separately from interactive execution.
- 2026-08-25: Allow repeatable analyzer inputs so minimal/full arms and other explicit immutable result roots can be aggregated without copying or relinking evidence.
- 2026-08-25: Use Job completion-port creation/exit notifications as the exact membership authority and retain one-second snapshots for live resource identity; never convert an unobserved member into an assumed zero-cost process.
- 2026-08-25: Disable optional in-loop GPU CIM collection after a live 5.18-second provider stall proved that a fast prelaunch probe cannot enforce cadence; record the limitation instead of risking core telemetry.
- 2026-08-25: Require the packaged media runtime beside the exact production app binary, with runtime/tool identities matching preparation, before preflight can pass.
- 2026-08-25: Keep `QB-REPLAY-008` in progress after the accepted one-trial rehearsal because the four-pair observer-control gate, complete ordered scenario/corpus matrix, accepted aggregate reports, and canonical result dispositions are still outstanding.
- 2026-08-25: Give benchmark WebView2 a sentinel-owned profile before window creation and require an immediate backend frontend-session request or flushed frontend initialization within a distinct 30-second startup watchdog.
- 2026-08-25: Build accepted desktop evidence through the package's Tauri production command, not a raw Cargo release build whose feature set can retain the development URL.
- 2026-08-25: Treat request route/method/status/ranged-response/terminal-outcome classes plus deterministic action, error, and recovery facts as exact observer semantics; retain requested/completed range boundaries, cancellation scheduling, request/completed/range/byte amplification, and dropped frames as raw evidence governed by repeatability gates rather than exact hashes.
- 2026-08-25: Scope the seek observer-control latency gate to seek actions rather than a mixed play/pause/seek order statistic, and increase deterministic seek repetitions in the replacement control instead of relaxing latency or byte thresholds after `r9` failed.
- 2026-08-25: After `r10` passed every gate except CPU amid large continuous-decode variance, warm playback normally and pause the observer-control seek sequence; retain playing seek coverage in the main matrix and keep the CPU threshold unchanged.
- 2026-08-25: Treat same-class completed-range multiplicity as schedule-sensitive after `r12` produced identical actions with one versus three completed ranges; preserve exact outcome classes and add an explicit request-error count gate rather than discarding request failure sensitivity.
- 2026-08-25: Keep focus/visibility evidence after CPU/drop variance remained unexplained by the one-event profile delta, but do not treat automatic visible-window blur as failure. Use a benchmark-only always-on-top window, invalidate document hiding/minimization, retain blur diagnostics, and leave normal window behavior unchanged.
- 2026-08-25: Accept the full observer profile for the baseline only after a fresh remount-corrected four-pair matrix passed exact outcomes, zero-loss accounting, latency, memory, CPU, request/completion/error/range/cancellation/byte, and dropped-frame gates; preserve `r3` through `r17` as superseded, invalid, diagnostic, or earlier-passing evidence, with `r18` the final authority.
- 2026-08-26: Accept the 67 valid supported-path trials as the current-tree matrix and retain the separate 8× stall as the truthful rate-capability outcome. A required unsupported capability is a disposition, not a reason to loop the same GUI run indefinitely.
- 2026-08-26: Omit positive HEVC arms after the production WebView probe reported unsupported. Keep the validated derived fixture and capability record; do not fabricate coverage or infer support from ffmpeg decode alone.
- 2026-08-26: Assign the post-payload media-readiness/opening result to `QB-REPLAY-010`, the 8× stall, transition-heavy resource trend, and explicit hardware-decoder-path proof to `QB-REPLAY-009`, and measured full-reencode cost to `QB-CLIP-004`. Keep the byte-range WebView foundation until a later before/after comparison demonstrates a replacement-worthy failure.
- 2026-08-26: Require a bounded `collection-result.json` and reject forced/slow/incomplete collector shutdown independently of the app terminal record. Collector finalization is evidence validity, not product latency.

## Completion

Complete only when the versioned harness and analyzer are implemented, automated/project checks pass, the full current-tree production-WebView matrix has valid short/representative/long evidence, every positive media/export output passes required integrity checks, source hashes remain unchanged, accepted sanitized reports are committed, every substantial result has a canonical disposition, architecture/verification/state are current, and no known issue contradicts the recorded baseline.

The harness, analyzer, planner, release executable, dedicated generated corpus, authoritative `r18` four-pair observer control, supported-path production-WebView matrix, capability dispositions, and sanitized 67-run report are complete as of 2026-08-26. All accepted trials used exact app binary SHA-256 `5d07f9ec6282eb05cba6efe9a0fbd34a665262f76029d5da2956e6be3c23c873`, WebView2 `151.0.4129.101`, and packaged media runtime `queueback-ffmpeg-8.1.2-windows-x86_64-r6`; every required event/request/Job/source-integrity gate passed. The first baseline remains characterization evidence rather than a product budget.

Final current-tree closure passed on 2026-08-26: 72 replay-benchmark tests, 60 capture-benchmark tests, 46 Tauri Rust tests, 16 frontend tests, 125 recorder library tests plus 3 recorder binary tests, 10 media-runtime unit tests, and the ignored real packaged-runtime test all passed. Recorder/app/media-runtime formatting and strict Clippy, recorder check and release build, frontend check/build, Tauri production desktop build, analyzer CLI, runtime preparation/atomicity/verify/smoke, portable release staging/verification, 60-item canonical roadmap validation, and diff hygiene also passed. The local PowerShell policy rejected the `npm.ps1` shim, so the identical npm scripts were invoked successfully through `npm.cmd`; this caused no worktree change. Final inspection found no unresolved acceptance criterion or sensitive local data in the sanitized report. `QB-REPLAY-008` is complete.
