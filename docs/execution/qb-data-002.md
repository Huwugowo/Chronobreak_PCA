# QB-DATA-002 execution checkpoint

Feature: `QB-DATA-002`
ExecPlan: `docs/exec-plans/qb-data-002-post-game-confirmation-v2.md`
Updated: 2026-10-08

## Current milestone

Planning pass finalized. QB-DATA-002 owns post-game exact EOG confirmation and
allowlisted final facts after the accepted QB-DATA-003 provisional handoff.

## Active unit

No product implementation has started. The user-reviewed v2 design is linked
above; the next implementation boundary is Milestone 1, defining the typed
result/coverage contract and synthetic confirmation fixtures.

## Completed

- Read the complete QB-DATA-002 feature entry, product/workflow/verification
  authorities, PLANS contract, recorder lifecycle and media-library architecture.
- Reviewed the sanitized 2026-09-29 EOG evidence: exact game identity agreement,
  result evidence, final-stat families and unavailable aborted Practice Tool
  behavior. History enrichment is intentionally deferred in v2. LOSS still
  requires explicit coverage.
- Accepted the user-directed boundary: QB-DATA-003 remains provisional; this unit
  alone may request EOG, transition to confirmed, and attach final facts. Recording
  duration is not an acceptance condition.
- Challenged retry bounds, in-memory job overflow, deletion races, alias/conflict,
  identity and consumer-compatibility risks. Accepted v2's deliberate deferrals:
  no persisted journal or recorder-restart recovery, no history endpoint and no
  redesign of app deletion. Rejected history-position matching, Live GameEnd
  inference, raw archives and a competing match-fact database.
- Adopted the user-reviewed immutable design at
  `docs/exec-plans/qb-data-002-post-game-confirmation-v2.md`; v1 remains
  superseded history.

## In flight

The feature is routed to implementation with no product code changed by the
planning pass. The candidate plan in ignored `build/` and v1 plan remain
provenance only.

## Remaining

1. Implement Milestone 1 result/coverage types, fixed allowlists and synthetic
   exact-confirmation fixtures.
2. Implement bounded in-memory EOG acquisition and retry expiry; no history
   endpoint or restart journal is part of v2.
3. Implement the generation-bound result writer and optional full-probe/library
   projections while preserving existing deletion semantics.
4. Integrate capture closure and app projections, then run real app-closed EOG,
   delayed/absent/aborted, loss-encoding and back-to-back-game verification.

## Verification

- Planning review covered exact `gameId` equality, local identity, participant
  conflicts, result precedence, missingness/provenance, bounded in-memory retry
  budgets, writer/deletion races and preservation of healthy media.
- No implementation or product verification is claimed yet. The finalized plan
  defines the synthetic, Windows filesystem, resource, recorder, app and manual
  checks required before completion.
- Cross-PC handoff, 2026-10-08: canonical validation passed for 62 items/10
  plan-checkpoint pairs. The reviewed v2 plan and this checkpoint are included in
  the source handoff; product implementation still starts at Milestone 1.

## Deviations

The original roadmap item was expanded from result-only acquisition to exact
provisional-candidate confirmation plus useful allowlisted final statistics, per
the user-approved data split. The user-reviewed v2 deliberately narrows the first
implementation to an in-memory post-game window: no history enrichment, persisted
job journal, recorder-restart recovery or deletion-protocol redesign. Recording
duration remains outside acceptance.

## Decisions

- EOG top-level `gameId` equality is mandatory; local-player/participant identities
  are corroboration. No history endpoint is used in v2.
- Only confirmed results may project WIN/LOSS or authoritative final totals.
- Missing, delayed, conflicting and aborted data remain explicitly partial or
  unknown; no unavailable result sidecar is created and values are never fabricated
  as zero.
- Attempts and pending jobs remain bounded in memory; recorder restart may lose the
  optional result. The late writer revalidates generation identity and yields to
  unchanged app deletion semantics.
- Provisional context, metadata.saved, video, game logs, replay clocks and Live
  polling remain owned by their existing features.

## Blockers

None for starting Milestone 1. Real LOSS encoding and app-closed post-game evidence
are verification targets, not planning blockers.

## Next action

Implement Milestone 1 from the user-reviewed v2 plan: add bounded result/coverage
types, the fixed EOG field mapping and synthetic WIN/LOSS/aborted/mismatch/conflict
fixtures before wiring transport or persistence.
