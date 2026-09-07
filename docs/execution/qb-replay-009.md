# QB-REPLAY-009 execution checkpoint

Feature: `QB-REPLAY-009`
ExecPlan: `docs/exec-plans/qb-replay-009-playback-boundary-and-speed-ladder-v2.md`
Updated: 2026-09-07

## Current milestone

M1 - Capture the small controller reference under v2. The user-requested
proportionality correction is a planning-only pass; product implementation has
not started. The feature remains ready / not-started.

## Active unit

None. The next implementation session starts with one bounded observation of
existing controller-owned behavior on an existing validated canonical schema-v2
H.264 fixture, then proceeds to the injected controller.

No native diagnostic adapter, benchmark contract upgrade, corpus build, or broad
matrix is a prerequisite to M2. Reuse current measurement seams or direct
documented observations. Preserve the existing dirty tree.

## Completed

- Original planning read the full mandatory contracts and relevant architecture,
  confirmed the three dependencies done, and completed bounded frontend/native
  exploration and plan review. No product implementation was made.
- The original design established exclusive primary-media ownership, distinct
  timing facts/generations, bounded scheduling/recovery, truthful failed-rate
  semantics, per-player decoder association, and the QB-REPLAY-015 boundary.
  Those architectural decisions remain in v2.
- 2026-09-07 proportionality review found that original M1/M5 and canonical
  verification required another broad benchmark campaign before controller work.
  The user explicitly requested correcting that scope before implementation.
- Created a self-contained versioned replacement at the linked v2 path. Preserved
  `docs/exec-plans/qb-replay-009-playback-boundary-and-speed-ladder.md` as immutable
  history and repointed this checkpoint and the canonical feature to v2.
- Replaced full baseline/corpus/observer/schema work with a small controller suite,
  material-result escalation, and an explicit stopping rule. Clarified canonical
  8x acceptance: visible bounded limitation handling is distinct from 8x success.
- Final read-only review of the v2 feature, plan, and checkpoint found no material
  scope, architectural-preservation, or consistency issue. Canonical validation
  and preservation checks passed; the corrected planning handoff is complete.

## In flight

None. The corrected plan is ready for a fresh implementation session. No
QB-REPLAY-009 product code was implemented in this planning session.

## Remaining

- M1: one focused before observation on existing validated media; accept 008/012
  evidence without reopening or reproducing their campaigns.
- M2: implement and deterministically test the concrete controller/HTML adapter.
- M3: integrate the persistent viewer at 1x, route existing benchmark media actions
  through the controller, and add bounded product decoder diagnostics.
- M4: add shared rate/audio controls and truthful observed-rate/8x fallback behavior.
- M5: applicable automated checks, targeted production after cases and focused
  resource comparison, relevant durable documentation, and completion evidence.
  Escalate only material findings that could change implementation/architecture;
  stop when the contract and targeted verification support completion.

## Verification

- Original 2026-09-07 planning: the canonical Python validator from
  `docs/development/VERIFICATION.md` passed for 60 roadmap items and six
  plan/checkpoint pairs with the original linkage. Its original feature diff and
  Markdown whitespace checks passed. These historical planning checks do not
  validate the replacement linkage.
- 2026-09-07 v2: executed the exact canonical Python validator extracted from
  `docs/development/VERIFICATION.md`; passed for 60 roadmap items and six
  plan/checkpoint pairs. QB-REPLAY-009 remains ready / not-started with done
  dependencies, and feature/checkpoint linkage resolves to the same v2 plan.
- 2026-09-07 v2: required plan sections, UTF-8/final-newline/trailing-whitespace
  checks, and `git diff --check` scoped to the three changed files passed.
  SHA256 verification confirmed the original plan unchanged
  (`1ba6425dd7f02a622d74082de3db3138b5bab09e3fb70fa0a91c0861435cc0e1`).
  A session-start file-hash comparison and roadmap comparison confirmed only the
  new v2 plan, this checkpoint, and the 009 feature entry changed; unrelated
  working-tree changes and all other feature entries were preserved.
- Product tests, production build, actual decoder identification, playback/rate
  measurements, and manual audiovisual checks: not run in either planning pass.
  No new playback, performance, or hardware-decoder success is claimed.

## Deviations

The user-requested pre-implementation correction supersedes the broad verification
design. Fresh full matrices, corpus preparation, observer pairs, benchmark
schema/event/report changes, and formal comparison coverage are removed from
default requirements. This is an explicit planning requirement revision, not a
relaxation after implementation failure.

The original plan remains unchanged. There are no implementation deviations
because implementation has not started.

## Decisions

- Accepted QB-REPLAY-008 and QB-REPLAY-012 evidence is authoritative input. Do not
  reopen predecessor plans/reports or reproduce their proof by default. Use
  current exact replay-time and seek-state helpers.
- The 008 ten-minute torture finding remains accepted; its short layout resource
  flag informs only the focused transition/cleanup check, not a proven leak.
- Record shared before/after behavior on existing fixtures; verify new UI,
  recovery outcomes, and decoder diagnostics after integration. Do not retrofit
  diagnostics into the old viewer to manufacture a matched observer baseline.
- An 8x selection with lower working fallback is an explicit runtime limitation,
  never successful 8x. Retain selected/applied/observed distinctions, bounded
  recovery, no automatic reapplication of failed 8x, and no seek-based emulation.
- Hardware decoder proof is per-player evidence, not generic GPU utilization.
  Unknown/software outcomes stay explicit. Missing evidence needed for the
  decoder-path requirement needs focused resolution; perfect telemetry is not
  a separate workstream.
- Stopping rule: once the functional contract and targeted validation show no
  material regression or unresolved architectural uncertainty, complete applicable
  checks/docs and stop. Benchmark/verifier/evidence imperfections are non-blocking
  unless they could invalidate that conclusion. Broader work needs a recorded
  material finding, the decision it could change, and why a narrower check cannot
  resolve it.
- General frame stepping remains QB-REPLAY-015; the clip editor's multi-element
  preview and alternate playback backends remain outside 009.
- Original planning located `forward-engineering` at
  `C:/Users/Hugo/.omp/skills/forward-engineering/SKILL.md`. Apply it for substantial
  implementation and the available Rust skill for Rust changes.
- Preserve existing unrelated product/canonical working-tree changes. This pass
  is limited to the 009 feature entry, v2 plan, and this checkpoint.

## Blockers

None established for the corrected planning handoff. Production behavior and
decoder acquisition remain future focused checks, not assumed passes or
prerequisite benchmark campaigns.

## Next action

In a fresh implementation session, perform mandatory bootstrap (checkpoint before
the linked v2 plan), apply `forward-engineering`, and activate M1 with feature
status in progress. Select one existing validated canonical fixture, record the
small shared-behavior before reference described by v2, then move directly to M2's
controller/adapter unit. Do not reopen QB-REPLAY-008/012, build a broad corpus,
upgrade benchmark schemas, or reproduce the 8x failure before controller work.
This planning session stops after the corrected canonical state validates.
