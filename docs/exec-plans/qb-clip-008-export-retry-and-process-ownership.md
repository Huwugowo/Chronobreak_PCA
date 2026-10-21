# QB-CLIP-008: Export retries and owned process cleanup

## Purpose
Allow another export from the same editor after library refresh, and terminate a
stalled export with a useful error while retaining ownership of cleanup.

## Relevant planning-time architecture
`App.tsx` passes the editor's immutable library origin to `ClipExporterScreen`.
`LibraryController` owns snapshot refresh/admission. Backend `export_clip` holds
the existing coordinator completion guard. `clip_export.rs` stages then publishes
an atomic batch, with encoder fallback and bounded probe commands. Its streaming
encoder and thumbnail processes currently have unbounded waits/diagnostics.

## Scope and non-goals
Repair retry admission and process ownership. Preserve presets, media timing,
runtime discovery, source protection, existing fallback and atomic publication.
No recorder changes, new job service, cancellation UI, export redesign or benchmark
campaign. Reuse the relevant implementation from preserved snapshot `573b41b`;
adapt its descriptor call to the current API rather than restoring old consumers.

## Exploration findings
The snapshot contains refresh completion/rebinding and a local streaming process
runner. These mechanisms are absent from current main. Shared bounded-process
helpers buffer output and suit probes but do not stream encoding progress.

## Chosen design and rationale
Refresh returns a promise of a current origin or null on supersession/failure.
After each backend attempt, rebind only the same editor/source/root/navigation to
the refreshed origin. Block retry during refresh or when the source is stale;
re-read the descriptor on token changes and revalidate media/range.

Move backend completion and all export side effects into one owned blocking worker.
Waiter cancellation requests cancellation without releasing admission. The focused
local runner owns both pipes and child until reaped; retain at most 8 KiB per
progress record and 64 KiB of diagnostics. Enforce a duration-derived 5-minute to
6-hour encoding deadline, 120-second progress stall and 30-second thumbnail limit.
On interruption kill/reap, clean staged artifacts and return the interruption
without software fallback. Ordinary encoder failures retain existing fallback.

## Rejected alternatives and planning decisions
Do not transplant the entire snapshot or its benchmark tools. Do not merely retry
stale tokens or kill a child while releasing its mutation guard early. Keep sampled
UI restoration and library presentation in their existing feature units.

## Milestones
1. Integrate refresh completion and editor rebinding; exercise success/failure retry
   and changed-source rejection in existing frontend tests.
2. Integrate owned streaming process execution and cleanup. Exercise progress,
   malformed output, deadline/stall and cancellation with the snapshot's small
   self-hosted child fixture; run a real isolated preprod export.

## Verification design
Run affected Vitest tests then the frontend suite/typecheck/build; app Rust tests,
fmt and Clippy; build production Tauri through its CLI. Use the isolated preprod
library to export twice in the same editor, inspect resulting media/thumbnail and
verify source hashes. Retain concise evidence in the checkpoint and feature entry.

## Performance and reliability gates
Progress/diagnostics remain bounded; stalled/deadline/cancelled children are reaped
before worker admission releases. No partial batch is presented as complete and
source media is unchanged. No capture or replay benchmark claim is made.

## Plan review
The repair stays within current controller/export ownership. Source replacement,
refresh failure, caller cancellation, malformed/flooded pipes and ordinary encoder
fallback are covered. Existing snapshot code and focused fixtures avoid a new
framework; no persistence or acquisition contract changes are required.
