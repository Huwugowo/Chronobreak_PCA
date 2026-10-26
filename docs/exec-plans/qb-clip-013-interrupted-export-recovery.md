# QB-CLIP-013: Interrupted export recovery

## Purpose

After an application process dies, the next library refresh reclaims abandoned,
app-owned export staging without touching recordings or completed clips. Every
visible exported MP4 has passed the existing media contract. A partly published
multi-preset batch can retain its completed members; thumbnails remain optional.

## Relevant planning-time architecture

`clip_export::export` encodes, probes and thumbnails all requested presets before
`publish_export_paths` renames their direct `.part` files. `ExportPaths::Drop`
cleans ordinary failures, but cannot run after process death. The publication
error path currently deletes final names, and `exists()` plus rename is not an
ownership proof. `export_process::run_owned` retains the in-process mutation
permit and child cleanup through dropped waiters.

The `LibraryCoordinator` captures a logical path and `ApprovedRoot` together. Its
scan runs in a bounded owned blocking worker; stale snapshots cannot publish.
The mutation slot excludes exports within one app, but is not a process-wide
filesystem lease. Library scans count direct MP4 files only; directories are
excluded, and a completed clip without a JPEG is already displayable.

## Scope and non-goals

Add one export-staging owner, a cross-process export/recovery lease, safe
publication and recovery before normal snapshots. Preserve the existing source
frame/audio validation, encoder fallback, cancellation, snapshot/root admission
and export result contracts. Do not migrate or delete old unowned `.part` files,
recover user-deleted clips, change the recorder, or claim power-loss durability.
Benchmark refresh remains read-only. Explicit benchmark exports retain their
existing isolated-fixture write behavior, but cannot reclaim earlier staging.
Recovery does not encode or probe media.

## Exploration findings

There is no durable batch identity today. Ordinary cancellation coverage is not a
process-death test. Existing packaged-runtime tests generate media in disposable
libraries and check output decode, frames/audio, source SHA and visible artifacts.
The source/preexisting-file boundary must be tested independently of an app-owned
manifest. Windows is the supported deployed platform. The installed Rust toolchain
supports OS-held `File::try_lock`; locks release when their owning process dies.
See [Rust File locking documentation](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock).

## Chosen design and rationale

1. A new `export_staging` module owns
   `clips/.chronobreak-export/<32 lowercase hex batch id>/`. The directory
   contains a bounded, strict, version-1 `owner.json` and at most three numbered
   MP4/JPEG staging pairs. The manifest is created once, flushed and synced before
   encoding starts; it binds the directory ID, canonical destination basenames
   and fixed numbered staging names. It contains no arbitrary path authority.
   A crash before a complete ownership record leaves an untrusted directory that
   recovery retains. No mutable progress journal is needed: recovery never promotes
   unfinished media or deletes final names.
2. Both export and recovery acquire the same nonblocking exclusive OS file lock
   at the fixed `.chronobreak-export/.lease` name, opened read/write with no
   delete sharing. The lease is never removed. Reparse/nonregular lock files fail
   closed; creation races use create-or-open under the guarded parent and the
   opened handle is validated. Export retains it through child work, publication and
   staging cleanup. An active lease makes recovery skip without touching batches;
   another exporter gets an actionable busy error. PIDs and timestamps never
   establish abandonment. Existing same-process admission remains in place.
3. Add a Windows guarded-directory API retaining handles for the captured root,
   clips, staging and batch chain (`FILE_SHARE_READ | FILE_SHARE_WRITE`, never
   `FILE_SHARE_DELETE`, with `FILE_FLAG_BACKUP_SEMANTICS`). Reject reparse points
   and verify each opened handle's resolved path against its exact expected path,
   starting from `ApprovedRoot::path()`. Export receives `RootCapture.root`; scan
   recovery receives `ScanCapture.root`. Path-based core scanning follows recovery
   and remains separate. Cleanup is flat: remove
   only manifest-enumerated regular staging files, then the manifest and empty
   batch directory. Never recursively remove a path. Unknown entries, unsupported
   or malformed ownership, reparse points and cleanup errors are retained and
   reported through diagnostics; they cannot authorize final/source deletion.
4. Allocate every destination and staging pair before encoding, then persist the
   ownership record. Keep existing collision avoidance for choosing names, but
   make final publication atomically refuse replacement. Windows uses
   `MoveFileExW` with flags zero (no replacement and no cross-volume copy).
   Non-Windows builds return an unsupported error, matching the existing native
   media authority boundary. Destination-exists is a nonretryable collision.
   Publish each validated MP4 before its optional JPEG; JPEG failure leaves the
   final MP4 intact and the staged JPEG for flat cleanup.
   Never remove final files on publication failure. An error can leave a valid
   completed subset and the normal compulsory refresh makes it visible.
5. Normal library refresh attempts reconciliation in its owned blocking scan
   before calculating cards/storage. It uses the captured approved root and
   checks cancellation at boundaries. `refresh_library` explicitly passes a
   `ReadOnly` recovery mode when `state.benchmark.is_some()`; this mode performs
   no recovery filesystem access or lease/staging creation. The same mode is
   passed into export: explicit benchmark exports may lease/create/clean their
   own new batch, but skip reconciliation of earlier batches. Normal mode is
   captured into the scan closure. Existing coordinator admission/invalidation stays intact.
   Export also reconciles under its newly acquired lease before creating a batch,
   so a library used without a preceding refresh remains safe. Recovery removes
   valid abandoned staging, preserves all final files and is idempotent. A live
   orphan media child can prevent deletion on Windows; retain failed staging and
   retry on a later refresh rather than disturbing completed/user data.

## Rejected alternatives and planning decisions

- Reject all-or-nothing rollback: deleting published names after restart cannot
  safely distinguish a replaced user file. A valid subset is already supported
  by filesystem authority and the optional JPEG contract.
- Reject roll-forward/re-probing: this adds media runtime work and more durable
  phases without improving source safety. Unpublished output is disposable.
- Reject PID liveness, age thresholds and broad `.part` glob deletion: these do
  not prove ownership or exclude active work.
- Reject recursive cleanup and trusting arbitrary manifest paths. Bounded flat
  ownership limits the effect of malformed or tampered state.

## Milestones

1. Implement the lease, guarded directory/manifest ownership, no-replace move and
   flat recovery. Unit-test live lease, strict ownership, collision and repeated
   recovery using generated temporary files.
2. Integrate batch staging into export and normal snapshot refresh, carry the
   captured approved root, preserve benchmark isolation and existing admission.
   Replace the unsafe rollback test with preservation tests. Check ordinary
   success, failure and waiter cancellation with the packaged media fixture.
3. Add deterministic test-only process boundary gates and a self-executing test
   child. Force termination after encode/thumbnail tool spawn, before the
   read-only output probe, and after each
   publication move in a three-preset batch. Restart recovery and inspect the
   actual resulting library, source/preexisting hashes and staging leftovers.
4. Update durable architecture and verification instructions; run required
   regression checks and review the destructive boundary before completion.

## Verification design

Use only `tempfile` libraries with a dedicated sentinel. Capture source and
preexisting MP4/JPEG SHA-256 before each scenario. A child test process invokes
the actual exporter with the packaged FFmpeg runtime and reports a test-only
gate via a file inside its fixture. The parent waits with a deadline, verifies a
live lease blocks recovery, forcibly kills the child, then invokes recovery.
Before exporting, the child assigns itself to a Windows Job Object with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; the only job handle stays in that process.
Its FFmpeg descendants therefore die when the parent kills it. Test-only gates
after process spawn publish the tool PID and confirm it has not exited; the
parent retains a process handle and waits for termination before recovery.
The probe gate is immediately before invocation: this is the same staged disk
state as death inside the read-only ffprobe and avoids changing the shared probe
runner solely for testing. Publication gates follow each MP4/JPEG move exactly.
Separately prove cleanup errors retain staging rather than deleting unrelated data.

At encoding, pre-validation, thumbnail and each video/JPEG move boundary,
assert that only already validated/published MP4 members survive, those members
decode and satisfy frame/audio expectations, no partial MP4 appears in cards or
clip storage totals, all owned abandoned removable staging disappears, sources
and preexisting files remain byte-identical, and a second recovery is a no-op.
Race a preexisting final MP4/JPEG into a chosen name and prove no overwrite or
rollback deletion. Malformed/oversized/unsupported manifests, foreign entries,
path traversal and Windows junctions must remain untouched. Include a captured
root mismatch and root switching check; benchmark config/library sentinels remain
unchanged.

Commands: focused `cargo test --manifest-path app/src-tauri/Cargo.toml
export_staging`; opt-in packaged export/crash tests with
`QUEUEBACK_TEST_MEDIA_RUNTIME` pointing at the staged validated runtime;
`cargo test --manifest-path app/src-tauri/Cargo.toml`; Rust formatting and clippy
for normal and `replay-benchmark` builds; frontend type/build if IPC changes;
canonical roadmap/linked-artifact validator and `git diff --check`.

## Performance and reliability gates

No recovery FFmpeg processes, source media reads, recursive traversal, or
unbounded manifest allocation. At most three members and an 8 KiB manifest per
batch. Recovery streams the staging directory's names and selects at most 64
batch IDs into bounded memory. A single process-local cursor (root plus last ID)
rotates lexicographically through retained entries, wrapping on a subsequent
refresh, so malformed/locked batches cannot permanently starve later batches.
The name enumeration is O(number of entries); manifest/file work is capped at 64
batches per call. A bounded report contains recovered/retained/deferred counts,
active-owner status and at most four 320-character diagnostics, emitted once to
stderr per attempt; no public IPC shape changes. A read-only refresh test with
library sentinels proves no lease creation or cleanup; a benchmark-mode explicit
export fixture proves existing staged batches survive its otherwise normal
export operation. A lease acquisition never
queues. No publication/cleanup happens while holding coordinator publication or
MediaRoots locks. Source files and every preexisting final path are preserved
under ordinary errors, collisions, termination and repeated recovery. Final moves
must fail rather than degrade to a non-atomic overwrite/copy on unsupported
filesystems. Hardware encoder selection is not changed by this feature.
