# Whole-codebase review — 2026-09-26

The review found **one P1 and four P2 implementation/build issues**, one P3 documentation discrepancy, and several bounded simplification opportunities. Existing automated suites pass with the execution-policy qualification below, but the new focused reproductions expose gaps in export ownership, export retry, and startup recovery.

This is an audit, not an implementation or release-readiness claim. No product source was changed. Eight draft follow-ups were added to `feature-list.json`; previous entries, statuses, and unrelated working-tree changes were preserved.

## Scope and four review passes

Baseline: HEAD `a840c645e5080ec13a7cb23c540eabc388676a83`, including the existing modified and untracked replay-opening work. The executable-source inventory contains **158 first-party files**, spanning the SolidJS frontend, Tauri backend, recorder, shared media-runtime and replay-time crates, benchmark/verification tools, and tests. The retained 194-line FFmpeg C/C++ patch, manifests, packaging configuration, and applicable architecture were inspected separately. Generated outputs, vendored dependencies, downloaded FFmpeg sources, and historical evidence were not treated as first-party implementation requiring exhaustive audit.

The repository's product, workflow, verification, relevant feature state, and active replay-opening checkpoint/design were consulted. Independent read-only scouts supplied evidence; the root reviewer checked critical paths and reproduced actionable failures.

| Pass | Instructions applied | Result |
|---|---|---|
| Cyclomatic complexity | [upstream skill](https://github.com/saurabhkumar8112/cyclomatic-complexity-skill/blob/master/skills/cyclomatic-complexity/SKILL.md) | Measured before recommending changes; ranked control-flow hotspots and preserved validation/lifecycle constraints. |
| Ponytail | [main skill](https://github.com/DietrichGebert/ponytail/blob/main/skills/ponytail/SKILL.md) and [audit mode](https://github.com/DietrichGebert/ponytail/blob/main/skills/ponytail-audit/SKILL.md) | Consumer-based deletion audit; about 100 removable lines, no justified dependency removals. |
| Rust | [local rust-skills](C:/Users/Hugo/.agents/skills/rust-skills/SKILL.md) | Reviewed async cancellation, blocking work, ownership, errors, unsafe/native boundaries, numeric invariants, and tests across all four Rust crates. Findings F1 and F4. |
| Independent correctness/build review | App/controller/IPC flows, recovery, packaging, tooling, and retained native patch | Findings F2, F3, F5, and documentation finding D1; focused reproductions plus existing suites. |

The online skills were read from their upstream repositories. They were not installed globally. Review scope takes precedence over their implementation/refactoring procedures.

## Prioritized findings

### F1 — P1: export children outlive an error return; diagnostics and waits are unbounded

**Location:** [clip_export.rs](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/clip_export.rs:906), `run_ffmpeg`, especially lines 914–938 and 958–960. **Follow-up:** `QB-CLIP-008`.

The export runner starts a Tokio child without a kill-on-drop fallback, spawns a detached task that reads all stderr into a `String`, and reads progress with an error-propagating `?`. A progress decoding/read error returns before the child is killed or reaped. The caller can then release library mutation ownership and attempt partial-output cleanup while that child is still writing. A stalled child also has no deadline; continuous stderr can grow retained memory without a bound.

**Evidence:** a disposable Rust harness used the exact extracted function body, the app's cached Tokio 1.53.1, and a stub progress sink. A finite controlled child wrote invalid UTF-8 to stdout, waited, and wrote a marker. `run_ffmpeg` returned `failed to read ffmpeg progress`; the marker appeared afterward. This proves the ownership failure without touching user media. It is not a claim that the packaged FFmpeg emitted malformed progress in a real export.

**Recommendation:** keep the process, pipe drains, and export admission under one owner through completion or termination and reaping. Bound diagnostic retention and implement an appropriate stall/deadline policy. Add a cancellation fallback and fixture coverage for early read failure, cancellation, stalls, and stderr floods. The existing bounded media-runtime process runner offers an architectural precedent; export progress needs its own compatible integration.

### F2 — P2: Retry and a second export keep the obsolete library snapshot token

**Locations:** [App.tsx](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/App.tsx:870), especially the `onExported` callback at 878; [ClipExporterScreen.tsx](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/components/ClipExporterScreen.tsx:482). **Follow-up:** `QB-CLIP-009`.

The exporter gets its token from the navigation state's captured `snapshotOrigin`. After success or failure, `onExported` invalidates the root and refreshes the library, but does not reconcile that navigation origin. The next `runExport` still submits the old token. The backend correctly rejects this stale selection, so the visible Retry/another-export flow fails until navigation reacquires admission.

**Evidence:** two focused tests used the actual App and LibraryController with mocked IPC and screen boundaries. For both success and failure callbacks, the library refreshed but the exporter's token remained `A-1`. Both desired-behavior assertions failed. The real exporter submits that prop unchanged.

**Recommendation:** after same-root refresh, revalidate the source media and reconcile the editor's origin while retaining its draft/options. Do not blindly replace tokens across root changes or source disappearance. Test the second command as well as prop reconciliation.

### F3 — P2: an initial library scan failure makes Settings-based recovery unreachable

**Locations:** [App.tsx](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/App.tsx:804), settings branch at 889 and settings save admission at 245; [library_coordinator.rs](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library_coordinator.rs:422). **Follow-up:** `QB-LIB-012`.

When the first scan fails, the error `Match` wins ahead of Settings. Settings additionally requires usage from a successful snapshot. Clicking Settings therefore leaves the error panel visible. Even bypassing presentation would not suffice: settings mutation currently requires a snapshot origin. An unreadable configured library can prevent users from choosing a healthy folder through the app.

**Evidence:** a third focused App/controller test injected an initial `refreshLibrary` rejection, clicked the Settings button, and failed to find the folder-selection control. The failure is reproduced at the app integration boundary; no real directory permissions were changed.

**Recommendation:** make storage configuration recoverable without a successful scan. Preserve serialized settings ownership, atomic config persistence, and validated paired-root publication. Destructive library actions should remain snapshot-gated.

### F4 — P2: synchronous export filesystem work runs on async executor workers

**Location:** [clip_export.rs](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/clip_export.rs:233), including authority loading/setup at 233–249 and publication/cleanup at 545–567; called from the async `export_clip` command. **Follow-up:** `QB-CLIP-010`.

Export performs synchronous file checks, strict bundle reads/JSON parsing, directory creation, metadata access, rename, and cleanup directly inside async execution. Slow external/network disks or large metadata can occupy a Tokio worker and delay unrelated async work. This differs from the coordinator's existing owned blocking-job pattern.

**Evidence:** code inspection against the Rust skill's `async-spawn-blocking` and cancellation guidance. No claim of measured UI latency or a reproduced production stall is made.

**Recommendation:** move these stages into bounded owned blocking work, keeping mutation admission alive until the work actually finishes even if its caller is cancelled. Prove unrelated task progress with a deterministic held-worker fixture.

### F5 — P2: Windows line-ending conversion breaks the locked FFmpeg source build

**Locations:** [.gitattributes](C:/Users/Hugo/Documents/perso/Chronobreak/.gitattributes:1), [build_ffmpeg.ps1](C:/Users/Hugo/Documents/perso/Chronobreak/tools/media_runtime/build_ffmpeg.ps1:75), raw hash check invoked at 94; [runtime-lock.json](C:/Users/Hugo/Documents/perso/Chronobreak/media-runtime/runtime-lock.json:56). **Follow-up:** `QB-DIST-005`.

The lock records the LF patch bytes, but no attribute pins patch line endings. This Windows checkout has `i/lf w/crlf`. The build hashes raw checkout bytes and rejects the otherwise unchanged patch before application/build can proceed. Existing staged binaries and ordinary Cargo/npm builds are not invalidated by this failure.

**Evidence:** invoked only the exact `Assert-Sha256` function extracted from the build script, without downloads, patch application, or rebuilding. It rejected the current file. Hashes:

```text
Locked / LF-normalized: 9de18bcfa2a72feaf2f98b3d815f1a4c7fc6f01aac8933534b1a89fbec95e3f6
Raw Windows checkout:  428213904d67451365fb2186c794680806a6ea87b8a5eff0401df65ad3799328
```

**Recommendation:** pin LF for hash-locked patches and deliberately normalize the checkout. Verify disposable checkouts with autocrlf enabled/disabled, and retain rejection of actual content drift. Do not weaken hashing or replace the lock with platform-dependent bytes.

### D1 — P3: durable runtime architecture overstates historical capture behavior

**Locations:** [media-runtime.md](C:/Users/Hugo/Documents/perso/Chronobreak/docs/architecture/media-runtime.md:23), [retained patch](C:/Users/Hugo/Documents/perso/Chronobreak/media-runtime/patches/ffmpeg-8.1.2-queueback-wgc.patch:79), [windows-capture.md](C:/Users/Hugo/Documents/perso/Chronobreak/docs/architecture/windows-capture.md:7). **Follow-up:** `QB-MAINT-002`.

`media-runtime.md` says the patch drains to the latest frame and counts superseded frames, and describes an explicit FFmpeg capture backend. The patch instead initializes `superseded` to zero and never increments it. The provenance notice accurately describes single-frame acquisition, while the current capture architecture retires the FFmpeg backend entirely.

Reconcile durable documentation with the retained artifact and current native-only behavior. This is a documentation/provenance discrepancy, not evidence of a frame-loss defect in the production native recorder. Do not revive historical code or rewrite immutable historical evidence to resolve it.

## Cyclomatic-complexity pass

Lizard 1.24.0 measured Rust, TypeScript/TSX, Python, and standalone C# functions. PowerShell was measured separately with an AST-based decision counter because Lizard does not support it. Initial fallback-parser PowerShell values were discarded.

| Lizard population | Functions | Mean CCN | CCN 1–5 | 6–10 | 11–15 | >15 |
|---|---:|---:|---:|---:|---:|---:|
| All supported source | 3,230 | 3.55 | 2,768 | 265 | 80 | 117 |
| App/shared/recorder files | 1,876 | 3.15 | 1,618 | 165 | 42 | 51 |
| Dedicated test files | 911 | 1.67 | 878 | 25 | 5 | 3 |
| Tooling files | 443 | 9.05 | 272 | 75 | 33 | 63 |

These are path-based buckets: app/shared/recorder files include inline Rust tests and compiled benchmark code. They are not a count of production-only functions. Four supported declaration/module files contain no detected functions.

The PowerShell supplement parsed all 20 scripts without errors: **205 named functions**, with 137 at 1–5, 36 at 6–10, 10 at 11–15, and **22 above 15**. Its convention is baseline 1 plus if/elseif clauses, loops, catches, explicit switch cases, and binary `-and`/`-or`. It excludes nested named functions from parent counts; anonymous scriptblocks belong to their enclosing named function. Top-level code, embedded C#, pipeline-chain operators, and ternaries are not fully represented. Its results are a separate approximate control-flow measure, not directly interchangeable with Lizard's grammar.

| Priority hotspot | Score | Recommended boundary |
|---|---:|---|
| [playbackController.dispatchPending](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/playbackController.ts:502) | 73 | Eligibility/deduplication, timeout setup, assignment failure; keep controller state and event ordering owned centrally. |
| [parse_ffprobe_media_timeline](C:/Users/Hugo/Documents/perso/Chronobreak/replay-time/src/lib.rs:1072) | 47 | Decode, stream selection, exact timeline invariants. |
| [validateMediaTimeline](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/replayTime.ts:273) | 41 | Identity, video grid/PTS, audio and cross-stream invariants. |
| [capture _cook_telemetry](C:/Users/Hugo/Documents/perso/Chronobreak/tools/capture_benchmark/analyze.py:947) | 133 | Normalize individual evidence sources, then compose metrics. |
| [capture _capture_validation](C:/Users/Hugo/Documents/perso/Chronobreak/tools/capture_benchmark/analyze.py:1442) | 125 | Manifest, telemetry, media, recorder, and encoder checks. |
| [replay _validate_scenario_evidence](C:/Users/Hugo/Documents/perso/Chronobreak/tools/replay_benchmark/analyze.py:715) | 86 | Identity/order, timing, and scenario rules. |
| [Invoke-ExportValidation](C:/Users/Hugo/Documents/perso/Chronobreak/tools/replay_benchmark/run.ps1:2539) | 56 (PS) | Export orchestration, artifact discovery, and assertions. |
| [Read-AndValidateManifest](C:/Users/Hugo/Documents/perso/Chronobreak/tools/replay_benchmark/run.ps1:1617) | 54 (PS) | Schema, scenario, process/collector, and output contracts. |

Other measured concentrations include benchmark `ViewerScreen.runBenchmarkAction` (63), app bootstrap (40), runtime-lock validation (39), and the native probe example (64). Tooling dominates the largest scores; replay dispatch is the first runtime priority. Scores use the upstream skill's advisory bands, not a repository CI threshold. High CCN is not itself a correctness defect. Preserve every integrity check and observable error while introducing cohesive helpers. Track bounded child tasks under draft `EPIC-CODE-QUALITY` rather than starting a repository-wide rewrite.

## Ponytail simplification pass

| Ranked opportunity | Evidence and boundary | Estimated net deletion |
|---|---|---:|
| Remove obsolete split-library IPC | [api.ts](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/api.ts:418) `loadGames`/`loadClips` have no frontend callers; remove corresponding Tauri wrappers and both registration entries. Keep internal scanners used by snapshots, storage/retention, and tests. | About 65 lines |
| Remove unused benchmark predicates | [benchmark.ts](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/benchmark.ts:133) `isCurrentMediaGeneration` and `isLatestBenchmarkAction` are referenced only by isolated tests. | About 22 lines including tests |
| Remove unused percentile helper | [benchmark.ts](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/benchmark.ts:539) has only isolated test consumers. | About 14 lines including tests |

Total: **about 100 lines, zero dependency removals**, tracked by `QB-MAINT-001`. Estimates include associated tests/registrations and need a final consumer search before deletion. Keep the media adapter test seam, explicit replay-time contracts, lifecycle/security checks, and historical benchmark provenance. No stronger stdlib/native replacement or dependency removal was justified by this pass.

## Verification and evidence

All commands ran against the existing worktree. Exact commands, exit codes, timing, logs, ranked CSVs, and disposable repros are retained under [build/codebase-review-20260926](C:/Users/Hugo/Documents/perso/Chronobreak/build/codebase-review-20260926). This directory is ignored and local; the durable report records the outcomes independently.

| Check | Result |
|---|---|
| Recorder: check/test all targets + all features; fmt; Clippy with warnings denied | Passed; 123 tests passed, 5 opt-in tests ignored. |
| Tauri backend: tests; fmt; Clippy all targets with warnings denied | 92 tests passed, 1 ignored after the child-only execution-policy adjustment below; fmt/Clippy passed. |
| Shared media-runtime: tests, fmt, Clippy | 11 tests passed, 1 opt-in test ignored; other checks passed. |
| Shared replay-time: tests, fmt, Clippy | 14 tests passed; other checks passed. |
| Frontend: npm test, typecheck, production build | 166 tests in 22 files passed; typecheck/build passed. Nonfatal bundler warning about ineffective dynamic import remains. |
| Capture benchmark Python suite | 60 tests passed. |
| Replay benchmark Python suite | 124 tests run: 123 passed, 1 skipped; includes collector harness coverage. |
| New focused App/controller desired-behavior tests | Three failed as expected, reproducing F2 success/failure and F3. Kept as review artifacts, removed from the application source tree. |
| Extracted Rust export-child repro | Confirmed post-error child write; finite fixture exited. |
| Extracted patch SHA checker | Confirmed F5; LF-normalized bytes match the lock. |
| Roadmap schema/references and diff whitespace | Passed after review follow-ups were added. |

The first backend test run had one failure because Windows PowerShell's process policy prevented its disposable `child.ps1` fixture from reaching the ready marker. The complete suite passed when invoked with **child-process-only** `PSExecutionPolicyPreference=Bypass`; no persistent machine/user policy was changed. Both runs are retained. Thus the default-policy run was not clean, although the fixture passed under the scoped test environment.

No live League recording, physical GPU matrix, 30-minute soak, perceptual A/V assessment, complete native media fixture campaign, packaged installer test, or full FFmpeg rebuild was performed. Existing pending performance and human-verification gates remain pending. An externally interrupted prior benchmark campaign is not reported as a code defect. Passing unit/build checks does not close those gates.

## Recommended order

1. Fix and regress F1's child ownership before more export features.
2. Fix F2/F3's user-visible retry and recovery flows, then F4's executor isolation.
3. Fix F5's deterministic Windows source-build failure; reconcile D1 separately.
4. Remove the proven unused helpers after the active replay-opening work settles.
5. Split the largest complexity hotspots one bounded change at a time, with before/after measurements and contract tests.

The review found useful existing safeguards: same-opened-handle containment for playback, strict timeline/identity validation, bounded native frame handoff, and explicit runtime provenance. Preserve those properties during cleanup. An initially suspected native-worker cancellation/join issue was rejected after checking that the join handle is transferred to the owned blocking task before suspension.
