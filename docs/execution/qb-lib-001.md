# QB-LIB-001 execution checkpoint

Feature: `QB-LIB-001`
ExecPlan: `docs/exec-plans/qb-lib-001-match-history-search.md`
Updated: 2026-11-18

## Current milestone

Milestone 3, full verification and the populated desktop walk-through.

## Active unit

Final verification of the integrated toolbar and its snapshot-bound query.

## Completed

Implemented the queue row, local played-champion picker, Favorites star, personal/custom Tags, secondary Filters form and clear behavior. The native query combines the selected criteria before keyset pagination. Added regression coverage for scope/revision attribution, grouped champion matching, queue taxonomy, custom tags, and refresh after an empty pending tag result. Updated both durable architecture sections.

Polished the toolbar with flat, underlined queue tabs, a 224 px champion field, and left-aligned 36 px controls. Search, Tags and Filters use Remix icons. The champion list opens on focus or click with up to six unselected champions. The controls use the demo's surface and border colors with the app's yellow for selected states and one focus border around the search field. Both Windows build profiles now use the graphical subsystem for desktop startup.

## In flight

None. Built the debug desktop and prepared an isolated copy of the checked-in two-game preprod library with a tagged clip and a saved match.

## Remaining

Run the populated desktop walkthrough once an isolated window can be accessed.

## Verification

The final native suite passed 216 tests with 11 explicitly ignored. Frontend passed 618 tests, TypeScript checking and production Vite build. Strict Tauri Clippy, Rust formatting, and the canonical roadmap validator passed. The release SQL fixture exercised 10,000 matches with 80 warm samples. p95 was 10.0 ms for browsing 50 rows and 16.8 ms for the 50-row combined queue, champion, player, personal moment, and custom tag query, below the provisional 50 ms history target. The optimized plan seeks events by both kind and media identity. These are local query measurements.

After the alignment and console fixes, all 618 frontend tests, TypeScript, the production frontend build, the debug desktop build, formatting and strict Tauri Clippy passed. The full Tauri rerun passed 216 tests with 11 ignored. Inspected the revised layout at 1440 × 900 with five browser preview matches. The screenshot is at `build/match-history-toolbar-alignment-1e2cff3b/toolbar.png`. Verified that the rebuilt executable declares the Windows GUI subsystem and preserves the exit code and redirected diagnostics for invalid `--prepare-library` arguments. The isolated desktop started on Match History with two recordings and no terminal in the window inventory.

For the focus and suggestion changes, 618 frontend tests and the production frontend and debug desktop builds passed. The browser smoke check confirmed an empty click opens the bounded list, the search uses one border, and ARAM hover opens the queue menu and dismisses suggestions. The refreshed desktop sample showed Akali and Master Yi on keyboard focus before any typing. Screenshots are under `build/match-history-focus-VfrU3C`.

## Deviations

The first benchmark measured the combined path at 1.19 s p95. Starting from the exact match group before seeking events and clips removed that bottleneck. The desktop executable built successfully. WebView2 could not write to the first scratch profile. A fresh isolated profile opened successfully with two matches. The approved walkthrough did not finish because the user stopped computer control before it could run.

The initial Tauri run after the console fix failed `invalid_ownership_foreign_entries_and_locked_files_are_retained` on the retained-count assertion. That check passed in isolation and in the full rerun.

## Decisions

Custom tags come from linked managed clips. The accepted queue taxonomy uses numeric queue facts. Unconfirmed outcomes remain unknown.

## Blockers

The feature stays in progress pending the required desktop walkthrough with the updated frontend.

## Next action

Run the approved walkthrough against a fresh desktop build and the populated isolated fixture under `build/match-history-live-a396ffcf`.
