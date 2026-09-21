# QB-REPLAY-010 — Shared library scan and optional clip durations (v2)

## Purpose

Make the Games library usable without waiting for one synchronous ffprobe per
clip. Build games, clip identities and storage accounting from one refresh scan;
request clip durations only as optional display information. Preserve filesystem
authority, strict replay identity and safe mutation/root-change behavior.

This version supersedes the attribution design at
`docs/exec-plans/qb-replay-010-measured-library-and-replay-opening.md` for the
attributed **library implementation unit**. Preserve that plan and its evidence.
This design does not authorize a playback-payload split. The full feature's
minimal replay descriptor/staged semantic timeline criterion and unexplained
post-payload delay remain separate acceptance gates. Do not mark all of 010 done
when this unit passes or weaken those criteria to fit this implementation.

## Relevant planning-time architecture

- `app/src-tauri/src/library.rs::list_games` enumerates game directories and
  `read_game_summary` reads video length and parses strict schema-v2 metadata/log
  bundles. Invalid/incomplete bundles produce the existing incomplete card;
  playback itself rejects invalid/v1 bundles. Preserve that distinction.
- `list_clips` repeats `list_games` for champion/date enrichment, enumerates clip
  media, reads sizes/thumbnails, and runs `probe_duration_ms` synchronously for
  every recognized MP4. `storage_usage` repeats game parsing and clip enumeration.
  Recognized clip names and game IDs already have validators. Source-less clips
  are supported. Storage totals count existing files independently of optional
  duration success.
- `lib.rs::AppState` owns configuration, `Arc<MediaRoots>`, playback origin and
  the validated packaged media runtime. Existing library Tauri commands are
  marked async but directly run synchronous filesystem/probe work. This must not
  be copied into the new command implementation.
- `playback_server.rs::{OutputDirectory,MediaRoots}` publish logical output path
  and `ApprovedRoot` together after settings persistence succeeds. The server
  retains capability URLs, validated opened-handle containment, Host/Origin/CSP
  checks and its 64-connection bound. Snapshot data cannot authorize delivery.
- Mutation command seams in `lib.rs`: `save_game`, `delete_game`, `delete_clip`,
  `run_auto_delete`, `save_settings`, and successful `export_clip` publication.
  `clip_export::export` returns after staged output files are published. Startup
  retention precedes AppState creation. External recorder/filesystem changes have
  no watcher notification today.
- `App.tsx` loads games/clips/storage/settings/Data Dragon independently; games,
  clips, storage and settings gate rendering. `reloadLibrary` independently
  refetches the first three. Save refetches only games; export refetches only
  clips/storage; root change returns the viewer to the library. Unify these
  refresh paths. Data Dragon currently supports cached/placeholder rendering.
- `LibraryScreen.tsx` needs current GameSummary fields for game cards. Clip cards
  use validated identity, source labels, bytes and URLs; duration is display-only.
  `ClipModal.tsx` can play with a validated video URL while duration is unknown.
  `StorageIndicator`/`SettingsScreen` consume aggregate counts and bytes.
- `api.ts`/`types.ts` define decoded wire contracts. Keep strict playback decoding
  and the full `PlaybackProbe` interface used by Viewer/Exporter unchanged.
  `playbackController` remains the primary-video and authoritative-frame owner.

## Scope and non-goals

Implement one process-local scan coordinator, a pure synchronous scan builder,
one frontend snapshot resource/controller, optional clip-duration requests, and
invalidation wiring. Update their focused tests and durable architecture.

No database, persistent cache, index, watcher, polling refresh, task per file,
speculative prefetch of every duration, playback payload staging, media-container
rewrite, recorder change or new playback backend. Do not reopen 009's deferred
human audio gate, 011's delivery design, or optional 015 stepping work.

## Exploration findings

Current v2 production-path attribution is retained under
`docs/performance/evidence/qb-replay-010-20260915/`. The frozen reference executable
is revision `3a750e2`, SHA-256
`f1e3390cb29b3c9465eee3898a46af5d2c3c69894aeb9f7a5ad5c09a1ff75b49`, runtime r6.
The five-cell campaign attempted 30 launches: 28 valid; N/L each lack one valid
cold trial. S/R/E have five valid cold processes; warm characterization is five
remounts in one process per cell. This is not a complete passing matrix.

Small S library useful median is 92.4 ms. N/E (50 games/50 clips) medians are
2121.6/2312.6 ms, dominated by clip-command medians 1992.6/2196.4 ms. Games/storage
are about 26/27 ms in N and 324/320 ms in E. A single narrower optimized native
diagnostic directly timed the same clip function: 50 probes take 1673.6–1772.1 ms
in N and 1665.4–1817.3 ms in E, accounting for 98%/84–85% of that function. The
remaining game enrichment takes 19–24/288–306 ms. The probe work comfortably
exceeds E's materiality thresholds of 115.6 ms (5%) and 129.3 ms (three MAD).

This identifies concrete removable work on the current useful-library path.
It is not an observed candidate improvement. N/E's useful difference alone is
within the sparse repeatability band and N is undersampled. Repeated parsing is
removed by sharing the same scan, without claiming its isolated UI speedup.

Replay backend payload medians remain 0.66–7.96 ms. Selected-media duration
increases metadata/first-frame time mainly after payload/mount; available evidence
does not resolve container, delivery or WebView causes. The old 008 schema-v1
baseline is incompatible as a current before/after reference. No descriptor split
or decoder change is selected from these observations.

## Chosen design and rationale

### 1. One completed scan, one set of derived results

Add `LibrarySnapshot` with an opaque version token, `games`, lightweight `clips`,
and `usage`. Reuse existing GameSummary semantics and validated identifiers.
Represent clip duration explicitly as unknown until optional enrichment resolves;
do not format unknown as a measured zero duration.

Build this value in one synchronous function under `library.rs`. Read each game
summary once; collect its identity and video bytes once. Build the source map from
those same summaries. Enumerate clips once, collecting accepted names, media
length and thumbnail state once. Preserve existing storage-only accounting for
MP4 entries whose names do not qualify as displayed clips; retain their sizes
and count in the scan's internal accounting, without exposing invalid IDs as
playable cards. Derive usage from these collected records; do
not parse logs for storage or call `list_games` again for clips. Do not call any
external media tool from the core scan. Sort with the existing ordering.

"Snapshot" means a coherent result derived from one completed scan, not an atomic
Windows filesystem snapshot. Concurrent external writes may affect what a scan
sees. Preserve current missing-directory-as-empty, incomplete-game, and per-entry
metadata behavior; a directory enumeration failure fails the refresh. Never mix
games from one scan with clips or totals from another. A failed refresh retains
the previous complete same-root view, visibly marked as not refreshed.

### 2. Bounded ownership and publication

Create a small process-local `LibraryCoordinator` owned by AppState. It holds one
current completed snapshot, its root/revision identity, optional duration results
for that snapshot, and short state locks. It is disposable and reconstructible.
Do not retain snapshot history or reuse scan results across explicit refreshes.

`refresh_library` is a real async Tauri command. Capture approved root/path,
playback origin and revision under the publication guard, release it, then run
the synchronous builder in `spawn_blocking`. Move the owned concurrency permit
into the blocking closure so cancellation of the async waiter cannot release a
slot while work still runs. A scan has one globally owned slot, not one per root.
The frontend coalesces refresh intent to one active request plus one latest
pending intent; the backend rejects excess admissions as Busy/Superseded rather
than accumulating unbounded semaphore waiters. No lock spans I/O or await.

On completion, publish only if the captured revision/root is still current.
Otherwise discard it. Invalidation increments the revision, clears optional
results and makes any old work unpublishable. Cancellation can skip work between
entries; it cannot interrupt a blocking OS read or release its owner early.

Pair root identity/revision capture and settings root publication under the same
short coordinator guard, with a fixed lock order: coordinator then MediaRoots.
Add a crate-visible MediaRoots API that clones its logical path and ApprovedRoot
from the same read guard; the current separate/private getters are insufficient
for this capture. This is a view of its existing authority, not a new root store.
Continue constructing `OutputDirectory` and persisting settings before publishing
it. Publish the new MediaRoots pair and coordinator revision together. Never add
a second authoritative path or independently canonicalized security root.

### 3. Optional clip details

Add a bounded `resolve_clip_durations(snapshot_token, clip_ids)` command. Accept
at most eight distinct IDs from the current snapshot. The UI requests only
visible clip cards or the active clip, after core library rendering. One global
batch owns at most one probe child at a time; at most one batch is active and one
latest bounded batch is pending in the frontend. No automatic all-library walk.

Use the packaged runtime. Perform blocking validation/metadata checks in the
owned blocking task; use explicit child ownership with bounded output, deadline,
and kill/reap on cancellation or timeout. Bound duration output to 4 KiB and the
individual probe deadline to ten seconds; discard stderr or retain only a bounded
diagnostic, never unbounded `Command::output`. A dropped caller must not leak a
child or free the probe slot early. Do not invent fallback to PATH tools.

Probe the existing capability-protected loopback clip route, not a pathname
reopened by the child. The existing server validates and streams the same opened
handle for each request; do not add a probe route or weaken its admission checks.
Only construct the URL from the app-owned origin/capability and a validated current
clip ID. Never accept a caller-supplied URL or log that bearer URL/command line;
discard child stderr because it can echo input URLs. Use ffprobe's explicit MOV
demuxer (`-f mov`), `-enable_drefs 0`, `-use_absolute_path 0`, and the narrow input
protocol whitelist `http,tcp`. Thus an MP4 cannot introduce external file/track
references; the existing server emits no redirects. Test these arguments and
reparse/replacement during admission: no bytes outside an approved root may reach
the child. A path precheck followed by a child pathname reopen is not equivalent.

Associate an optional result with snapshot token, clip ID and observed validated
file identity/length/modification facts from the captured root pair. Revalidate
after the probe and discard changed/deleted files or stale tokens. A root switch
between HTTP requests makes the old batch unpublishable; retained old handles
remain governed by the existing server contract. This does not promise an atomic
multi-request filesystem snapshot. Do not hash media to validate a display
duration. Equal timestamp/length is not proof of unchanged bytes; a new refresh
always discards duration results, even for matching metadata. Within a snapshot,
duration is non-authoritative UI information; playback/export retain their own
strict current source validation.

Cache optional success or unavailable state only for the current snapshot and
its finite clip set. Runtime absence, probe error, timeout or file replacement
produces a local unavailable state with explicit user retry; it never blocks
games, changes source bytes, or fails the whole snapshot. Superseded batches skip
remaining IDs. Optional processing must not overlap unbounded scan work: core
scan slot one, duration slot one, with existing delivery/export limits preserved.

### 4. Mutation and refresh contract

Route save/delete-game/delete-clip/retention/root settings and export publication
through a common invalidation helper. Save must refresh the same contract as
delete/export/cleanup. Invalidate after successful publication and also after an
attempt that can have partially mutated files before returning an error; keep the
error visible. A failure before side effects need not invalidate. Do not turn
partial failures into success.

Move synchronous library mutation and settings filesystem work off async workers
through one owned mutation slot; never retain a state/configuration lock across
await. Existing async export retains its process ownership. Capture its original
root/version: completion invalidates only that root's current view, and cannot
publish old export UI into a newer navigation. Preserve export security and exact
frame/source contracts. An export already admitted against a root continues to
its captured destination; root switching does not retarget it.

UI-triggered item mutations carry the selected snapshot token. Validate token,
root and ID at admission so an old selection cannot mutate an identically named
item after a root switch. Serialize root-change admission with library mutation
root capture; reject stale selections explicitly. Preserve test-library-only
destructive verification and benchmark mutation restrictions.

Refresh on startup, explicit refresh/retry, return to the library, and completion
of any applicable mutation. This observes external recorder writes at defined
refresh points without a new watcher or timer. Deduplicate simultaneous intents;
do not reuse an obsolete view solely because an in-memory cache exists.

### 5. Frontend state and user-visible boundaries

Replace independent games/clips/storage resources with one typed snapshot
controller (`api.ts`, `types.ts`, `App.tsx`; a small separate controller is useful
for deterministic race tests). Strictly decode snapshot/version/optional-result
contracts. Every result admission checks current root intent, refresh token and
navigation/clip selection as applicable. Avoid trusting request completion order.

Render Games from the core snapshot. Clip-duration pending/unavailable states and
Data Dragon placeholders remain local; the Games screen does not wait for them.
Usage comes from the same completed scan. Settings have their own failure state;
do not accidentally make Data Dragon an initial dependency. On same-root refresh,
retain the previous complete snapshot with refresh feedback. On root change,
clear old selections/cards immediately and show loading for the new root; do not
present old-root cards under new-root settings. Preserve viewer return-to-library
behavior after an accepted output-root change.

Clip cards/modal display an unknown marker or local unavailable state until
duration resolves. Playback can use an admitted clip URL independently. When the
user leaves Clips, supersede pending requests and reject late results; already
owned OS work remains bounded until cancellation/termination completes.

### 6. Benchmark observables without redefining the reference

Keep existing `library_useful` semantics, including settings, HEVC and two paint
frames. For compatibility, its clips-ready prerequisite continues to mean the
full historical clip-detail readiness; do not quietly move it to core-snapshot
readiness and compare the faster event against the old value. In benchmark mode
only, drain optional clip durations through the same bounded batch API to satisfy
that historical milestone. Production does not eagerly drain every clip.

Add a separately named benchmark-only `games_library_usable` frontend milestone
after core games, settings and two animation frames, with clip/usage counts and
the applicable snapshot token. Instrument the same user-visible condition in a
reference build: on the current reference UI it remains blocked by required clip
loading; on the candidate it is released by the core snapshot. Keep the event's
condition and paint rule identical across arms. Begin optional benchmark draining
after this milestone to avoid benchmarking synthetic work as part of first paint.
Keep native old command spans semantically stable or explicitly absent; new
snapshot/duration spans get new names, not misleading reused elapsed fields.

The new primary comparison is this games-usable milestone. Freeze both instrumented
source identities and event meaning before collecting a fresh matched comparison.
The current attribution data cannot substitute for those reference trials. Retain
the existing analyzer's strict validity, observer bounds, source hashes and
same-clock rules. Extend only the minimal event extraction/metric allowance
required for this exact milestone; no new manifest/observer framework.

Pin the measurement contract to the existing manifest `scenario_id` namespace
`qb010-library-v2-` for both reference and candidate, with matched cell/kind IDs.
Only these scenarios require exactly one `games_library_usable` event at startup.
Derive `library_request_to_games_usable_ms` from the frontend `library_requested`
origin. Leave the legacy required `library_useful` event and metric unchanged;
old scenarios/reports do not acquire the new requirement. This is independent
of the existing minimal/full observer profile; do not add an observer mode.

The new event includes `measurement_contract: "games-library-usable-v1"`, the
frontend refresh request token, admitted snapshot/view token, games/displayed
clips/storage counts, and `after_library_paint: true`. Emit a frontend
`library_view_admitted` marker with the same tokens/counts when the displayed view
is accepted, before its two animation frames. The reference uses a diagnostic
view token for its admitted existing resources; it does not pretend to have a
backend snapshot. Validate marker/event token and count equality, event ordering,
matching request token and the declared contract. Reject missing, duplicate,
stale/wrong-token or pre-paint events for this namespace. Test reference/candidate
condition parity and historical-scenario compatibility. Primary metric absence
must fail the new comparison, not silently disappear from its report.

## Rejected alternatives and planning decisions

- Eager parallel ffprobe fan-out preserves the dependency and adds process load;
  optional bounded work removes it instead.
- Persisted duration/catalog caches add migrations and stale disk authority before
  they are needed. One disposable completed scan is sufficient for this unit.
- Merely hiding clip loading in the UI leaves duplicate scans and inconsistent
  counts/identities. Share the producing scan as well as the rendering boundary.
- Parsing only metadata for playable game cards would weaken strict bundle
  semantics. Reuse existing validated summary behavior; storage reuses its bytes.
- Dropping a future does not cancel synchronous OS work. Permits and children
  belong to the actual workers until completion/reaping.
- Sharing one full PlaybackProbe cache or splitting semantic playback data is not
  justified by the library probe attribution and is outside this version.

## Milestones

1. **Core scan and owned coordinator.** Add snapshot types/builder, root/revision
   publication and bounded blocking scan admission. Prove coherent derived data,
   no core probes, stale-build rejection and same-root failure behavior with
   deterministic fixtures and barriers before connecting the UI.
2. **Optional duration lifecycle and invalidation.** Add the bounded current-token
   duration command, child deadline/output ownership, all mutation invalidations,
   selected-token admission and root-switch ordering. Exercise cancellation,
   stale results, replacement/deletion, failure and partial mutation tests.
3. **Games-first UI and compatible measurements.** Integrate the single controller,
   honest optional states, every refresh trigger and strict decoding. Add deferred
   promise tests. Freeze reference/candidate builds and the new games-usable
   observation; prove the historical milestone was not silently redefined.
4. **Focused production-path comparison and disposition.** Execute the exact
   comparison below, dedicated-library interaction checks and relevant project
   verification. Retain implementation/resource evidence and durable architecture.
   If the library unit passes, record it as completed while leaving the descriptor
   and full-feature gates explicit; do not automatically restart attribution or
   implement a payload split.

## Verification design

Use the prepared immutable S/N/E libraries under
`build/perf/qb-replay-010/.chronobreak-replay-benchmark` and their preparation/hash
receipts. New candidate/reference copies, manifests and results must use fresh
sentinel destinations; preserve invalid attribution runs. Do not rebuild/decode
unchanged media unless an input identity changed or the required preparer demands
it. Do not use the schema-v1 baseline or add R/L duration campaigns for this
library-only unit.

Automated checks:

```powershell
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
python -m unittest discover -s tools/replay_benchmark/tests -v
npm run desktop:build --prefix app
npm run desktop:build:benchmark --prefix app
git diff --check
```

Run the canonical validator from `docs/development/VERIFICATION.md` after handoff
or canonical changes. Check benchmark-only taps are absent/inactive in normal
builds. Recorder/media-runtime suites need not be repeated for an app-only unit
unless the actual changes reach those components.

Focused Rust cases: one game parse per scan; exact same-scan game/clip/usage IDs,
counts and bytes; missing/invalid/v1/v2 bundle semantics; no ffprobe in core scan;
source-less clips; enumeration failure; invalid ID; bounded scan/probe/mutation
ownership under dropped callers; ten-second probe timeout and capped output;
capability-route probe admission and blocked external references; reparse/file
replacement during child admission with no out-of-root bytes;
revision invalidation during a blocked scan/probe; all successful/partial mutation
paths; failed settings persistence preserving published root; stale selection
admission; export completion after root change; disposable cache rebuilt from disk.
Use injected barriers/process fixtures, not sleeps that assume a race occurred.

Frontend cases: delayed optional durations/Data Dragon cannot block Games;
unknown and unavailable duration rendering; full-snapshot consistency; same-root
refresh failure versus new-root loading; rapid A/B/A root/navigation changes;
late old scan/clip results; save/delete/retention/export/root refresh triggers;
one active plus one coalesced intent; strict new API decoders. Preserve Viewer,
Exporter, strict replay decoding and security tests unchanged where applicable.

Manual dedicated-library sequence: open Games while duration work is delayed;
enter Clips, select a clip, switch away/back; delete/save/export/cleanup while an
optional batch is owned; switch between two disposable roots with overlapping
numeric IDs; induce optional failure; inspect fresh counts, correct cards, source
preservation, viewer return navigation and playable exported media. No wrong-root
deletion, stale visible result or optional failure blocking games is acceptable.

Comparison design: current-reference-with-new-tap versus candidate, same fixtures,
runtime/device/config/cache policy and new event. Five independent cold processes
per arm for S/N/E (30 launches), fixed seed 20260915 and existing alternating order
rules, five-second active warmup/cooldown. Five independent warm processes per arm
for N/E only (20 launches), each initial mount plus five remounts; keep initial
mount separate and aggregate remounts within each process before across-process
comparison. Warm replay is a regression characterization; the targeted improvement
is games usability at startup. Reuse startup spans rather than adding idle runs.
No percentile claim with fewer than forty observations in its exact stratum.

These 50 planned launches are a fresh comparison ceiling. Preflight before live
collection. Preserve invalid runs and stop on invalid data; do not selectively
top up the matrix. Resolve the failure and record a bounded paired-repeat
disposition under the existing protocol before any additional launch. Do not
expand into seek/scrub/decoder/capture campaigns without a concrete changed risk.

Acceptance mapping: criterion 1 is the attributed cause underlying this design;
2 maps to the shared core scan and optional probes; 3's games/optional-data portion
maps to the controller while its minimal replay descriptor portion stays open;
4 maps to owned blocking/process bounds for this unit (unmodified playback parsing
still requires disposition before full-feature completion); 5 maps to filesystem,
mutation and root tests; 6 maps to the fresh comparison and resource/error gates.

## Performance and reliability gates

Require N/E games-usable improvement exceeding both 5% and
`max(5 ms, 3 * matched reference MAD)`; S must have no disposition-requiring
regression. Evaluate both new and historical milestones with their stated
semantics. Do not claim speedup merely from fewer probes or removed code.

Apply versioned 008 comparison gates: floors 0.1 normalized CPU percentage point,
8 MiB memory, 1% bytes/counts; unfavorable change exceeding both 5% and three-MAD
band/floor needs explicit disposition. New errors, timeouts, stale results,
source mutation, event loss, or sustained-growth changes always need disposition.
Job I/O is aggregate accounting, not disk I/O; preserve 011's attribution limit.
No improvement claim can excuse wrong-root mutation or weakened delivery checks.

Maximum new ownership is one scan, one duration child/batch and one synchronous
mutation job. Shutdown/cancellation must stop admitting new work and retain all
permits until workers/children finish or are terminated and reaped. No per-frame
telemetry, timed hashing, unbounded outputs or background all-library probing.
Retain source hashes outside measured windows and the strict prepared-fixture,
manifest, terminal and process-telemetry validity gates.
