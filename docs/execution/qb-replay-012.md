# QB-REPLAY-012 execution checkpoint

Feature: `QB-REPLAY-012`
ExecPlan: `docs/exec-plans/qb-replay-012-canonical-replay-time-contract-v6.md`
Updated: 2026-09-07

## Current milestone

Complete. The v6 implementation, lifecycle evidence, accepted resource sample,
representative finalizer benchmark, dedicated replay-time verifier, and all applicable
project correctness checks are complete.

## Active unit

None. QB-REPLAY-012 is complete; no implementation or verification unit remains.

## Recorder architecture disposition

- The native WGC/D3D11/NVENC recorder is the sole Windows production recorder backend.
- The FFmpeg-driven WGC capture backend, its selector/candidate retry, active runner, and production support claims have been removed.
- The focused capability audit found no unique production audio, muxing, finalization, publication, recovery-policy, or validated media obligation in the retired backend. Existing comparison evidence was sufficient; no duplicate benchmark was run.
- FFmpeg remains where it is the appropriate implementation for audio encoding, muxing, probing, export, transcoding, fixture generation, and other non-capture responsibilities.
- Historical compiled WGC/AMF/QSV capability in the immutable r6 runtime remains truthful artifact provenance, not a callable production recorder path. Removing it requires a separately locked runtime rebuild.
- Shared discoveries made while debugging the FFmpeg recorder must still be evaluated on their own merits if they affect the native/shared muxing, A/V, finalization, publication, or recovery paths.

## Completed

- Finalized the exact 48,000,000-tick design and strict schema-v2 clean break.
- Added the pure Rust replay-time contract, TypeScript mirror, strict decimal/rational conversions, and cross-language golden fixtures.
- Implemented strict schema-v2 metadata/game-log parsing, media identity, recorder candidate finalization/publication, recorder timing integration, and exact game mapping.
- Migrated Tauri/frontend payloads, frame-addressed clip/export requests, viewer seek epochs/presented state, clip-preview synchronization, and durable replay-time architecture documentation.
- Completed the first automated implementation pass recorded below.
- Created and exercised `tools/replay_time/verify.ps1`; the verifier is no longer a missing-file task.
- Migrated the FFmpeg/WGC fixture runner away from the stale assumption that a successful backend stop directly publishes canonical `video.mp4`; the recorder candidate contract uses `video.partial.mp4` before common finalization/publication.
- Ran the production-WebView replay benchmark through the current schema-v2 path and obtained a complete `r6` matrix with all declared trials completed, complete telemetry, valid media, and unchanged source hashes.
- Exercised the existing production export path through the packaged Tauri/WebView replay benchmark, including boundary/middle/end export scenarios. This production path should be reused by the dedicated verifier rather than replaced by a verifier-only export implementation.
- Ran a final-review pass that identified concrete seek/presentation and ffprobe/export correctness issues. The root repaired the inconsistent seek/presentation tolerance, temporary seek-nudge restoration, orphan off-target telemetry path, and duplicate video/audio stream-index acceptance, with focused validation during the session.
- Consolidated the bounded child-process implementation in `media-runtime` and migrated recorder finalization and Tauri clip export to it. The shared path enforces one combined stdout/stderr cap while streaming, kills and reaps on timeout/overflow, and has exact-limit and Windows handle-release regression coverage.
- Rechecked and repaired the viewer seek/presentation invariants. New seeks supersede prior presented-seek authority, seek nudges are cancelled/restored on every terminal path, and a WebView without `requestVideoFrameCallback` now emits an explicit `seek_presentation` failure and settles instead of hanging.
- Integrated the dedicated verifier with the genuine packaged Tauri/WebView replay-benchmark export path. The accepted boundary/middle/end run proved source intervals `[0,300)`, `[30,330)`, and `[60,360)`, exact first/last source-frame identities, 300 output frames, zero-based output timestamps, H.264/AAC full decode, bounded A/V marker alignment, and unchanged source SHA-256.
- Added deterministic production-code coverage for a failure during final publication: after one staged output and a later thumbnail have been renamed, a forced video-rename failure removes every partial and canonical MP4/JPG. The dedicated verifier now requires that exact Rust test before passing its publication hook gate.
- Repaired bounded log reading/process disposal in the recorder fixture tooling. Independent six-second FFmpeg/WGC and native WGC/NVENC fixtures passed after those runner repairs.
- On 2026-09-01, established the FFmpeg/WGC timestamp-normalization root cause with `-debug_ts`: the intended CFR timestamps survived the filter and encoder but were shifted during fragmented-MP4 muxing by AAC negative/priming timestamp normalization. A controlled `-avoid_negative_ts disabled` run preserved the exact video grid. This is useful diagnostic evidence but should not justify retaining the FFmpeg-driven capture backend.
- On 2026-09-01, obtained a fresh native-recorder exact-grid proof after assigning timestamps to the already-scheduled raw H.264 packet sequence at the mux boundary: 360 frames at 60/1, first PTS 0, 256-tick frame step at time base 1/15360, last PTS 91904, one-past-last 92160, and packet PTS matching presentation order. This native timestamp bridge is provisionally strong and should be preserved unless the current audit finds a Milestone 4 contract conflict.
- On 2026-09-01, audited current code and existing recorder evidence. The external recorder has no unique audio, muxing, finalization, publication, recovery-policy, or validated media-quality obligation; those responsibilities are shared or already stronger on native. Existing matched evidence reports materially lower native CPU/private-memory use, and fresh native evidence already proves the exact frame grid, so no additional FFmpeg-versus-native benchmark is required for retirement.
- Retired the FFmpeg-driven Windows WGC recorder from production code: removed backend selection/candidate retry, the external capture example and runner, external capability plumbing/tests, and the active matched-backend runner while preserving generic non-Windows recording and all FFmpeg audio/mux/probe/export/tool uses.
- Updated current recorder architecture, verification guidance, README, and canonical AMD/Intel follow-ups to describe the sole native path and explicit unsupported surface.
- Material architecture change invalidated the dual-recorder portions of the original immutable plan. Preserved the original, v2, and v3 artifacts unchanged as history, finalized and reviewed `docs/exec-plans/qb-replay-012-canonical-replay-time-contract-v4.md`, and linked canonical feature/checkpoint state to that self-contained design.
- On 2026-09-02, completed the strict native marker implementation and obtained a fresh
  six-second recorder proof with 360 exact frames, zero invalid marker words, three
  A/V measurements under 15 ms, and drift-growth within the five-millisecond bound.
- Fresh evidence invalidated v4's immediate 120-write mux-failure assumption. Preserved
  v4 unchanged, finalized and reviewed the self-contained v5 plan, and linked canonical
  feature/checkpoint state to the corrected fragment-observed trigger design.
- On 2026-09-03, reclassified the WGC visual marker result as diagnostic. Decoded
  WGC/compositor frames do not expose a source-publication-to-final-PTS lineage or a
  separately bounded capture-latency term, so that fixture cannot prove end-to-end
  drift below five milliseconds.
- On 2026-09-03, completed the controlled post-capture native-mux comparison through
  the real `NativeMuxPlan`/`NativeMuxProcess` and packaged FFmpeg. Three baseline and
  three minimal-wallclock arms all preserved the exact 360-frame grid, constant
  `-0.125 ms` early/middle/late A/V disagreement, and zero measured drift. A real-mux
  injected eight-millisecond video-PTS skew failed the signed drift gate.

- Applied the v6 native H.264 input cutover: production now uses exactly
  `-use_wallclock_as_timestamps 1 -r 60 -f h264 -i pipe:0`, omits the rejected
  `nobuffer`/tiny-analysis discovery options, and retains `-shortest`.
- Completed clean, target-close, NVENC-failure, complete-fragment mux-failure, and
  pre-first-fragment lifecycle scenarios against the v6 production mux plan. Evidence:
  `build/perf/qb-perf-002-native-fixture/20260907-094027-steady-none-1dd162c2/`,
  `20260907-094119-close_window-none-f037f009/`,
  `20260907-094141-steady-nvenc_failure-5b4096d3/`,
  `20260907-094154-steady-mux_failure-3df609b2/`, and
  `20260907-094212-steady-pre_first_fragment-74a644c8/`.
- Reconciled the WGC marker verifier as diagnostic-only and passed a fresh marked
  six-second native run at
  `build/perf/qb-perf-002-native-fixture/20260907-094753-steady-none-23b48f66/`.
- Added the feature-gated production-finalizer benchmark wrapper and strict harness.
  The accepted rerun over five generated H.264/AAC fixtures, five repetitions each,
  including 3,600 seconds, passed with aggregate p95 `58.8945 ms`, one-hour p95
  `60.05592 ms`, source/candidate hashes unchanged, no publication, and packaged
  ffprobe identity bound to the runtime manifest. Evidence:
  `build/replay-time/qb-replay-012/finalizer-benchmark.result-v2.json`.
- The final dedicated verifier passed every required gate, including packaged-runtime
  identity, synthetic exact/nonzero grids, production boundary/middle/end exports,
  production schema/publication hooks, fresh native recorder evidence, and strict-v2
  corpus negative cases. Evidence:
  `build/replay-time/qb-replay-012-v6-final-3/.chronobreak-replay-time/runs/r-260907124154237-b496dfc2/result.json`.

- Collapsed the two nested control-flow sites in `recorder/examples/wgc_fixture.rs`
  identified by three Clippy diagnostics. The final recorder all-target/all-feature
  strict Clippy command passed.

## In flight

None.

## Remaining

None for QB-REPLAY-012. Do not reopen accepted benchmark, lifecycle, resource, timing,
or verifier work.

## Verification

Restart assumptions from the 2026-08-26 / 2026-08-27 implementation passes:

- `cargo test --manifest-path replay-time/Cargo.toml`: 11 passed.
- `cargo test --manifest-path recorder/Cargo.toml`: implementation-pass suites passed; hardware/profile fixtures explicitly ignored where documented.
- `cargo test --manifest-path app/src-tauri/Cargo.toml`: implementation-pass suites passed.
- `npm run test --prefix app`: implementation-pass suites passed.
- Strict Clippy passed for replay-time, recorder, and Tauri at the recorded implementation checkpoint.
- `npm run check --prefix app` and `npm run build --prefix app` passed at the recorded implementation checkpoint.
- `git diff --check` passed at the recorded implementation checkpoint.

Recorded concrete evidence:

- 2026-08-27 current-tree core rerun: replay-time 11 passed; recorder `--all-targets --all-features` 133 passed with 5 ignored; Tauri 48 passed; frontend 29 passed; frontend type-check and production build passed.
- 2026-08-27 packaged/runtime and build checks passed: media-runtime 10 tests with 1 ignored, strict format/Clippy, ignored staged-runtime identity test, prepare atomicity, runtime verify/smoke, recorder debug/release builds, Tauri target check, production desktop build, portable release staging, and release/runtime verification.
- 2026-08-27 hardware fixture: `tools/native_backend/run_native_fixture.ps1` steady 10-second native WGC/D3D11/NVENC run passed with 600 decoded frames, 599 unique frame hashes, exact 60 FPS, zero video/audio start, 10-second H.264/AAC coverage, full decode, and reconciled 600 submitted/completed/muxed frames. Evidence: `build/perf/qb-replay-012-native/20260827-173602-steady-none-ecb4f666/`.
- 2026-08-27 FFmpeg/WGC fixture capture reached a successful 631-frame/10.496-second H.264/AAC `video.partial.mp4`; the preserved failure at that time was caused by the old fixture runner still requiring canonical `video.mp4`. Evidence: `build/perf/qb-replay-012-external/20260827-173701-steady-none-bdab662c/`.
- 2026-08-31 production-WebView `r6`: all 16 declared trials passed; the matrix verifier accepted 12 arms and 16 required results; aggregate preservation reported no event/request loss. This is current-behavior/reliability evidence only, not the missing balanced before/after comparison.
- 2026-08-31 dedicated replay-time verifier production exports: `build/replay-time/qb-replay-012/.chronobreak-replay-time/runs/r-260831141522580-2a0c026d/result.json`. Its packaged Tauri/WebView boundary/middle/end gate passed exact half-open source-grid, output-grid, decode, A/V marker, and source-immutability checks. The overall run remained failed because its recorder-backed corpus gates preceded subsequent runner repairs.
- 2026-08-31 failed-publication regression: `cargo test --manifest-path app/src-tauri/Cargo.toml clip_export::tests::publication_failure_removes_every_staged_and_final_output -- --exact` passed.
- 2026-08-31 FFmpeg/WGC fixture after log-lock repair: `build/perf/qb-replay-012-external/20260831-162331-steady-none-fbc524b0/` passed with 388 decoded frames, 387 unique hashes, full decode, and ABI-1 finite-pool evidence.
- 2026-08-31 native WGC/D3D11/NVENC fixture rerun: `build/perf/qb-replay-012-native/20260831-162439-steady-none-76be1d19/` passed with 360 reconciled scheduled/submitted/completed/muxed frames, exact six-second video/audio coverage, full decode, bounded ownership, and zero admission failures.
- 2026-09-01 FFmpeg/WGC diagnostic work proved that the CFR filter and NVENC output already carried the intended exact timestamps and that the finalized fragmented MP4 shift matched AAC priming/negative timestamp normalization. `-avoid_negative_ts disabled` produced a fresh short exact-grid pass. This evidence may be retained as diagnosis even if the capture backend is removed.
- 2026-09-01 native exact-grid proof passed with first PTS 0, exact 256-tick 60 Hz spacing at 1/15360, 360 frames, and packet PTS equal to presentation PTS.
- 2026-09-02 native marker proof passed at `build/perf/qb-replay-012-native-marker/20260902-105837-steady-none-78405654/`: 360 exact 60/1 frames, first PTS 0, last PTS 91904, zero invalid marker words, early/middle/late A/V disagreement of 12.271/2.771/14.625 ms, and accepted drift growth.
- 2026-09-03 post-capture native-mux A/B:
  `build/perf/qb-replay-012-native-mux-av/20260903-141553/summary.json`. Baseline 3/3
  and minimal-wallclock 3/3 passed. Every arm decoded 360 frames at time base 1/15360
  with PTS 0 through 91904 in steps of 256; independently identified early/middle/late
  audio markers each measured `-1/8 ms` video-minus-audio disagreement and signed
  drift from early was zero samples. The real-mux `+8 ms` late-video-PTS control was
  rejected at the five-millisecond drift gate. Capture was absent by construction.

Closure-pass verification:

- `python -m unittest tools.replay_time.tests.test_fixture_tools -v`: 14 passed,
  including exact-grid-before-publication and signed-drift controls.
- Corrected finalizer benchmark: all gates passed across 25 observations; aggregate
  p95 `58.8945 ms`, repeatability comparison favorable, and no disposition used.
- Final dedicated replay-time verifier: `status=passed`, zero required gate failures,
  all ten required/recorded gates passed.
- Accepted resource sample:
  `build/perf/qb-perf-002-native-fixture/20260907-100413-steady-none-e605064f/`.
  It collected 195 samples through `1200.512 s`; output advanced on every interval,
  capture private memory changed by `+2.0 MiB`, dedicated GPU memory by `+8 KiB`,
  handles fell from 403 to 390, and the preserved 1,200.405-second H.264/AAC MP4
  decoded fully with 72,024 exact-60-Hz video frames.
- Fresh project checks passed: replay-time tests 14; recorder tests 123 with 5
  explicitly ignored; Tauri tests 49; replay-benchmark tool tests 80; recorder and
  Tauri all-target checks; all three Rust format checks; replay-time and Tauri strict
  Clippy; frontend type-check/build; recorder release build; and desktop production
  build.
- The initial recorder-wide strict Clippy run reported three style-only
  `collapsible_match`/`collapsible_if` diagnostics in
  `recorder/examples/wgc_fixture.rs`. After the authorized idiomatic control-flow
  collapse, `cargo clippy --manifest-path recorder/Cargo.toml --all-targets
  --all-features -- -D warnings` passed on the completed tree.
- Final blocker-only review: `APPROVE — no closure blocker` for production behavior,
  architectural conclusion, or accepted evidence.

These results satisfy the feature-specific and universal completion gates.



## Deviations

The planned balanced schema-v2 before/after replay-performance gate remains unresolved. No version-controlled, buildable pre-change schema-v2 subject exists in reachable repository state. The preserved QB-REPLAY-008 baseline (`4c9400d...`, parent `2105b9...`) is schema-v1 historical evidence and therefore protocol-incompatible. The `r6` matrix is current-only and undersampled for comparison at one to two trials per arm versus the required five matched trials per arm; planned p95/p99 reporting also requires at least 40 observations.

Disposition: preserve the earlier `r1` and current `r6` artifacts as separate characterization/current-behavior evidence. Do not manufacture a backport/hybrid subject, combine incompatible fingerprints, or use `r6` as its own baseline. No before/after delta, non-regression, or performance conclusion is claimed. This evidence deviation remains explicit and does not weaken any other acceptance gate.

The original ExecPlan was written while both recorder backends remained implementation and verification subjects. The completed capability audit and explicit native-only architecture decision materially invalidated that design. Per repository workflow, the original through v5 artifacts remain unchanged as history and `docs/exec-plans/qb-replay-012-canonical-replay-time-contract-v6.md` is the current approved design. External-recorder-specific gates are inapplicable; the underlying replay-time, A/V, resource, recovery, and finalization intent remains required on the production native path.

Fresh mux-failure evidence materially invalidates the v4 Milestone 3 trigger. With the
unchanged 120-frame/two-second production GOP, writer call 120 precedes delivery of the
second IDR at packet index 120 and no fragment is yet probeable. The required 120-call
terminal kill produced a zero-byte candidate. Per workflow, v4 remains immutable; the
reviewed v5 replaces input-write inference with separate exact pre-fragment and bounded
complete-`moof`/`mdat`-observed failure triggers.

The first v5 implementation run materially invalidated its pinned `-fflags nobuffer`
input design: the native session completed 600 packets while FFmpeg reconciled only
480, with the lost count equal to the first two-second GOP. No complete fragment was
observable while the input remained live. V5 remains immutable; the reviewed v6
supersedes it with the validated minimal wallclock input contract.

The earlier WGC marker interpretation was also invalidated. For decoded WGC frame
observations, `V_i = S_i + D_i + L_i`, where variable compositor/capture/CFR latency
`L_i` is not independently measured or bounded below five milliseconds. Moving an
arbitrary term between `D_i` and `L_i` leaves every current observation unchanged.
Neither QPC anchoring nor a visual publication epoch creates missing per-frame
lineage. The WGC fixture is retained as diagnostic and is not used to claim the strict
end-to-end drift bound.

The reviewed v6 plan narrows A/V proof to the post-capture mux boundary and corrects
drift to the signed invariant `abs((V_i - A_i) - (V_early - A_early)) <= 5 ms`.
An earlier instruction mistakenly grouped `-shortest` with the falsified input-analysis
experiment. The retained six-run artifact used `-shortest`; no independent evidence
shows it causes a product problem, and current DirectShow/silent audio inputs are
unbounded. V6 therefore preserves that existing EOS policy and changes only the
validated H.264 input options.

The planned 30-minute native resource soak was cancelled by the user after roughly
20 minutes. The preserved run is accepted by explicit product decision as a resource
stability sample, not represented as a completed 30-minute or clean-stop soak. It has
no terminal recorder accounting because cancellation closed the WGC target. Separate
clean lifecycle evidence proves graceful stop and exact terminal accounting; the
cancelled run proves only bounded resource/output/media behavior over its observed
interval.

## Decisions

- Preserve the strict schema-v2-only cutover. Do not add compatibility, repair, migration, or approximate fallback paths.
- Use the versioned superseding ExecPlan as the current implementation design. Preserve the original immutable ExecPlan unchanged as planning history.
- Make the native WGC/D3D11/NVENC H.264 High 1080p60 recorder the sole supported Windows production recorder backend.
- The audit found that HEVC, non-High profiles, AMD/AMF, and Intel/QSV exist only in the external implementation and are not validated production combinations. The native-only product decision retires those choices and their current roadmap claims rather than preserving an unproved compatibility backend or silently degrading their configuration.
- Accept the native in-process GPU worker's documented lack of hard driver-hang process isolation for this architecture; retain bounded cooperative cleanup and truthful diagnostics rather than a second capture backend solely for kill containment.
- Do not complete an FFmpeg-recorder-specific 240-second acceptance campaign before removing that backend; its backend-specific normalization gate becomes inapplicable while the underlying native resource, A/V, target-close, finalization, and recovery intent remains required.
- Reuse the existing production Tauri/WebView export mechanism in the dedicated replay-time verifier rather than adding a verifier-only production surrogate.
- Treat missing historical comparison evidence as an evidence/protocol problem, not as justification for manufacturing a baseline.
- Prefer fixing shared subprocess/resource-bound semantics at their appropriate owning abstraction rather than duplicating defensive checks at each consumer.
- Require deterministic production-code proof that a failed clip publication removes every staged and canonical output; successful packaged-app exports alone are insufficient.
- Treat the native recorder's no-slot telemetry as retriable pressure rather than a dropped tick, but retain the existing zero-retry normal-fixture gate unless new evidence justifies changing the contract.
- Treat a failing verification instrument as untrusted until production-media, fixture-generation, and verifier-decoding causes have been distinguished.
- Do not weaken marker validity, A/V, drift, or other acceptance bounds reactively just to obtain a passing result.
- Treat WGC visual-marker timing as diagnostic unless future instrumentation exposes
  per-frame source-to-finalized-media lineage with independently bounded uncertainty.
- Compute A/V drift from signed disagreements: `D_i = V_i - A_i` and
  `abs(D_i - D_early) <= 5 ms`; subtracting disagreement magnitudes is not equivalent.
- Retain `-shortest` as the current FFmpeg-owned audio/video output-lifetime policy;
  it was present in the accepted paired artifact and is not part of the falsified
  raw-H.264 input-analysis experiment.

- Accept the cancelled approximately 20-minute run as a documented resource-stability
  deviation only. Pair it with the separate clean lifecycle run; never claim that it
  proves 30-minute duration or graceful terminal accounting.
- Accept the existing production finalizer benchmark, lifecycle matrix, and final
  dedicated verifier as sufficient engineering evidence. Do not reopen them during
  closure.

## Blockers

None.

## Fresh-session handoff

QB-REPLAY-012 is complete. Do not resume its investigation, benchmark, lifecycle,
resource, timing, verifier, or implementation work. Route new work through the
canonical feature list.

## Next action

Route to `QB-REPLAY-013` (`Versioned Replay Index and Staged Opening`) through its
planned-work workflow.
