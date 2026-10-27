# QB-DEV-007 execution checkpoint

Feature: `QB-DEV-007`
ExecPlan: `docs/exec-plans/qb-dev-007-viewer-benchmark-boundary.md`
Updated: 2026-10-27

## Current milestone

Planning handoff complete. Implementation has not started.

## Active unit

None. User requested a plan only for this feature.

## Completed

- Mandatory product/workflow/feature/plan-contract/verification bootstrap, QB-DEV-004 checkpoint then immutable v2 plan, and applicable current replay architecture reads.
- Two read-only scouts mapped remaining Viewer execution/ownership and measurement/test contracts. Root inspected critical readiness, synchronous seek/event, action, render/resize and terminal/cleanup paths.
- Independent plan review found two required clarifications: preserve existing secondary clip-preview media under production ownership, and specify the exact render subject/load/source fields and helper-owned action-entry identity comparisons. Both are incorporated. Root additionally made timer arming, successful endpoint cleanup, intentional media-release eligibility and opening workload IDs explicit; no code changed.
- Final immutable plan defines one harness-owned Viewer runner, bounded production host/shared seek primitive, real-source observation notifications, narrowed resize/render subjects and unchanged action/evidence/release/terminal contracts.

## In flight

None.

## Remaining

M1 current-production baseline builds/native workflows before source or test edits, then frozen contract coverage; M2 runner extraction; M3 automated/native/ordinary UI verification, boundary review, architecture and completion evidence.

## Verification

Planning-only canonical schema/invariant/linked-artifact validation and `git diff --check` passed. No QB-DEV-007 source/test edits, builds, product tests or native baseline/candidate runs were performed. QB-DEV-006's tool/native results prove its validator correction only; they are not a Viewer baseline or extraction acceptance.

## Deviations

None.

## Decisions

- Keep App coordination and synchronous captured cycle callback unchanged.
- Runner owns experiment policy/resources/evidence waiters; Viewer retains primary controller/data/layout/clip ownership and authoritative emitters. No raw controller/video exposure or new player.
- Native baseline must be captured against current executable inputs before editing source/tests; historical retained binaries are provenance.
- Preserve measurement/filter/timing contracts. Representative native and deterministic cleanup/order checks are mandatory; statistical/control escalation follows explicit observed triggers and remains necessary for numerical claims.

## Blockers

None for the planning handoff. Implementation is outside the current user request.

## Next action

When implementation is authorized, read this checkpoint before the immutable plan, set M1 as the active unit, and build/archive current normal and benchmark desktop baselines with input/fixture/runtime/tool hashes before changing any source or tests. Then collect the plan's native baseline workflows.
