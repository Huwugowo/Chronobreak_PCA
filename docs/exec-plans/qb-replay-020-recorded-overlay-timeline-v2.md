# QB-REPLAY-020 recorded overlay timeline v2

## Purpose

Keep immediate, stable champion status across seeks and show recorded spell icons when a replay first opens. Remove the visible team headings from the overlay while retaining player names, fixed rail positions and accessible grouping.

## Relevant planning-time architecture

The production filesystem → checked SQLite → desktop path supplies an independent compact overlay timeline. `ReplayOverlayController` owns one selected source, checked pagination, complete samples, consistent identities and preloaded spell variants. `ViewerScreen` projects it at accepted observed video positions; event windows have a separate lifetime. `ChampionFilter` retains each recorded spell image node and changes visibility according to the selected labels.

The earliest mapped Swiftplay roster arrives about 25 seconds after video zero. At zero, the release has ten portraits and twenty loaded spell images, but no selected spell labels. This is a before-first projection rule, not an image loading race. Identities contain no spells; final facts cannot supply all players' starting loadouts. Recorded Teleport and Smite variants change during these fixtures.

## Scope and non-goals

Preserve the existing compact timeline, SQLite query/IPC contract, strict source admission, temporal lookup, image lifetime, controls and isolation. Add only explicit first-recorded spell-loadout context before the first mapped sample and omit fullscreen team headings. No recorder, stored-schema, backend query, timing repair, asset coverage, event pagination or framework changes.

## Exploration findings

The selected-source timeline already contains the required spell labels. A global chosen asset variant, final loadout or arbitrary later non-null slot would misrepresent evolving spells. Before-first context must therefore derive only from the earliest complete mapped sample and retain its missing slots. It supplies no level, death, respawn, clock or score facts.

## Chosen design and rationale

Retain pages of at most 128 compact samples / 2 MiB, exclusive `(replay_tick, observation_id)` cursors and one committed revision. One selected source retains at most 131,072 samples / 64 MiB serialized samples; failures are explicitly unavailable. No raw payloads, recorder changes or persisted migrations. Complete loading is part of details readiness; optional failure never prevents video playback.

Binary search selects the latest complete sample at/before accepted observed video position (RVFC, or the existing explicitly approximate clock where unavailable). Pending pages cannot prove their final tick or later positions. Never substitute requested seek positions or stale state. Dead timers subtract replay elapsed time, rounding up at zero; only a recorded alive sample restores color. Preserve original signed game clocks and Practice Tool's unmapped/out-of-media facts.

On successful completion only, derive a distinct `initialLoadout` from the first mapped sample: its tick and participant IDs with two nullable spell labels. Before that tick, or before the first accepted video observation, use these labels as first-recorded context. Show their provenance in spell tooltips. This is an explicit opening spell exception to the previous before-first rule; it is not a temporal sample. Do not expose any other future field. Once a temporal sample is available, its slots replace the context completely, including nulls. Never use opening context for later missing samples, pending-page frontiers or errors. Source change/retry/disposal clears it. Fixed source identities and retained asset nodes remain the rendering owners.

Omit team headings in fullscreen overlay markup, keeping accessible group labels and panel/rail headings. Names remain above portraits without Riot tags.

## Rejected alternatives and planning decisions

Selecting the first preloaded asset conflates availability with loadout authority. Final facts lose starting spell variants and only cover the local player. Advancing video or fabricating a tick-zero roster invents temporal facts. The distinct small opening context uses the already validated first observation without changing timing or normal sample semantics.

## Milestones

1. Add source-owned opening spell context, narrow projection and fullscreen heading removal.
2. Verify first-open without timeline input, null slots, normal temporal replacement and source/error invalidation. Recheck distant paused seeks and existing status controls in the actual isolated desktop release.

## Verification design

Extend the existing focused controller/projection and viewer tests; run the frontend suite, TypeScript, production frontend/Tauri build, canonical validator and diff check. Existing unchanged backend evidence remains valid. On Swiftplay, open normally at paused zero without video/timeline input and require twenty visible loaded spell icons, ten named portraits, no headings and no future levels/death timers. Seek into recorded state and back to zero; retain nodes, upgrades and normal status behavior. Inspect ordinary and compact viewports. Use only the isolated preprod library.

## Performance and reliability gates

Preserve zero spell-node churn and no overlay/event reads during seeks after loading. Initial context adds only the first snapshot's compact spell slots, no extra query or asset request. Repeat the existing distant-seek measurement and status check for the affected projection. Preserve bounded memory, source isolation, unavailable fields and original mappings.
