# Package 10: Cooperative native lifecycle and Drop audit

Date: 2026-08-19

Scope: prove that cooperative native lifecycle paths consume their worker
through the async cleanup path, and prevent the service cancellation timeout
from aborting native ownership into a synchronous Tokio-worker join.

## Audit result

Every branch inside `NativeRecordingSession` already called
`NativeWorker::stop_and_join`. That method requests stop, takes the only worker
thread handle, and performs `JoinHandle::join` through `spawn_blocking`.
Successful startup transfers the still-live owner into
`NativeRecordingSession`; active stop and failed active stop also consume it
through `stop_and_join`.

One outer risk remained. After sending startup cancellation, the service
aborted any startup task that exceeded 20 seconds. For native startup, an
abort at a suspension point before ownership moved into `spawn_blocking` could
drop a live `NativeWorker` and synchronously join it on a Tokio control worker.

## Change

Startup attempts now carry a timeout action selected before their task is
spawned:

- FFmpeg retains the existing bounded task abort;
- native startup waits for the already requested cooperative cleanup after the
  warning deadline instead of aborting its ownership future.

Consequently, native startup cannot report cancellation complete, schedule a
retry, or emit service shutdown completion before native ownership is cleaned
up. This can wait indefinitely if a GPU/driver FFI call never returns. That is
intentional truthfulness, not hard-hang containment: solving that case still
requires a separately supervised process and an explicit resource/partial-
output contract.

The fail-safe synchronous `NativeWorker::drop` join remains for invariant-
breaking unwinds. No live thread is detached, no resource is freed while a
worker may reference it, and no second native owner is started after a timed-
out cancellation.

## Durable lifecycle tests

The startup wait loop is now a separately testable ownership boundary. A
test-only observer counts explicit joins and live-handle Drop fallbacks.
Fixtures cover:

- successful active stop and failed active stop;
- startup cancellation and first-frame/startup timeout;
- protocol and anchor failure;
- worker exit before readiness;
- target-close, injected NVENC-failure and injected mux-failure protocol
  classifications; and
- service cancellation timeout after cleanup takes longer than its warning
  deadline.

All cooperative fixtures record one explicit join and zero live-handle Drop
fallbacks. A separate observer self-test deliberately drops a short-lived
fixture worker and records one fallback, proving that the regression signal
itself is effective.

## Verification

The final broad suite passed 123 library tests and three binary tests. Five
explicit environment/profile tests remained ignored, and only the unavailable
R11 empirical fixture was filtered. Formatting, all-target/all-feature check,
strict Clippy, and `git diff --check` passed.

Fresh pinned-r6 hardware evidence is under:

`evidence/package10-native-lifecycle-20260819-124613`

Against a visible non-League Notepad HWND:

- real pre-start cancellation passed in 0.23 seconds;
- real start, five-second active run, stop, worker join, terminal evidence and
  canonical publication passed in 7.04 seconds;
- terminal evidence reported capture and mux completion, no protocol error,
  fixed WGC capacity 2 and pending-source capacity 1; and
- the canonical MP4 fully decoded in the ignored fixture, with SHA-256
  `F6016026148F885AC41BA2A1D47B4E2049F4F2BAE8BA492E36AC1D344A641E76`.

This is cooperative non-League lifecycle evidence. It does not establish
hard-driver-hang containment or authorize League/M8 claims.
