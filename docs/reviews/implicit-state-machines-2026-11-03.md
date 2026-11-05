# Implicit state-machine correctness audit — 2026-11-03

Reviewed production source at `d01c17634513dfc9798f10260d6cb68160d24561`, with the
pre-existing working tree preserved. This is an audit, not an implementation or
a completion claim for any existing feature. The inventory below covers the
meaningful recording, publication, acquisition, preparation, library, replay,
preview, export, supervision and settings lifecycles. It does not claim exhaustive
driver/hardware behavior or a review of every benchmark/tool lifecycle.

Implementation follow-up: QB-LIB-025 / QB-LIB-026 execution and current verification
are recorded in `docs/execution/qb-lib-025.md` and `docs/execution/qb-lib-026.md`.
The findings and source line references below describe the audited revision;
they are historical evidence rather than claims about the subsequently changed tree.

## Findings, ordered by severity

### F1 — P1: an old snapshot can delete content in a replacement library root

**Proven and reproduced.** A logical path and two matching cached directory
identities are treated as if they still identify the live filesystem object.

1. A successful refresh publishes snapshot token T for directory P, generation A.
   `MediaRoots::OutputDirectory` and `PublishedSnapshot` both retain A.
2. Another process renames P away and creates a new ordinary directory at P,
   generation B, containing the same clip identifier. This needs no junction,
   malformed input or filename-validation bypass.
3. `delete_clip(T, id)` calls `LibraryCoordinator::mutate`. Both admission checks
   in `capture_selection` compare the two cached A values and accept T.
4. The owned blocking worker passes only `capture.path` to the mutation closure.
   `library::delete_clip` resolves P again and deletes the clip in B.
5. Completion invalidation occurs after the deletion and cannot repair it.

The missing transition is `admitted(A) -> obsolete` when the filesystem replaces
the directory without an application root publication. The safety invariant is
that an admitted destructive operation may affect only its captured directory
generation for the entire operation.

Evidence: [cached admission](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library_coordinator.rs:343),
[path-only worker](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library_coordinator.rs:684),
[delete command](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/lib.rs:273),
[actual removal](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library.rs:633).
The same worker serves save, game deletion and retention, so the ownership gap
also reaches metadata changes and recursive recording deletion. F1 was executed
with clip deletion; those other impacts follow from their shared admission and
path-only dispatch, not separate runtime reproductions.

The application already has the required identity vocabulary:
[ApprovedRoot::matches_directory](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/playback_file.rs:25)
and [guard_root](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/export_staging.rs:167).
Refresh retains a root guard and scans the approved canonical root; mutation
does neither. The existing replacement test invalidates through refresh and
then explicitly republishes a root, so it does not exercise direct mutation
with the old token before refresh.

**Modeling would materially improve correctness** if admission produces a guarded
mutation capability that retains the captured directory identity and is the only
input accepted by destructive workers. Merely adding another enum or checking
identity once before path-based work would leave a replacement race. Guard the
relevant directory chain or use handle-relative operations throughout the effect.

### F2 — P1: successful save protection races with deletion in another app owner

**Proven and reproduced with independent coordinators.** General library mutation
exclusion exists only within one `LibraryCoordinator` instance. It is not shared
between desktop processes. A single recorder lease does not serialize app saves
and deletions; the export lease covers a different set of operations.

1. Owner A admits deletion of an unsaved recording and reads `saved == false`.
2. Before A removes the directory, owner B admits `save_game(..., true)` through
   its independent mutation slot. B atomically writes the metadata and succeeds.
3. A removes the directory without rechecking or excluding B. A also succeeds.

Both successful results cannot describe a valid serialized lifecycle: if save
happens first, deletion should refuse the saved recording; if deletion happens
first, save should fail because the recording no longer exists.

Evidence: [per-instance semaphore](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library_coordinator.rs:160),
[save and atomic metadata write](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library.rs:598),
[saved check followed by removal](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library.rs:624).
Retention has the same gap, with a potentially longer interval:
[list/filter saved state, then remove](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library.rs:670).
The retention variant is code-proven but was not separately executed.

The reproduction uses two independent coordinators in one test process. This
exercises the same lack of shared mutation arbitration as two app processes; it
is not a claim that two actual desktop windows were launched. A test-only barrier
in the copied source pauses immediately after A's real saved-state check. B's
real save runs to completion, then A's real removal resumes. No deletion logic,
validation or saved-state logic was replaced.

**Explicit modeling helps only with shared arbitration.** Model save/delete as
exclusive transitions for the same recording across processes, retain that
ownership through metadata validation and removal, and fail closed on ambiguous
ownership. A last-moment saved-state recheck alone still races. Preserve the
existing cancellation/worker permit behavior and source protection.

## Follow-up review: retained recommendations and limits

Both findings retain P1 priority because their demonstrated outcome is unintended
data loss. Their triggering conditions differ: F1 requires external directory
replacement before a refresh invalidates the token; F2 requires independent
mutation owners sharing a library. Neither reproduction establishes how often
these conditions occur in ordinary use. No additional defect or runtime result
is claimed by this review.

For F1, reuse the existing approved-root identity and Windows directory-handle
approach rather than inventing a parallel authority mechanism. Protect the
relevant chain below the root as well: `games`, `clips` and the selected recording
directory can otherwise change while path-based work proceeds. Replacement
below the root is an additional regression requirement, not a separately executed
finding in this audit.

Reusing `DirectoryGuard` is a starting point, not a complete deletion design.
Its [open flags](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/export_staging.rs:67)
omit delete sharing, so holding the same guard on a deletion target also prevents
its intended removal. Dropping that guard and then resolving the target path
again would reopen the ownership gap. Export's
[flat batch cleanup](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/export_staging.rs:421)
is not a ready-made recursive recording deletion primitive. The implementation
must reconcile retained identity with actual removal, for example through
appropriate handle-based operations, and test both successful intended deletion
and replacement rejection throughout the effect. Extract a small shared helper
only if needed; do not make general library mutation depend on export staging.

For F2, a single OS-held mutation lease per library is the preferred initial
design candidate. Existing export staging already demonstrates a
[nonblocking file lock](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/export_staging.rs:177)
whose handle lifetime releases ownership, including on process exit. A new
library lease should coordinate all participating save, manual deletion and
retention operations against the same guarded canonical library, including
logical aliases, while allowing unrelated libraries to proceed. This protects
cooperating application owners; it does not make arbitrary external file writers
obey the lock.

The shared lease is needed before saved-state validation, not necessarily before
the coordinator's in-memory token admission. Acquire it in the owned worker,
validate captured root authority on acquisition, and retain it through all
filesystem effects and partial-error cleanup even if the caller disappears.
Retention must derive or revalidate saved/incomplete/age eligibility under this
same lease; locking only around removal leaves an earlier scan stale. Specify
lock ordering with existing export/settings ownership and bounded busy behavior.
Do not couple long-running export encoding to this lease without a demonstrated
need. A shared lock alone does not solve F1, and directory guards alone do not
solve F2.

Prefer this coarse lease over per-recording locking until a concrete contention
problem justifies more complexity. Negligible overhead is an expectation, not a
measured result: verify bounded contention, independent-library progress and
release after cancellation/process death. The existing two-coordinator test
proves the missing arbitration, but the fix also needs an actual separate-process
regression. The other ten lifecycle reviews remain bounded observations, not
proofs of correctness; recorder hardware continuity, loading-screen behavior and
performance remain outside the executed checks.

## Lifecycle inventory and field ownership

### 1. Recorder service admission and retry

Logical states: idle -> observing a candidate -> window stability pending ->
starting -> recording -> stopping/finalizing -> idle. Retryable startup failure
enters bounded backoff; terminal failure blocks that exact process generation;
unproven native cleanup blocks further capture in the recorder owner.

Fields/owners: the service's nullable `active` and `starting`, blocked original
process, startup retry identity/count/deadline, readiness evidence and command
receiver jointly encode the machine. `run` writes them on watcher ticks, startup
completion, progress failure and shutdown. `startup_candidate`,
`RecordingStartupRetryState`, `spawn_recording_start` and `cancel_starting` are
the other transition owners. The tray/control projection consumes emitted
lifecycle events; it does not own admission.

References: [state types](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/service.rs:70),
[retry policy](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/service.rs:178),
[main select loop](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/service.rs:535),
[startup cancellation](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/service.rs:1007),
[readiness](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/launch_readiness.rs:75).

Invariant: at most one startup/session owner; a changed enumeration result or
closed window cannot establish original-process death. Cancellation joins owned
startup before retry. No demonstrated violation found. An explicit service-state
enum would reduce illegal `active + starting` combinations, but a corrective
rewrite is not justified by a proven defect here.

### 2. Native process lifetime, replaceable source and media progress

These are orthogonal machines: original process live/ended/query-failed; WGC leaf
attached/detaching/absent/reattaching/failed; GPU/encoder/mux graph
starting/running/draining/failed/retained-for-unsafe-cleanup. Detached capture can
remain running with last-image CFR output.

`OriginalProcess` owns the exact-generation handle. `NativeRecordingSession`
owns worker/completion/mux/progress resources. The GPU worker changes the optional
source, reattach deadline and cumulative frame/error diagnostics. FrameArrived
and Closed callbacks update leaf state; close/detach waits for quiescence.
`NativeWgcSource::Drop` and startup rollback handle partial construction.
The service watchdog reads source and downstream progress independently.

References: [session ownership](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/native/lifecycle.rs:149),
[source transitions](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/native/session.rs:396),
[source cleanup](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/native/source.rs:192),
[watchdog](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/service.rs:369),
[exact-process platform checks](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/platform/windows.rs:200).

Invariant: one source at a time; same original process and supported adapter;
no encoder/timeline replacement on ordinary window loss; first real failure is
not erased by later process exit; unsafe callback cleanup retains the entire
guarded context. No proven defect found. Keep source state separate from process
ownership in any explicit model. Hardware continuity remains unverified in the
existing checkpoint and was not retested by this audit.

### 3. Stop, recording publication and interrupted recovery

Stop joins poller, video and provisional-context owners. Healthy joined video
then passes strict media/timeline validation before canonical publication;
failure preserves partials. On orderly service shutdown, one joined recovery
pass can repair receipt-bound interrupted publication.

Disk states include partial video + pending metadata + receipt; validated
canonical video + pending metadata; canonical video + canonical metadata.
Ambiguous, unsupported, mismatched or unreceipted artifacts stay untouched.
`stop_recording_inner`, finalizer validation/publication, recovery discovery,
receipt revalidation and no-replace moves own these transitions. The library's
eligibility scan reads the canonical outcome, not a success flag from the service.

References: [joined stop](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/service.rs:1610),
[native failure precedence](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/native/lifecycle.rs:403),
[pinned publication](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/finalizer/publication.rs:95),
[recovery rechecks](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/finalizer/recovery.rs:178).

Invariant: only validated media can become canonical; crash recovery cannot
overwrite an existing final member or infer authority from age/PID alone.
No demonstrated defect found. Receipt and guarded-resource ownership already
provide the meaningful explicit state; a parallel persisted lifecycle would
create competing authority.

### 4. Optional Live/EOG/history acquisition and sidecar installation

Live polling runs independently of media readiness. Capture close snapshots
provisional match identity and submits optional final-fact jobs. Jobs progress
pending -> reserved/in-flight -> retry-wait or accepted -> awaiting canonical
publication -> installed, with cancellation/deletion/expiry/failure exits.

The EOG coordinator owns queued jobs, publication/cancellation commands, reserved
attempts, expiry/cadence and durable ledger state. Acquisition callbacks provide
facts; only `Published(media_id)` permits installation. The history coordinator
separately owns its admission order, next-attempt timestamps, accepted result and
installation state, retaining bounded work across restart. Exact match/account
identity and directory-generation checks gate sidecar writers. Readers prefer
valid history facts and otherwise accept valid EOG facts.

References: [EOG job state](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/league_client/post_game.rs:571),
[EOG commands](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/league_client/post_game.rs:759),
[EOG worker](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/league_client/post_game.rs:834),
[history queue/worker](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/league_client/history.rs:281),
[capture-close handoff](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/service.rs:1642).

Invariant: optional facts cannot publish unhealthy media, recreate a removed
recording or invent a match association. No demonstrated defect found in the
reviewed handoff. An explicit job phase may help future ledger work, but existing
publication and identity predicates are essential even with such an enum.

### 5. Backend snapshot, mutation and save-protection ownership

Snapshot states: absent -> refreshing -> accepted; invalidation keeps display
data but removes action authority; failed refresh can retain display data;
shutdown closes all admission. `Publication` stores `revision`, `root_epoch`,
`selected_token`, `current`, cancellation flags, duration cache and `closed`.

`refresh_with`, `publish_output_directory`, `invalidate_capture` and `shutdown`
write publication state. `capture_selection`, replay/index readers and mutation
admission consume it. `MutationCompletion::Drop` invalidates after any admitted
attempt, including error/unwind, and owns the mutation permit until worker exit.
`save_game`, `delete_game`, `delete_clip` and `run_auto_delete_at` write disk state;
scan/summary code reads `saved` and canonical media eligibility.

References: [publication fields/invalidation](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library_coordinator.rs:35),
[completion guard](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library_coordinator.rs:130),
[root publication/shutdown](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library_coordinator.rs:302),
[all mutation effects](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/library.rs:598).

Invalid reachable states are F1's `token-current but live directory-obsolete`
and F2's `saved-successfully but concurrently deleted`. These require capability
and cross-process ownership fixes; frontend generation checks cannot repair them.

### 6. Background preparation and disposable League cache

States: clean idle -> dirty/coalescing -> preparing bounded pages -> ready or
source-version unavailable; writer contention/transient failure -> retained
dirty work with backoff. Startup, root/cache replacement, watcher overflow and
periodic recovery request authoritative rescan. Shutdown requests cancellation.

`Background` owns root/cache, optional watcher/approved identity, retry hints,
`next_attempt`, `next_rescan`, and `Preparation { games, completed stamps }`.
`Changes::event/merge/take` are callback-side dirty writers. `tick` consumes hints,
reestablishes identity and retains work on failure. `prepare` reconciles catalog,
imports four journal batches or one overlay page per unit and commits a terminal
source-version result. `Owner::Drop` sets stop; blocking pages check stop and
root identity. App/headless workers and recorder-owned helper have distinct
process lifetimes but share the index writer lock. UI status reads do not own
the preparation worker.

References: [background fields/tick](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/league_preparation.rs:23),
[app owner](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/league_preparation.rs:101),
[bounded prepare](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/league_preparation.rs:142),
[notification callback](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/league_preparation/changes.rs:108),
[recorder helper owner](C:/Users/Hugo/Documents/perso/Chronobreak/recorder/src/preparation.rs:3).

Invariant: source changes invalidate derived rows; a failed source version can
settle without blocking later sources; cancellation cannot publish an unchecked
root as current media authority. No additional proven defect found in this
bounded review. Root switches may leave one old bounded preparation unit running;
that alone is not proof of stale current-root publication.

### 7. Frontend library navigation, query and preparation status

`LibraryOrigin { root, rootEpoch, request, token, navigation }` is the admission
identity. States include refreshing/accepted/display-only, mutation active,
queued reads/writes and disposed. `setRoot`, `refresh`, `navigate` and disposal
supersede queued intents; active IPC remains owned until settlement.
`writeIndex/pumpIndex` hold one active and latest maintenance/replay/history
intent, with maintenance then replay priority. `readReplay/pumpReplay` serialize
details/overlay lanes. Export completion refreshes its captured root and rebinds
only the exact surviving editor/source/navigation.

`HistoryController` owns context, query generation, query pending/active/dirty,
import active, catalog cursor/pending items, timer and closed state. Its pumps
and async finally paths release lanes; current-context predicates reject stale
results. Initialization ends only after preparation/status work and a current
query settle. Errors retain previous cards while disabling stale pagination.

References: [origin and lanes](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/libraryController.ts:53),
[index and replay dispatch](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/libraryController.ts:185),
[query state/pump](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/historyController.ts:28),
[initialization/import pump](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/historyController.ts:134),
[editor rebind](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/App.tsx:541).

Invariant: superseded work may finish but cannot publish into a newer origin;
running IPC is not declared cancelled simply because navigation changed.
No demonstrated defect found. Keep separate lanes and origin identity rather
than flattening all loading flags into one global state.

### 8. Replay optional details, overlay and scoreboard

Details initialization, requested event window and compact overlay are separate
machines. Loading has precedence while current work exists; when idle, useful
survivors yield limited data and no useful payload yields unavailable.

`ReplayDataController` writes initialization/window generations, pending reads
and a four-page event cache; async facts/status/scope/window callbacks check
their captured generations. `ReplayOverlayController` owns selected-scope
generation, active/latest pending scope, compact samples/revision, scoreboard
offset/bytes/maps, idle cancellation and seek quiet-period state. Its select,
pump, loadScoreboard and dispose paths own writes; Viewer/ReplayScoreboard read
snapshots and position lookups. Source changes clear source-owned data; equal
tick continuation boundaries must not supply an unproven predecessor.

References: [details init/window/disposal](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/replayDataController.ts:112),
[overlay fields](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/replayOverlayController.ts:18),
[overlay/scoreboard dispatch](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/replayOverlayController.ts:85),
[scoreboard pages](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/replayOverlayController.ts:155).

No additional demonstrated defect found. Current source and data-pipeline
architecture explicitly start scoreboard warmup before first open; older lazy
warmup wording elsewhere is not evidence of a behavioral defect.

### 9. Primary playback, seek presentation, play intent and rate recovery

Readiness is already explicit: closed/loading/ready/recovering/degraded/disposed.
Seek phases remain a product of `active`, latest `pending`, presentation `waiter`,
generation/epoch and requested/dispatched/seeked/presented positions. Play intent,
native play-promise ownership, selected/applied/observed rate and recovery budgets
are additional independent dimensions.

`open/beginLoad/recover/dispose` write source identity/readiness and invalidate
callbacks. `seek/dispatch/onSeeked` and RVFC write seek evidence; frame callbacks
capture generation/frame-version/intent/epoch. Public play tokens reject late
promise resolution/rejection. `cancelWork` clears timers, settles owned work and
invalidates callbacks; metadata, seek, presentation and rate deadlines lead to
bounded recovery or terminal degradation. The HTML adapter owns the current
element binding, while Viewer consumes snapshots and authoritative frame time.

References: [readiness/seek fields](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/playbackController.ts:121),
[cleanup](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/playbackController.ts:218),
[play ownership](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/playbackController.ts:393),
[RVFC/seek transitions](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/playbackController.ts:443),
[load/recovery/disposal](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/playbackController.ts:688).

Invariant: native seek completion alone is not presented-frame evidence; either
RVFC/seeked ordering must settle correctly; only one native seek and latest
successor are admitted; old source callbacks cannot update the new generation.
No proven defect found. A local seek-phase union could improve future transition
exhaustiveness, but replacing the separate clocks with one enum would lose
necessary evidence. Known unsupported-rate fallback is intentional behavior.

### 10. Clip preview and imported music ownership

Preview has a primary video clock plus optional backdrop/music followers;
paused/seeking/playing/looping, source selection and editor lifetime are separate.
`ClipExporterScreen` owns its seek timer/latest seek and follower synchronization.
Imported music has none -> preparing -> registered -> releasing, with stale and
disposed exits. The helper's `generation`, `current`, serialized `work` promise
and `disposed` flag control release-before-prepare and disposal of late tokens.
Backend `MediaRoots` stores one optional token/path; release only removes the
matching token, so old release cannot revoke a newer preview.

References: [preview synchronization](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/components/ClipExporterScreen.tsx:189),
[music integration](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/components/ClipExporterScreen.tsx:389),
[async music owner](C:/Users/Hugo/Documents/perso/Chronobreak/app/src/importedMusicPreview.ts:3),
[backend token registry](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/playback_server.rs:464).

No proven leak or stale-token publication found. A late native play promise after
pause is a hypothetical risk here, not a demonstrated defect; browser promise
and pause behavior must be accounted for before claiming it.

### 11. Export admission, child process, staging and partial publication

States: admitted -> leased/staging-owned -> encoding (optional ordinary-error
fallback/size retry) -> validating -> thumbnailing -> publishing members ->
complete. Cancellation/deadline/stall goes through child kill/reap and staging
cleanup before permit release. Process death leaves leased ownership abandoned;
next eligible recovery removes only strictly owned unpublished members.

`run_owned` ties cancellation to the waiter's lifetime while the blocking worker
retains `MutationCompletion`. `run` owns child, both pipes, EOF flags, exit status,
progress watermark and timestamps. `ProgressRecords` advances only on increasing
output time. `Lease` owns guarded directories and OS lock; `Batch` owns strict
manifest and numbered staging paths. `publish_export_paths` moves validated MP4
then JPEG with no replacement. `Batch::Drop/recover/cleanup` never remove finals.
UI progress is observational and cannot authorize publication.

References: [worker cancellation](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/export_process.rs:19),
[child lifecycle](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/export_process.rs:149),
[admission/completion handoff](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/lib.rs:351),
[batch execution](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/clip_export.rs:237),
[staging lease/recovery](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/export_staging.rs:176),
[publication/cleanup](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/export_staging.rs:375).

Invariant: waiter cancellation is not worker completion; interruption cannot
trigger software fallback; published subsets survive errors; active/foreign/
malformed staging is retained. No proven defect found. RAII resources already
encode the important states; changing a progress enum would not improve these
ownership guarantees.

### 12. Recorder availability and explicit login/configuration updates

Recorder service state (Starting/Available/Stopping/Failed/Stopped), capture
activity, optional capture problem and service failure are independent. `Status`
apply/validate/project owns coherent combinations; tray/headless events supply
transitions. `RecorderSupervisor` observes a bounded authenticated pipe,
serializes initial ensure/explicit Retry, and caches projected status. Its
`ensure` flag, `starting_since`, retry channel and stop sender govern launching;
observer failure does not establish owner death. Other-session owners never
grant local capture capability. Drop cancels observation rather than recording.

Login state is desired TOML preference × actual Run value × update/rollback
outcome; OS effective execution remains unknown. `ConfigMutation` holds the
cross-process config lock, rereads latest TOML and patches only owned fields.
`update_owned` reads/writes/verifies registry, persists config and conditionally
rolls back its own value on failure. Status/read and UI consume actual versus
desired separately. HEVC, storage and autostart writers share config mutation
serialization; configuration reads do not implicitly repair registration.

References: [status fields/transitions](C:/Users/Hugo/Documents/perso/Chronobreak/recorder-control/src/lib.rs:55),
[supervisor](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/recorder_supervisor.rs:195),
[registry/config update](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/autostart.rs:78),
[config callers/root reconciliation](C:/Users/Hugo/Documents/perso/Chronobreak/app/src-tauri/src/lib.rs:558).

No demonstrated defect found in reviewed transitions. Crash between registry
and TOML writes creates an observable mismatch by design. An external registry
writer can race read/conditional-write operations; that remains a risk requiring
a concrete external interleaving, not a reproduced in-app failure. More enum
variants cannot make two OS resources atomic.

## Verification and limits

- Focused original frontend suite: six files, **143 passed**. Covers playback,
  library/history, replay data/overlay and imported music. Log:
  `.codex/logs/command-20261103-161102.log`.
- Focused original backend suites: **21 passed** — export process 6, staging 6,
  supervisor 6, autostart 3. Logs: `command-20261103-160735.log`,
  `command-20261103-160834.log`, `command-20261103-160842.log`,
  `command-20261103-160846.log` under `.codex/logs/`.
- Disposable copied-backend safety regressions: **2 expected failures**, both
  after actual deletion. Root replacement: `delete_result=Ok(())`,
  `replacement_exists=false`. Concurrent save: `save_result=Ok(())`,
  `delete_result=Ok(())`, `game_exists=false`. No real media or desktop owner was
  involved. Both reviewed production source hashes remained unchanged.
- Reproduction script, copied sources, source hashes and raw log are retained
  under ignored `build/state-machine-audit-20261103/`; the persistent follow-up
  definitions are `QB-LIB-025` and `QB-LIB-026` in `feature-list.json`.
- Canonical validation passed: 111 roadmap items / 28 linked plan-checkpoint
  pairs at audit time. All 69 source file/line references resolved then; both
  follow-ups were draft/not-started with no implementation plan, and whitespace
  validation passed. Current lifecycle state belongs to the linked feature entries
  and execution checkpoints.
- Reproduce in a fresh output directory with the script, then run
  `cargo test --manifest-path build/state-machine-audit-20261103/backend/Cargo.toml --target-dir app/src-tauri/target --offline --lib state_machine_audit -- --nocapture --test-threads=1`.
  The script refuses to overwrite an existing copy. The tests assert safety and
  therefore intentionally fail on the audited implementation.
- Initial restricted test attempts could not write the local logging folder;
  the permitted reruns above actually executed. No full project suite, production
  build, long soak, real League/WGC/NVENC continuity or manual A/V validation was
  performed for this audit. Passing focused tests do not negate the failing
  interleavings or certify every hardware/error path.

Other inspected error paths were not promoted to defect findings:
`PollerSession::stop` can abort acquisition after its bounded wait, leaving an
incomplete optional telemetry tail; this does not authorize healthy-video
publication by itself. An original-process handle query failure can propagate
out of the service rather than remain a per-recording problem; that is
fail-closed and has no reproduced Win32 failure here. These are distinct from
the two executed destructive violations.

The highest-value correction is to make destructive mutation authority real and
shared. Broad conversion of already guarded replay/export state into enums is
lower priority than fixing the two demonstrated ownership violations.
