# Recorder review checked against the current repository

Date: 2026-10-29. Inspected revision: `20f9fa952ed9aef6aeb37d4fab8f68728fecc000`.
Input: the user-supplied `chronobreak-recorder-review-updated.md`, whose code references
are pinned to `ff00d99d4243a5effe7e658b01136b94798528fe` in Chronobreak_PCA.

This is a dated source-review artifact. `feature-list.json` remains the lifecycle
authority, execution checkpoints own implementation progress, and approved ExecPlans
remain unchanged. The supplied document's recommendations were treated as review
claims to check, not as authorization to implement every recommendation.

## Assessment

The main observations still hold. The review does not establish a new P0 defect or
prove that Chronobreak caused the reported Thresh loading-screen failure. It correctly
distinguishes missing assurance from demonstrated memory unsafety and preserves the
known in-process driver-hang limitation.

Two qualifications matter: the app can display an interrupted publication as an
incomplete entry, and Windows audio supports an explicit DirectShow device override
in addition to its narrow automatic discovery. Neither qualification supplies the
missing recovery workflow or guarantees recorded gameplay audio.

| Finding | Current conclusion | Canonical disposition |
| --- | --- | --- |
| L1: launch safety | No independent game-ready gate before native graphics startup. Historical incident causality is unknown. | New draft `QB-CAP-006`; choose the readiness policy before implementation. |
| N1: partial WGC construction | Explicit cleanup differs from successful-owner teardown. An unsafe outcome is unproven. | New draft `QB-CAP-007`; establish the WinRT contract and failure-injection design. |
| N2: uninterruptible driver call | Existing documented architectural limitation. Cooperative cancellation cannot contain every driver hang. | Retain the accepted boundary in `QB-PERF-002` and Windows capture architecture. Process isolation needs a separate evidence-backed decision. |
| M1: finalized-media proof | Dimensions/profile are not independently enforced; aggregate facts do not prove every sample's CFR timing. | Existing `QB-CAP-004`; corrected its stale ffprobe note and refined the missing verification contract. |
| M2: publication crash window | Separate video/metadata renames have no recorder-media startup reconciler. The app can surface an incomplete entry. | Existing `QB-CAP-003`; include publication crash states, identity, non-overwrite behavior, and idempotence. |
| D1: Windows audio | Automatic discovery is narrow and can select silence. An explicit device override also exists. | Existing draft `QB-AUDIO-001`; audio remains deferred. |
| V1: League performance proof | The required target-hardware capped/uncapped comparison is still outstanding. | `QB-PERF-002` remains blocked; no performance-completion claim. |
| A1: runtime documentation | Confirmed and corrected: `media-runtime.md` described a selectable external capture backend that current production does not provide. | This review aligns the documentation with the sole native recorder and distinguishes historical compiled capabilities. The remaining `QB-PERF-002` completion gates are unchanged. |
| A2: native ABI assurance | Substantial Rust assertions and pinned C probes already exist. No active automated C-probe/Rust comparison was found. No ABI defect was demonstrated. | Preserve the current assertions and compare against the exact pinned headers on SDK changes. Automating the companion probes is a bounded tooling improvement, not grounds for cosmetic module decomposition. |

## Source evidence and constraints

### L1: readiness after capture is different from readiness before capture

`watcher.rs:53-76` polls the League process every two seconds.
`platform/windows.rs:82-110,306-360` selects the largest visible non-minimized window
for that PID with client bounds of at least 320x200. `service.rs:760-780` can then
immediately spawn recording startup. `service.rs:983-1007` creates a bundle directory
before starting `NativeRecordingSession`.

`native/lifecycle.rs:164-307` initializes the worker and then waits for source/QPC,
encode, and mux progress. This proves that capture began, not that League was ready
before graphics initialization. `service.rs:1183-1205` starts Live Client polling only
after native capture is ready; it does not supply a pre-start gate.

The existing 2/5/10/30-second backoff, terminal suppression, cancellation handling,
and prevention of implicit segments after an active failure are real safeguards
(`service.rs:156-244,703-811,857-900`). They are not a busy loop, but they can still
retry native initialization during the same League loading phase. Existing tests
cover these policies, not positive game readiness or absent/passive/capture launch
comparisons.

There is no incident log or controlled reproduction in this pass. Three directories
do not prove three successful recordings. The fixed 1x1 WGC **frame-content** case
is not evidence explaining this startup incident.

### N1 and N2: preserve callback and driver ownership

`NativeWgcCapture::start` registers FrameArrived at `native/capture.rs:478-506`,
Closed at `:508-518`, and calls StartCapture at `:520-522`, before returning `Self`
at `:524-539`. Construction errors therefore do not execute the explicit successful
owner's shutdown at `:703-775`. That shutdown revokes registrations, waits for
activity, drains frames, closes the pool before the session, and checks handler-owner
release. `native/source.rs:117-135` retains the source stack when quiescence fails.
No injected registration/StartCapture failure coverage was found.

Microsoft documents that a free-threaded pool delivers callbacks on its internal
worker thread; the Rustonomicon explains why asynchronous callbacks need a lifetime
guarantee. These references support requiring a proof, but do not themselves prove
that this project's ordinary COM destruction is unsafe. The missing partial-start
guarantee is an inference from the differing code paths.
[Microsoft WGC reference](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframepool.createfreethreaded?view=winrt-28000),
[Rustonomicon asynchronous callbacks](https://doc.rust-lang.org/stable/nomicon/ffi.html#asynchronous-callbacks).

`native/lifecycle.rs:63-90` joins the native worker cooperatively, including a
blocking Drop fallback. Startup cancellation/errors also wait for that cleanup.
Bounded encoder-completion cancellation and retained resources do not terminate a
GPU/D3D driver call that never returns. This is already an explicit architecture
boundary. Do not detach unsafe work or free resources still referenced by FFI.

### M1: strong producer intent, incomplete independent validation

The native plan is H.264 High 1920x1080 at 60 FPS, and NVENC explicitly selects High
(`service.rs:1306`, `native/encode.rs:553`). The finalizer independently checks codecs,
stream cardinality, aggregate rate, start PTS, duration, frame count and producer
reconciliation. Fragmented sample counts come from bounded container-header reads
(`finalizer.rs:115-175`), not a guessed duration-times-FPS value.

However, `replay-time/src/lib.rs:965-989` requests no pixel dimensions and has no
dimensions/profile expectations. Profile is copied into the timeline at `:1300`
without High enforcement. The parser sets `exact_cfr: true` at `:1309` without
inspecting every sample's presentation grid. Existing tests do not cover wrong
dimensions/profile or adversarial per-sample timing with correct aggregates.
`QB-CAP-004` owns this remaining work; the completed `QB-REPLAY-012` contract is not
silently reopened or relabeled incomplete.

### M2: incomplete publication is visible but unreconciled

`finalizer.rs:297-386` creates and syncs pending metadata, renames the video to
`video.mp4`, then renames metadata to `metadata.json`. An ordinary second-rename
error attempts video rollback; process death cannot execute that rollback. There
is no recorder startup handler for the resulting video/pending-metadata pair.

`app/src-tauri/src/library.rs:395-416,595-605` requires canonical metadata for
playback and can surface the directory as incomplete. Optional post-game recovery
and clip-export recovery operate on different artifacts and do not repair it.

Fragmented MP4 can preserve decodable completed fragments after interruption.
That container property does not reconcile the application's separate sidecar
publication; this distinction is an inference from the recorder's two-file protocol.
[FFmpeg fragmentation documentation](https://www.ffmpeg.org/ffmpeg-formats.html#Fragmentation).

### D1, V1, A1, and A2

- `encoder.rs:443-491,1115-1130,1275-1285` supports
  `LEAGUE_REPLAY_AUDIO_DEVICE`, otherwise recognizes Stereo Mix or names containing
  both WASAPI and loopback in DirectShow listings, then falls back to silent stereo.
  An AAC stream is not proof of audible system audio.
- The `QB-PERF-002` checkpoint explicitly retains the target Ryzen 5 5600X/RTX 4060
  matrix as missing. Its current design requires one capped pair and one uncapped
  pair, not the historical eight-run `QB-PERF-001` protocol. No current finalized
  `docs/performance/results/qb-perf-002-nvenc` report was present.
- At the inspected revision, `docs/architecture/media-runtime.md` Consumer behavior
  described an explicit FFmpeg backend and said both Windows backends use the
  runtime, contradicting `windows-capture.md` and production startup. This review
  corrects those paragraphs and distinguishes historical compiled runtime capability
  from supported recorder paths.
- `native/nvenc.rs:500-518` and `native/encode.rs:1770-1852` assert ABI sizes,
  offsets, versions and function-table positions. Companion C probes are retained
  under `reference/nv-codec-headers-n12.2.72.0/`. Preserve this pinned assurance;
  an automatic C-probe comparison was not found in the active checks.

Paths under `recorder/src/` are shortened in this section unless another root is shown.

## Verification actually performed

| Command/check | Result on 2026-10-29 |
| --- | --- |
| `cargo test --manifest-path recorder/Cargo.toml --locked --all-targets --all-features` | Passed: 196 library, 4 binary, and 3 fixture tests; 9 opt-in tests ignored. |
| `cargo test --manifest-path replay-time/Cargo.toml --locked` | Passed: 14 tests; doc tests also passed. |
| Canonical schema, dependency/lifecycle, and linked-artifact checks | Passed: 104 roadmap items and 22 plan/checkpoint pairs. |
| `git diff --check` | Passed after preserving the runtime document's LF line endings. |

Raw successful test logs are retained under
`build/verification/recorder-review-20261029/recorder-tests.log` and
`replay-time-tests.log`. Initial restricted test invocations did not start because
the required logger could not write its `.codex/logs` folder; the reported passes
come from the subsequent successful normal-local-access invocations.

No League launch experiment, GPU failure injection, hardware fixture, full project
quality run, or formal performance matrix was run in this review. Passing
deterministic tests does not close those gates. No product code, user media, or
immutable ExecPlan was changed, and no capture feature was marked done.

## Recommended next unit

Refine `QB-CAP-006` into a testable readiness policy and an approved plan before
changing startup behavior. Keep historical causality unknown. Then address recorder
publication recovery (`QB-CAP-003`), independent media validation (`QB-CAP-004`),
and the partial-WGC lifetime proof (`QB-CAP-007`) as separately planned units.
The existing formal performance and audio dispositions remain in force.
