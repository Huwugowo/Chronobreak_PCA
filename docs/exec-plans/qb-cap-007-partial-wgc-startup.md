# QB-CAP-007: partial WGC startup rollback

## Purpose

Make partial WGC startup fail without closing native capture objects under an
active frame callback, and retain resources only when callback release cannot
be established. League stability takes precedence over recovering this source.

## Relevant planning-time architecture

`native/source.rs` constructs the complete capture/device/thread-affine MTA owner
before `NativeWgcCapture::start`. Every returned startup error explicitly closes
that owner. `native/session.rs` drops partial dependents before closing the source;
`native/lifecycle.rs` joins its worker during cancellation and failure.

`native/capture.rs` owns two registration tokens, a capacity-one handoff, one
pending frame, callback activity and handler-owner counters. Each generated
`TypedEventHandler` captures an owned `FrameCallbackOwner` and Arcs, never a raw
source pointer. The SeqCst stopping/activity protocol rejects late frame callbacks
before accessing WGC. Source fields drop in capture/device/MTA order.

## Scope and non-goals

Correct startup rollback and shutdown proof/error classification, with small
operation seams for deterministic tests and a generated-window WGC fixture.
Preserve cancellation, worker joins, frame accounting and normal capture.
Do not change WGC capacity/sizing, CFR, conversion, NVENC, audio, timelines,
admission or publication. No hard driver-hang containment claim.

## Exploration findings

The original pre-owner registration concern was already corrected by QB-CAP-006.
Current shutdown records an idle timeout but continues to drain/Close before
returning failure; later full-source retention cannot undo those Close calls.
It also retains the full stack for any API error, even after proving all handlers
released. No use-after-free has been demonstrated.

Microsoft documents the [internal callback worker](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframepool.createfreethreaded)
and [Close resource disposal](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframepool.close),
but does not specify synchronous revocation, handler-release timing, atomic
registration failure or a required pool/session close order. The locked windows
0.62.2 bindings forward HRESULTs directly. Their COM delegate implementation owns
the closure until its final Release. Ordinary COM destruction is not itself
evidence of unsafety; explicit counters establish the stronger project policy.

## Chosen design and rationale

Keep the existing ownership model and stopping protocol. Attempt both token
revocations, clearing a token only on success. If the bounded idle proof fails,
return before draining frames or closing either WGC object and retain the full
source. Otherwise drain frames, attempt pool then session Close as before, and
wait for all handler owners to release. Mark shutdown complete only after that
proof. Retain the source on incomplete proof; propagate API errors without
retaining it when callback release is established. Report disposition accurately
and preserve the primary initialization error and cleanup error chain.

Factor only the three startup Result/token-commit operations and the bounded
idle/close/owner-release sequence into private closure-based functions. Keep real
handler construction shared with tests. These seams permit precise failure and
call-order assertions without mocking COM interfaces or adding runtime services.

## Rejected alternatives and planning decisions

- Requiring owner release before Close can unnecessarily retain handlers owned by
  the pool itself. Idle proof plus the stopping gate permits Close; owner release
  is required before source destruction.
- Retaining on every HRESULT failure confuses cleanup success with lifetime proof.
- Detached cleanup, retries, new workers, polling and proxy COM frameworks add
  cost or uncertainty without resolving this boundary.
- A generated fixture tests real registration/rollback, not an actual vendor
  failure HRESULT or a permanently blocked native call.

## Milestones

1. Correct shutdown ordering/classification and preserve error disposition.
2. Add a few deterministic startup-failure, real COM owner and shutdown-order
   tests; add one ignored generated-window WGC partial-start fixture.
3. Run focused tests and recorder checks; record limits and commit.

## Verification design

Startup tests fail each add/add/start operation, including an add that retains a
real COM delegate before returning error. Assert token commits, owner release and
post-stop rejection using the production handlers. Zero-timeout shutdown tests
assert no drain/Close on active callbacks, retention on unresolved owners and
drop-safe error propagation after release. An ignored WGC test resolves only a
generated `wgc_fixture` PID, injects boundary errors, and checks real handler
release/teardown without encoding or media writes. Run it once if available;
retain hardware verification as pending when unavailable.

Run recorder locked all-target/all-feature check/test, formatting and strict
Clippy from `docs/development/VERIFICATION.md`, plus canonical validation.
Retain bounded logs under ignored `build/verification/qb-cap-007`.

## Performance and reliability gates

No frame-path synchronization, GPU resources, copies, processes, retries or
startup work added to production. Preserve existing two-second proof waits and
their cooperative limitation. A failed idle proof performs less cleanup work.
Do not claim measured League FPS/GPU stability or native call interruption.
Keep the feature non-done until required real WGC evidence exists.
