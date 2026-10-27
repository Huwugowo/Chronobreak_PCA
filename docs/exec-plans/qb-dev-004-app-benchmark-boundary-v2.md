# QB-DEV-004 — App benchmark coordination boundary

Supersedes: `docs/exec-plans/qb-dev-004-app-benchmark-boundary.md`. This version changes verification proportionality only; the architecture, implementation scope and measurement contracts are unchanged.

## Purpose

Reduce the production application's exposure to benchmark scenario policy while preserving the existing valuable harness, actual production library/viewer paths, and measurement contracts. This is a meaningful first slice: remove application-level experiment selection, readiness policy, export workload interpretation, cycle sequencing, and their lifecycle resources from `App.tsx`. Preserve the viewer action executor for a separately verified migration.

## Relevant planning-time architecture

`App.tsx` creates the real `LibraryController` and `HistoryController`, admits library snapshots, selects a game with a fresh origin, and renders the real `ViewerScreen`. `openViewer` checks the current root/request/token/game selection; `openTab` additionally refreshes when returning from another screen. Viewer-cycle benchmark returns currently change the actual navigation signal without requesting that extra refresh. That distinction is part of the existing workload and must remain explicit rather than being silently changed during extraction.

`App.tsx` also contains benchmark-only observer/capability state, cancellation, flags and frame/timer handles; `runBenchmarkExport`; `beginBenchmarkScenario`; opening and historical library paint/duration readiness effects; and viewer-cycle handling. Their relevant source regions are approximately lines 117–165, 354–623, 656–837 and 863–976. Production HEVC probing/refetch/error display and ordinary asset polling must remain in App.

`ViewerScreen.tsx` owns one real `PlaybackController`/HTML video adapter, data controllers, layout and clip policy. Its benchmark executor uses those same objects and helpers for play, rates, seek, scrubbing, fullscreen, clip editing, native resize and render observation. It emits `viewer_cycle_completed` after the current media-release delay, then synchronously dispatches `REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT`. App listens on `window`, increments cycle counts, returns to Games, and schedules reopen or terminal completion. No acknowledgement is currently awaited.

Playback evidence distinguishes request, dispatch/dedupe, native seek completion and authoritative RVFC presentation, retaining generation/action/epoch identity. `benchmarkSeekEvidence.ts` accepts either arrival order of native completion and qualifying presentation. For ordinary scenarios the viewer waits for details and ready presented media; versioned opening workloads intentionally start media actions before full details but require details before successful completion. Preserve both contracts.

`benchmark.ts` owns the singleton session observer, bounded telemetry, event filtering, clock/action/media identity, workload builders and opening identity. `benchmarkSearch.ts` drives the actual search input and observes normal debounce/query admission and two-frame paint opportunity. `LibraryController.drainDurations` is a bounded historical compatibility operation through the existing eight-ID duration API, refusing competing active work. It is not a second library implementation.

Rust activation is already isolated behind the `replay-benchmark` Cargo feature and explicit absolute `--replay-benchmark-manifest` launch. Validated sentinel roots isolate config, library, results and WebView profile; network, retention and destructive user mutation are constrained. Backend telemetry observes the same playback server/export pipeline. Frontend benchmark definitions ship in the normal bundle and initialization asks Rust for a nullable session. Runtime/data isolation is strong; frontend source/bundle separation is incomplete. This slice addresses ownership, not a claim that harness code is absent from production artifacts.

## Scope and non-goals

Change App-level benchmark orchestration and the viewer-cycle notification boundary only. Preserve every existing scenario, fixture, timing/duration, counter, emitted event/payload, terminal failure, action identity and analyzer rule. No recorder changes, UI redesign, playback tuning, controller replacement, corpus alteration, alternate data path or analyzer relaxation. Do not reorganize Rust benchmark modules or introduce a build mode in this slice.

The viewer executor, production-path telemetry, search observation, decoder diagnostics and historical duration-drain seam remain. Extracting those responsibilities together would couple two substantial lifecycle migrations and obscure the source of ordering regressions. Document their remaining ownership and migration conditions rather than marking full frontend separation complete.

## Exploration findings

1. The avoidable dependency is policy ownership: App interprets scenario kinds/IDs, dictates repetitions and terminal semantics, and owns benchmark asynchronous resources. Merely moving functions that retain arbitrary access to component internals would not establish a boundary.
2. The global cycle CustomEvent can be replaced by an explicitly passed callback with the same synchronous notification semantics. It is control flow rather than instrumentation and does not need a global bus.
3. Readiness is experiment policy, but observations must stay attributable to the production state. Opening snapshot/paint and legacy full-duration readiness are different contracts. Root epoch, request, snapshot token, navigation identity, actionability and refresh state must be checked again after frames and asynchronous duration work.
4. Search/export are not all UI workflows. Search drives the real field; export scenarios deliberately call the production backend command and label `benchmark_scope: backend_command`, `ui_workflow_included: false`. Preserve that honest scope and source invalidation after an attempted export.
5. Full/minimal observer filtering and exact semantic outcomes must be preserved; WebView request boundaries/bytes and timing can vary and require the existing measured gates, not byte-for-byte transport assertions.

## Chosen design and rationale

Use a small benchmark-owned application coordinator plus a Solid binding, following the driving-adapter principle without adopting a framework.

### Ownership and interfaces

Add `app/src/benchmarkApplication.ts` (and a small binding file only if it improves readability). The coordinator accepts the active observer and an explicit application host, is created when the nullable observer resolves to a session (before library admission), and exposes `update()`, `viewerCycleCompleted()` and idempotent `dispose()`. Dispose/recreate it only when observer-session identity changes; ordinary host-state updates must not recreate it.

The host is a typed, bounded capability object. It supplies reads of the current library/history/navigation/capability/load state, current origin/admission checks, and the minimum existing production actions: open a selected replay; publish the existing Games navigation transition; read a replay descriptor under the captured origin; drain historical durations; perform the existing production export command; and invalidate its captured source root. Use existing types and focused `Pick` interfaces where appropriate. It must not expose signal setters, arbitrary component closures, a replacement controller, media element, raw IPC, or a generic service registry. The coordinator can import benchmark search/delivery helpers directly; those stay harness-owned.

Definitive host contracts (names may follow existing type conventions):

- `readState()` reads live accessors synchronously, including the existing `LibraryOrigin | null`, actual navigation value, library/history snapshots, loading/error and HEVC result. `captureContext()` retains only the origin/navigation fields relevant to that operation, comparing origin values rather than object references. Never treat unrelated settings changes as navigation invalidation.
- `isContextCurrent(context)` uses the existing production origin/admission authority plus navigation checks. `openViewer(gameTimestamp, context)` verifies that context immediately before calling App's existing `openViewer` unchanged. The coordinator has a separate private session identity; it checks both session currency and operation context before invoking the host.
- `publishBenchmarkGamesReturn(context)` verifies the current session/viewer context, then performs only the existing `setNavigation({ screen: "library", tab: "games" })`. It must not call `controller.navigate`, refresh, drain durations or directly request index work. Normal reactive effects from actually mounting Games remain intact. Capture the new context after this transition for any subsequent wait/reopen.
- `readReplayDescriptor(origin, gameTimestamp)` delegates to the same token-bound `controller.readReplay`; `drainDurations(ids, context)` delegates to the existing compatibility operation without a second queue. A resolved promise is not admission: recheck session and captured context before using its result or publishing readiness.
- Export preparation captures the existing origin/token and uses the same production export command/progress callback. Guard progress/terminal continuation by session currency, and require current source/navigation context for scenario continuation. Source-root invalidation after an attempted command remains a mandatory production cleanup obligation even if the coordinator becomes obsolete; it is not conditional on publication eligibility.

Every RAF stage, timer, search/duration/export continuation and cycle callback must recheck private session currency and its operation's captured host context before navigation or terminal output. An obsolete/disposed session produces no late output; an active session whose admitted origin/navigation changes retains the existing explicit failure phase rather than silently passing or waiting forever. Carry forward the existing rules for each operation; do not add a universal navigation lock that rejects expected cycle transitions.

Keep a short adapter in App adjacent to controller/navigation creation. It binds these capabilities to the existing live objects/functions. Name the Games-return capability as benchmark compatibility rather than merging it with `openTab`. This exposes the actual rendering/navigation owner; it must not manufacture a fake library-ready state. Retain the current difference between benchmark cycle returns and user `openTab` refresh, document it as a remaining compatibility coupling, and do not claim click-by-click UI equivalence for that transition. Assert no newly introduced refresh/controller-navigation/duration/index commands in its action trace, while retaining normal effects of mounting the real library.

Move `runBenchmarkExport`, scenario dispatch/parameter interpretation, benchmark readiness and cycle policy, benchmark flags, AbortController, timers and RAF handles into the coordinator. The coordinator never changes the host's production controller ownership. Production settings/library/HEVC/DDDragon initialization, subscriptions and cleanup stay with App. Feed capability results to the coordinator instead of moving the real probe or changing its error handling.

The Solid binding creates/disposes the coordinator in a synchronous owner and forwards reactive changes through one separate focused effect. The creation effect tracks only observer-session identity; creating the host must not accidentally subscribe that effect to library/navigation reads and restart the coordinator on every update. `update()` rereads live host state in its own tracking scope; do not cache a stale resource snapshot, accidentally suppress its tracked reads, or let asynchronous callbacks register Solid cleanup outside an owner. Capture session identity so an obsolete coordinator cannot publish or reopen after replacement/disposal. With no observer session, allocate no coordinator, timers, RAFs, abort listeners or search workloads; this does not mean waiting until library admission. Normal app observations remain inert as before.

### Typed viewer notification

Add an optional `onBenchmarkViewerCycleCompleted?: () => void` prop to the viewer, passed only for the active App coordinator. The closure captures that coordinator's identity and expected viewer/cycle context; it must not look up a replacement coordinator and accidentally credit an outgoing viewer to the new session. `viewerCycleCompleted()` ignores stale/disposed session or viewer identities. Replace the CustomEvent dispatch at its existing site with this callback: media release and its 100 ms delay complete, required `viewer_cycle_completed` is emitted, then the synchronous callback performs the App transition/unmount. Keep the current finished/disposal bookkeeping at its existing boundary; do not await a new promise. Test replacement/unmount between emission and callback. Remove the window listener and unused event constant after checking all consumers, including tests.

Do not move or modify the viewer action runner, playback controller, event sink, seek helpers, details admission or diagnostics subscription order. The callback transfers a completed cycle notification; it does not transfer player ownership. Disposal order must preserve completed-cycle admission before an expected unmount and still fail unexpected incomplete viewer disposal.

### Cancellation and failures

The coordinator owns its abort signal, pending timers and both RAF stages. `dispose()` first makes future work ineligible, aborts waits, cancels scheduled frames/timers and releases subscriptions it owns. Async export/duration operations retain existing backend/controller ownership; disposal must prevent subsequent coordinator navigation/terminal publication without pretending the backend command was cancelled. Preserve the current invalidation obligation after an attempted backend export. Do not add silent catches, retries, new timeout tolerances or fallback successful terminals.

Retain existing required failure payloads/phases and full/minimal filtering. Test late completion, root/navigation change, replacement and double disposal explicitly. If extraction reveals a pre-existing independent defect, record it separately; do not quietly rewrite semantics under this refactor.

## Rejected alternatives and planning decisions

- A wholesale App plus viewer executor extraction would entangle both lifecycle owners and require a much larger equivalence matrix. Stage the migrations.
- A viewer-only observer prop would reduce singleton discovery but leave most experiment execution inside screens. It is a possible later step, not the most valuable first separation here.
- A new global bus, parallel playback controller or benchmark video would weaken ownership and observation identity. Use direct typed notification and existing production capabilities.
- Passing all of App's setters/controllers to a helper would relocate coupling. Bound the host and assert that screens contain no App-level scenario interpretation after extraction.
- Dynamic imports or a separate frontend build could remove harness bytes, but would add loading/initialization timing and potentially create a different measured application. Preserve the current bundle/activation contract now; evaluate packaging separately with paired production measurements.
- Replacing cycle returns with `openTab` would introduce refresh/index activity into historical workloads. Preserve the existing real navigation transition explicitly and report its scope.

## Milestones

1. Freeze baseline and equivalence expectations before structural edits. First build/retain immutable baseline executables and collect the mandatory representative native workflow evidence below before any source or test edits; planning-only documentation changes are outside executable inputs and must be identified in the receipt. Record repository revision, dirty paths, hashes of executable inputs/executables, manifest/fixture/runtime identities, traces and analyzer reports under fresh ignored sentinel roots. Run existing frontend/benchmark/analyzer checks. Then add integration coverage for App scenarios and cycle callback wiring while the existing executor still exists, without changing production inputs. Representative runtime evidence is mandatory before M2; a full statistical campaign is required only under the escalation rules below. Prior reports or this plan do not establish the baseline.
2. Introduce the bounded host/coordinator, move App experiment state/policy/resources, bind it to App's live owner, and replace the cycle event with the optional callback. Preserve callback timing and no-session behavior. Run focused tests immediately. Keep this a coherent reviewable implementation commit.
3. Verify the completed dependency cone against the frozen baseline with mandatory behavioral tests and representative native runs, and apply the explicit statistical-campaign escalation rules below. Review the boundary and failure/cleanup cases, update durable architecture and canonical execution evidence. Do not expand into viewer executor extraction during this slice. Commit tests/docs with the implementation or as a coherent follow-up.

## Verification design

### Automated before and after

From the repository root:

```powershell
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
python -m unittest discover -s tools/replay_benchmark/tests -v
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo test --manifest-path app/src-tauri/Cargo.toml --features replay-benchmark
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets --features replay-benchmark -- -D warnings
npm run desktop:build --prefix app
npm run desktop:build:benchmark --prefix app
```

Use the focused frontend command during implementation: `npm run test --prefix app -- App.benchmark.test.tsx components/ViewerScreen.test.tsx benchmark.test.ts benchmarkSeekEvidence.test.ts benchmarkResize.test.ts benchmarkSearch.test.tsx benchmarkApplication.test.ts`. The final frontend suite also covers normal library navigation, settings returns, replay identity/data admission, fullscreen/clip interaction and disposal. No recorder code changes means recorder hardware/capture suites are outside this dependency cone.

Extend App/coordinator tests to freeze ordered emitted kinds/payloads/action IDs and host action traces for idle, opening (cold/warm), legacy duration readiness, cold-index/settled search, whole-app, export success/failure, media capability rejection, delivery failure, lifecycle and settled-memory. Assert historical defaults/clamps and cycle counts. Cover root/request/token/navigation invalidation between each RAF and after duration work, no-session side effects, repeated update, disposal during each wait and scheduled reopen, late export/duration results, and exactly one terminal outcome. Keep Viewer tests for ordinary versus versioned opening readiness ordering, failure visibility, exact media identity, fullscreen continuity and unexpected disposal. Test the actual callback connection, not only a mocked coordinator method.

Add a targeted dependency assertion: App no longer selects scenario kinds/IDs or owns benchmark timers; neither screen registers/dispatches the cycle window event; no coordinator imports a playback/video implementation. Do not weaken analyzer fixtures. Run canonical schema/invariant/linked-artifact validation from `VERIFICATION.md` and `git diff --check`.

### Mandatory representative native workflows

Use only existing generated corpus sources that are explicitly identified and hash-checked, a fresh sentinel-owned library/result/profile root, the locked packaged runtime and populated offline assets. Never launch against the user's configured media root. Build each baseline/candidate executable through the Tauri CLI and retain an immutable copy/hash before rebuilding the other role. Follow `prepare.ps1` preflight/preparation, `matrix.py plan/verify`, `run.ps1` preflight/run, and `analyze.py` from the replay benchmark protocol. Existing ignored manifests are inputs to inspect, not permission to overwrite a prior destination. Preserve all failed runs.

Run at least one complete, identity-matched baseline and candidate run of each representative H.264 workflow: cold open; six-cycle warm open; `layout` fullscreen transitions during playing/paused/seeking/clip mode; ordinary 20-cycle lifecycle; and `whole_app` (the frozen eight search iterations, populated replay rendering, 30-second advancing playback, 160 stratified seeks, five eight-request scrubs, playing/paused native resizes, library return). Exercise backend-command export as an accurately labeled smoke on both roles because its orchestration moves. Do not shorten or alter the selected scenarios' declared actions, repetitions, timings or completion rules to make the verification smaller; proportionality comes from selecting representative workflows and avoiding an unconditional statistical repetition campaign.

The composed workflow supplies representative settled search, seek/scrub, rendering, resize and library-return evidence. Keep deterministic equivalence coverage for all App branches, including settled/cold-index search, settled-memory, idle, rates/play-pause/scrub, delivery and export failures. Add focused native cold-index search, settled-memory or other scenario runs when the mandatory workflows and contract tests leave an affected branch or lifecycle/ordering risk unobserved. Missing coverage must be closed with real evidence; it cannot be waived because the default representative list passed.

For every representative run, retain and inspect the real event/action trace and available timing/resource evidence. Require matching workload, event/action ordering and outcomes, generation/opening/seek/RVFC identity, required-event/request-loss accounting, terminal/release/return sequence, media integrity and source hashes. Use the existing runner/analyzer and its supported observer profiles without suppressing missing-evidence, invalid-run or failure findings. Representative runs are behavioral/regression evidence, not an accepted statistical performance baseline. If the chosen profile/contract requires control evidence to validate a necessary observation, collect that evidence under the escalation rules; do not relabel an invalid run or remove a protocol prerequisite.

Also exercise ordinary non-benchmark production UI with the dedicated fixture library: browse, type/clear search, open/back/reopen, seek, fullscreen, clip endpoint editing and settings return. Verify one persistent media element and visible error states. Use native automation or a recorded operator procedure; do not treat jsdom as a native workflow check.

### Conditional statistical performance campaign

A full statistical campaign is not an unconditional completion gate for this ownership-only extraction. It becomes mandatory for the affected measurements/workflows if any of these conditions holds:

- Representative native evidence reveals or credibly suggests a latency/resource regression, new timeout/error/recovery, event/request loss, changed outcome class or sustained-growth signal. A source mutation or integrity failure is also an immediate correctness failure; further sampling never makes it acceptable.
- Inspection or evidence shows a change to emitted measurement meaning, attribution/identity, observation timing or coverage, event filtering, observer overhead or completion boundaries. Replacing the control callback at the same established event/release boundary alone is not evidence of a changed measurement contract. An unintended contract change must be corrected, not normalized by new results.
- Representative evidence is insufficient, ambiguous, inconsistent or not valid for an affected path, and focused additional native runs cannot resolve the uncertainty. A protocol requirement for missing observer-control evidence also triggers escalation for that measurement. Do not accept uncertainty as equivalence.

Record the observed trigger, affected dependency cone, additional runs and outcome in the checkpoint. Collect focused native evidence first where it can resolve a coverage gap; when a campaign trigger remains, run the existing full statistical/control procedure for the affected paths rather than automatically rerunning unrelated corpus cells. If no trigger holds and all mandatory checks pass, record the evidence and rationale for not running a statistical campaign. These representative results may support behavioral equivalence and the absence of an observed regression in the exercised workflows; they do not establish statistical performance equivalence or a numerical improvement.

For an escalated campaign, retain the existing four interleaved 60-second minimal/full observer-control pairs for each measured role where required by the protocol. Apply the unchanged analyzer gates: identical action/seek/error/recovery and request outcome classes; zero required event/request loss; median seek increase at most 5%, p95 at most 10%; private-memory increase at most 16 MiB; CPU within the larger of 2% or one accounting quantum; count/byte/drop repeatability rules unchanged. Required control failure blocks acceptance rather than inviting new tolerances.

Any numerical before/after performance claim requires the existing statistical procedure, including at least five identity-matched trials per role. P95/P99 requires 40 observations in the exact stratum. Apply the existing `max(resolution floor, 3 × baseline MAD)` band and >5% disposition rule; every new error, timeout, recovery, source change, loss or sustained-growth signal requires disposition. Keep all analyzer thresholds, resolution floors, sampling/eligibility rules and measurement contracts unchanged. Do not infer statistical equivalence from one successful pair, a static test pass or the absence of a formal campaign.

## Performance and reliability gates

Preserve real production controller/player and data-access paths, scenario action counts/order/defaults, generation/action/opening identity, two-RAF meaning, source hashes, terminal ordering, required-loss invalidation, media release and finite async ownership. Normal mode must start without a coordinator or benchmark lifecycle resources and preserve appearance and behavior. Behavioral equivalence tests, event/action ordering, cancellation/cleanup checks and representative native before/after workflows remain mandatory. Statistical performance/control campaigns are conditional on the explicit escalation rules, and any numerical performance claim requires their unchanged evidence standards. An unavailable mandatory check or a triggered but incomplete campaign retains non-done status; an untriggered campaign is documented as not required, never as passed.

Document the resulting boundary and remaining viewer/search/duration/frontend-bundle coupling in `docs/architecture/desktop-replay.md` (or a focused benchmark architecture document linked from it). Update feature/checkpoint evidence only with checks actually run. Full frontend separation is not the completion claim of this bounded feature.

## Reference architectures

The design uses the small driving-adapter boundary described in [Cockburn's original ports and adapters article](https://alistair.cockburn.us/hexagonal-architecture), while keeping Chronobreak's actual production implementation as the driven application. It does not adopt a domain framework. [Solid's onCleanup contract](https://docs.solidjs.com/reference/lifecycle/on-cleanup) requires synchronous owner-bound registration; asynchronous import/callback code must not acquire implicit cleanup ownership. [Vite's feature documentation](https://vite.dev/guide/features) explains asynchronous chunk loading, supporting the decision to treat packaging/timing changes as a separate measured migration.
