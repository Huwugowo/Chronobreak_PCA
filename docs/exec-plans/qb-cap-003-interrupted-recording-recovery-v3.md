# QB-CAP-003 — Capture-first recovery correction v3

## Purpose

Preserve safe interrupted publication while removing historical recovery from
League detection and capture admission, and make bounded discovery advance across
orderly restarts. This supersedes v2 scheduling/discovery only; its receipt,
validation, no-replace publication and app compatibility design remains intact.

## Relevant planning-time architecture

`service::run_inner` currently awaits recovery before constructing ProcessWatcher.
The loop owns cancellable startup, active recording and post-game coordination.
Its shutdown arm joins those owners before ShutdownComplete; the process owner
lease outlives service return. Recovery uses synchronous Windows filesystem calls
and joined five-second probes. Its ten-second budget limits admission, not OS-call
completion. An idle-loop pass would still delay detection of a newly launched game.
`inspect` pins full ancestry before checking canonical metadata. Fresh read_dir
passes can indefinitely revisit the first 512 entries or first four invalid probes.
Windows filename searches are unsorted and have no portable persistent seek token.

## Scope and non-goals

Only recovery scheduling, discovery progress, completed-entry fast path and focused
tests. No pipeline, audio, timeline, framework, thread, timer or background service
change. Real League performance and interactive playback remain pending and do not
block this correction's implementation/commit.

## Exploration findings

A joined post-teardown pass removes all recovery filesystem/probe work from startup
and the running watcher loop. It delays orderly exit instead. It must be skipped
when a fresh process observation sees League, stop further probe admission when
League appears, retain ownership and join admitted work before completion.
A memory-only cursor cannot advance across process restarts; a growing skip or full
sorted scan would violate the discovery budget. Kernel filename-prefix searches
permit bounded subdivision without assuming directory order.

## Chosen design and rationale

- Remove startup recovery. After shutdown joins startup/capture/post-game owners,
  run one best-effort maintenance pass before ShutdownComplete. Fresh process checks
  gate the pass and each admitted probe. Never race/drop recovery to start capture.
- Check canonical metadata existence before ancestry pins. That check only skips
  work; absence still requires pinned ancestry, a repeated canonical check, all
  receipt/media/metadata validation and no-replace publication.
- Persist one small advisory prefix in a recognizable recorder-private cursor bound
  to the pinned games directory identity. Exclusive create for new state; validate
  fixed header/length/prefix and ordinary-file semantics before writing an existing
  cursor through its held write/delete-excluding handle. Malformed, copied, foreign
  or inaccessible cursor bytes are retained and progress unavailability is logged.
  Update only the fixed prefix slot: a lost/torn hint never authorizes media changes.
- Search prefix* with Windows enumeration, filtering actual long names. Inspect the
  exact prefix separately before its descendants. Completed regions advance to the
  next grammatical sibling; a budget-limited region descends deterministically to
  its first grammatical child. Empty regions are pruned, never recursively expanded.
  Canonical timestamp/collision grammar bounds prefix depth to 24. A finite stable
  library with writable valid cursor and repeated eligible passes is eventually
  covered, independently of directory order. New names behind the cursor wait for
  wrap. Cursor corruption or perpetual League presence has no completion guarantee.
- Retain caps of 512 discovered/direct entries, four probes, ten seconds admission;
  additionally cap prefix searches to 512. No recursive walk, retained runtime
  handles between service lifetimes, growing skip, decode, hashing or GPU activity.

## Rejected alternatives and planning decisions

Startup/idle-loop recovery inverts capture priority; concurrent recovery requires
cancellation/joins and still competes for I/O. Shutdown maintenance is simpler.
In-memory enumeration cursors fail restart fairness. Sorted whole-library discovery
and offset replay grow work. Native persistent enumeration indexes are not portable.
An advisory fixed prefix needs no database, queue or transaction framework.

## Milestones

1. Correct service ordering and completed-entry fast path.
2. Add bounded persistent prefix discovery with unchanged candidate protection.
3. Verify scheduling refusal, actual restart progress and retained conflicts using
   temporary generated libraries; rerun recorder baseline and packaged crash fixture.
   Update concise checkpoint/architecture/evidence and commit.

## Verification design

Focused ordinary tests cover a refused pass performing no library work, canonical
skip through a path whose pinning would fail, retained candidate reparse rejection,
progress beyond 512 entries/four invalid candidates across reopened cursor handles,
and malformed/foreign cursor preservation. Reuse the generated publication-crash
fixture for successful recovery/idempotence and full decode/seeks. Recorder locked
all-target/all-feature tests, fmt and strict Clippy; canonical validator and diff
whitespace check. App/frontend code is unaffected, so prior results remain valid.

## Performance and reliability gates

No recovery call during startup or steady-state watcher execution. Maintenance is
owned/joined, never concurrent with capture. No GPU initialization or decoding.
Measure generated pass counts, discovered entries, searches, probes and elapsed time
without claiming League FPS/GPU proof. Filesystem calls remain OS-blocking and may
extend orderly exit; a newly launched game can overlap an already admitted bounded
probe while the recorder is exiting. It cannot delay admission in a running service.

## Plan review

Challenged shutdown owner/availability ordering, disconnected command receivers,
unsorted enumeration, prefix-valued bundle names, empty-region pruning, foreign
long names/short aliases, bounded memory/calls, cursor generation and partial writes.
The cursor is scheduling evidence only, never publication authority. Strict source
protections remain unchanged. Do not claim complete recovery without eligible exits
or healthy progress storage; preserve source and unrecognized cursor bytes.
