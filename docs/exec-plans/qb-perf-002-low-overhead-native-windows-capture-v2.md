# QB-PERF-002: Low-Overhead Native Windows Capture

This immutable ExecPlan supersedes
`docs/exec-plans/qb-perf-002-gpu-agnostic-windows-capture.md`. The original plan is
preserved as planning history. This revision removes the dual-backend production design
and defines the sole supported Windows recorder plus the remaining physical performance
proof.

## Purpose

Chronobreak must record League's exact window at 1920x1080 60 FPS without the
synchronous GDI acquisition, system-memory pixel conversion, and upload costs identified
by the pre-change diagnostic. The supported Windows production path is one in-process
native WGC/D3D11/NVENC H.264 High recorder. It must remain bounded, GPU-resident,
truthfully diagnosed, interruption-safe, and low-overhead on the target League system.

FFmpeg remains one supervised child for audio encoding and fragmented-MP4 muxing and
remains the packaged probe/export runtime. It does not capture, scale, or encode
production Windows video. The former FFmpeg-driven WGC recorder is historical comparison
provenance, not an alternative, fallback, or vendor-support path.

Success still requires a valid capped and uncapped League comparison on the specified
Ryzen 5 5600X/RTX 4060 system. No local fixture or historical A/B result substitutes for
that physical gate.

## Relevant planning-time architecture

- `recorder/src/service.rs` owns League process/HWND discovery, startup cancellation,
  active lifecycle, polling, stop, common finalization, metadata, and partial-output
  safety. On Windows it selects no capture backend; it starts the native session.
- `recorder/src/platform/windows.rs` owns exact top-level HWND/PID identity, physical
  bounds, temporary per-monitor-v2 DPI context, monitor selection, and DXGI adapter
  identity.
- `recorder/src/native/` owns free-threaded WGC acquisition, one bounded latest-frame
  handoff, D3D11 resize/BGRA-to-NV12 conversion, four direct NVENC slots, CFR
  admission, completion, diagnostics, and the FFmpeg audio/mux boundary.
- `recorder/src/finalizer.rs` validates private completed candidates before canonical
  video/metadata publication. A failed or inconsistent session never becomes success.
- `media-runtime/` and `tools/media_runtime/` own the exact paired r6 FFmpeg/ffprobe
  lock, resolution, staging, verification, source provenance, hashes, and no-PATH rule.
  The artifact contains historical capture filters/encoders but production recorder
  code does not invoke them.
- `tools/native_backend/` owns sentinel-generated exact-HWND source and recorder
  fixtures. There is no active external-recorder runner.
- `tools/capture_benchmark/` owns collector/analyzer schema, validity checks, the target
  League protocol, and unchanged capped frame/resource budgets.
- `QB-PERF-001` remains invalid-but-directional pre-change characterization.
  `QB-PERF-003` and `QB-PERF-004` are draft future native AMF/QSV implementation plus
  physical-validation work, not current product support.

## Scope and non-goals

In scope:

- exact League HWND capture through Windows Graphics Capture on Windows 10 version
  1903 or later;
- one native D3D11/NV12/direct-NVENC H.264 High 1080p60 production graph;
- same-adapter GPU-resident acquisition, conversion, and submission without per-frame
  full-surface host readback or CPU pixel conversion;
- explicitly finite source, handoff, conversion, encoder, completion, pipe, startup,
  and shutdown ownership;
- first-frame/QPC readiness, monotonic machine-readable flow diagnostics, stable target
  identity, deterministic lifecycle failure, and partial-output safety;
- packaged FFmpeg audio/mux/probe/export responsibilities without FFmpeg video capture;
- automatic match lifecycle, audio, metadata, Live Client polling, collision-safe bundle
  IDs, playback/export compatibility, and common finalization;
- sentinel native fixture/lifecycle/resource evidence and the required target League
  capped plus uncapped comparison;
- truthful support matrix and benchmark identity for the one implemented combination.

Out of scope:

- an FFmpeg-driven WGC recorder, backend selector, cross-backend retry, or compatibility
  fallback;
- HEVC, non-High recording profiles, AMD/AMF, Intel/QSV, software encoding, or
  cross-adapter transfer in the current supported product;
- rebuilding r6 only to remove unused compiled capabilities;
- League injection, graphics hooks inside League, input interception, anti-cheat-sensitive
  behavior, or automatic League-setting changes;
- GDI, Desktop Duplication, primary-display, cross-adapter, or hidden host-copy fallback;
- lowering resolution, FPS, bitrate, quality, or benchmark thresholds to manufacture a
  pass;
- general recording-health UI, segmentation/restart, interrupted-media repair, or
  destructive verification against real recordings.

## Exploration findings

- The accepted-invalid pre-change matrix implicated synchronous capture/conversion and
  transfer: its clean subset showed small average-FPS change but large 1%-low and tail
  frametime regressions, elevated system CPU, and a severe uncapped reduction without
  GPU-3D or encoder saturation. It chooses the optimization target but cannot pass the
  post-change gate.
- Exact-HWND WGC supplies D3D11-backed frames and QPC-domain `SystemRelativeTime` from
  a finite free-threaded frame pool. Desktop Duplication cannot satisfy the same target
  and privacy semantics.
- D3D11 resize and BGRA-to-NV12 conversion can keep complete frames GPU-resident even
  when bounded GPU-to-GPU copies are required. “GPU-resident” is intentionally narrower
  than a universal zero-copy claim.
- Native NVENC can consume same-adapter NV12 surfaces directly. Full-frame CPU staging,
  raw-frame pipes, and cross-adapter copies recreate the measured cost and are forbidden.
- FFmpeg video capture/encoding is not needed for the supported path. Its historical
  backend did not uniquely own audio, muxing, publication, or recovery policy, and its
  AMF/QSV combinations lacked physical production validation.
- An in-process GPU worker cannot forcibly terminate a driver call that never returns.
  Bounded cooperative cleanup and truthful diagnostics are the accepted current
  boundary; a duplicate capture backend is not retained solely for process kill
  containment.
- Local generated-window and long-run evidence can prove media/lifecycle/resource
  behavior on the current host. Only the specified controlled League matrix can prove
  the frame-pacing and negligible-impact claim.

## Chosen design and rationale

### 1. One production path and support claim

Windows initialization accepts the native backend ID
`native-wgc-d3d11-nvenc` internally and exposes no backend-selection environment
variable. Supported recording configuration is H.264 High, 1920x1080, 60 FPS on a
same-adapter NVIDIA NVENC device. Unsupported codec/profile/vendor/interoperability
fails before capture with actionable diagnostics; it is never remapped.

Support labels distinguish implemented, automated-tested, hardware-validated,
performance-validated, unsupported, and unvalidated. The current claim is only the
implemented native NVENC combination. An encoder adapter that has not been implemented
is unsupported, not merely hardware-unvalidated. Future native adapters must satisfy the
same contract before any label changes.

### 2. Exact target and adapter identity

One descriptor binds League PID, exact HWND, target generation, physical client bounds,
DPI context, selected output canvas, monitor identity, and DXGI adapter LUID. Monitor
selection uses largest physical client-area intersection. Thread DPI state is restored
after every temporary per-monitor-v2 query.

PID/HWND/generation remains validated throughout capture. Same-HWND size changes
recreate only bounded WGC resources while preserving the output canvas. HWND reuse, PID
replacement, target close, adapter movement, or invalid target identity terminates the
candidate; none falls back to a display or another adapter.

### 3. Bounded GPU-resident native graph

The graph:

1. creates an exact-HWND WGC capture item on the selected D3D11 adapter;
2. receives BGRA textures through a two-frame free-threaded pool;
3. coalesces surfaced work through one capacity-one latest-frame handoff;
4. performs GPU resize and BGRA-to-NV12 conversion into four owned slots;
5. submits the NV12 surface directly to native NVENC H.264;
6. completes encoded packets through one bounded completion owner;
7. writes accepted Annex-B packets to one FFmpeg child that captures/encodes audio and
   muxes fragmented MP4;
8. commits/reconciles CFR ticks only after successful encoder admission.

The WGC callback never waits for conversion, encoder, muxer, disk, or runtime work.
There is no steady-state full-frame readback, CPU pixel conversion, raw-video pipe,
unbounded queue, or synchronous game-facing wait. Every pool/depth and GPU copy is
explicit in diagnostics and the resource budget.

### 4. Timing, diagnostics, and readiness

The first accepted WGC `SystemRelativeTime` anchors recording and poller calibration.
Startup becomes ready only after a real source frame, encoded output, mux progress, and
positive output timestamp. Process survival is insufficient.

Diagnostics expose monotonic source surfaced/superseded, CFR discard/duplicate,
submitted/completed/muxed, recreation, current/high-water depth, QPC, progress,
writer/flush timing, and terminal-error facts. The terminal owned-flow equation accounts
for every accepted and in-flight unit. Compositor frames Windows never surfaces remain
outside the claim.

### 5. Lifecycle and media safety

Focus loss and ordinary occlusion retain exact-window capture. Minimize may pause WGC;
the progress watchdog pauses only while the target is hidden and resumes finite
monitoring after restore. Resize recreates bounded resources. Target close, target
replacement, device/encoder failure, mux loss, output stall, malformed terminal
accounting, or timeout yields explicit failed/partial state.

Normal stop ends acquisition, drains finite GPU/encoder work, flushes and reaps FFmpeg,
and reconciles terminal counters. The common finalizer probes a private candidate and
publishes canonical `video.mp4`/metadata only after coherent media and identity checks.
No failure can claim frozen/corrupt media as success. Startup retry follows the bounded
schedule only after complete cleanup; active-session failure suppresses runaway retry
for that process generation.

Live Client failure remains descriptive-data degradation and never refreshes video
health or stops otherwise healthy capture.

### 6. Runtime and application boundary

Resolve only the exact paired packaged r6 FFmpeg/ffprobe runtime. Production uses the
release resource location; the one development override must satisfy the same embedded
lock. PATH and single-executable overrides are rejected. Startup checks the pair needed
for AAC/mux/probe while native code verifies D3D11/NVENC requirements directly.

Metadata records backend/diagnostics identity, target and adapter facts, direct interop,
GPU stages, finite bounds, runtime identity, QPC/counter evidence, codec/profile/rate,
and finalization outcome. Existing automatic process lifecycle, collision-suffixed
bundle naming, app browsing, playback, save/delete, clip association, and asset routing
remain intact.

### 7. Benchmark and performance decision

Collector schema identifies native backend, runtime, target/capture/encoder adapters,
interop, bounds, diagnostics ABI, codec/profile/rate, process topology, and media. A
valid run proves exactly one recorder and one FFmpeg audio/mux child, focus, complete
telemetry, target identity, advancing counters, media integrity, and resource safety.

Run one controlled capped baseline/capture pair and one independent uncapped
baseline/capture pair on Ryzen 5 5600X, RTX 4060, 1920x1080. Preserve invalid/noisy
roots and rerun a fresh complete pair rather than selectively averaging. The uncapped
pair is diagnostic; all substantial findings still receive a canonical disposition.

## Rejected alternatives and planning decisions

- Retain the external FFmpeg WGC backend: rejected because it duplicates capture and
  lifecycle ownership without a unique validated product obligation.
- Keep a backend selector for diagnostics: rejected because an operator switch preserves
  a production-shaped unsupported path and splits reliability evidence.
- Retain external capture for AMD/Intel: rejected because unimplemented native vendor
  support must not be disguised by an unvalidated compatibility backend.
- Remove FFmpeg entirely: rejected because audio encoding, fragmented-MP4 muxing,
  probing, export, and fixture generation remain appropriate responsibilities.
- Rebuild the immutable runtime during this cutover: rejected because current hashes and
  source provenance must remain truthful; compiled unused capability is not a callable
  recorder path.
- Keep `gdigrab`, Desktop Duplication, primary-display, cross-adapter, or system-memory
  fallback: rejected because each violates target/privacy/performance claims.
- Optimize polling or only encoder presets: rejected because the pre-change evidence
  points to capture/conversion/transfer and polling is not on the frame hot path.
- Generalize the current claim beyond H.264 High NVENC: rejected until a native adapter
  and full physical evidence exist.
- Require a clean QB-PERF-001 rerun: waived by explicit product decision while retaining
  its INVALID label; no post-change gate is waived.
- Use three mandatory post-change repetitions: replaced by one capped and one uncapped
  pair. Any invalid or doubtful pair is rerun whole.

## Milestones

1. Establish exact target/DPI/adapter identity, the packaged media-runtime boundary,
   and application/bundle compatibility.
2. Implement the bounded native WGC/D3D11/NV12/direct-NVENC recorder, first-frame/QPC
   readiness, diagnostics, startup cleanup, and finite shutdown/finalization.
3. Remove the FFmpeg-driven WGC backend, selector, candidate retry, external fixture
   route, and support claims while retaining FFmpeg's audio/mux/probe/export uses.
4. Prove native steady, resize, minimize/restore, occlusion, target-close, encoder/mux
   failure, normal stop, interruption, changing media, exact accounting, and a
   1,800-second bounded resource soak on sentinel fixtures.
5. Run the valid target League capped and uncapped pairs, analyze every gate/finding,
   update the support matrix and durable documentation, and execute all applicable
   completion verification.

Unsupported aliases,
fallbacks, external-runner gates, and production claims are removed in the same cutover
rather than deprecated.

## Verification design

Automated checks:

```powershell
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo fmt --manifest-path recorder/Cargo.toml -- --check
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path media-runtime/Cargo.toml
cargo fmt --manifest-path media-runtime/Cargo.toml -- --check
cargo clippy --manifest-path media-runtime/Cargo.toml --all-targets -- -D warnings
python -m unittest discover -s tools/capture_benchmark/tests -v
npm run check --prefix app
npm run build --prefix app
```

Packaged/runtime/release proof runs:

```powershell
$env:QUEUEBACK_TEST_MEDIA_RUNTIME = (Resolve-Path 'build/media-runtime/windows-x86_64').Path
cargo test --manifest-path media-runtime/Cargo.toml --test packaged_runtime -- --ignored --nocapture
Remove-Item Env:QUEUEBACK_TEST_MEDIA_RUNTIME
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/test_prepare.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/prepare.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/verify.ps1 -RuntimeRoot build/media-runtime/windows-x86_64
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/smoke.ps1 -RuntimeRoot build/media-runtime/windows-x86_64
cargo build --manifest-path recorder/Cargo.toml
cargo build --release --manifest-path recorder/Cargo.toml
npm run desktop:build --prefix app
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/stage_release.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/verify.ps1 -RuntimeRoot build/media-runtime/windows-x86_64 -ReleaseRoot build/release/queueback
cargo run --manifest-path recorder/Cargo.toml -- --diagnose
```

The staged-runtime test, atomic preparation/corruption suite, sanitized-PATH generated
media smoke, optimized binaries, release staging, paired-runtime release check, and
hardware diagnostics must all bind the same runtime identity. Inspection proves that
the production Windows video path contains no FFmpeg capture/filter/encoder invocation
even though r6 retains historical compiled capability.

Native fixture verification:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_wgc_source_probe.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario resize -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario minimize_restore -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario occlusion -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario close_window -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -Interruption nvenc_failure -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -Interruption mux_failure -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -Interruption pre_first_fragment -DurationSeconds 2
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -DurationSeconds 1800 -CollectResources -KeepTargetVisible -ResourceSampleSeconds 5
```

All outputs remain under sentinel-owned ignored roots. Milestone 4 extends the runner
`Interruption` set with `mux_failure` and `pre_first_fragment` and adds the
feature-gated probe argument `--fail-mux-after-writes <N>`. The native session kills
and reaps its owned FFmpeg child after writer call 120 for `mux_failure`, which must
leave a probeable partial fragment and no canonical success. It fires after writer call
one for `pre_first_fragment`; absent or unprobeable bytes are preserved but never called
recoverable or canonical. Normal cases require H.264/AAC, full decode, changing frame
hashes, exact 60-FPS accounting, finite ownership, and clean finalization. Every
failure case requires its exact terminal error, no canonical success, and finite reaping
of the fixture, native worker/completion owner, pipes, and FFmpeg.

Formal League comparison:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture_benchmark/run_qb_perf_002.ps1
python tools/capture_benchmark/analyze.py --input build/perf/qb-perf-002 --output-json docs/performance/results/qb-perf-002-nvenc.json --output-markdown docs/performance/results/qb-perf-002-nvenc.md
```

The environment must match the required Ryzen 5 5600X/RTX 4060/1920x1080 protocol.
Every run must pass warmup, focus, telemetry, identity, process-topology, diagnostics,
resource, finalization, and media validity. Final checks include the canonical roadmap
schema/invariant/linked-artifact command, current architecture/operator docs, all
applicable project-wide commands, and `git diff --check`.

## Performance and reliability gates

The valid capped pair must satisfy:

- average-FPS loss no more than 2%;
- 1%-low FPS loss no more than 5%;
- p95 frametime increase no more than 5%;
- p99 frametime increase no more than 8%;
- additional non-displayed/dropped-frame rate no more than 0.1 percentage point;
- CPU-at-or-above-90% sample-proportion increase no more than 5 percentage points;
- GPU-3D-at-or-above-95% sample-proportion increase no more than 5 percentage points;
- no sustained recorder/FFmpeg memory growth, output stall, process disappearance,
  media failure, or encoder/poller/finalization lifecycle failure.

Median system and League CPU deltas should remain below the protocol's two-percentage-
point substantial-finding threshold. Every larger capped or uncapped result requires an
evidence-backed disposition that does not contradict negligible game impact.

The graph and diagnostics must agree that steady state has no full-frame host readback
or CPU pixel conversion and that all depths remain finite. A decodable but frozen file
fails. Focus/occlusion, minimize/restore, resize/DPI, target close, device/encoder/mux
failure, normal stop, forced interruption, and poller degradation require explicit
bounded outcomes. User recordings and libraries remain untouched.
