# QB-LIB-002 execution checkpoint

Feature: `QB-LIB-002`
ExecPlan: `docs/exec-plans/qb-lib-002-clip-metadata-and-search.md`
Updated: 2026-11-11

## Current milestone

4. Verify storage, export, search and performance.

## Active unit

Run the app verification gates against the implemented clip workflow.

## Completed

The exporter publishes one complete versioned clip bundle per preset. The clip editor previews MP4 filenames and saves custom tags and selected in-range highlights. The Clips library uses the accepted inventory and paged SQLite projection for search, browsing, playback and editing. Persisted bundles retain their facts when the source is removed or the cache is rebuilt.

## In flight

None. Local verification is complete.

## Remaining

Run the packaged export and crash matrix. Compare prepared desktop first paint and exercise the full media workflow with generated recordings.

## Verification

The frontend type check, production web build and all 595 frontend tests passed. Rust formatting, strict Clippy, the `replay-benchmark` feature check and 210 native tests passed with a resource-free Tauri config. The native suite had 11 ignored tests. The Windows test runner inherited process-local settings for child PowerShell scripts and the invalid executable fixture. Native fixtures covered staging collisions, bounded recovery, checked renames, replacement media rejection, source deletion and cache rebuild, and highlight range and search semantics.

The production desktop build failed because the pinned runtime directory is missing. The runtime prepare script found no installed pinned FFmpeg. Source acquisition failed to connect to GitHub, and the local UCRT64 compiler was also absent.

## Deviations

None.

## Decisions

Use the earliest known recording-part timestamp as the played date when preparing a validated game group. Fall back to the source part date when the group date is unavailable. QB-LIB-003 consumes the clip contract.

## Blockers

Packaged build, export checks and prepared desktop benchmarks require `build/media-runtime/windows-x86_64`.

## Next action

Stage the pinned runtime and run the packaged and prepared desktop gates against a generated fixture library.
