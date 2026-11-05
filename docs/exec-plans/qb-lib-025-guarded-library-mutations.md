# Guarded library mutations — QB-LIB-025 and QB-LIB-026

## Purpose

An admitted save, deletion or retention operation must affect its approved library
directory, and concurrent desktop owners must agree on saved-recording protection.
These coordinated features share one worker-owned mutation capability. Their
acceptance criteria remain in `feature-list.json`.

## Relevant planning-time architecture

`LibraryCoordinator::mutate` takes a nonblocking local mutation permit, captures
the logical path and `ApprovedRoot` twice around admission, then transfers its
completion guard into `spawn_blocking`. Completion invalidates even partial errors
and survives waiter cancellation. Its worker currently passes only a logical path.
Settings and export admission share the local permit, but the permit is not shared
between application processes.

`ApprovedRoot` in `playback_file.rs` stores canonical final path and Windows volume /
file identity. `export_staging.rs::DirectoryGuard` opens directories without delete
sharing, rejects reparse points and checks opened-handle final paths. Export staging
also has a regular-file-validated, nonblocking `File::try_lock` lease. Recorder
sidecar cleanup already uses `SetFileInformationByHandle(FileDispositionInfo)` on
an owned DELETE-capable handle; it does not reopen a deletion pathname.

`library.rs` owns strict bounded metadata parsing, save, game/clip deletion and
retention. Startup in `lib.rs` invokes retention outside the coordinator and must
acquire equivalent filesystem ownership. Existing tests cover mutation invalidation,
caller cancellation and partial clip deletion. Ordinary app tests can self-execute
test children using sentinel-marked temporary libraries and bounded ready/go files.

## Scope and non-goals

Implement both features as one coordinated unit. Preserve token membership,
configuration publication, saved/incomplete protection, metadata format, export
staging semantics and explicit partial-error reporting. No recorder lifecycle,
hardware, playback, cache or UI redesign is required. This coordinates cooperating
app mutations; arbitrary external content writers and the deliberately accepted
recorder optional-result publication race are not newly serialized.

Approved snapshots name items within a directory generation, not immutable media
bytes. Descendant handles retain the exact objects opened for a mutation; this
does not add persistent per-file generation inventories to library snapshots.

## Exploration findings

- Root identity comparison is cached until a live guard is acquired. A new root at
  the same path can otherwise receive an old token's destructive effect.
- Guarding only a parent does not freeze its child names. A target cannot be
  checked, dropped and subsequently removed by pathname.
- A target opened without delete sharing can still be removed through its own
  DELETE-capable handle. Path-based `remove_dir_all` cannot provide that ownership.
- Startup retention is an additional mutation caller. Save/delete/retention must
  all require the capability in their function signatures to prevent bypasses.
- Retention must inspect current eligibility while holding both shared library
  ownership and the recording target; earlier snapshot flags cannot authorize it.

## Chosen design and rationale

### Shared directory and file primitives

Extract the existing export directory guard into an app-internal
`directory_guard.rs`, retaining its Windows open flags, final-path checks,
generation validation and fail-closed non-Windows behavior. Reuse it from export
staging and refresh. Share validated no-reparse regular-file opening for leases
and mutation metadata reads. This is a small internal primitive, not a new public
filesystem or security service.

Add a distinct removal target opened with read and DELETE access, read/write
sharing but no delete sharing, and OPEN_REPARSE_POINT/BACKUP_SEMANTICS. Validate
its opened type, reject reparses and require its final path to equal the guarded
parent plus one leaf. Borrow the parent guard/target while children are owned.
This keeps root, collection, recording and nested directory names stable through
the actual effects. Directory enumeration is followed by validated child-handle
opening, never an unchecked recursive pathname delete. Remove ordinary files and
then empty directories by marking their exact handles with FileDispositionInfo;
close the same handle to complete removal. Do not drop and reopen target names.
Bound recursive depth at 64 and reject unsupported entries explicitly. File count
does not accumulate open handles; ownership is bounded by traversal depth.
Partial I/O errors remain errors and are invalidated by the existing completion
guard. Authority rejection at acquisition performs no media mutation.

### Worker-owned capability and lease

`LibraryMutation` owns the guarded approved root and a regular root-relative
`.chronobreak-mutation.lease` file holding an exclusive nonblocking OS lock.
Acquisition verifies the captured root generation and the current logical alias
against that approval before creating/acquiring the lock. Work uses the canonical
guarded root thereafter, so later alias retargeting cannot redirect effects.
The lock file remains present after use, is never time/PID-reclaimed, and cannot
be replaced while its checked handle is open. Aliases resolving to the same
library contend on the same file. Different libraries use independent files.

Order: local coordinator permit and selection recapture -> approved root guard ->
library lease -> collection/target handles -> current metadata validation ->
effects. No publication/configuration mutex spans filesystem I/O or await. Shared
lock contention returns a bounded explicit busy error without queueing. Export
encoding/recovery retains its separate staging lease; mutation does not acquire
it, so there is no nested export/mutation lock cycle or full-encode serialization.
Verify distinct lock handles contend both within one process and across processes;
do not assume the coordinator-local permit supplies same-process arbitration. If
the supported OS lock does not provide this, supplement it with process-local
ownership keyed by the guarded root identity before integrating mutation callers.

Construct and retain the capability within the admitted blocking worker. A dropped
async waiter cannot release it, the local permit or completion invalidation before
the worker exits. Acquisition failures still run admitted completion invalidation.
Change mutation function signatures to require `&LibraryMutation`; startup obtains
an approval and the same capability after creating its configured output folders.
Disabled startup retention does not need a lock or scan.

### Mutation effects

Save holds root/games/recording guards, opens and strictly parses the regular
metadata handle, then retains directory guards and lease through existing atomic
metadata replacement. Split the existing parser to accept a checked file without
weakening its bounds or schema checks. The metadata read handle closes before
atomic replacement; the directory chain and shared mutation lease stay owned.

Manual game deletion acquires a removal-capable recording target before reading
strict saved intent. Only an unsaved recording proceeds to handle-based recursive
removal. Clip deletion pins `clips`, removes the recognized MP4 by its opened
handle, then its optional JPEG. Preserve current visible partial errors if JPEG
removal fails after the MP4 has been removed.

Retention enumerates recognized game IDs under the held `games` directory while
holding the lease, acquires each recording removal target, and derives strict
saved state, timestamp and canonical-video completeness again under that target.
Skip missing/invalid metadata, saved, incomplete, recent and invalid-date entries.
Only eligible retained targets are removed. No pre-lease scan authorizes removal.

## Rejected alternatives and planning decisions

- Cached identities, an extra enum or a one-time live precheck do not retain
  directory authority during effects.
- A parent guard plus a delete-shared target dropped before pathname deletion
  still permits replacement of that child name.
- Holding today's read-only deny-delete guard then calling `remove_dir_all`
  prevents intended final deletion. Use DELETE-capable owned target handles.
- Rechecking saved intent immediately before removal still races across app
  processes. Serialize the validation and effect together.
- Per-recording locks add coordination complexity without demonstrated contention
  need. Begin with one nonblocking lease per canonical library.
- Do not share export's staging lease or hold a library mutation lease through
  encoding. Those owners protect different effects.

## Milestones

1. Extract/reuse directory primitives, implement removal-capable targets and the
   guarded per-library lease. Verify intended deletion, descendant replacement
   exclusion, aliases, same-library busy and independent-library progress.
2. Integrate the capability into coordinator, every library mutation and startup
   retention. Preserve metadata validation, retention completeness and partial
   errors. Add deterministic root replacement, stale eligibility and cancellation
   coverage through real production functions.
3. Add actual subprocess contention/crash-release tests; run the applicable backend
   baseline and frontend interface checks, document current architecture and
   record truthful completion evidence in both checkpoints/features.

## Verification design

All destructive fixtures use `tempfile` or sentinel-marked generated libraries.
No test accepts a real media path from normal configuration.

- Root replacement before worker acquisition rejects old authority for save,
  game/clip deletion and retention; originals and replacements retain their bytes.
  Test the acquisition boundary independently from coordinator token admission.
- Alias retargeting before acquisition rejects; retargeting after acquisition
  cannot redirect canonical guarded work. Attempts to replace root, `games`,
  `clips`, recording and opened clip/nested targets while owned fail without
  redirecting effects. Intended final recording/clip deletion still succeeds.
- Existing mutation tests prove successful save/unsave/delete/retention,
  saved/incomplete protection and partial-error invalidation. Add stale pre-lock
  eligibility scenarios and malformed/reparse fail-closed fixtures.
- Self-executing test children use bounded ready/go barriers and RAII kill/wait.
  Exercise save versus manual/retention deletion with real independent processes,
  canonical/alias contention, bounded busy responses, independent roots, killed
  holder release and a successful subsequent owner. Verify operation results and
  actual surviving content, not just lock acquisition.
- Two separate capabilities in one process must also contend on the same root
  and aliases; different libraries remain independent.
- A current-thread async test aborts an admitted waiter while the owned worker is
  paused. Root replacement and another owner remain excluded until worker exit;
  afterward the lease/permit release and completion invalidation are observable.

Commands from repository root:

```powershell
cargo test --manifest-path app/src-tauri/Cargo.toml library_mutation -- --nocapture
cargo test --manifest-path app/src-tauri/Cargo.toml library_coordinator -- --nocapture
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run check --prefix app
npm run build --prefix app
python build/validate_features.py
git diff --check
```

Run focused export/root regressions after extracting the shared guard; the full
backend baseline subsumes those checks for final evidence. Frontend unit tests
for library/history are appropriate if command/error behavior changes. Recorder
hardware/League tests are outside this backend-only storage change.

## Performance and reliability gates

Same-library contention must reject without waiting on another owner's release;
bounded tests use at most ten-second process/barrier deadlines rather than brittle
millisecond performance thresholds. Independent libraries must progress while one
lease is held. Owned handles remain bounded by directory depth, cancellation does
not detach ownership, and process death releases the kernel lease. Measure and
report test timings only as fixture behavior, not whole-app or capture overhead.
Require original/replacement media preservation on rejected authority, strict
saved/incomplete retention protection, successful final directory removal and
truthful partial errors. No negligible-overhead or hardware-reliability claim is
implied by these checks.
