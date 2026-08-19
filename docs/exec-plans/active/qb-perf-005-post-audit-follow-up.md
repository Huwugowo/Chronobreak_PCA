# QB-PERF-005 post-audit follow-up

Status: active; Packages 9 and 10 are complete and the Package 11 gate is next.

Date: 2026-08-19

Baseline: `21ffa82 Complete native audit follow-up`

## Authority and purpose

This is the successor to
`qb-perf-005-recorder-review-remediation.md`, whose implementation is complete
through Package 8. It addresses only the remaining concrete review risks:

1. make native mux/output backpressure measurable without changing media;
2. prove that cooperative native lifecycle paths never synchronously join a
   live GPU worker on a Tokio worker;
3. replace the one-millisecond full-ring retry only if normal recording
   evidence proves that it is material; and
4. remove the redundant steady-state Live Client `eventdata` request.

This plan does not authorize League execution on PC B, backend removal,
runtime replacement, active-session restart, timestamp-aware muxing, retention,
or a broad unsafe/FFI refactor.

## Preserved invariants

- WGC pool capacity remains 2.
- Callback-to-worker handoff remains capacity 1 and nonblocking.
- Worker-owned pending source capacity remains 1.
- NV12/NVENC slot count and maximum in flight remain 4.
- The WGC callback never waits for worker, encoder, mux, disk, or runtime work.
- D3D11 immediate-context ownership remains on one GPU worker.
- The completion thread remains bounded and performs no D3D11 immediate-context
  work.
- Tick commit follows successful NVENC submission; catch-up remains bounded to
  two submissions per scheduler pass.
- No full-frame CPU readback, software conversion, automatic fallback, or
  hidden cross-adapter copy is introduced.
- Both recorder backends and the developer selector remain available.
- Only the pinned r6 runtime may be used for comparative evidence.
- Existing recordings and evidence roots are never overwritten or deleted.

## Package order and ownership

Each implemented package is a separate reviewed commit. Run focused tests
before the broad static suite. A conditional package that does not cross its
gate is closed with evidence and no production change.

| Package | Scope | Reasoning owner |
| --- | --- | --- |
| 9 | Native mux/output observability and deterministic stall seam | Sol |
| 10 | Cooperative native lifecycle/drop audit and remediation | Sol |
| 11 | Event-driven full-ring waiting, only if Package 9 triggers it | Sol |
| 12 | Remove redundant steady-state `eventdata` request | Terra/Luna after the Sol checkpoint |

## Package 9: Native output backpressure observability

### Implementation

Measure the concrete completion-thread boundary rather than inferring pipe
backpressure from aggregate process CPU.

Extend native mux telemetry with final-session counters for:

- video-writer calls;
- cumulative and maximum writer-call duration;
- writer calls lasting at least one exact 60-Hz frame interval;
- explicit flush calls;
- cumulative and maximum explicit-flush duration; and
- deterministic injected mux-writer stalls.

The writer owns its timing accumulators locally. Publish them to the existing
shared mux telemetry when the writer is dropped after the completion thread
finishes. Do not add atomic read-modify-write operations to every encoded frame
merely to expose live experimental telemetry. Published telemetry atomics are
statistics only and use relaxed ordering; they must never coordinate resource
ownership or shutdown.

Under `native-failure-injection`, add a one-shot writer-stall seam selectable
by write index and duration. The stall runs on the existing completion/output
thread and must not alter queue capacities or WGC callback behavior. Expose it
through `native_mp4_probe` with paired arguments and reject partial/invalid
configuration.

Keep pipe-write timing distinct from explicit flush timing. A buffered write
may itself flush internally; its full call duration remains writer duration.

### Focused tests

- local timing aggregation publishes exact call counts, totals, maxima and
  slow-call counts;
- one-shot stall configuration validates positive index and duration bounded
  to five seconds;
- the stall fires exactly once and is accounted truthfully;
- existing early-mux-exit and monotonic-progress tests retain their behavior;
- fixture output includes every new field without changing existing field
  names.

### Exit criteria

- normal recording still reconciles scheduled, submitted, completed and muxed
  frames exactly;
- output remains nominal 60-FPS H.264/AAC and fully decodes;
- timing telemetry is zero before publication and complete in terminal
  evidence;
- production queue capacities, FFmpeg arguments and media metadata do not
  change;
- an unstalled native arm and 500-ms, 2-second and 5-second injected writer
  stalls terminate within existing deadlines and preserve truthful partial or
  canonical output according to the existing lifecycle contract;
- formatting, focused tests, all-target check, strict Clippy, available full
  tests, optimized recorder/probe builds and `git diff --check` pass.

Package 9 adds evidence only. It does not replace the one-millisecond retry.

## Package 10: Cooperative native lifecycle and Drop audit

### Question

Can any normal startup, cancellation, active stop, target-close, or injected
failure path reach `NativeWorker::drop` while its thread handle is still live,
thereby joining synchronously on a Tokio worker?

### Implementation method

Enumerate every `NativeWorker` ownership transition. Add a narrowly scoped
test observer that distinguishes explicit `stop_and_join` from a live-handle
Drop fallback. Exercise:

- successful startup and stop;
- startup cancellation;
- first-frame timeout;
- protocol/anchor failure;
- worker exit before readiness;
- target close;
- injected NVENC/mux failure; and
- service startup-cancellation timeout behavior.

If a cooperative path reaches the fallback, restructure ownership so that it
consumes the worker through `stop_and_join`, which keeps the blocking join on
Tokio's blocking pool. Preserve the fail-safe Drop join for invariant-breaking
unwinds unless a stronger safe owner is proven.

Do not claim that an in-process change can terminate a GPU/driver call that
never returns. If the only remaining fallback requires hard containment of a
hung FFI call, close that case as the existing supervised-process design gate;
do not detach a live thread, free resources it may still reference, retry into
duplicate native ownership, or report shutdown complete.

### Exit criteria

- every cooperative lifecycle fixture records explicit async cleanup and zero
  live-handle Drop fallbacks;
- cancellation does not synchronously join on a Tokio worker;
- no worker, completion thread, poller or mux child survives cooperative
  cleanup;
- retryable startup still retries only after cleanup;
- partial output and error-chain behavior are preserved;
- the hard-driver-hang boundary is stated truthfully and not represented as
  solved; and
- focused lifecycle/service tests and the broad Package 9 static gates pass.

If the audit proves the fallback unreachable for every cooperative path, keep
the production lifecycle unchanged and commit only the durable test seam and
evidence needed to prevent regression.

## Package 11: Conditional event-driven full-ring wait

Package 11 is not triggered by synthetic stall behavior alone. Injected stalls
prove containment; they do not prove normal recording spends material time in
backpressure.

### Trigger gate

Run at least four interleaved 60-second unstalled native arms with the pinned
r6 runtime. Implement Package 11 only if normal arms repeatedly show both:

- mux writer/flush calls exceeding one output interval or recurring full-ring
  admission failures; and
- attributable native-worker CPU/wakeup cost, media catch-up pressure, or stop
  latency outside normal run variance.

Any media/accounting/watchdog failure triggers investigation, but not an
automatic synchronization rewrite.

### Implementation constraints if triggered

Replace fixed one-millisecond full-ring polling with a bounded slot-availability
notification. The protocol must compose with source arrival, stop, target
closure and evidence publication without blocking the WGC callback. It must be
isolated into a small synchronization unit with an exhaustive interleaving
test where practical.

### Exit criteria if triggered

- no fixed one-millisecond full-ring polling remains;
- no slot-release or stop wakeup can be lost;
- stop and target-close remain finite;
- every resource-capacity and tick-commit invariant remains unchanged;
- normal-arm CPU/wakeup evidence improves materially;
- injected stalls retain truthful accounting and valid media or explicit
  partial-output failure; and
- all Package 10 lifecycle and broad static gates pass.

If the gate is not crossed, record `not triggered` and make no production
change.

## Package 12: Remove redundant steady-state event request

Split initial aggregate acquisition from steady snapshots. Initial acquisition
may fetch `eventdata` to seed the log. Once the dedicated one-second event loop
starts, steady snapshots fetch only game stats, active player and player list.

Exit criteria:

- initial events are preserved;
- steady snapshots issue no `eventdata` request;
- the event loop remains the sole steady-state event authority;
- event ID deduplication and chronological ordering remain exact;
- final game-log semantics and persistence durability do not change; and
- focused poller tests and the broad static suite pass.

Package 12 must stop and return to Sol if it exposes a lifecycle, concurrency,
durability or media interaction outside this local request split.

## Verification and evidence

Use `docs/VERIFICATION.md` and the pinned r6 runtime. Every runtime run writes
a new ignored timestamped evidence root. Never overwrite prior roots.

The available full test suite may continue to filter only
`focused_snapshot_parts_remain_parseable_from_empirical_capture`; record that
R11 remains absent. Environment/profile tests may remain explicitly ignored,
but no newly failing test may be hidden by another filter.

No PC-B result may claim League impact, formal M8 selection, runtime segment
support, hard driver-hang containment, or permission to remove a backend.

## Progress

- [x] Package 9: native output backpressure observability.
- [x] Package 10: cooperative lifecycle/drop audit.
- [ ] Package 11: normal-arm gate and conditional implementation.
- [ ] Sol checkpoint and evidence refresh.
- [ ] Package 12: redundant poller event request removal.
- [ ] Canonical full-repository reconciliation and external League gates.
