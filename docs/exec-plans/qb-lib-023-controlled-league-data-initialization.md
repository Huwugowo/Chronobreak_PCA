# QB-LIB-023: Controlled League data initialization and recovery

## User-directed scope amendment (2026-10-29)

The user revised acceptance on 2026-10-29: deliberate native fixture-recovery
observation is optional follow-up and must not prolong implementation or block
delivery/completion. Retain actual verification and unobserved scenarios in the
checkpoint; never infer individual manual passes from broad release feedback.
The original implementation design below is retained.

## Purpose

Opening Games or a replay presents honest, calm match-data loading. The app
coordinates its own optional indexing rather than making users recover from an
internal writer collision. Successfully imported but imperfect recorded evidence
is not described as a broken video or work that is still loading. Optional-data
errors and developer identifiers do not appear as an unstyled banner/button over
the replay.

## Relevant planning-time architecture

`App.tsx` owns `LibraryController` and `HistoryController`. The library controller
owns root/request/token/navigation admission, one active replay read with latest
pending details/overlay intents, and library mutations. `App.tsx` removes the
History context outside the Games tab or during non-independent mutation.
An already-issued IPC still owns backend resources until its worker completes.

`HistoryController` has one independent query and one import lane. Catalog and
recording imports run through `indexCatalog`/`indexRecording` in `leagueIndex.ts`.
Four-commit pending work resumes after a 4ms rendering yield. Import completions
coalesce a query refresh through the dirty bit approved by QB-LIB-019. A thrown
import error ends continuation and sets sticky `indexError` until a new context.

`ViewerScreen.tsx` mounts a persistent media element from its descriptor first.
After two animation frames, `ReplayDataController` loads facts, indexes the
selected recording, loads scopes/windows, and `ReplayOverlayController` loads
the complete selected timeline. Productive pending imports yield 100ms. Terminal
failure is currently exposed with raw error text and a Retry replay details button.
The index-unavailable branch can stringify a null error. A separate core descriptor
failure has its own recording-open screen; it is not optional League initialization.

Catalog/recording index writes use `LibraryCoordinator::index_job` with a single
non-waiting writer permit. Two reader permits and the independent media descriptor
permit remain separate. `reset_league_index` uses the distinct `index_admin_job`,
which exclusively acquires writer plus both readers. The frontend owner covers
all three write IPC commands without changing those native admission mechanisms.
The native Busy contract is intentional capacity protection, not evidence of a
bad recording. `league_index_lifecycle_tests.rs` proves that even an aborted caller
retains writer ownership until its blocking worker exits.

The index remains a disposable local SQLite cache. Filesystem recordings and
committed observation journals remain authoritative. `history_summary.rs` flags
pending, source-unchecked, gaps/tails, absent queue/outcome, actor resolution and
event coverage separately. `query.rs` combines the flags relevant to the query
with catalog entries missing from the index into `unknown_recordings`. These
coverage rules must remain conservative.

## Scope and non-goals

Change bounded application-side index-write coordination, expected incomplete
bundle classification during catalog import, and library/replay match-data status
presentation. Add focused regressions and isolated release-path validation.

Do not change recorder production, source telemetry, calibration, integrity
validation, recording recovery/deletion, SQLite schema or cache version, semantic
query truth, event-to-video mapping, video playback/seek ownership, native permit
counts, benchmark thresholds or observer authority. Do not repair or delete the
user's partial bundles, rewrite historical journals, or build general logging or
automatic retry infrastructure. Missing recorded observations cannot be created
by re-indexing.

## Exploration findings

The normal configured preprod library contains nine game directories: seven
canonical recordings and two partial-only directories without metadata. The
current cache has seven entries, all ready/current with zero uncommitted tail.
Master Yi, Graves and Syndra each have history flags 25 (base, actor and event
uncertainty). Recorded event-endpoint coverage reports respectively 5/5/2 gap
attempts, 2/9/1 unresolved actors and 18/14/2 unmapped observations. These are
coverage diagnostics, not counts of lost video frames or proven missing game
events. No user library was mutated to obtain these findings.

Catalog currently calls `accepted_game` for every filesystem entry, including
recognized incomplete entries. Such entries have no media identity, so they
become item errors and set global `indexError`. The same entries also contribute
to `catalog_recordings - indexed_recordings`. The seven healthy imported sources
still contribute to uncertainty because of source gaps. Thus a total of nine is
consistent with the current data even after all productive loading finishes.

Library status currently concatenates independently changing query-loading,
indexing, unknown-count, error and retained-results fragments. Each productive
import can dispatch another query, rapidly changing the line. Within one stable
context the index error itself is sticky; this source evidence explains rapid
line changes, not proof that the error flag toggled in the reported launch.

A second deterministic problem exists across navigation: an old History writer
can still be active when selected replay indexing starts. The latter receives
Busy and treats it as terminal unavailable. Returning to Games can produce the
inverse. Neither consumer owns the other's in-flight writer. There is no retained
runtime exception proving this was the particular replay failure in the user's
launch; reproduce both directions before correcting the shared ownership.

## Chosen design and rationale

### One bounded frontend writer owner

Extend the existing library controller's ownership rather than adding another
backend worker pool or a generic retry loop. Provide an origin-bound generic
index-write entry point used by the App's History catalog/recording adapters,
the selected Viewer's index-recording adapter, and the Settings reset operation.
Keep the low-level native IPC functions in `leagueIndex.ts`.

Retain exactly one active write promise across navigation, refresh and root
changes until it settles. Keep at most one latest queued intent in each explicit
purpose: history, selected replay, maintenance/reset. Replacement rejects the
superseded queued promise without dispatch. Prefer maintenance then selected
replay over queued background History work; neither can preempt the active native
worker. Existing controllers already bound submissions, so this is a finite queue
of at most three pending intents, not one task per recording.

Capture the existing LibraryOrigin and validate before dispatch and publication.
History/replay requests require current navigation as well as root epoch, request
and token. Navigation supersedes queued history/replay purposes, but retains the
active ownership barrier; root/refresh/disposal invalidate queued work and never
pretend that the active native call has stopped. Maintenance is a captured-root
mutation: before dispatch use the existing mayStart identity rather than requiring
the original navigation to remain visible; retain existing mutation completion
and reconciliation rules after dispatch. A reset queued behind a prior writer
does not discard that writer's ownership. Native reset still acquires its readers
exclusively and reports an actual reader conflict according to the current contract.

Expose the adapter explicitly to History/Viewer rather than having a module-global
queue infer the active selection. Preserve replay's existing details/overlay read
serialization and independent History query lane. No read is made a prerequisite
for showing the video. Route every normal application call to the three native
write commands through this owner; tests may inject the existing direct fakes.
Superseded callbacks retain the consumers' current stale-publication guards.
Any remaining real Busy, validation, IO or query error is retained as a real
terminal outcome; do not relabel it as loading or retry it indefinitely.

### Expected incomplete bundles are not a catalog-wide failure

For a filesystem GameSummary already marked incomplete, catalog emits an item
with the original timestamp, `media_id: null`, `progress: null`, `error: null`;
retain its cursor position and contribution to catalog count. Do not invoke
`accepted_game` or `Index::recording` for this recognized state. Keep its
filesystem membership, incomplete card and conservative
unknown coverage. Do not turn a duplicate identity, malformed source belonging
to an otherwise eligible recording, unsupported telemetry or corrupt commit into
a successful import. Those remain observable controller failures/limited data.

### Stable, truthful library status

Add an explicit generation-bound initialization session to HistoryController.
It starts on accepted context admission, survives all intermediate query results,
and ends only after catalog and every productive pending import have settled and
the post-import/coalesced final query has admitted or terminally failed. Tie the
final-query barrier to the current session and dirty/import revision; an older
query cannot close a session after newer imports. Preserve explicit-input priority:
the latest current user query can satisfy that final-query barrier after imports
settle. A terminal catalog/import failure also ends the session once its current
query has settled or been invalidated, without waiting for a query that can no
longer dispatch. Context replacement/disposal supersedes the whole session;
stale A/B/A completions cannot finish the new one.

Use mutually exclusive status precedence. While the initialization session is
active and current work is outstanding,
display one stable `Loading match data…` line. Automatic intermediate query
loading/completion cannot replace it with separate updating/previous-results
fragments. Derive the state from owned requests and current generations, not a
fixed success delay or a timer that assumes work completes.

At settlement, stop loading, including all terminal-error paths. A query failure
retains the previous cards with honest failed-query feedback. After initialization,
explicit typed/discrete queries preserve existing debounce/updating/retained-page
feedback and pagination guards. Permanent source gaps are not a loading condition.

The ordinary unfiltered library does not print the aggregate N incomplete-query
warning. With League-dependent filters active (exact player, actor, event,
stolen/minimum streak, queue/outcome as applicable), settled unknown coverage may
show one restrained notice that some recordings have limited match data and the
results may be incomplete. This must not claim the video is incomplete or nine
imports failed. A terminal initialization/source failure has a distinct restrained
message (whole operation failed versus only some source data unavailable). Retain
the real internal error for diagnosis; do not render raw exception strings.

### Product replay status outside the picture

Replace the unstyled replay-details section with a compact status in the existing
header. Implement a total reducer over current initialization ownership, pending
viewport/overlay work, terminal failures and surviving usable payload:

1. While any current facts/index/scopes/window/overlay work is pending or scheduled,
   show `Loading match data…`, even when another lane has already failed. Waiting
   on the shared writer is pending work; neither idle source gaps nor a terminal
   failure is pending work.
2. Once idle, an initialization failure with usable surviving facts/window/overlay
   data yields restrained partial-data feedback. A failed facts lane with a usable
   event/overlay payload is limited, not indefinitely loading. A scope-only identity
   without usable facts/window/overlay is not sufficient payload for this rule.
3. Once idle, failure or unavailable index with no usable optional payload yields
   `Match data unavailable`. Successful initialization with known absence of source
   data may use the same restrained unavailable state; no spinner waits for data
   that was not recorded.
4. Successful usable initialization clears the loading status. Known gaps may
   qualify only the relevant data, never create a failure or loading state.

Keep terminal/init state explicit when no scope/window can be requested. Preserve
available facts and controls and clear stale temporal data according to existing
controllers. A subsequent viewport load can enter loading again only while its
owned intent is scheduled/active, and must settle after completion or error.

Do not render raw errors, null, UUIDs/epochs or the Retry replay details button
over the picture. If multiple source epochs need selection, keep the source
choice with ordinary segment labels rather than raw run IDs/epoch diagnostics;
do not merge scopes or infer facts. Existing Return to Games, library refresh and
Settings rebuild remain deliberate recovery. A genuine core-media open failure
continues to have an actionable recording-open failure screen with safe product
copy; it must not become a never-ending optional-data loading label.

Keep visual readiness distinct from terminal failure and from benchmark success.
Do not make a failed optional request report ready just to dismiss the indicator,
change benchmark thresholds, or modify the media element to refresh data.

## Rejected alternatives and planning decisions

Reject replacing every error with Loading: stopped work is not loading. Reject
removing diagnostics without fixing writer ownership. Reject relaxing gap/actor
coverage or excluding partial entries from canonical filesystem membership to
make the count smaller. Reject timed automatic retries for all errors, increasing
native permits, an unbounded backend wait queue and cancellation claims for
already-running blocking workers. A single finite frontend owner solves the
known app-generated collision while preserving native capacity enforcement.

Do not require a new benchmark campaign: no latency or CPU improvement is claimed.
Use deterministic admission/call counts plus actual isolated release interaction.

## Milestones

1. Demonstrate controlled writer-overlap failures in both navigation directions
   and the incomplete-bundle catalog error. Add focused failing tests for a stable
   loading session, permanent failure settlement and the replay raw/retry notice.
2. Implement the single writer owner and route History, selected replay and reset.
   Correct expected incomplete-bundle classification. Pass focused admission,
   invalidation, reset and pending/ready/error tests before UI integration.
3. Implement library and replay product states and segment labels. Pass focused
   UI/controller regressions, then full affected checks and release build. Perform
   isolated release acceptance and preserve source hashes. Update durable
   architecture and canonical completion evidence only after all gates pass.

## Verification design

Use deferred promises in `libraryController.test.ts`, `historyController.test.ts`,
`replayDataController.test.ts` and an App wiring regression. Hold a catalog writer,
open replay and require its index call not to dispatch until release; invert on
return. Assert max active native writers = 1, queue bound, selected/maintenance
priority, latest-intent replacement, identical-context coalescence where existing,
refresh, A/B/A, rejected active work and disposal. A stale in-flight response must
release ownership and allow current work without admitting stale state. Reset
retains existing root-captured mutation reconciliation and reader-conflict safety.

Backend temporary-library tests cover canonical + partial-only catalog entries
without global failure, asserting null progress/error/media identity plus retained
timestamp/cursor/count, preserving partial membership and errors for eligible
corrupt/unsupported/duplicate-identity sources. Keep native writer-retention,
reader/reset and strict-source tests unchanged. Do not use the real preprod
library as an import or destructive test target.

Library UI tests resolve many productive import/query pages rapidly and require
one continuous loading message without unknown-count/error fragment churn. Cover
the final query still pending after the last import, final-query failure, explicit
input satisfying the barrier and stale A/B/A completion not ending the new session.
Cover
settled ordinary view, semantic-filter limited coverage, explicit typed/debounced
replacement, retained-page failure, terminal import failure and context reset.
Replay tests cover loading to ready, unavailable without error text, thrown error,
partial facts failure, overlay failure, no scopes, productive pending, ready with
tail, disposal/navigation, safe core open failure and recovery by fresh context.
Explicitly hold another lane after a partial failure: loading persists only until
that lane settles, then surviving usable payload yields limited or no payload
yields unavailable. Test every reducer branch, including success with source gaps.
Verify no raw error, literal null, debug source labels or optional retry button;
unchanged video element and working controls during all optional states. Ensure
terminal failures do not become benchmark success or indefinite loading.

Commands after focused cases: `npm run test --prefix app`,
`npm run check --prefix app`, `npm run build --prefix app`,
`cargo test --manifest-path app/src-tauri/Cargo.toml`,
`cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check`,
`cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings`,
`npm run desktop:build --prefix app`, `python build/validate_features.py`,
and `git diff --check`. Run the existing replay tooling tests if changed adapters
touch its integration; preserve observer/benchmark tests and require no new
performance claim.

Actual production acceptance uses a dedicated profile/cache and a temporary
fixture library with representative valid telemetry, one partial-only bundle,
limited coverage and a malformed optional source. Cold index followed by early
replay open/return and repeated switches must settle without self-generated Busy,
banner churn or raw/retry overlay. Observe play/pause/seek/fullscreen, terminal
failure settlement, deliberate refresh/rebuild and source hashes. Record exactly
what was observed; do not claim interactive success from unit tests alone.

## Performance and reliability gates

One active index writer; at most three latest pending purpose intents; existing
one-query/import/read limits, four-commit pagination and yields remain. No
unbounded task or retry fan-out, media remount, SQLite schema change, corruption
tolerance, source mutation or recording deletion. A ready journal with positive
tail stops productive imports; failed/stalled work settles truthfully. Test call
and ownership counts establish bounds. Native workers retain permits through
caller cancellation; frontend supersession never releases ownership early.
