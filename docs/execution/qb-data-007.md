# QB-DATA-007 execution checkpoint

Feature: `QB-DATA-007`
ExecPlan: `docs/exec-plans/qb-data-007-ranked-lp-v2.md`
Updated: 2026-11-23

## Current milestone

Live League notification verification.

## Active unit

Observe the current ranked notification during an ordinary recorded ranked match when a signed-in League client is available.

## Completed

Audited LCU capture, exact recording identity, post-game enrichment, background preparation and History rendering. Superseded the initial plan after comparing transport cost and reliability, reviewing notification reasons and tracing grouped History records. Implemented bounded acquisition, durable immutable receipts, group projection, targeted preparation and stable History rendering.

## In flight

Automated verification is complete. Live verification is pending.

## Remaining

Confirm the ordinary post-game reason strings and sampling timing in a real ranked session.

## Verification

The full recorder suite passed 304 tests with 18 opt-in tests ignored across all targets and features. It covers stale or unrelated responses, exceptional signs and zero, durable shared scheduling, late publication and restart, and immutable conflicting receipts. The Windows fixture tests require normal directory-handle access and passed on rerun outside the restricted shell.

The desktop Rust suite passed 230 tests with 13 opt-in tests ignored. Focused checks prove that a receipt on a nonrepresentative part is visible, changing representatives preserves the delta, conflicting parts suppress it, and LP hints leave already prepared replay data ready. The production SQL plan searches the partial LP index by match group. The legacy snapshot continues to query while the additive migration runs.

The frontend suite passed all 643 tests, including a mounted row with unchanged focus and image nodes and no image source mutations when LP arrives. Type checking and the production build passed. Recorder check, strict Clippy for both Rust applications, formatting and canonical validation passed.

## Deviations

The account check shares the reserved notification slot. If it fails or is interrupted, a later slot can try again. The normal successful case still makes one identity request, and the failure case makes at most one per slot. No separate account reservation write is needed.

## Decisions

Reuse bounded post-game HTTPS checks and the existing history ledger. Require an exact match and genuine notification cause. Keep immutable part receipts and project their confirmed value by match group.

## Blockers

No League client process is currently available for live observation.

## Next action

Use a fresh ranked recording to verify the ordinary LCU payload and the resulting receipt and History row. The automated suites are complete.
