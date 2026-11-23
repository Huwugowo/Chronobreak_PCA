# QB-LIB-002 execution checkpoint

Feature: `QB-LIB-002`
ExecPlan: `docs/exec-plans/qb-lib-002-clip-metadata-and-search.md`
Updated: 2026-11-22

## Current milestone

4. Verify storage, export, search and performance, including the requested tagging correction.

## Active unit

Prepared desktop performance and the remaining generated-media walkthrough.

## Completed

The exporter publishes one complete versioned clip bundle per preset. It previews MP4 filenames, saves selected manual tags, and derives read-only assignments from verified in-range events. The Clips library uses the accepted inventory and paged SQLite projection for search, browsing, playback and editing. Persisted bundles retain their facts when the source is removed or the cache is rebuilt.

One searchable pill picker now handles filter selection and metadata edits. Creating a definition selects it in the active editor. The exporter waits for that write before submitting metadata. Tag mutations share the index writer with background recording preparation. Cards show all saved badges, with color and editability derived from the assignment's source.

Selected tag definitions travel with the filter selection and supply labels immediately on remount. Indexed queries receive IDs. The shared suggestions dropdown floats above the editor field and below the toolbar field, with ordinary outside-press and focus dismissal.

## In flight

None.

## Remaining

Compare prepared desktop first paint with matched baseline runs and complete the broader media workflow with generated recordings.

## Verification

All 642 frontend tests and the Windows desktop release build passed after the picker follow-up. The isolated release walkthrough verified unchanged Save and Export bounds across open and close, first-click submission with suggestions open, label retention across Clips and Match History without a tag lookup, and a renamed chip with the same clip card. Evidence is in `build/verification/clip-tagging/follow-up-N3OT68/browser-report.json`.

The Windows release build, Rust formatting, strict app Clippy, and the all-target all-feature native suite passed. The native suite passed 241 tests with 14 ignored, including a held-index regression that verifies the subsequent durable definition and indexed prefix lookup. All four targeted packaged export tests passed with the staged runtime, including fractional duration, silent source, collision isolation and crash recovery.

All 640 frontend tests, TypeScript and the production web build passed. The Windows release walkthrough created a custom tag by keyboard and exported it after two manually assigned event definitions. The card displayed all three grey badges. The copied clip header contained the assignment and the library stored the definition. Searching found the custom tag beyond the initial 24 suggestions and immediately filtered the grid. A later editor reused it, created a second name by mouse and saved both with one click. The existing clip card kept its DOM identity and showed the updated badges. Both toolbars stayed aligned with one primary search. Report and screenshots are under `build/verification/clip-tagging`.

The frontend type check, production web build and all 595 frontend tests passed. Rust formatting, strict Clippy, the `replay-benchmark` feature check and 210 native tests passed with a resource-free Tauri config. The native suite had 11 ignored tests. The Windows test runner inherited process-local settings for child PowerShell scripts and the invalid executable fixture. Native fixtures covered staging collisions, bounded recovery, checked renames, replacement media rejection, source deletion and cache rebuild, and highlight range and search semantics.

The earlier production desktop build was blocked by a missing pinned runtime. The runtime prepare script found no installed pinned FFmpeg. Source acquisition failed to connect to GitHub, and the local UCRT64 compiler was also absent at the time.

## Deviations

None.

## Decisions

Use the earliest known recording-part timestamp as the played date when preparing a validated game group. Fall back to the source part date when the group date is unavailable. QB-LIB-003 consumes the clip contract.

## Blockers

None.

## Next action

Resume the prepared desktop performance comparison.
