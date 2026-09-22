# QB-REPLAY-010 — Minimal replay descriptor

## Purpose

Let a selected valid recording become useful for playback and exact replay
navigation independently of optional clip details, Data Dragon and construction
of the existing semantic playback probe. Preserve the single primary video,
controller and protected delivery boundary.

This design succeeds the closed library-snapshot-v2 unit only for the descriptor
scope. M4 attribution, campaigns, resource dispositions, cancellation investigation
and manual acceptance remain closed. No descriptor implementation follows from
the old library plan itself.

## Relevant planning-time architecture

- `LibraryController` owns snapshot, selected game and origin (root epoch, request,
  token and navigation). `LibraryCoordinator` owns backend root/revision
  publication, current-token membership and bounded workers. Preserve those
  authorities, LibrarySnapshot shape and coordinator-before-MediaRoots lock order.
- `library.rs::read_recording_bundle` strictly parses both JSON files, validates
  schema-v2 time and checks matching media IDs. `playback_probe` then constructs
  the roster, mapped events, player timeline and KDA. The descriptor reuses the
  validator, not these semantic projections.
- `ViewerScreen` gates its entire `PlaybackSurface` on the full probe. The surface
  captures semantic arrays once. `playbackController` requires only protected URL,
  media ID and validated `MediaTimelineV2`. `replayTime.ts` already strictly decodes
  exact 48 MHz ticks, video frame boundaries and half-open clip intervals.
- The playback server alone authorizes bytes through capability, Host/Origin,
  CSP, ApprovedRoot and validated opened handle. A snapshot token or descriptor
  is not delivery authorization. The existing route resolves at each admission;
  it does not promise immutable bytes or a pinned-file URL.

## Scope and non-goals

Add the descriptor projection, strict frontend decoder, bounded selected-replay
read seam and descriptor-first opening in the current viewer. Keep the existing
full PlaybackProbe wire payload and semantic construction intact as optional
enrichment. Preserve export's independent strict source/frame validation.

No ReplayIndex/database, persistent cache, watcher, payload decomposition,
semantic paging/staging, ReplayTimeline UI, new player/controller/decoder,
ffprobe/media hashing, new route or unrelated replay optimization. Preserve
LibrarySnapshot, LibraryCoordinator and LibraryController as the owning contracts.
Do not reopen 009's deferred human A/V gate or require optional 015 controls.

## Exploration findings

The opening dependency is explicit in the resource that gates PlaybackSurface.
Its removal requires making existing event/state consumers reactive; remounting
the surface on enrichment would destroy playback generation and user state.

Strict structural bundle validity differs from complete semantic availability.
Keep the current parser, including game-log identity, calibration and field
validation. Empty observations and absent calibration remain valid where currently
allowed. Do not use a permissive header reader. Descriptor construction avoids
semantic projection, but does not claim parsing cost independent of log size.

Existing full-probe construction performs blocking filesystem/parsing work in a
synchronous function marked `#[tauri::command(async)]`. Both commands need explicit
bounded blocking
ownership so this unit does not duplicate that problem. No performance attribution
justifies more extensive playback changes.

## Chosen design and rationale

### Descriptor contract

Return `ReplayDescriptor { snapshot_token, game_timestamp, video_url,
media_timeline }`. Use existing timeline wire types and strict frontend validator.
Display labels come from the GameSummary selected from the admitted snapshot;
those labels do not authorize media time or export. Do not duplicate duration,
frame-rate or media-ID authority outside the timeline, or include semantic arrays.

Validate recognized ID and current selection, read the current strict v2 bundle,
require a nonempty regular video, and return recorded timeline and app-owned
protected URL. Do not build GameSummary/event/roster/state projections for the
descriptor. Missing video, v1, malformed fields, mismatched media IDs and invalid
time fail explicitly. Valid older v2 bundles need no new persisted fields.

### Read ownership and ordering

Add one replay-read permit to LibraryCoordinator, shared by descriptor and existing
full-probe commands. Use `try_acquire_owned` and immediate Busy/Superseded errors,
never queued semaphore waiters or task-per-entry work. Under the publication guard,
check token/game membership and capture revision/root epoch and the paired logical
path/ApprovedRoot. Release guards before work. Move the permit into spawn_blocking
and retain it until real worker exit even when its async waiter is dropped.

Recheck captured token/revision/root epoch and root pair after work. Refresh,
mutation invalidation, settings publication including A/B/A and shutdown reject
old completions. Cancellation suppresses admission without claiming an OS read
can be interrupted. Convert join failures to explicit errors; no lock spans I/O
or await. Do not create another library cache, store or root authority.

The existing full-probe endpoint keeps its payload. Require originating snapshot
token for selected viewer/exporter calls and route its builder through the same
read owner. Enrichment must match the descriptor's game, media ID and entire
decoded media timeline before admission. It never replaces descriptor authority.

### Viewer integration

Keep scheduling in the existing app-owned LibraryController: one active selected
replay read plus one replaceable pending intent shared by descriptor/full-probe
requests. Superseding a pending intent settles its caller explicitly; an active
read retains ownership through completion. `mayPublish` gates dispatch and result
admission. This owner survives viewer/exporter remounts. It stores no replay cache.

Capture the library origin after entering viewer navigation. Carry the selected
snapshot token through replay commands; only the live root/request/token/navigation,
selection and local request generation can admit responses. Coalesce rapid opens
to one active request and one latest pending intent. Disposal/root changes reject
pending results. No polling or timer-based Busy retry. Current errors have explicit
retry; stale errors cannot overwrite a newer selection.

Use one App entry helper for ordinary open, benchmark first open/remount, exporter
return and Settings return to viewer. It advances navigation, selects the game and
captures a fresh origin. Root invalidation clears/unmounts the selection before
another root's cards or media can appear. Refresh-invalidated origins cannot open
a same-ID recording merely because the previous ID still exists. An exporter
keeps its original mutation origin for export completion while its read admission
uses its current navigation origin. Thread tokens through all three existing full
probe callers (viewer, exporter and benchmark export); no tokenless bypass.

Each open deliberately validates JSON twice when optional details are requested:
once for the descriptor and once for the unchanged probe. This bounded duplicate
read preserves fresh filesystem authority without retaining a parsed-bundle cache
or introducing a staged payload producer. It is a disclosed I/O tradeoff, not a
non-regression claim; later replay performance disposition must account for it.

Mount PlaybackSurface when the descriptor is strictly decoded and admitted. Open
the existing controller from its URL/time authority. Play, pause, seek, fullscreen,
rate/audio controls and frame-based clip selection use only this authority.

After descriptor admission and an initial two-frame paint opportunity, request
the unchanged full probe once as optional
details. This single dependency removal is necessary to make the selected replay
useful independently; it does not decompose or implement a new semantic pipeline.
Make existing event/participant/state consumers reactive. Pending/failure is local
and visible, cannot remove/remount/reset the video or disable ordinary controls,
and cannot masquerade as a verified empty log or zero player state. Event actions
use only admitted details. Retry is explicit. Late details preserve controller
generation, position, play intent, filters and clip selection. The exporter may
continue using the full probe; exporter staging is outside this unit.

### Benchmark compatibility

Keep library_useful and games-library-usable-v1 unchanged. playback_payload_ready
continues to mean the full probe and retain its semantic counts; a descriptor must
never emit that event. Any descriptor marker has a new name. Existing benchmark
semantic actions wait for admitted full details while ordinary playback does not.
No new observer framework, no M4 remeasurement and no reinterpreted old results.

The benchmark start gate must observe both media readiness and optional-details
readiness and rerun when either arrives, so a frame arriving first cannot strand
the scenario. A details error remains a benchmark failure under existing semantic
scenario requirements even though normal video stays usable. Full payload may now
arrive after mount; keep request-to-milestone metrics honest. The historical
opening_attribution helper assumes payload-before-mount; it belongs to the closed
campaign and is not a valid descriptor comparison without a separately versioned
measurement design. Do not change it or replay its historical campaign here.

## Rejected alternatives and planning decisions

- A header-only or schema-v1 fallback reader weakens strict bundle validity.
- Constructing a full probe and slicing its result retains the expensive dependency.
- A separate preview screen/controller duplicates playback ownership.
- Remounting after enrichment loses decoder state and generation evidence.
- Persistent caches/history add authority and invalidation without need.
- Semantic payload splitting or staged ReplayTimeline exceeds the descriptor unit.
- Additional media probes/hashing are unnecessary for recorded timeline authority.

## Milestones

1. Implement the minimal projection/decoder and bounded selected-replay command
   ownership. Prove v2 parity, strict failures and deterministic cancellation/root
   ordering before frontend integration.
2. Integrate descriptor-first opening and optional unchanged details in the current
   viewer. Preserve media/controller lifetime and tokenized exporter compatibility.
3. Verify the changed app boundaries and normal/benchmark builds. Update durable
   architecture and checkpoint with precise evidence and remaining full-feature
   gates. Stop at the descriptor boundary.

## Verification design

Use existing `library::tests::write_game` fixtures and dedicated tempfile roots.
Accept sparse/empty/older/current v2 bundles and absent calibration as allowed by
the current schema. Reject v1, unknown/malformed fields, identity mismatches,
invalid timeline and missing/deleted/nonregular video. Descriptor wire data must
have no semantic collections; no media tool is invoked. Retain strict parser tests.

Barrier/channel tests on a single-thread runtime establish async progress,
immediate Busy, no queued work, permit retention after caller drop, worker failure,
root A/B/A and refresh/mutation invalidation, stale selection and shutdown. Do not
use sleep-based race assertions.

Deferred frontend tests independently hold/fail full details, Data Dragon and
optional durations while descriptor admission mounts the existing primary video
and enables controller operations. Admit details late and verify identical video
element, generation and clip state. Cover selection A/B/A, overlapping-ID roots,
unmount/retry, wrong token/game/media/timeline and strict descriptor decoding.
Preserve current semantic event, clip/export and benchmark event tests.

Required commands (repository root; focused tests first):

```powershell
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
npm run desktop:build --prefix app
npm run desktop:build:benchmark --prefix app
python -m unittest discover -s tools/replay_benchmark/tests -v
git diff --check
```

Run the canonical validator from docs/development/VERIFICATION.md after handoff
and final canonical edits. Do not rerun recorder/media-runtime or closed M4 work.

Normal-build descriptor acceptance uses a dedicated fixture: open with held/failed
details, play/pause/seek/fullscreen, then release details without reload. Switch
away and between overlapping-ID roots; verify no stale result and source hashes.
Record unavailable interactive verification honestly. Deterministic tests do not
substitute for native video acceptance. Full-feature performance/manual gates
remain open when not actually verified.

Acceptance mapping: prior closed V2 evidence retains criteria 1 and 2 and the
library portions of 3–6. This unit proves criterion 3's descriptor dependency,
criterion 4's replay-read ownership and criterion 5's unchanged v2/filesystem and
selection behavior. Criterion 6's complete replay performance claim stays distinct;
no speedup or full-feature completion follows from this dependency change alone.

## Performance and reliability gates

At most one new replay blocking job, no unbounded queue, child process or cache
history. Filesystem, parsing and semantic projection stay off async workers.
Descriptor projection allocates no derived semantic collections and waits on no
optional resource. Failure of details leaves primary media usable and unchanged.
Source files remain untouched. Timing/schema validity, correct generation and
root admission, and protected delivery are hard gates.

Any observed wrong/stale media, source mutation, playback regression or sustained
ownership growth requires disposition before unit acceptance. This is a dependency
and correctness change, not a quantified latency claim. Any later matched replay
comparison needs a separately bounded design and cannot reopen M4 by implication.
