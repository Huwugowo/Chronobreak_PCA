# QB-LIB-001 library filters and detected match tags

## Purpose

Give Match History and Clips one shared search and filter row. Show the recorded player's best detected event beside KDA in Match History.

## Relevant planning-time architecture

`LibraryFilters` already renders both toolbars using the same queue and filter components. `ChampionAutocomplete` owns suggestion navigation. Clips also renders a separate text field and debounces its query in `ManagedClips`. History sends committed criteria through `HistoryController` and uses the accepted snapshot for its champion list.

The index already publishes local event tags from checked, ready recording parts. The event join binds the effective killer to the local participant in the same media, run, epoch, sequence and revision. It requires an explicit stolen flag. The independent personal-moment query repeats these checks. The current group tag projection also carries manual match assignments. Custom clip definitions and legacy match assignments share a library file, and clips also carry their definitions in headers.

## Scope and exploration findings

The user replaced manual match tags, linked clip tags in History, and the standalone personal-moment filter with read-only local event tags. Clip tag editing and discovery, queue/date/results, saved matches, and existing playback remain in scope for preservation. Earlier approved search behavior and verification outside the changed criteria carry forward from `qb-lib-001-match-history-search.md`.

## Chosen design and rationale

Render the queue row and a single search, Tags and Filters row in both tabs. History typing narrows local played-champion suggestions. Clips typing searches titles, tags and champions through the existing debounce. Clip champion suggestions use the same field; choosing one clears the free-text term and adds a champion chip. Keep History's Favorites in Filters and move the Clips folder action outside the shared row.

Remove the My events control, filter chips and typed query path, including the redundant raw-event predicate. The History Tags menu uses built-in event choices only. All selected tags must match; higher multikill tiers satisfy lower tiers. Use the existing indexed group projection to filter before pagination. Only detected rows may be read from an older cache. Stop projecting manual assignments and remove the match edit command and client update path. Retain legacy assignments in the library file for compatibility with existing data and continue to support clip vocabulary changes.

Choose one card badge from the checked local event projection, in order from Pentakill through Double Kill, then Baron, Herald and Dragon steals. Return a nullable detected event with each card and place it beside left-aligned KDA within the existing card rail. Filtering uses all qualifying detected events even when one higher-priority badge is displayed.

## Rejected alternatives and planning decisions

Hiding the duplicate control would keep two search meanings and a redundant native query. Rebuilding the SQLite cache is unnecessary because the existing source discriminator allows detected-only reads and existing checked positives remain valid. Preserve legacy authored data on disk and retire its History behavior.

## Milestones

1. Implement one detected-only native query and card response. Retire the History mutation and personal-moment paths. Reuse clip vocabulary and keep legacy library data readable.
2. Make the two tabs use the same input and filter row. Place the event by KDA and remove the old match tag strip and editor.
3. Verify the changed behavior, prior History interactions, bounded query performance, and the isolated populated library walkthrough.

## Verification design

Use deterministic app fixtures for matching all selected tags, lower-tier search, multipart and actor attribution, legacy manual rows, and priority across all steals. Exercise one search per tab, keyboard and hover, favorites, filter chips, clip search and selection, read-only card badges and opening/returning from a match. Run `cargo test --manifest-path app/src-tauri/Cargo.toml`, strict Clippy and Rustfmt for the app, `npm run test --prefix app`, `npm run check --prefix app`, `npm run build --prefix app` and the roadmap validator. Inspect the shared row and card placement at narrow and wide desktop widths.

## Performance and reliability gates

Retain local History suggestions and the existing 50-row and 2 MiB bounds, checked attribution and stable cursors. Adapt the generated 10,000-match query and command fixtures to use detected tags for common, rare and empty searches. The expanded query must meet the provisional 50 ms warm p95 target. Use only dedicated test libraries for mutations and desktop verification.
