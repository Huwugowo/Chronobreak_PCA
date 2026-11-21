# QB-LIB-001 execution checkpoint

Feature: `QB-LIB-001`
ExecPlan: `docs/exec-plans/qb-lib-001-library-filters-v3.md`
Updated: 2026-11-21

## Current milestone

The v3 layout and Favorites follow-up, K/D/A alignment, and highest-multikill presentation are verified.

## Active unit

None.

## Completed

History uses the shared searchable tag picker with built-in events. A card displays at most the local player's highest verified multikill. The index still filters by objective steals and lower tiers implied by higher kills.

Match cards center the K/D/A above the items. The ratio and event badge share a centered second line, and the ratio stays centered by itself on an untagged card. Both variants reserve the same height.

The v3 pass gives the standalone Clips grid and both toolbars the same section width. The History star immediately applies Favorites and the Filters form owns player, date, and result. The Clips grid automatically retries a busy native read up to twice before offering manual retry, and stops issuing reads for a superseded request.

The v2 native query exclusively uses detected tags and returns the highest-priority badge. Removed manual match mutation and the separate personal-moment query. Both tabs use the same single-input toolbar. The card badge appears beside KDA.

The earlier design introduced the queue row, local played-champion picker, Favorites, the Tags and Filters menus, and clear behavior. The native query combines the selected criteria before keyset pagination. Added regression coverage for scope/revision attribution, grouped champion matching, queue taxonomy, and refresh after an empty pending tag result. Updated both durable architecture sections.

Polished the toolbar with flat, underlined queue tabs, a 224 px champion field, and left-aligned 36 px controls. Search, Tags and Filters use Remix icons. The champion list opens on focus or click with up to six unselected champions. The controls use the demo's surface and border colors with the app's yellow for selected states and one focus border around the search field. Both Windows build profiles now use the graphical subsystem for desktop startup.

Reused the accepted snapshot's fingerprint, match count and frontend recording lookup. Materialized match coverage counts in SQLite and pruned candidate matches for slow multikill and absent-player searches.

Reserved the viewport's vertical scrollbar gutter on Match History so applying or clearing filters cannot shift the card rail sideways.

Matched the Ranked, ARAM and Other menu gaps to the Filters popup's 4 px spacing. The gap is transparent and remains part of the hover area.

## In flight

None.

## Remaining

None.

## Verification

The current native fixture checks a steal-only match with no badge, every supported steal filter, the four verified local multikill tiers, and exclusion of unverified and authored events. The frontend covers keyboard selection through the shared History Tags control.

The K/D/A refinement passed all 640 frontend tests, TypeScript, the Vite production build, and the Windows release build. The isolated desktop walkthrough confirmed the lower-row alignment and equal spacing on tagged and untagged cards at 1200 and 1440 px, as well as thumbnail alignment, filter interactions, replay return, and first-click clip playback. Screenshots and report are under `build/verification/qb-lib-001-kda`.

The v3 run passed all 640 frontend tests, TypeScript, the Vite production build, and the Windows release desktop build. The focused regression reproduced the manual retry prompt from a native `busy` response before the fix. Isolated frontend tests verified that temporary contention recovers automatically, repeated contention reaches the manual retry limit, and a replaced catalog stops further reads against the old token.

The Windows release walkthrough measured exact left-edge alignment between the first decoded thumbnail and Clips search at 1200 and 1440 px. The History-only Favorites star applied and cleared without horizontal card movement and remained selected after replay return. The fixture clip opened and advanced video playback on the first click in a fresh session. Tag filtering and Clips title, tag and champion search also passed. Screenshots and report are under `build/verification/qb-lib-001-v3`. Canonical validation and `git diff --check` passed.

The following verification records the v2 design and the unchanged native query.

2026-11-21: The complete all-target/all-feature Tauri test run passed 240 tests with 14 ignored, including event priority and legacy manual-cache coverage. App Rust formatting and strict all-target/all-feature Clippy passed. All 638 frontend tests, TypeScript, the Vite production build, and the debug and release Windows desktop builds passed. Canonical validation passed for 138 items and 36 plan/checkpoint pairs.

Both updated release fixtures used 10,000 matches and 80 warm samples per case. The final Tauri command run reached 9.4 ms p95 for browsing, 21.1 ms for the combined queue/champion/player/tag query, 15.2 ms for three Pentakill matches and 12.6 ms for an empty Herald Steal query. The standalone SQL fixture reached 21.5 ms p95 for combined tier and exact tag predicates. Both combined queries met the 50 ms target. Logs are under `build/verification/qb-lib-001-v2`.

The isolated Windows walkthrough passed with two copied recordings and a fixture clip. At 1200 and 1440 px, History and Clips had identical measured search, Tags and Filters control bounds. The recorded player's Pentakill was the only badge on the matching card and appeared to the right of left-aligned KDA. Ranked, ARAM and Other stayed open across their hover gaps. History also passed keyboard and pointer champion selection, lower-tier and combined event filtering, Favorites without horizontal card movement, player/date/result filters, and replay return with filters restored. The single Clips input matched title, custom tag and champion, produced an empty result for a missing term, and accepted keyboard champion selection. The clip's tag editor remained available and the fixture's legacy History assignments were unchanged on disk. The screenshots and completed desktop report are under `build/verification/qb-lib-001-v2`.

The following verification records the earlier design.

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

An isolated Edge preview at 1200×680 and 1440×680 with classic scrollbars reproduced a 7.5 px sideways shift when Favorites reduced the history from five cards to one. With the gutter reserved, toggling Favorites on and off kept the rail and card positions identical at both widths, with no horizontal overflow. The Clips tab retained its default gutter behavior. All 619 frontend tests, TypeScript, the Vite build and canonical validation passed for this change.

An isolated Edge preview measured a 4 px gap for Filters and all three queue menus at 1200×680 and 1440×680. Each queue menu stayed open with the pointer paused in the gap and closed after the pointer left. Arrow Down, Escape, and pointer selection worked. All 619 frontend tests, TypeScript and the Vite build passed.

The production Tauri rebuild passed and was staged at `build/release/chronobreak-match-history-20261119-134259`. The staged executable matched the rebuilt binary, declared the Windows GUI subsystem, and contained the updated stylesheet. The portable media runtime passed its verification.

## Deviations

The user moved the card's event badge to the ratio line and restored centered K/D/A. The feature acceptance criterion records the new placement.

The user replaced the old personal-moment and manual/linked History tag design and requested toolbar parity with Clips. The v3 ExecPlan supersedes v2 for section alignment and the History-only Favorites shortcut.

The first benchmark measured the combined path at 1.19 s p95. Starting from the exact match group before seeking events and clips removed that bottleneck. The desktop executable built successfully. WebView2 could not write to the first scratch profile. A fresh isolated profile opened successfully with two matches. The approved walkthrough did not finish because the user stopped computer control before it could run.

The initial Tauri run after the console fix failed `invalid_ownership_foreign_entries_and_locked_files_are_retained` on the retained-count assertion. That check passed in isolation and in the full rerun.

The initial mock-dispatch harness build lacked the Windows Common Controls manifest and failed before entering the test. The preserved baseline ran after embedding the manifest. The first candidate timing spanned system sleep and was discarded. The two paired runs completed without that interruption.

## Decisions

History now uses only detected event tags. The accepted queue taxonomy uses numeric queue facts. Unconfirmed outcomes remain unknown.

The performance follow-up adds a derived coverage projection to the existing v8 cache. Its first writer backfills from recording rows in one transaction. Existing read transactions can finish using their original schema and counts. The query prunes absent players and exact multikill candidates before card lookup, then applies the existing representative and actor checks.

## Blockers

None.

## Next action

No remaining implementation or verification work for QB-LIB-001.
