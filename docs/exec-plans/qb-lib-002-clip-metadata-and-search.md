# Durable clip metadata, editing, and search

## Purpose

Players can name, label, and find clips by the game and moment they remember. Each exported file remains independently playable and retains its game context when the source recording disappears.

## Relevant planning-time architecture

The app owns the filesystem inventory in `library.rs` and its accepted snapshot in `library_coordinator.rs`. The exporter validates exact frame ranges and publishes under the owned staging lease. Mutations pin the approved root and use `LibraryMutation`. Protected playback validates the opened file. The disposable League index has bounded read and write ownership, checked source revisions, and prepared observations with effective actors and mapped replay ticks.

## Scope and non-goals

This feature owns new clip exports, durable clip facts, title and tag editing, and grouped Clips browsing and search. QB-LIB-003 owns Match History access and navigation from a clip into the viewer. Game acquisition, recorder preparation, encoding presets, and storage accounting semantics stay under their current owners.

## Exploration findings

The existing timestamp filename conflates presentation and identity and loses game context with its source. Grouped History already provides account-scoped match identity and checked display facts. Optional events come from the exact recording's committed projection. The replay window's inclusive SQL endpoint is unsuitable as the clip's exclusive end. A confirmed steal needs an explicit source flag. Title edits require a stable identity independent of the MP4 leaf.

## Chosen design and rationale

- Publish one UUID directory per exported preset under `clips`. Each bundle contains a titled MP4, thumbnail, bounded versioned `clip.json` and bounded immutable `events.json`. The header carries the clip ID, exact source media ID, canonical frame interval, opaque group, played date, champion, account, queue or mode, nullable result and final K/D/A, preset, validated duration, event availability, and selected/custom tags. The actual MP4 stem is the title. Match identity is account-scoped; an absent validated group falls back to the exact recording for display grouping.
- Capture context from the existing checked source and SQL reader once for the accepted draft. Reuse the native result bound to the root, snapshot, source stamp and frame range at export admission. The optional data can be unavailable without blocking export. Keep known mapped committed events in the half-open range. Keep only the highest multikill tier per resolved player, with the earliest tied occurrence, and a separate unattributed maximum. Candidate highlights are tiers two through five, First Blood and explicitly recorded objective steals. Default-select confirmed local highlights and let the user choose other labelled candidates. Persist selected highlight summaries for search, with references to the event file.
- Validate Windows filename stems and preview the actual filenames. Use an editable champion/date/position default and suffix format names for a multi-preset export. Preserve stable identity on a no-replace handle-relative MP4 rename. Save tag edits by atomically replacing the checked header. Preflight owned leaves before deletion, including exact directory and file identity. Leave unrecognized assets intact.
- Stage and validate complete directories before publication. Extend the existing lease, manifest and bounded cleanup to the owned bundle layout. Publish each UUID directory by an atomic, no-replace handle move. Partial multi-preset completion can expose a subset of complete outputs. Recovery touches only owned unpublished staging.
- Admit protected clip assets by stable ID and fixed asset kind. Use the accepted inventory map to resolve the checked relative leaf, then use the existing approved-root and opened-handle authority. Retire stale mappings on refresh, mutation and root switch. Persisted duration avoids further probes for healthy clips.
- Add a disposable clip projection under the existing index writer and readers without changing the existing cache version. Reconcile new, removed and changed descriptors in bounded work after the main inventory is accepted. The projection reads compact headers and never opens the event payload for browsing. A stable inventory stamp makes unchanged refreshes reusable. Check root, accepted snapshot and catalog revision at publication and query admission. Keep the Games path independent of clip projection readiness.
- Query group pages ordered by played date descending and stable group key, with clips ordered by part, start frame and clip ID. Filter individual clips first. Return a bounded first page per group and a continuation for more clips. Search titles, custom tags, saved game context and selected highlight labels. Multikill terms and filters match a minimum tier. Actor filters use recorded attribution. The player shows title, tags, saved context and optional recorded events. Reuse existing keyboard and mutation lifecycles for rename, tag edit and reveal.

## Rejected alternatives and planning decisions

A flat title-based pair would require coordinating multiple renames and gives no atomic clip publication boundary. The UUID bundle provides one directory move for publication and one MP4 leaf rename for a title edit. The native index projects compact headers so event payloads can stay lazy. Lower multikill tiers are dropped after the clip range check and search uses thresholds.

## Milestones

1. Define and verify the durable clip contract, protected asset identity, publication and guarded mutations.
2. Bind checked match and event context to export, including editor naming, selection and custom tags.
3. Reconcile the clip index and connect grouped browsing, search and clip detail actions.
4. Verify the complete storage, export, search and performance contract.

## Verification design

Use dedicated temporary libraries and generated media. Cover frame boundaries, local/ally attribution, missing mapping, explicit steals, max-tier reduction, selected versus unselected search and threshold filters. Cover interrupted publication, rename and tag replacement, collisions, malformed metadata, source deletion and cache rebuild. Run the app backend suite, Rustfmt and strict Clippy, frontend tests/type check/build, production desktop build and the applicable packaged export matrix from VERIFICATION.md. Retain evidence in the execution checkpoint.

## Performance and reliability gates

Compare prepared first paint and the large History fixture against repeated matched baseline runs. Added time must fit baseline run variation. Cold clip index reconciliation must not gate Games paint. Ordinary browse and range delivery perform zero journal reads, media probes, per-card filesystem reads or per-range directory enumeration. Keep bounded page sizes, read and writer ownership, cancellation, and generation checks. Validate playable output and unchanged source bytes after failure and interruption.
