# QB-PERF-002: GPU-Agnostic Low-Overhead Windows Capture

This immutable ExecPlan records the revised planning-time design approved during canonical repository reconciliation. `feature-list.json` owns the Definition of Done. Dependency plans and `docs/reconciliation/2026-08-24-canonical-repository-reconciliation.md` are provenance only; the implementation facts needed for normal execution are summarized here.

## Purpose

Chronobreak's pre-change Windows path captured a desktop rectangle through GDI, converted frames in system memory, and uploaded them to a hardware encoder. The accepted-invalid QB-PERF-001 diagnostic strongly implicated acquisition, conversion, transfer, and scheduling rather than the low-frequency Live Client poller or encoder saturation.

The approved design uses exact-HWND Windows Graphics Capture and D3D11 resources. The default path performs capture, resize, NV12 conversion, and NVIDIA encoding in process while retaining one packaged FFmpeg child for system audio and fragmented-MP4 muxing. The packaged external FFmpeg WGC/D3D11 path remains an explicit alternative and the available AMF/QSV implementation route. Shared target, lifecycle, capability, diagnostics, metadata, and benchmark contracts remain vendor-neutral; hardware claims remain specific to matrices actually run.

Success requires a valid post-change NVENC League matrix that passes the common frame-pacing and resource gates without weakening recording reliability. AMD and Intel remain explicitly unvalidated until their dedicated features run the same physical protocol.

## Relevant planning-time architecture

- `recorder/src/service.rs` owns automatic League discovery, target selection, backend startup, Live Client polling, stop/finalization, metadata publication, and partial-output safety.
- `recorder/src/platform/windows.rs` discovers the League process and exact top-level HWND, validates PID/HWND identity, observes physical client bounds, and maps the target to a monitor/DXGI adapter.
- `recorder/src/native/` contains the in-process Windows Graphics Capture, D3D11/NV12, direct NVENC H.264, bounded handoff, native CFR clock, and FFmpeg audio/mux integration.
- `recorder/src/encoder.rs` and `recorder/src/encoder/capabilities.rs` own the packaged external `gfxcapture`/D3D11 path and capability-selected NVENC/AMF/QSV planning.
- Both Windows backends retain exactly one FFmpeg child per recording. The native backend supplies encoded video to that child for audio/mux; the external backend keeps capture, conversion, encoding, audio, and mux inside it.
- The shared `media-runtime` crate and `tools/media_runtime/` own the exact QueueBack FFmpeg/ffprobe r6 lock, paired resolver, source build/staging, capability checks, release layout, and no-PATH rule.
- `recorder/src/poller.rs` performs isolated low-cadence Live Client requests. It is not on the capture-frame hot path.
- The application browses recorder-created bundle directories, including canonical Unix-second IDs with optional `-1` through `-999` collision suffixes. Capture work must preserve that contract and existing playback/export behavior.
- `tools/capture_benchmark/` owns the versioned collector/analyzer and the common capped frame/resource budgets. `tools/native_backend/` and `tools/capture_fixture/` own sentinel-generated native and external fixture paths.
- QB-PERF-001 is accepted only as invalid-but-directional pre-change characterization. QB-DIST-001 supplies the packaged runtime. QB-PERF-003 and QB-PERF-004 own physical AMD/AMF and Intel/QSV validation.

## Scope and non-goals

In scope:

- exact League HWND capture with Windows Graphics Capture on Windows 10 version 1903 or later;
- vendor-neutral target, adapter, capability, lifecycle, buffering, diagnostics, metadata, and support-label contracts;
- an in-process D3D11/NV12/direct-NVENC H.264 default path;
- the packaged external WGC/D3D11 NVENC/AMF/QSV alternative;
- same-adapter GPU-resident acquisition, resize/conversion, and encoder submission without steady-state full-frame host readback or CPU pixel conversion;
- bounded asynchronous ownership, first-frame readiness, deterministic failure/finalization, and machine-readable flow evidence;
- preservation of automatic recording, audio, fragmented MP4, codec/profile semantics, match association, metadata, polling, and recoverable partial output;
- deterministic capability/lifecycle fixtures, changing-media proof, a bounded resource soak, and the valid League performance matrix;
- benchmark schema/support-matrix changes needed to keep hardware claims precise.

Out of scope:

- League process injection, graphics hooks inside League, input interception, anti-cheat-sensitive behavior, or automatic League-setting changes;
- automatic GDI, primary-display, Desktop Duplication, cross-adapter, or cross-backend fallback;
- a vendor-specific capture core or marketing-name/PCI-vendor branch in common lifecycle logic;
- general recording-health state/UI, interrupted-recording repair, segmentation/restart, or strict library classification;
- arbitrary PATH media tools, runtime download/build during ordinary Cargo/npm/application work, or a second media-tool resolver;
- reducing resolution, frame rate, bitrate, codec quality, or League settings to manufacture a pass;
- claiming AMD/Intel or broad GPU performance from the available NVIDIA system;
- destructive verification against real recordings.

## Exploration findings

### Performance attribution

QB-PERF-001's complete matrix is formally invalid because of focus contamination and telemetry gaps, but its clean diagnostic subset is coherent enough to choose the optimization target. Capture changed average FPS by about 0.92% while worsening 1%-low FPS by 21.44%, p95/p99 frametimes by 28.80%/27.02%, and non-displayed presents by 0.565 percentage point. The uncapped diagnostic fell from 625.33 to 231.58 FPS. Median system CPU rose 19.91 percentage points while recorder CPU was effectively idle; FFmpeg used about 5.94% of total-machine CPU, roughly 529 MiB private memory, 13% GPU 3D, and 18.19% Video Encode. Neither GPU 3D nor the encoder was saturated.

This evidence does not pass a performance gate. It supports replacing synchronous GDI acquisition/system-memory conversion/upload rather than tuning polling or encoder presets. The approved post-change protocol therefore requires its own valid capped and uncapped pairs.

### Windows capture and encoder constraints

- Windows Graphics Capture can create an exact-HWND capture item and deliver D3D11-backed frames from a free-threaded bounded pool. `SystemRelativeTime` supplies the QPC-domain source timestamp.
- GPU-resident resize and BGRA-to-NV12 conversion may use bounded GPU copies; “GPU-resident” does not imply a universal zero-copy claim.
- Same-adapter NVENC and AMF can consume D3D11 resources. QSV must use a direct D3D11-derived mapping or fail closed rather than hide host transfer.
- Desktop Duplication captures an output, not an exact private window. It cannot satisfy the target/privacy semantics.
- The system FFmpeg 6.1 lacks required `gfxcapture`/`scale_d3d11` surfaces. Runtime identity and capability must be verified, not inferred from an executable name.
- Windows may show the operating-system capture indicator. The design accepts it and does not require restricted border-suppression capability.

### Reconciled topology

Repository reconciliation found two useful implementation lines rooted in the same recorder baseline: the whole application already contained the packaged external WGC/D3D11 path, while the recorder-development continuation contained an in-process WGC/D3D11/NVENC path with bounded ownership and failure handling. The approved design semantically integrates the native continuation into the whole-application chassis, preserves the Tauri/Solid app and distribution/benchmark systems, makes native NVIDIA capture the default, and retains the external route instead of replacing either side wholesale.

## Chosen design and rationale

### Backend policy and support claims

The unset/empty Windows selection uses backend ID `native-wgc-d3d11-nvenc`. `QUEUEBACK_WINDOWS_RECORDER_BACKEND=ffmpeg` and the `ffmpeg-wgc` alias select the external alternative deliberately. Startup failure never switches backend silently.

The native backend claim is intentionally narrow: exact-HWND WGC/D3D11/NV12/direct-NVENC H.264. The external path remains the current NVENC/AMF/QSV route. Capability presence, automated testing, hardware validation, and performance validation are separate labels. Missing AMD or Intel hardware means `unvalidated`, not unsupported, implemented-by-assumption, or exempt from the common budget.

### Exact target and adapter identity

Retain PID, exact HWND, physical client bounds, DPI context, selected output size, target generation, monitor identity, and DXGI adapter LUID in one target descriptor. Select the monitor by largest physical client-area intersection. Restore thread-scoped per-monitor-v2 DPI state after each query.

Validate PID/HWND/generation throughout recording. Same-HWND size/DPI changes may recreate bounded surfaces while preserving a stable 1920x1080 canvas. HWND reuse, PID replacement, target close, or cross-adapter movement is terminal for the current candidate; it never becomes primary-display or hidden cross-adapter capture.

Common capability planning uses stable backend IDs and explicit adapter identity. Marketing GPU names remain diagnostic text only. Same-adapter direct interop is required for an optimized claim.

### Native GPU-resident graph

The native path:

1. creates a free-threaded Windows Graphics Capture session for the exact HWND on the selected D3D11 adapter;
2. receives BGRA D3D11 frames into a finite source pool;
3. coalesces superseded source frames in one bounded latest-frame handoff;
4. performs crop/resize and BGRA-to-NV12 conversion on the GPU into a finite slot ring;
5. submits NV12 surfaces directly to NVENC H.264 with finite in-flight depth;
6. feeds encoded Annex-B video to one packaged FFmpeg child that owns system audio and fragmented-MP4 muxing;
7. commits the CFR tick only after encoder admission and preserves exact final accounting.

No steady-state full-frame readback, CPU pixel conversion, raw-frame pipe, unbounded queue, or synchronous game-facing wait is allowed. GPU copies and surface-pool sizes are explicit and included in diagnostics.

### External GPU-resident alternative

The external path uses the exact packaged runtime with `gfxcapture` against the selected HWND, D3D11 hardware frames, `scale_d3d11` GPU resize/NV12 conversion, a single explicit CFR authority, and capability-selected same-adapter NVENC/AMF/QSV encoding. Direct QSV mapping must fail closed when host transfer cannot be excluded.

This backend has its own fixture runner and support evidence. An external-path result cannot prove the native default, and a native-path result cannot erase external reliability findings. No retained GDI/manual mode contributes to optimized, GPU-agnostic, or negligible-impact claims.

### Runtime and packaging contract

Use the exact paired QueueBack FFmpeg/ffprobe r6 runtime supplied by QB-DIST-001. Production resolves `resources/media-runtime`; `QUEUEBACK_MEDIA_RUNTIME_DIR` is the only development override and must satisfy the same embedded lock. PATH and single-executable overrides are ignored.

Startup verifies exact runtime/build/file identity, required filters/hardware support, the selected encoder, and the applicable capture diagnostics ABI. Ordinary builds and tests do not download or rebuild FFmpeg. Source/build provenance, patches, hashes, licenses, and notices remain in the shared distribution contract.

### Bounded flow, timing, and diagnostics

All pools, latest-frame slots, handoff queues, encoder in-flight resources, child ownership, startup attempts, and shutdown waits are finite. Backpressure coalesces or drops recorder-owned source work under a documented latest-frame/CFR policy; it never grows memory or stalls League synchronously.

Machine-readable diagnostics keep distinct monotonic counters for source frames surfaced, source supersession, CFR discard, CFR duplication, encoder submission/completion, muxed frames, size/pool recreation, current/high-water depths, first/latest WGC QPC timestamps, progress bytes/time, and terminal errors. The final owned-flow equation identifies every in-flight remainder. Frames never surfaced by Windows are explicitly outside the claim.

The first accepted WGC `SystemRelativeTime` anchors video timing and the poller calibration. Startup is healthy only after the exact graph reports a real first frame plus advancing encoded/mux output; process survival alone is insufficient.

### Lifecycle and failure behavior

Preserve the automatic match lifecycle and Live Client independence. Focus loss and ordinary occlusion must retain changing capture on the supported fixture. Minimize/restore is an explicit backend-specific scenario; it may pause and resume only within finite bounded behavior and cannot be called successful while frozen.

Normal stop stops acquisition, drains bounded native/encoder work, flushes audio/mux, reaps every task/thread/child under finite deadlines, and publishes canonical success only from consistent terminal evidence. Target loss, device/encoder failure, output stall, malformed diagnostics, forced kill, or inconsistent counters yields an actionable failed/partial result while preserving recoverable fragmented media. There is no runaway retry or active-session segment restart.

Poller failure remains descriptive-data degradation and does not refresh video health. Video-path failure is never blamed on polling.

### Metadata, application, and benchmark contracts

Extend existing recording metadata rather than introduce another manifest. Record backend ID, diagnostics ABI, support label, capture/encoder adapter identity, interop path, runtime ID/hash, codec/profile/resolution/FPS, explicit copy/conversion stages, host-readback claim, configured bounds, final flow counters, clock source, and finalization outcome. Preserve application browsing/playback and canonical collision-suffixed bundle IDs.

Benchmark schema v2 records backend, runtime, adapter, interop, diagnostics, topology, selected encoder, support label, and per-run media identity. Historical schema-v1 evidence remains readable. The collector proves one recorder/one FFmpeg child, focus/telemetry/media validity, process identities, resource safety, and the selected graph. AMF/QSV parameters change identity expectations, never thresholds.

## Rejected alternatives and planning decisions

- Optimize Live Client polling: rejected because cadence and measured recorder idleness do not explain capture-correlated frame pacing.
- Tune only encoder presets: rejected because it leaves GDI acquisition, CPU conversion, and upload intact.
- Keep `gdigrab` as automatic fallback: rejected because it silently recreates the measured path and invalidates optimized claims.
- Use Desktop Duplication or primary-display retry: rejected because output capture does not provide exact-HWND privacy/identity semantics.
- Build per-vendor capture cores: rejected because target/lifecycle/buffering should remain common; vendor differences belong at the encoder boundary.
- Inject or hook League: rejected as unnecessary and anti-cheat-sensitive.
- Use raw CPU frame pipes or full-frame host staging: rejected because they recreate the performance problem.
- Accept arbitrary system FFmpeg: rejected because required filters, interop, and diagnostics vary by version/build.
- Hide automatic cross-adapter transfer: rejected until a separate measured direct-sharing design exists.
- Generalize the native claim beyond NVENC/H.264: rejected until that code and physical evidence exist.
- Require a clean QB-PERF-001 rerun: waived by explicit product decision while retaining its `INVALID` label; no post-change gate is waived.
- Use three mandatory post-change repetitions: replaced by one capped baseline/capture pair and one independent uncapped baseline/capture pair. A noisy or invalid pair is preserved and rerun whole, not averaged selectively.
- Remove the external backend after native integration: rejected because it is an explicit alternative and current AMD/Intel-capable path.

## Milestones

1. Integrate the exact r6 runtime/build/staging contract and preserve the whole-application chassis, recorder lifecycle, media library, and bundle-ID contracts.
2. Implement exact target/DPI/adapter identity plus deterministic capability and backend selection with no hidden display, GDI, cross-adapter, or cross-backend fallback.
3. Integrate the bounded in-process WGC/D3D11/NV12/direct-NVENC default and retain the packaged external WGC/D3D11 NVENC/AMF/QSV alternative.
4. Complete first-frame readiness, QPC timing, monotonic flow diagnostics, metadata, failure-safe finalization, startup cleanup, and finite shutdown ownership.
5. Add deterministic capability/flow/lifecycle tests and sentinel-generated native/external media fixtures covering steady state, resize, minimize/restore, occlusion, target close, encoder failure, normal stop, and forced interruption.
6. Generalize benchmark schema/tooling without weakening historical parsing, topology, media, focus, telemetry, resource, or performance gates; keep vendor support labels explicit.
7. Run the bounded native soak, disposition every alternative-backend reliability finding, execute the valid target League capped/uncapped comparison, update durable support/architecture material, and run all completion verification.

Each substantial milestone updates the separate execution checkpoint. This immutable plan contains no milestone status.

## Verification design

### Automated code and contract checks

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

Tests cover native bounded ownership and lifecycle; NVIDIA/AMD/Intel capability matrices; same-/cross-adapter selection; unsupported interop; flow accounting; surface loss/recreation; encoder and mux failure; shutdown ordering; fallback disclosure; metadata and bundle compatibility; benchmark schema v1/v2; sanitization; and exact gate boundaries.

### Packaged runtime and release proof

Run the real staged-runtime contract, atomic preparation/corruption checks, exact verifier, sanitized-PATH encode/probe/export/full-decode smoke, optimized application/recorder builds, portable release staging, and recorder diagnostics from `docs/development/VERIFICATION.md`. Inspect source/build provenance, required capture/conversion/encoder capabilities, diagnostics ABI, imports, hashes, licenses, and notices.

### Native and external fixture proof

Run the native source probe, the exact-HWND native fixture matrix, and the 1,800-second native resource soak using only sentinel-owned targets and output roots. Validate H.264/AAC structure, strict single-thread decode, changing frame hashes, exact 60-FPS counter reconciliation, bounded resource ownership, normal finalization, and decodable failed partials.

Run the external fixture separately for implemented encoders and transition cases. Reproduce and disposition minimize/restore, surface-pool, target-loss, and finalization behavior before broad external reliability claims. Never substitute one backend's result for the other.

### Formal League comparison

Use `tools/capture_benchmark/run_qb_perf_002.ps1` for exactly one controlled capped baseline/capture pair and one independent uncapped baseline/capture pair on the Ryzen 5 5600X/RTX 4060 1920x1080 NVENC target. Every run must pass warmup, focus, telemetry, process, media, identity, diagnostics, and resource-safety validity checks. Preserve invalid/noisy roots and rerun a fresh complete pair.

Generate the sanitized report with:

```powershell
python tools/capture_benchmark/analyze.py --input build/perf/qb-perf-002 --output-json docs/performance/results/qb-perf-002-nvenc.json --output-markdown docs/performance/results/qb-perf-002-nvenc.md
```

Inspect every substantial capped/uncapped frame, CPU, GPU, memory, flow, media, and lifecycle finding. The report records exact backend, capture/conversion path, adapter, encoder, codec/profile, resolution/FPS, runtime/build, binaries/config, all four runs, media validation, and limitations.

### Final checks

Run all applicable project-wide commands and the canonical feature-list schema/invariant/linked-artifact validation from `docs/development/VERIFICATION.md`, inspect the GPU/encoder support matrix and architecture/operator documentation, and run `git diff --check`.

## Performance and reliability gates

The valid capped NVENC pair must satisfy:

- average-FPS loss no more than 2%;
- 1%-low FPS loss no more than 5%;
- p95 frametime increase no more than 5%;
- p99 frametime increase no more than 8%;
- additional non-displayed/dropped-frame rate no more than 0.1 percentage point;
- CPU-at-or-above-90% sample-proportion increase no more than 5 percentage points;
- GPU-3D-at-or-above-95% sample-proportion increase no more than 5 percentage points;
- no sustained recorder/FFmpeg memory growth, output stall, process disappearance, media failure, or encoder/poller/finalization lifecycle failure.

Median system and League CPU deltas should remain below the protocol's 2-percentage-point substantial-finding threshold. Any larger result requires an evidence-backed disposition that does not contradict negligible game impact.

The runtime graph and diagnostics must agree that optimized steady state performs no full-frame host readback or CPU pixel conversion. Every pool/depth is finite and included in a texture/resource budget. Source supersession, CFR discard, CFR duplication, encoded/muxed output, recreation, and PresentMon non-displayed frames remain distinct.

A merely decodable but frozen file fails. Focus/occlusion, minimize/restore, resize/DPI, target close, device/encoder failure, normal stop, forced interruption, and poller failure require explicit bounded outcomes. Source recordings and user libraries remain untouched.

The uncapped pair is diagnostic rather than a capped completion gate, but every substantial result receives a canonical disposition. The same capped and resource-safety budgets govern every GPU/encoder combination later declared supported; unavailable hardware remains unvalidated, never exempt.
