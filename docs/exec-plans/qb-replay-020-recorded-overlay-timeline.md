# QB-REPLAY-020 recorded overlay timeline

## Purpose

Make champion spells, levels, death status/countdowns and the recorded local HUD follow the presented video immediately across arbitrary distant seeks. Keep the existing fixed rails and production filesystem → SQLite → desktop path.

## Relevant planning-time architecture

`ViewerScreen` currently obtains its sole temporal roster from `ReplayDataController.activeWindow`. A two-second playhead bucket requests a debounced event page, with four retained pages. `Index::roster_at_replay` returns one mapped observation. This couples unrelated event paging and overlay availability. Static source descriptors already provide stable participant identities. RVFC is the presented-frame authority; a requested seek and `seeked` are not.

The disposable SQLite index already stores mapped rosters ordered by source, replay tick and observation ID, with deduplicated member payloads. Commands use the accepted library snapshot, selected media descriptor and current source stamp. These boundaries remain mandatory.

## Scope and non-goals

Add a compact paged overlay read model and independent source-scoped frontend reader; project that timeline at accepted video observations. Preserve fixed identity slots, decoded spell images, controls and event pagination. No recorder changes, persisted schema changes, timing repair, fake API, special preprod consumers, asset coverage expansion or new framework.

## Exploration findings

Large paused backward seeks expose a 220–280 ms unavailable interval; forward seeks briefly retain old levels. The existing future-sample guard is correct but cannot manufacture a missing target observation. Real Swiftplay has 488 mapped roster observations across ten participants: compact full snapshots are roughly 557 KB, versus megabytes of raw source payloads. Source-wide eager raw rosters are inappropriate.

## Chosen design and rationale

Expose a production overlay-page command, selected and authorized exactly like the existing replay-window command. Each page contains source identity, local participant, the committed journal revision, ordered compact complete samples, and an exclusive `(replay_tick, observation_id)` continuation. Continuations must supply the same revision; reject a changed committed prefix instead of mixing pages. The first page also owns source-wide static identities, which event paging cannot clear. Each sample retains original game clock and mapped replay tick, participant IDs, optional level/dead/timer, two recorded spell labels, and optional local KDA/CS. Omit raw descriptions, items, evidence duplication and event payloads. Absence is preserved; no interpolation or invented alive state. Exclude unmapped and outside-media observations.

Bound pages to 128 samples and 2 MiB serialized data, with checked increasing cursors. A separate frontend controller owns one selected source, one page request at a time, source-generation cancellation, errors and background continuation. Retain at most 131,072 samples / 64 MiB serialized samples; on capacity failure report unavailable rather than silently truncating a complete timeline. This covers the 24-hour ordinary two-second roster cadence while bounding pathological input. Release retained samples on source change/dispose. No full-array cloning on each page.

Use binary search to select the latest complete sample at/before the accepted video tick, with observation ID ordering breaking equal-tick ties. Preserve the existing explicitly approximate media-clock path only on platforms without RVFC; it cannot become frame-seek authority. While pages remain pending, positions at/after the last loaded replay tick are not proven complete and remain unavailable; earlier covered positions are usable. After final continuation all positions are covered, including before-first unavailable and after-last predecessor semantics. Never substitute request positions, future samples or a stale target lookup. Failures do not affect video playback and retry restarts the selected timeline.

Event windows depend only on selected source and viewport. Their legacy single-roster field may remain for compatibility, but no temporal overlay consumer reads it. Static source identities retain their own source lifetime. Spell image nodes remain mounted; data lookup changes bindings, not rail ownership. Asset unavailability keeps existing placeholders.

## Rejected alternatives and planning decisions

Keeping the old roster visible hides the symptom while showing incorrect state. Increasing the event LRU or reducing its debounce leaves arbitrary seeks dependent on SQL and unrelated events. Prefetch at seek dispatch races video presentation and adds seek orchestration. A small compact timeline loaded in bounded pages removes that coupling without changing recorder data or the playback controller. Full snapshots avoid reconstructing sparse patches and preserve unavailable fields.

## Milestones

1. Add checked compact SQLite page projection, accepted-snapshot command and strict frontend decoder. Verify ordering, scope, pagination and exclusion of unmapped samples.
2. Add independent bounded timeline loading and binary lookup; connect frame-driven overlay projection and viewport-only event requests. Preserve source identity and image nodes.
3. Run focused and project checks, then measure large paused jumps in the actual Swiftplay desktop. Inspect timer/death transitions, before-first unavailable, playback/rate, fixed rails, event filters and fullscreen. Record truthful evidence and leave a current production preprod available.

## Verification design

Use the existing Rust and Vitest suites. Add only meaningful tests for page contract, strict decoding, lookup/frontier/equal ticks, source switch/disposal, and event-page independence. Run frontend suite, TypeScript, production build; app Rust tests, fmt and Clippy; canonical validator and diff check. Use the isolated preprod library and real Swiftplay/Practice source facts with their synthetic videos. No personal library mutation.

## Performance and reliability gates

Measure frame-by-frame overlay state after accepted RVFC across alternating distant paused seeks. Once the timeline is loaded, require zero blank or incorrect-level frames and no unchanged spell-node removals/loads. Confirm seeking does not request overlay SQL or event pages. Record initial page count, retained samples/bytes and load latency. Preserve strict source admission, bounded requests/memory, no future-state leakage, and Practice Tool's discontinuous/unavailable mappings.
