# Recorder remediation Package 6 evidence

Date: 2026-08-18

## Gate decision

Package 6 was conditional. An ignored, explicit disk-profile seam constructed
a synthetic 50-minute game with 300 ten-second snapshots, 100 event batches,
10 players per snapshot, items, private active-player statistics, and derived
changes. It used the same clone, pretty-JSON serialization, atomic temporary
file, `sync_all`, and rename path as the recorder.

The pre-change profile produced:

```text
writes=400
final_bytes=1,242,062
cumulative_bytes=248,612,898
rewrite_ratio=200.161x
p95_write=38.846 ms
slowest_write=61.335 ms
total_requested=7,906.757 ms
```

Both the p95-above-10-ms and cumulative-bytes-above-100x gates triggered, so
the package was implemented rather than left as speculative optimization.

## Writer contract

The event and snapshot loops no longer clone, serialize, or write the game log.
Each mutation increments a monotonic revision and performs a nonblocking watch
notification. A single writer task owns durable installation:

- the watch channel retains only the newest pending revision, so notifications
  cannot create an unbounded work queue;
- the first dirty revision opens a fixed 250-ms window; later revisions within
  that window update the target but do not extend the deadline;
- after the window, the writer locks state only long enough to verify and clone
  the newest revision, then releases the lock before serialization and I/O;
- one atomic pretty-JSON write installs that revision and advances the durable
  revision only on success;
- diagnostics distinguish mutation requests, installed writes, and coalesced
  writes while retaining clone/serialization/write/sync/rename timings;
- writer failure is latched in diagnostics, terminates the writer, and is
  returned by finalization or the next notification;
- dropping a poller aborts both its polling task and writer task.

Normal stop first cancels and joins the event/snapshot task, captures the newest
required revision, and sends a capacity-one final-flush command. That command
bypasses the coalescing delay and force-installs the newest state. It remains
inside the existing five-second finalization deadline; timeout aborts and reaps
the writer rather than detaching it.

## Correctness verification

Focused tests prove that:

- two concurrent revisions produce one installed write containing revision
  two, with one explicitly accounted coalesced write;
- final flush bypasses a modeled 60-second coalescing delay and the stored JSON
  contains the newest acknowledged value;
- an invalid output path latches one write failure, leaves durable revision
  zero, and returns the underlying temporary-file error;
- stopping an unresponsive poller remains bounded and still installs a final
  game log;
- dropping a poller cannot orphan its polling task;
- event reconciliation, calibration, snapshot diffing, stable-field omission,
  metadata, and stop behavior remain unchanged.

## Before/after synthetic profile

The coalesced profile modeled each event and its same-cycle snapshot as
concurrent notifications, then required the snapshot revision to become
durable before the next ten-second logical cycle. It retained the final forced
flush and produced the identical 1,242,062-byte final JSON.

| Metric | Direct rewrites | Single writer | Change |
| --- | ---: | ---: | ---: |
| Mutation requests | 400 | 400 | unchanged |
| Installed writes | 400 | 301 | -24.75% |
| Coalesced writes | 0 | 100 | explicitly accounted |
| Cumulative serialized bytes | 248,612,898 | 188,311,700 | -24.25% |
| Rewrite amplification | 200.161x | 151.612x | -24.25% |
| Clone time | 220.088 ms | 196.750 ms | -10.60% |
| Serialization time | 6,116.774 ms | 5,346.570 ms | -12.59% |
| Atomic-write time | 7,658.880 ms | 6,666.196 ms | -12.96% |
| Total requested-write service | 7,906.757 ms | 6,881.388 ms | -12.97% |

The after-profile cycle latency includes a 25-ms test coalescing window and is
not compared with the direct-write p95. Production uses the audited 250-ms
window. Exact byte/write reductions are deterministic; wall-clock timing is
supporting evidence from the same PC and build profile.

The 50-minute worst-case rewrite ratio remains above 100x because 300 periodic
snapshot revisions are intentionally still made durable. Reducing that further
would require append-only journaling, a changed JSON format, or a wider crash
loss/fsync policy, all explicitly outside this package. The kept change removes
only provably redundant concurrent writes and does not broaden crash-loss
exposure beyond 250 ms. A later real-match profile remains an external gate.

## Static verification

The package passed formatting, all-target/all-feature checking, strict Clippy,
all focused poller tests, and the full available suite: 113 library tests
passed, five environment/profile tests were ignored, only the known absent R11
empirical fixture was filtered, and the recorder binary test passed.
