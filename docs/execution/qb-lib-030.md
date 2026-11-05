# QB-LIB-030 execution checkpoint

Feature: `QB-LIB-030`
ExecPlan: `docs/exec-plans/qb-lib-030-grouped-match-history.md`
Updated: 2026-11-05

## Current milestone
Complete.

## Active unit
None.

## Completed
- Group keys travel from validated recording context to accepted snapshots and the disposable index. History chooses one representative before filtering and paging. Search can inspect any part, and the chooser keeps each part's playback and storage actions.
- Normal library refresh uses the guarded Windows move to archive complete unmatched bundles. Settings exposes the folder and failures. Retention applies the existing protections across both recording collections.
- The generated replay corpus carries synthetic match context so new benchmark fixtures stay eligible for History.

## In flight
None.

## Remaining
None. The independent loading-screen comparison is tracked in QB-CAP-006.

## Verification
- Windows app tests passed (194 passed, 10 ignored). The grouped test covered three playable parts, an account with the same game ID, invalid identity, newest final facts, cross-part event search, Saved, card filters and stable cursoring. It fetched all three replay videos and the clip whose source was archived through the app's playback server. The bundled FFmpeg decoded the same generated video bytes without errors.
- Temporary libraries verified complete archive moves, whole-bundle byte preservation, collision and invalid-destination safety, read-only refresh, stable storage totals, and saved and incomplete retention protection.
- App formatting and strict all-target Clippy passed. Frontend type checking, 580 tests and production build passed. The chooser test covered card counts, chronology, single-recording direct open, and per-part actions. The generated replay corpus suite passed all 16 tests with the staged runtime.
- The Windows debug query fixture used 10,000 catalog rows and 80 warm samples per query. Default and event-filtered 50-row History p95 were 18.9 ms and 20.7 ms. The final SQL plan walks the date index in order and uses the match index for member lookups. These timings cover local SQL and row projection.
- A follow-up on three Thresh cards found that the existing desktop builds predated the grouping change. Read-only metadata inspection found one shared valid match identity across all three recordings. The Tauri release build succeeded and the portable release at `build/release/grouped-history-20261105` passed runtime verification and executable hash comparison. Live library confirmation awaits opening that build.
- Canonical validation passed for 116 roadmap items and 31 plan/checkpoint pairs. Diff whitespace checks passed.

## Deviations
The chooser expands below the selected card. A synthesized benchmark match sidecar preserves History eligibility for newly generated benchmark corpora.

## Decisions
Scope keys to the existing validated local account and game ID. Preserve existing checkout edits.

## Blockers
None.

## Next action
None.
