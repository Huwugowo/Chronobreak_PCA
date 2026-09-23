# QB-REPLAY-010 — UI foundations reconciliation

## Purpose

Combine the existing replay timeline and Match History UI with the accepted
descriptor-first library/replay behavior. Preserve useful work from both lines
instead of replacing either presentation or playback ownership. This is the
next coherent integration unit, not a new semantic-payload pipeline.

## Relevant planning-time architecture

The QB010 line is `3426cfa`; its descriptor and V2/M4 units are accepted.
The UI line is `6c7174f` (`ui/app-wide-foundations-v1`), descending from named
viewer foundations `82d6704`, with common ancestor `e030161`. The newer line
includes participant summaries and redesigned Match History cards. Integration
targets the QB010 worktree; the separate UI worktree and branch stay intact.
Include its additional uncommitted CSS work: the shared `--radius-surface: 4px`
token and its App/Chrome/Library/MatchHistory consumers, plus removal of legacy
match-card selectors from Library.module.css now owned by MatchHistory.module.css.
Capture that diff before integration; preserve any subsequent sibling edits rather
than overwriting them or silently broadening the captured input.

`LibraryController` owns the snapshot, root/request/token/navigation origins,
selection, mutation invalidation, refresh coalescing and bounded selected reads.
`LibraryCoordinator` owns backend publication and blocking permits.
`ViewerScreen` mounts one primary video from a four-field strict descriptor;
a tokenized full probe is optional after a two-frame paint opportunity.
The controller owns native media intent, generation and presented-frame evidence.

The UI line adds `ReplayTimeline`, pure `replayTimelineGeometry`, shared
windowed/fullscreen viewport, cluster/zoom/pan/overview/clip interactions, local
icons/tokens/styles, Match History card anatomy and its canonical UI documents.
Its App/API/library call sites predate the current snapshot/token owner.

Core `read_game_summary` already calls the strict bundle reader and derives KDA
and loadout. UI `derive_roster_context` can use this same parsed bundle; adding
participant display data does not require another file read or full probe call.
Storage-only accounting must still avoid bundle parsing.

## Scope and non-goals

Integrate the complete confirmed UI tip, resolve textual and behavioral conflicts,
retain descriptor-first playback, validate the combined UI/controller boundary,
and record actual results in the existing checkpoint. No commit is authorized.

No replacement visual design, controller, decoder, persistent index/database,
new delivery route, media probing, payload split, staged semantic backend or
new benchmark campaign. Do not reopen accepted V2/M4 evidence. No performance or
full-feature completion follows merely from combining these changes.

## Exploration findings

A tentative three-way merge identifies App, API and LibraryScreen conflicts.
Other auto-merged files require focused semantic checks, not wholesale replacement.
The tentative Viewer merge retains descriptor resources and optional-details
memos while inserting the existing timeline; verify that behavior directly.

The UI's resource-based optimistic save cannot own the current snapshot.
Its visible immediate Star behavior is compatible with a transient, origin-keyed
presentation overlay, but the UI spec's prohibition on refresh conflicts with the
current filesystem/mutation invalidation authority. Preserve mandatory refresh;
update only that conflicting UI requirement with its technical rationale.

Participant derivation reuses already parsed logs. Keep it out of the minimal
descriptor. Incomplete/sparse bundles yield truthful empty/neutral rosters;
never fabricate team, queue or outcome. Test summary/probe roster parity and
unchanged strict-v2 failures/snapshot scan ownership. This is an IPC display-schema
addition, not a change to persisted recording schema or descriptor fields.

## Chosen design and rationale

Use a non-committing three-way integration of the confirmed UI tip into the
current branch. Preserve clean auto-merges after targeted review. Resolve conflicts
with apply_patch; never choose an entire side merely to clear conflicts.
Keep both original commits/branches available; do not edit the sibling worktree.
Apply the captured uncommitted CSS delta after reconciling the committed UI line.
The radius token and all its consumers land together; removed Library selectors
must have no remaining consumer before accepting the stylesheet separation.

### Library and App

Keep LibraryScreen's current actionable/snapshotOrigin/duration-state props and
origin-bearing callbacks. Carry them into the UI's new primary card-open target,
sibling Star/More controls and menus. Retained invalidated cards are display-only;
unavailable media cannot open. Keep visible naming, artwork/layout fallbacks and
new roster cards. No extra per-card full-probe requests.

Preserve the current App's controller, viewer-entry helper, settings/export return
origins and benchmark lifecycle. Keep at most one optimistic Star overlay, keyed
by the captured LibraryOrigin (root, rootEpoch, request, token, navigation) and
game timestamp, with desired saved value and pending/success-awaiting-refresh
phase. It derives display only: never mutate LibrarySnapshot or authorize a request.
Start only from an admissible origin; disable duplicate pending saves. Persist
through the current tokenized controller mutation and retain its compulsory
same-root refresh on success, failure and stale completion.

Clear the overlay on failure, root/navigation/selection-origin invalidation or
any newly published snapshot. The sole request-epoch exception is the successful
mutation's own compulsory refresh: bridge its retained old cards until that
refresh publishes, then canonical data wins even if its saved value differs.
Do not bridge an unrelated refresh or carry the overlay across A/B/A. A failed
refresh clears the bridge and exposes the existing refresh error. Stale operation
completion produces neither a notice nor a new overlay; an old completion cannot
clear a newer operation's overlay. Tests must control mutation and refresh delivery
separately. Do not restore separate games/clips/usage resources.

Add required `GameSummary.participants: ReplayParticipant[]`, where each entry has
exactly `summoner_name`, `champion` and relation `ally | enemy | neutral`. Reuse
the existing roster-context derivation from the already parsed strict bundle for
both summary and full probe. Missing team/local-team knowledge stays neutral;
absent observations yield an empty roster and incomplete summaries do not invent
participants. Preserve existing ordering/name normalization and probe semantics.
Update Rust GameSummary/summary and incomplete constructors, TypeScript type,
`decodeLibrarySnapshot` game keys and `decodePlaybackProbe.game` exact decoding,
development mocks and every affected fixture together. Unknown/malformed fields
still reject; no optional field or permissive decoder conceals a boundary mismatch.

Merge API development fixtures with strict current decoders and tokenized commands.
Keep richer preview fixtures development-only; no mock mode or fabricated queue,
participant or outcome may leak into production. Preserve binary mock media
provenance. Use one development-only mock selection consistently for snapshot,
descriptor, probe, durations and mutations; never mix mock selections with native
library writes. Descriptor/probe mock URL and exact timeline must agree with each
other and the actual mock media. Preserve imported-music token release semantics.

### Replay UI

Keep the existing ReplayTimeline/geometry implementation and shared viewport.
Use descriptor media timeline/URL and selected summary labels; consume only
admitted optional details for mapped events, roster and semantic state.
Pending/failure remains explicit and retryable, not verified empty statistics.
Timeline seek/zoom/pan/clip operations remain useful before semantic admission.

Preserve the same primary element/controller across late details, filters and
fullscreen: optional admission must not change a component key, remount the video,
reopen the controller or replace descriptor URL/time authority. Assert the same
element, source, load count, currentTime, play intent/rate and clip/viewport state
through held delivery, failure/retry and fullscreen transitions. Keep the viewport
and clip draft in the persistent surface, not in
disposable windowed/fullscreen children. Canonical 48 MHz replay ticks and exact
half-open source-frame intervals remain authoritative. Retain keyboard/pointer
endpoint behavior and displayed controls; no second seek track or decoder.
Pointer capture/listeners and resize observers must release on cancel/disposal,
including disposal of a windowed/fullscreen rail during a gesture.

Keep benchmark Games and full-payload milestone meanings, including readiness
when media and details arrive in either order. Retain instrumentation even when
the normal UI hides diagnostics. Tests may use an explicit diagnostic seam or
assert product behavior instead of requiring a removed presentation panel.

## Rejected alternatives and planning decisions

- Replacing the current viewer/App with the UI versions loses accepted ownership.
- Keeping the old simple rail discards implemented UI work without a technical need.
- Deferring all roster UI on the assumption it adds JSON reads contradicts current
  summary code; reuse the parsed bundle and disclose added projection/IPC work.
- Optimistically modifying the canonical snapshot bypasses filesystem authority.
- Dropping all UI changes because the former descriptor plan excluded ReplayTimeline
  mistakes that unit's boundary for a permanent prohibition.
- Full semantic staging or a new performance comparison is a separate design.

## Milestones

1. Integrate the UI tip without committing; resolve App/API/library conflicts,
   preserve participant cards and accepted read/mutation ownership. Typecheck.
2. Verify and repair the combined timeline/descriptor behavior with focused
   deferred tests, including late details, fullscreen viewport and clip continuity;
   retain geometry and existing controller/benchmark tests.
3. Run the affected complete automated checks and desktop builds, inspect the
   combined UI on dedicated fixtures where supported, update architecture and
   canonical checkpoint truthfully, and leave all work uncommitted.

## Verification design

Required automated commands from the root:
```powershell
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run desktop:build --prefix app
npm run desktop:build:benchmark --prefix app
git diff --check
```
Run focused tests before the full affected groups and canonical validation from
VERIFICATION.md after the handoff and final state edits. No recorder/media-runtime
or closed campaign rerun. Existing unchanged backend time/security tests remain
the baseline; Rust suite covers the summary contract addition.

Extend, do not delete, accepted tests: held/failed/late optional details and retry
retain video/source/load/generation, rate/filter/clip/viewport; mapped events become
available reactively; same-ID A/B/A and Settings/exporter returns reject stale
results; card opens/mutations use current origins; optimistic success bridging,
failure, unrelated refresh/new snapshot, navigation and same-ID A/B/A cannot
publish stale stars; unknown/missing data stays truthful. Geometry tests
retain zoom/pan/clustering coverage, with DOM tests for the actual shared rail.
Strict decoder tests cover required roster fields in snapshot and probe, sparse
neutral/empty roster parity and existing malformed/v1 rejection. Audit that the
shared roster extraction adds no read and preserves scan cancellation checks.

Use dedicated schema-v2 fixtures for native checks: browse/select, timeline
seek/zoom/pan, filter, windowed/fullscreen controls, clip endpoints/exporter return,
late/failed details and root changes with source-hash auditing. Record actual
observations and distinguish browser/DOM visual evidence from native A/V. If
native access is unavailable, retain the exact unperformed gate, not a pass.
Retain build receipts, native observations, isolated fixture/subject identities
and before/after source hashes under ignored `build/qb010-ui-reconciliation/`;
link concise actual results from the canonical execution checkpoint. Do not retain
capability URLs or use real recordings as destructive fixtures.
Previously accepted standalone descriptor/M4 results remain historically valid,
not evidence that the newly combined UI passed native acceptance.

This unit preserves criteria 2–5 in the combined UI. Criterion 6's full replay
performance comparison remains a separately bounded gate.

## Performance and reliability gates

No added filesystem read, child, persistent cache, listener or unbounded request
fan-out for cards/rosters. Roster projection adds display data to the current scan;
do not claim aggregate CPU/IPC/I/O non-regression without measurement.
Keep one active/one-latest replay read, original backend permit lifetime, and
no lock across I/O/await. Strict-v2 validation, filesystem authority, exact replay
time, stale-result rejection and capability/opened-handle delivery are hard gates.
Native source mutation, wrong/stale media or reset of active media/clip state
requires focused disposition before this integration can be accepted.
