# QB-LIB-001 Match History search

## Purpose

Use the approved Match History demo as the production interaction. Make queue and played champion fast to choose. Keep favorites and personal tags close by, with player presence, recording date and result behind Filters.

## Relevant planning-time architecture

`app/src/components/LibraryScreen.tsx` renders Games from the accepted library snapshot and `HistoryController`. The snapshot already carries the checked recording's local champion, recording time, saved flag and account-scoped match key. The controller admits query results against the accepted snapshot and retains the previous page until a replacement settles. Cards own replay, multipart, save and clip actions.

`app/src-tauri/src/league_index/query.rs` queries checked SQLite facts. `recordings` carries raw queue ID, champion, outcome, recorded date and coverage. The newest part anchors the keyset cursor and the best checked part supplies card facts. `scopes.local_participant` and `effective_event_actors` bind attribution to the exact recording, run and epoch. `league_index/clips.rs` has an independently reconciled clip header/tag projection keyed by the accepted clip inventory stamp. Catalog writes, raw media, recording sidecars and clip headers keep their existing ownership.

## Scope and non-goals

Implement the approved queue row, played-champion picker, personal and custom tags, star, and secondary Filters form. Keep current card structure and replay, clip, save, pagination and navigation behavior. Custom tags filter matches by tagged managed clips linked to the exact account-scoped match. Enemy champions, other players' events, new match annotation storage and recorder changes are outside this release.

## Exploration findings

Riot's queue definitions provide Solo/Duo (420), Flex (440), ARAM (450), Mayhem (2400), Arena (1700, 1710), Normal Draft (400), Swiftplay (480) and Pick URF (1900). A current versioned client-observation matrix supplies the 2026 Ranked 5s (710), Mayhem Classic-ish (2450, 3280) and Arena (1740, 1750) IDs absent from Riot's static list. Retain documented equivalent legacy IDs where unambiguous, including legacy ranked teams (42). Unknown queue evidence remains visible under All. [Riot queue definitions](https://static.developer.riotgames.com/docs/lol/queues.json) · [Versioned queue support matrix](https://github.com/shepherdjerred/monorepo/blob/f27ff2b48f167306d4ba4afb9f419d293ade2df5/packages/scout-for-lol/docs/queue-support-matrix.md#L11-L27)

Validated match-history sidecars can confirm either outcome. Older EOG-only or incomplete recordings can lack confirmation. Personal multikills and objective steals need the recorded local scope and resolved killer. A scope from another part, account or source epoch cannot attribute the event.

The existing replay benchmark driver issues synthetic free-text searches and measures its old debounce. Keep that diagnostic functional in the benchmark session only, through its existing query and instrumentation. Production champion typing has no history scheduling path.

## Chosen design and rationale

Extend the typed History query. Preserve legacy IPC fields for current callers and benchmark fixtures. A new queue choice maps to a centralized numeric taxonomy in the backend. Other means any checked queue ID outside the displayed Ranked, ARAM and Arena sets. The Other menu also offers Normal Draft, Swiftplay and URF. A missing queue ID is unavailable for specific queue filters. A new played-champion array matches if any accepted constituent part has any selected canonical champion. A player's full Riot ID is an independent checked roster predicate. Typed date bounds are local calendar presets converted to half-open UTC instants. Compare the newest part's recorded instant. Use the existing checked outcome and group-any saved predicates.

For each selected personal moment, require an observed matching event from a ready, checked part. Use exact recorded streaks of four and five. Require the explicit stolen flag alongside the resolved local killer for Baron/dragon steals. Join local attribution through the same media, run, epoch, event sequence and revision. For each selected custom tag, require the existing normalized clip tag on an accepted linked managed clip. Tags on multiple clips may jointly satisfy the match. Read that projection only when its inventory stamp is ready. A coherent incomplete state keeps semantic results from being presented as settled. A clip projection becoming ready triggers the existing history refresh even when the current filtered result is empty. A clip projection error surfaces as unavailable. Existing query admission, response limits and cursor order apply to all filters.

Use account-scoped grouping before paging. Resolve every predicate against persisted group/representative facts before the SQL limit. Bind a bounded set of values through parameters. Use the existing group, event and clip indexes. Add no schema migration, writer, raw journal reads, SQL queries on champion keystrokes or media work.

Create a focused Solid Match History controls component using Chronobreak styling. Give All, Ranked, ARAM, Arena and Other a dedicated row. A pointer may open a group by hover and move into its panel. Click/tap and keyboard can open and select, Escape dismisses, and opening alone leaves the filter intact. Build a memoized champion index from eligible accepted recordings, independent of the current query and page. Normalize discovery keys, order exact/prefix before substring, show at most six suggestions with existing Data Dragon portraits and text fallback, and exclude selected champions. Arrow keys, Enter and pointer selection share one commit path. Render removable chips under the toolbar. Clear all clears queue, favorites, champions, tags and advanced fields. The collapsed form validates the full Riot ID, applies date presets and result in one commit, and supports clearing individual criteria. Fetch custom tag choices through existing inventory-bound clip suggestions when Tags is open, with prefix discovery for more than one suggestion page. Keep read failures and stale answers local to that menu. Preserve selected criteria while a replacement loads. Show a distinct filtered empty state when the current query settles.

## Rejected alternatives and planning decisions

Filtering loaded cards would miss games outside the first page. A separate search database would duplicate the existing read model. A broad mode name cannot establish particular ranked and ARAM variants. A match annotation store would add durable state while linked clip tags already serve the approved search. Triggering native history on text edits would spend work before a criterion exists. Keeping all-player event controls in the first release would reintroduce the complexity the demo removes.

Review the combined fixture specifically for incorrect joins across source scopes and clip inventory changes. Preserve the group key, representative and continuation independent of filter evidence. Check old and absent facts, tag projection pending/error/settlement, calendar boundaries, and viewport focus. No capture or storage work is needed.

## Milestones

1. Add the bounded backend query fields and deterministic tests. Cover queue taxonomy, champion OR, date/player/result composition, event attribution, custom tag AND across accepted clips, missing facts and stable paging.
2. Wire the Solid controls, local champion suggestions, clip tag discovery, and production history refresh, reusing the existing controller and card rendering. Exercise hover, click, keyboard, chip behavior, clear, and zero history work during composition.
3. Verify the accepted-snapshot runtime against a populated dedicated fixture, run the changed-component suites, and update durable architecture for query and UI behavior.

## Verification design

Run `cargo test --manifest-path app/src-tauri/Cargo.toml league_index` and the relevant full Tauri suite, formatting and strict Clippy. Run the affected frontend tests, then `npm run test --prefix app`, `npm run check --prefix app` and `npm run build --prefix app`. Validate feature-list state per VERIFICATION.md. Keep the existing explicit benchmark query path working while proving no ordinary champion keystrokes send history requests. Use a dedicated populated fixture for the desktop walk-through. Check queue hover and keyboard, two champions, favorites, both tag sections, the secondary form, clear all, a filtered empty state and opening/returning from a match.

## Performance and reliability gates

Verify local-only suggestions, bounded option and request sizes, the existing 50-row/2 MiB response limits, stable pagination and no scan/journal/media work on filtering. Run the representative local SQL measurement for the expanded filter path if the fixture can run in the available environment. Measure that path separately from ordinary browsing. Exercise rapid filter replacement, optional projection settlement and root changes through existing admission and navigation tests. This work adds no capture-thread work or source writes.
