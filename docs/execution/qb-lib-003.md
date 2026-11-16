# QB-LIB-003 execution checkpoint

Feature: `QB-LIB-003`
ExecPlan: `docs/exec-plans/qb-lib-003-match-clip-navigation-v2.md`
Updated: 2026-11-16

## Current milestone

Complete. Compact clip navigation and History performance are verified.

## Active unit

None.

## Completed

The action rail, single-click lookup and attached strip are implemented on the checked multipart counts and exact source-frame navigation. The backend supplies an optional sampled clock for expanded pages. The player's Close and the viewer's Back action both restore focus to the originating control. The strip stays expanded and keeps its loaded thumbnails during the source navigation.

## In flight

None.

## Remaining

None.

## Verification

- On 2026-11-16, all 602 frontend tests and 212 backend tests passed. The backend suite reported 11 explicitly ignored tests. Strict Clippy, Rustfmt, the production frontend and desktop builds, and the canonical validation passed.
- The isolated Match History commit snapshot passed all 600 frontend tests, the production frontend build and canonical validation.
- The 1,200 px browser fixture reserved 17 px for a scrollbar, kept all three collapsed cards at 1,120 × 109 px, and verified keyboard disclosure and zero initial clip reads. The backend fixtures also covered account isolation, multipart clocks, pagination, exact frame ordering and absent timing.
- The packaged Windows fixture with the pinned runtime decoded the single clip and both thumbnail cards displayed their own recorded clock. Opening a clip's source sought to 45 seconds. Closing the player and using the actual header Back button both restored focus. The strip stayed expanded. A clip still decoded with its source held outside the isolated library and retained its game context. Deleting a generated clip removed its export and preserved the source video's SHA-256 digest.
- Prepared process-to-settled-paint pairs on the same two-match fixture and warmed WebView profile were 1,094 → 962 ms, 1,043 → 985 ms and 1,060 → 1,087 ms. This is a small startup comparison against the prior local Windows build. The unchanged 10,000-recording SQL path previously measured p95 pairs of 9.38 → 9.76 ms, 7.39 → 7.84 ms and 9.08 → 7.61 ms.

## Deviations

The user revised the visible entry point to single-clip direct play and requested the expanded strip share the card surface. The original ExecPlan is retained. Native verification exposed that the header Back button bypassed the clip return path and that unchanged clip query inputs could reload the strip. Both were corrected and verified in the rebuilt package.

## Decisions

Reuse the managed clip player and existing bounded page query. Display the last mapped sample at or before the first source frame as approximate game time.

## Blockers

None.

## Next action

No remaining action for this feature.
