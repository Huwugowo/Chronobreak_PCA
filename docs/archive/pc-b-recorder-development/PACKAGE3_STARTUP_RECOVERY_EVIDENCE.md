# Recorder remediation Package 3 evidence

Date: 2026-08-18

## Scope

Package 3 replaces the permanent `failed_process` latch with typed,
per-process startup recovery. It applies only before `ActiveRecording` is
published. A failure after recording becomes active still preserves the
partial recording and blocks an implicit second segment for the same PID.

The retry schedule is 2 seconds, 5 seconds, 10 seconds, 30 seconds, and 30
seconds thereafter. Deadlines use Tokio monotonic time so the complete
schedule and exact boundary behavior can be tested with paused time. State is
reset when the PID appears, changes, disappears, or a startup succeeds. The
service continues to permit at most one startup task or one active recording.

## Typed failure boundary

The service accepts only these startup dispositions:

- `Cancelled`: process/service state changed; cleanup completed and no alert
  or retry is scheduled by the finished attempt.
- `Retryable`: dynamic target/backend/poller readiness failed after cleanup;
  the same live PID is eligible at its next deadline.
- `Terminal`: configuration, diagnostics protocol, worker ownership, or
  timestamp/resource invariant failed; that PID is blocked until a process
  transition resets it.

FFmpeg and native lifecycle startup now classify at their backend boundary,
instead of requiring the service to match `anyhow` message strings. FFmpeg
target changes, process spawn/early exit, readiness timeout, and premature
pipe closure are retryable. Cancellation is explicit. Invalid argument
contracts, missing owned pipes, diagnostics protocol violations, and invalid
first-frame anchors are terminal. Every failure after process spawn kills and
reaps the child and drains or aborts both stream tasks before returning.

Native target invalidation and a source/encode/mux readiness timeout are
retryable. Cancellation stops and joins the worker. GPU-thread spawn,
immediate graph/protocol failure, premature worker/evidence termination, and
invalid first-frame anchors are terminal, with the worker stopped and joined
before the typed error crosses into the service.

Poller startup begins only after recording metadata invariants are validated.
If poller startup fails, the video session is explicitly stopped with failure
and its partial media is preserved before the attempt becomes retryable. A
published poller has a bounded normal stop path and a `Drop` abort safeguard,
so aborting a parent startup task cannot orphan the poller task. Startup
cancellation allows 20 seconds for the component cleanup bounds before its
last-resort task abort.

The first retryable failure emits one tray error that says an automatic retry
will occur. Subsequent retry alerts for the same PID are suppressed, while
structured logs and counters retain every failure and deadline. A later,
different terminal failure may emit one terminal alert.

## Partial-output and segment behavior

Each retry calls `create_game_directory`, whose collision-resistant suffixing
creates a new directory even when two attempts share the same Unix second.
Tests verify both directories remain present and distinct. No failed attempt
directory is deleted or reused.

Active-session restart is intentionally excluded. In particular, the known
two-screen cross-adapter case remains terminal once an active window's
selected monitor changes adapter. Retrying that active session would create a
second media/poller time origin and requires the deferred match/segment
contract.

## Verification

The following passed:

```text
cargo fmt --manifest-path recorder/Cargo.toml --all -- --check
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
git diff --check
```

The filtered static suite passed 106 library tests and one recorder-binary
test. Three intentional live tests remain ignored. The only filtered test is
the pre-existing R11 empirical-capture test because the PC-B fixture is the
known placeholder without `samples`; no fixture was fabricated.

Focused coverage proves:

- the exact paused-time 2/5/10/30/30-second sequence;
- no eligibility before a deadline and eligibility exactly at it;
- first-alert-only retry coalescing and repeated terminal-alert suppression;
- PID replacement/reset and cancellation reset behavior;
- active failures cannot create an implicit second segment;
- retry directories are unique and preserved;
- FFmpeg cancellation terminates its child before the readiness deadline;
- a diagnostics protocol violation is terminal and reaps its child;
- bounded poller stop aborts an unresponsive task; and
- dropping a poller session aborts its owned task instead of detaching it.

No League process was started and no user recording was used. A live
end-to-end service retry was therefore not forced on PC B; backend lifecycle,
paused-time policy, ownership, and cleanup paths are covered deterministically,
while real non-League WGC lifecycle remains part of Package 7.

## Decision

Keep Package 3. It converts transient pre-publication failure from a lost
match into bounded automatic recovery, without changing capture queues,
texture pools, media timestamps, codecs, mux arguments, or replay semantics.
Package 4 may proceed independently.
