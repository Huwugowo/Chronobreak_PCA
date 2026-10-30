# QB-CAP-005 recording storage preflight

## Purpose

Refuse to start recording when destination storage is critically low, using an
actionable error and preserving every existing recording.

## Relevant planning-time architecture

`recorder/src/service.rs::start_recording` waits for game readiness and checks
cancellation/process identity before emitting Starting, allocating a bundle and
initializing native capture. `run_inner` already creates the `games` directory.
`storage.rs` owns bundle allocation. A terminal startup error uses the existing
ServiceEvent::Error/tray error tooltip and suppresses retries for that process.
The Windows dependency already exposes GetDiskFreeSpaceExW.

## Scope and non-goals

One Windows startup check; no cleanup, storage manager, active-recording polling,
new UI/configuration, media changes, or unrelated capture work.

## Exploration findings

Native H.264 is fixed at 1080p60, 12 Mb/s target and 18 Mb/s maximum
(`native/encode.rs`); AAC is 192 kb/s (`native/mux.rs`). One hour at maximum
combined rate is 8,186,400,000 bytes, below 8 GiB (8,589,934,592 bytes).
There is no bounded product match duration; a preflight cannot guarantee an entire
match fits or reserve space against other writers.

## Chosen design and rationale

Before Starting/bundle allocation, resolve the existing `<output>/games` directory
and query its available-to-caller bytes with GetDiskFreeSpaceExW. Query the full
resolved destination rather than extracting the configured drive letter so a
junction/mount targets its actual volume. Fail closed on resolution/query failure
with an action to check destination availability/access. Below 8 GiB, return a
terminal startup error that leads with the action to free 8 GiB or change the
recording folder, then includes the path and available capacity. At the threshold,
allow startup. Keep the small query/policy helper in storage.rs with a private
injected reader for deterministic tests; do not introduce a trait or manager.

## Rejected alternatives and planning decisions

Full-match reservation has no duration requirement to derive from. Checking the
system/configured root drive misses redirected games directories. Filling a real
disk, changing quotas, adding retry polling or invoking retention is unnecessary
and conflicts with this bounded feature. Existing error presentation is reused.

## Milestones

1. Add the storage query/policy, startup call and focused deterministic tests.
2. Run recorder baseline and canonical validation, document the threshold and
   concise evidence, then commit.

## Verification design

Focused tests inject low/boundary/sufficient available space, validate actionable
query errors, verify the reader receives the resolved destination, and preserve a
sentinel recording without allocating a new bundle. A real Windows query smoke
uses only a temporary directory. Inspect the existing terminal startup and tray
error route for presentation; no new UI or capture/performance campaign is needed.
Run `cargo check/test/clippy --manifest-path recorder/Cargo.toml --all-targets
--all-features` (Clippy with `-- -D warnings`) and `cargo fmt --manifest-path
recorder/Cargo.toml -- --check`, plus canonical validation and `git diff --check`.

## Performance and reliability gates

Exactly one resolution/query per admitted startup, before native initialization;
no active capture overhead or repeated low-space retries. Errors must not allocate
a recording bundle or delete/modify existing recordings. The query is a snapshot,
not a guarantee against later disk exhaustion. Hardware throughput/media tests are
outside this pre-capture-only change.
