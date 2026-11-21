# QB-LIB-001 library alignment and Favorites

## Purpose

Align the first Clips thumbnail with the shared toolbar. Restore the one-click Favorites filter in Match History.

## Relevant architecture and scope

History and Clips use `LibraryFilters` for the queue row and shared search, Tags, and Filters controls. History owns `favorites` and maps it to `saved_only` through `historyQueryForFilters`. The native filter, clear behavior, and replay navigation already use this state. The Clips grid currently uses the full library width while the toolbar is centered on the History rail. Inline match clips have separate layout.

The checked local event projection, detected-only History tag filter, and badge priority established in `qb-lib-001-library-filters-v2.md` still apply. Clip playback and projection retain their existing ownership and must be checked on first open.

## Chosen design

Apply the shared desktop width to the History and standalone Clips sections. Keep Clips responsive below that width, with its toolbar and grid sizing from the same parent. The grid columns continue to follow the space within the grid. The toolbar uses the full width of the section in each tab.

Place the History-only star between Tags and Filters, as in the earlier toolbar. Keep its pressed state and accessible name, and apply changes directly through the existing History filter patch. Remove the moved checkbox from the advanced form. Favorites remains a removable chip and participates in Clear all with the other criteria.

## Alternatives

An offset on the first thumbnail would leave the grid using a different width from the toolbar. A second Favorites control in the form would duplicate the star and introduce unnecessary draft state.

## Milestones and verification

1. Align the standalone Clips section and restore the quick Favorites toggle. Check the existing History integration tests for immediate query, composition and clear behavior. Assert Clips has the shared search and controls without a Favorites button.
2. Run the frontend suite, TypeScript and production frontend build, the Windows desktop build, and canonical validation. Use an isolated populated library at 1200 and 1440 pixels to check the left edges of search and first thumbnail, History Favorites on/off and card position, and the first clip open from a fresh session. Repeat the affected scenarios when a verified playback issue changes their dependency cone. Preserve valid native verification for the unchanged event-tag query.
