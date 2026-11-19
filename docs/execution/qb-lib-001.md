# QB-LIB-001 execution checkpoint

Feature: `QB-LIB-001`
ExecPlan: `docs/exec-plans/qb-lib-001-match-history-search.md`
Updated: 2026-11-19

## Current milestone

Milestone 3, full verification and the populated desktop walk-through.

## Active unit

None. The search performance follow-up and its automated verification are complete.

## Completed

Implemented the queue row, local played-champion picker, Favorites star, personal/custom Tags, secondary Filters form and clear behavior. The native query combines the selected criteria before keyset pagination. Added regression coverage for scope/revision attribution, grouped champion matching, queue taxonomy, custom tags, and refresh after an empty pending tag result. Updated both durable architecture sections.

Polished the toolbar with flat, underlined queue tabs, a 224 px champion field, and left-aligned 36 px controls. Search, Tags and Filters use Remix icons. The champion list opens on focus or click with up to six unselected champions. The controls use the demo's surface and border colors with the app's yellow for selected states and one focus border around the search field. Both Windows build profiles now use the graphical subsystem for desktop startup.

Reused the accepted snapshot's fingerprint, match count and frontend recording lookup. Materialized match coverage counts in SQLite and pruned candidate matches for slow multikill and absent-player searches.

## In flight

None.

## Remaining

Run the populated desktop walkthrough.

## Verification

The final native suite passed 216 tests with 11 explicitly ignored. Frontend passed 618 tests, TypeScript checking and production Vite build. Strict Tauri Clippy, Rust formatting, and the canonical roadmap validator passed. The release SQL fixture exercised 10,000 matches with 80 warm samples. p95 was 10.0 ms for browsing 50 rows and 16.8 ms for the 50-row combined queue, champion, player, personal moment, and custom tag query, below the provisional 50 ms history target. The optimized plan seeks events by both kind and media identity. These are local query measurements.

After the alignment and console fixes, all 618 frontend tests, TypeScript, the production frontend build, the debug desktop build, formatting and strict Tauri Clippy passed. The full Tauri rerun passed 216 tests with 11 ignored. Inspected the revised layout at 1440 × 900 with five browser preview matches. The screenshot is at `build/match-history-toolbar-alignment-1e2cff3b/toolbar.png`. Verified that the rebuilt executable declares the Windows GUI subsystem and preserves the exit code and redirected diagnostics for invalid `--prepare-library` arguments. The isolated desktop started on Match History with two recordings and no terminal in the window inventory.

For the focus and suggestion changes, 618 frontend tests and the production frontend and debug desktop builds passed. The browser smoke check confirmed an empty click opens the bounded list, the search uses one border, and ARAM hover opens the queue menu and dismisses suggestions. The refreshed desktop sample showed Akali and Master Yi on keyboard focus before any typing. Screenshots are under `build/match-history-focus-VfrU3C`.

Two paired Windows release runs used `representative_history_command_measurements` on identical 10,000-match fixture distributions with 80 warm samples per scenario. The timer covers Tauri's test transport, generated command handler, coordinator, a fresh SQLite reader, clip counts and response serialization. The fixture has one ready player and two events per synthetic recording, with one imported journal source. Catalog setup runs before timing. Each run checks the total match counts, expected result size and clip counts.

| Scenario | Rows | Before p95 (ms) | After p95 (ms) |
| --- | ---: | ---: | ---: |
| Browse | 50 | 23.6–31.9 | 9.4–11.7 |
| Queue, champion and Favorites | 50 | 22.2–36.9 | 12.6–22.9 |
| Queue, champion, player, Dragon steal and clip tag | 50 | 23.1–40.3 | 13.5–22.1 |
| Rare pentakill | 3 | 192.1–307.4 | 8.3–9.9 |
| Absent quadra | 0 | 247.2–282.8 | 6.6–9.7 |
| Nonexistent Riot ID | 0 | 269.2–290.2 | 6.0–10.2 |
| Old date range | 12 | 29.0–33.9 | 7.0–13.1 |

Raw p50/p95/p99 logs and preserved executables are under `build/perf/history-command-20261118`. Use `cargo test --locked --release --manifest-path app/src-tauri/Cargo.toml --features history-command-benchmark --lib representative_history_command_measurements -- --ignored --nocapture --test-threads=1` to reproduce the command measurements.
That command passed against the final tree and embedded Windows manifest. Its highest p95 was 13.9 ms on the combined filter.

The Windows app suite passed 231 tests with 12 opt-in tests ignored using `cargo test --locked --manifest-path app/src-tauri/Cargo.toml --all-targets --all-features`. Coverage checks exercised legacy backfill, multipart-group changes, rollback and readers spanning a writer commit. The frontend replacement test rejected a stale recording with a reused timestamp and admitted the correct media after retry. All 619 frontend tests, TypeScript, the Vite build, Rustfmt, strict all-target/all-feature Clippy and canonical validation passed.

## Deviations

The first benchmark measured the combined path at 1.19 s p95. Starting from the exact match group before seeking events and clips removed that bottleneck. The desktop executable built successfully. WebView2 could not write to the first scratch profile. A fresh isolated profile opened successfully with two matches. The approved walkthrough did not finish because the user stopped computer control before it could run.

The initial Tauri run after the console fix failed `invalid_ownership_foreign_entries_and_locked_files_are_retained` on the retained-count assertion. That check passed in isolation and in the full rerun.

The initial mock-dispatch harness build lacked the Windows Common Controls manifest and failed before entering the test. The preserved baseline ran after embedding the manifest. The first candidate timing spanned system sleep and was discarded. The two paired runs completed without that interruption.

## Decisions

Custom tags come from linked managed clips. The accepted queue taxonomy uses numeric queue facts. Unconfirmed outcomes remain unknown.

The performance follow-up adds a derived coverage projection to the existing v8 cache. Its first writer backfills from recording rows in one transaction. Existing read transactions can finish using their original schema and counts. The query prunes absent players and exact multikill candidates before card lookup, then applies the existing representative and actor checks.

## Blockers

The feature stays in progress pending the required desktop walkthrough with the updated frontend.

## Next action

Run the approved walkthrough against a fresh desktop build and the populated isolated fixture under `build/match-history-live-a396ffcf`.
