# Match History and clip replay navigation v2

## Purpose

Players can play an exported clip from Match History and return to the match that opened it. A multi-clip strip expands as part of the card.

## Relevant architecture

Match History groups recordings by game and local account. The checked page contains clip counts keyed by the group. The managed clip projection serves bounded pages and holds exact source media and frame ranges. The existing managed player plays exported MP4s and opens the existing replay viewer at the clip start when its source is available.

## Scope and findings

A separate footer and a sibling strip currently add height and a gap. The History page carries counts, while visible clip cards are fetched on demand. Source recording clocks and the exported clip clock are independent.

## Design

- Move clip controls into the summary action rail beside recording metadata. Keep the fixed-width match layout and existing replay, save, menu and recording-parts actions. Hide the control for unresolved or zero counts. For exactly one, read the existing group page upon activation and open the sole accepted clip. Coalesce repeated requests and reject stale origins and superseded selections. If the page no longer contains exactly one clip, refresh the History count.
- For multiple clips, expand the bounded strip inside the article with one outer surface and a top divider. Keep continuation and keyboard disclosure. Each play button shows title, duration and sampled in-game time. Restore the originating strip and focus after playback or source replay.
- Enable sampled clock enrichment only for History strip reads. Validate the exact source bundle, media generation, current source stamp and cached timeline. Convert the frame boundary exactly and look up the last mapped roster sample at or before it within the default prepared scope. Return an optional clock on the response, label it approximately, and show unavailable when timing is missing. Keep clip persistence, export and protected playback unchanged.
- Resolve the clip's exact source part against the accepted inventory and seek the existing viewer to its first included frame. Preserve independently playable exports when the source is absent.

## Alternatives

Continue using the existing clip page for direct open. Prefetching each card would add I/O to ordinary History rendering. Source recording elapsed time cannot stand in for game time.

## Milestones

1. Integrate the action rail, direct playback and attached paged strip.
2. Enrich strip clips with optional scoped recorded clocks.
3. Verify the changed paths and finish the desktop navigation and prepared paint checks.

## Verification

Use generated recordings with multiple parts and accounts. Cover zero, single and multiple clips; stale and mismatched results; keyboard disclosure; game clocks with fresh and missing sources; exact source seek and Back focus. Run frontend tests, TypeScript, production frontend build, app backend tests, Rustfmt, strict Clippy and canonical validation. Exercise the packaged Windows app at minimum width using fixture clips and compare prepared History first paint.

## Performance and reliability

Initial History uses the established batched count and adds zero clip reads. Enrichment is bounded by the expanded page size and reuses source validation within the page. Preserve the existing protected media and root/revision admission rules.
