# QB-PERF-002: GPU-Agnostic Low-Overhead Windows Capture

Status: implementation active. All canonical dependencies are complete. QB-PERF-001 remains explicitly accepted as invalid-but-diagnostic pre-change characterization, not as a formal performance pass.

This ExecPlan is the authoritative implementation guide for QB-PERF-002. It must be read together with feature-list.json, PLANS.md, docs/product/PRODUCT.md, docs/architecture/recorder-lifecycle.md, docs/development/VERIFICATION.md, and the completed outputs of every dependency. Implementation starts from a fresh context after the dependencies are complete. Product code must not be changed as part of the planning pass that created this file.

## Purpose

QueueBack currently captures a desktop rectangle through Windows GDI, converts frames in system memory, and uploads them to a hardware encoder. The first QB-PERF-001 dataset is formally invalid, but its clean diagnostic subset shows severe frame-pacing damage while the recorder process and Live Client poller are effectively idle. The work at stake is therefore capture acquisition, conversion, transfer, and scheduling rather than the low-frequency HTTP polling path.

This feature replaces the Windows video path with one vendor-neutral Windows Graphics Capture and Direct3D 11 pipeline. Full video frames remain GPU-resident through acquisition, resize, color conversion, and encoder submission. NVIDIA, AMD, and Intel differences are restricted to the final hardware-encoder interop adapter. The available RTX 4060 may validate the common path plus NVENC, but the architecture, tests, packaging, diagnostics, and support labels must also cover AMF and QSV without claiming hardware performance that was not measured.

Success means the valid post-change NVENC League matrix passes the common QB-PERF-001 frame and resource budgets, capture reliability is not weakened, and the same implementation contract is ready for separate physical AMD and Intel validation. It does not mean merely lowering average CPU usage, switching encoder presets, or making an NVIDIA-only fast path.

## Relevant current architecture

- recorder/src/service.rs resolves FFmpeg, chooses an encoder plan, and detects audio before League appears. It then watches for League every two seconds, discovers a capture target, starts one long-lived FFmpeg child, and starts the Live Client poller.
- recorder/src/platform/windows.rs finds the largest visible, non-minimized top-level window owned by the League process. It currently discards the HWND and returns only the corresponding desktop rectangle. It can fall back to the primary display.
- recorder/src/encoder.rs builds the Windows input as gdigrab over that desktop rectangle, uses a video packet queue of 1024, performs ordinary software scaling when needed, always requests yuv420p, and then selects NVENC, AMF, or QSV. Encoder selection is based on advertised names and a CPU-generated synthetic encode; it does not prove capture-device, adapter, or texture interop.
- RecordingSession owns a single FFmpeg child. Startup succeeds when the child survives roughly 900 ms. Health means process liveness. Stderr is drained to debug logs. Stop sends q, waits, and then kills on timeout while preserving fragmented MP4 output.
- recorder/src/poller.rs polls events about once per second and snapshots about once per ten seconds. The requests are asynchronous and isolated from video acquisition. Its clock is currently anchored to child-spawn time.
- metadata.json stores encoder, codec, profile, resolution, FPS, and League metadata after stop. The application tolerates additional deserializable metadata fields, but recording configuration rejects unknown fields. Do not add a new user configuration surface for automatic backend selection.
- QB-PERF-001 tooling assumes one recorder process with exactly one FFmpeg child and currently hard-codes the tested NVENC machine and schema-v1 telemetry roles. Keeping one child avoids a topology rewrite, but the collector and analyzer still need versioned backend/adapter fields.
- The development machine currently resolves FFmpeg 6.1. It exposes ddagrab and NVENC but not gfxcapture or scale_d3d11, so it cannot implement or prove this feature.

## Scope and non-goals

In scope:

- Exact League HWND discovery, window-to-output-adapter mapping, and stable adapter identity.
- A Windows Graphics Capture source backed by an explicitly selected D3D11 device.
- GPU-only crop/resize and BGRA-to-NV12 conversion for the optimized path.
- Direct D3D11 texture submission to NVENC and AMF, and a direct same-device D3D11-to-QSV mapping for QSV.
- Capability-driven deterministic planning, first-frame readiness, bounded buffering, capture/progress diagnostics, and explicit support labels.
- Preservation of audio, codec/profile policy, fragmented MP4, automatic match lifecycle, Live Client polling, and partial-file safety.
- A dedicated changing-window fixture, automated capability/command/diagnostic tests, a bounded soak, and the complete QB-PERF-001 League protocol.
- Versioning the benchmark manifest so future AMF and QSV validation can use the same gates without weakening schema-v1 evidence.

Non-goals:

- Injecting into League, hooking DirectX calls inside the game, intercepting input, modifying League settings, or depending on anti-cheat-sensitive APIs.
- In-process libav integration, raw-frame pipes, CPU frame conversion, or a second capture helper process unless implementation evidence disproves the selected external-FFmpeg design and the feature is explicitly re-planned.
- Treating Desktop Duplication as exact window capture, silently using GDI, or automatically capturing the whole primary display after an optimized-path failure.
- Claiming that all NVIDIA, AMD, or Intel hardware is performance-validated from one RTX 4060 result.
- Solving general recording-health UX, interrupted-recording repair, recording-state authority, strict library classification, or application-wide media-tool installation in parallel mechanisms. The rejected QB-CAP-002 implementation is explicitly outside this feature; QB-DIST-001 owns the packaged media-tool contract.
- Lowering 1920x1080, 60 FPS, bitrate, codec quality, or League settings to manufacture a benchmark pass.
- Destructive tests against real recordings.

## Exploration findings

### Performance attribution

The 2026-08-12 matrix cannot pass QB-PERF-001 because focus changes contaminated runs and three runs contain five-to-six-second telemetry gaps. It remains useful only as diagnostic evidence. In the clean capped subset, capture changed average FPS by 0.92% but worsened 1%-low FPS by 21.44%, p95/p99 frametimes by 28.80%/27.02%, and non-displayed presents by 0.565 percentage point. The uncapped diagnostic dropped from 625.33 to 231.58 FPS. Median system CPU rose 19.91 percentage points while recorder CPU was effectively zero; FFmpeg used about 5.94% of total-machine CPU, 529 MiB private memory, 13% GPU 3D, and 18.19% Video Encode. Neither 3D nor the encoder was saturated. This is consistent with synchronous desktop acquisition, copies, conversion, and scheduling disturbance. It is inconsistent with Live Client polling being the primary cause.

These numbers remain diagnostic rather than a formal gate. On 2026-08-12 the product owner explicitly accepted the complete single-monitor matrix as sufficient pre-change characterization and waived a clean QB-PERF-001 rerun. Preserve the report's INVALID label; the waiver does not change any post-change validity or performance requirement in this feature.

### API and runtime evidence

- Windows Graphics Capture can create a capture item for an HWND on Windows 10 version 1903 and later. A free-threaded frame pool delivers bounded D3D11-backed frames without requiring a UI dispatcher.
- Direct3D11CaptureFrame.SystemRelativeTime uses the QueryPerformanceCounter clock. That is the correct source timestamp for a video-to-Live-Client clock anchor.
- Desktop Duplication exposes monitor output frames through DXGI. It is valuable for monitor capture but cannot provide the exact-window privacy and occlusion semantics required for QueueBack's automatic League capture.
- The official FFmpeg 8.1.2 source includes gfxcapture and scale_d3d11. gfxcapture accepts an HWND and a caller-supplied D3D11 device, uses a free-threaded Windows Graphics Capture pool, and emits D3D11 hardware frames. scale_d3d11 performs GPU video-processor conversion and can emit NV12 textures suitable for encoders.
- FFmpeg 8.1.2 NVENC accepts and registers D3D11 resources; AMF creates surfaces from native D3D11 textures; QSV can derive a QSV device from D3D11 and directly map compatible textures. A direct QSV map must fail closed if it would copy through host memory.
- gfxcapture is absent from the locally resolved FFmpeg 6.1. The runtime must therefore be exact-version/capability checked rather than accepted because its executable is named ffmpeg.
- Windows Graphics Capture may display the OS capture indicator. QueueBack must accept that indicator when border suppression is unavailable and must not depend on restricted capabilities, user-hostile workarounds, or a packaged-only permission to hide it.

Primary implementation references, pinned here so a future context does not repeat broad research:

- https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture
- https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframepool.createfreethreaded
- https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframe.systemrelativetime
- https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow
- https://learn.microsoft.com/en-us/windows-hardware/drivers/display/desktop-duplication-api
- https://ffmpeg.org/download.html
- https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.1.2/libavfilter/vsrc_gfxcapture.c
- https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.1.2/libavfilter/vsrc_gfxcapture_winrt.cpp
- https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.1.2/libavfilter/vf_scale_d3d11.c
- https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.1.2/libavcodec/nvenc.c
- https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.1.2/libavcodec/amfenc.c
- https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.1.2/libavutil/hwcontext_qsv.c

## Chosen design

### Dependency and delivery gate

Do not start implementation until the three dependencies in feature-list.json are done. QB-CAP-001 and QB-PERF-001 are complete; only QB-DIST-001 remains:

1. QB-CAP-001 supplies the existing reliable recording baseline.
2. QB-PERF-001 supplies the completed, explicitly accepted-invalid single-monitor diagnostic matrix and unchanged common performance gates. It provides direction, not a formal pre-change pass.
3. QB-DIST-001 supplies the canonical installed FFmpeg/ffprobe discovery, version checking, packaging, notices, and no-PATH behavior. Reuse it; do not add a second media-tool resolver.

QB-CAP-002 is neither complete nor a dependency. Its experimental recording-state authority and strict client classification were rolled back after they made a valid Practice Tool bundle Unknown/non-playable. Preserve the stable release-era metadata/video library contract. Add only the bounded first-frame/output evidence needed to prove the new graph starts and changes; do not rebuild CAP-002 inside this feature.

At implementation start, read the completed code and evidence for these dependencies. If their contracts materially differ from this plan, update the Decision log and the plan before changing product code.

### Process topology and pinned media runtime

Retain one recorder process and one FFmpeg child per active recording. FFmpeg continues to own audio input, video filtering/encoding, muxing, fragmented MP4, and graceful flush. This preserves the proven lifecycle and the QB-PERF-001 process topology.

The optimized Windows backend requires an audited build pinned initially to FFmpeg 8.1.2. Runtime preflight must verify, at minimum, exact compatible version/build identity, gfxcapture, scale_d3d11, D3D11 hardware-device support, the selected encoder, ffprobe compatibility, and QueueBack's capture-diagnostics ABI described below. A random PATH FFmpeg that lacks any required capability is incompatible, not a reason to fall back to GDI.

The packaged build, configure flags, source URL/tag, source/archive hashes, patches, binary hashes, license classification, and notices belong to the QB-DIST-001 contract. Ordinary Cargo builds must not download or rebuild FFmpeg. Do not enable GPL/nonfree components casually; the actual distributed configuration and obligations require explicit inspection and documentation.

### Target descriptor and adapter identity

Replace the Windows rectangle-only capture target with a descriptor that retains:

- League PID and exact HWND;
- client bounds, DPI, and selected output dimensions;
- the monitor containing the largest client-area intersection;
- DXGI adapter LUID and output identity for that monitor;
- target generation so a stale/replaced HWND cannot be confused with the original target.

Capture and encoder selection occur only after this descriptor exists. On Windows, remove plan selection from the pre-League service startup path. Platform-neutral audio discovery may remain early.

Enumerate D3D11 adapters and outputs through DXGI, correlate the chosen monitor to its adapter, and pass an explicit adapter selector to FFmpeg. Never infer physical identity from encoder names, marketing names, PCI vendor IDs, or enumeration order. Record both capture and encoder adapter LUIDs. Stable test fakes use synthetic LUIDs.

The supported optimized production path requires capture, conversion, and encoding on one physical adapter. If the only compatible encoder is on another adapter, return cross-adapter-unvalidated or unsupported with a clear diagnostic. Do not allow Windows or FFmpeg to hide a cross-adapter copy. A future feature may validate an explicit GPU-to-GPU shared-handle path.

### Capability model and deterministic planner

Introduce separable data types for:

- CaptureCapability: OS build, WGC/Win32 interop, D3D11 device, exact-window support, indicator/border state, adapter LUID, supported input formats, and runtime ABI.
- EncoderCapability: backend ID, adapter LUID, codec/profile, accepted hardware-frame formats, direct-interoperability mode, driver/runtime availability, and bounded async-depth support.
- RecordingRequest: codec/profile, output size, target FPS, target descriptor, and audio source.
- CapturePlan: selected backend, D3D11 adapter, filter graph, encoder adapter, all bounded limits, expected path labels, and explicit unsupported reason.

Make the planner a pure deterministic function covered by matrix tests. Eligible candidates must match codec/profile, reside on the capture adapter, accept the produced D3D11/QSV hardware format without host transfer, expose a bounded submission depth, and satisfy the requested 1920x1080 60-FPS profile. Rank only by documented capabilities and stable backend IDs. Attempt each eligible candidate at most once at real startup; never cycle indefinitely.

An encoder advertised by FFmpeg is not proven. Startup becomes ready only after the exact production graph reports the first source frame and advancing encoded/mux progress. Failure advances to the next eligible same-adapter candidate within one bounded startup deadline. If none succeeds, preserve diagnostics and report unsupported/failed without GDI or full-display retry.

Use these externally visible support labels consistently in diagnostics, metadata, architecture documentation, and benchmark manifests:

- optimized-validated: implemented and the exact physical combination passed its complete hardware matrix;
- optimized-unvalidated: implemented and automated/integration tested where possible, but no complete physical performance matrix exists;
- cross-adapter-unvalidated: an observed combination that is deliberately not selected for production;
- legacy-gdi-manual: explicit developer/diagnostic mode only, excluded from performance and support claims;
- unsupported: a required capability is absent or direct bounded interop cannot be proved.

### GPU-resident frame graph

Build the Windows graph from these stages:

1. gfxcapture targets the exact League HWND on the explicit D3D11 device and limits acquisition to the requested maximum FPS.
2. The source emits D3D11 BGRA hardware frames. No hwdownload, software scale, CPU pixel-format conversion, raw-frame pipe, or ordinary yuv420p request may appear in the optimized graph.
3. GPU crop/resize/pad keeps the configured aspect and stable output canvas when the HWND size or DPI changes.
4. scale_d3d11 uses the D3D11 video processor to produce encoder-bindable NV12 for SDR H.264/HEVC.
5. NVENC and AMF consume the D3D11 NV12 texture directly. QSV derives a QSV device from the same D3D11 device and uses a direct hardware map; direct-map failure makes the candidate ineligible.
6. A 60-FPS CFR stage intentionally discards excess frames and duplicates only when the source supplies too few. CFR discards and duplicates are distinct counters.
7. Existing audio and fragmented-MP4 output behavior remain, with video/audio timestamps normalized to a stable zero epoch and tested for start skew and drift.

Call this GPU-resident or no-host-readback, not universally zero-copy: WGC, resize, format conversion, and encoder surfaces may require bounded GPU-to-GPU copies.

macOS keeps its current AVFoundation path. Windows versions before 10 version 1903 are unsupported by the optimized path. HDR capture is outside this feature unless required for the current League fixture; do not silently mislabel HDR input as validated SDR.

### Bounded flow, timing, and diagnostics

Define one internal CaptureLimits contract and include its values in diagnostics. Initial limits are:

- Windows Graphics Capture frame-pool capacity: 2;
- gfxcapture hardware output pool: the audited pinned-source capacity, expected to be 8;
- scale_d3d11 output pool: the audited pinned-source capacity, expected to be 10;
- FFmpeg global buffered filter frames: at most 32;
- encoder in-flight depth: 4 when the backend exposes an enforceable option; a backend without a proven finite limit is unsupported until bounded;
- FFmpeg children per recording: exactly 1;
- candidate startup attempts: each eligible same-adapter backend at most once within one documented total deadline;
- recovery loops after target/device/encoder failure: 0 unbounded loops.

Implementation must verify the expected upstream pool constants against the pinned source rather than copy these numbers blindly. Compute and log the negotiated maximum texture allocation from dimensions, formats, and pool counts. Do not carry the gdigrab video packet queue of 1024 into the optimized graph.

Use FFmpeg progress output at a one-second cadence and drain it continuously. Parse the minimal bounded evidence needed for this path: encoded frame count, output timestamp/size where provided, duplication, CFR discard, speed, and terminal progress. Keep this evidence session-local for startup, diagnostics, and benchmark proof; do not introduce recording-state files or new client classification.

Stock gfxcapture does not expose enough source-flow and first-QPC information for truthful diagnostics. Carry a minimal, version-pinned FFmpeg 8.1.2 patch with a versioned QueueBack capture-diagnostics ABI. It must:

- emit the first and most recent Windows Graphics Capture SystemRelativeTime values;
- report source frames surfaced, frames submitted downstream, source frames superseded by the latest-frame-wins policy, pool recreation/size-change count, current bounded depth, and terminal/device error;
- use aggregate startup/periodic/final messages, never per-frame logging;
- keep all counters monotonic and distinguish source supersession from CFR discard, CFR duplication, and encoded frames;
- expose an ABI version that runtime preflight rejects when absent or incompatible;
- include deterministic parser fixtures and a patch application/build verification in the distribution contract.

Do not claim to count compositor frames that Windows never surfaces. Document that boundary. For recorder-owned flow, final diagnostics must make the accounting relation and any in-flight remainder explicit.

Use the first WGC SystemRelativeTime as the video epoch. Sample QueryPerformanceCounter and wall time together to map it to recorded_at, and pass the same monotonic epoch to the Live Client poller. Do not use FFmpeg child-spawn time. Duration comes from final encoded/mux progress when available, with a clearly labeled fallback only for failed partial sessions.

### Lifecycle and failure behavior

Retain the release-era service/session lifecycle and fragmented-MP4 preservation behavior. The only lifecycle addition is a bounded startup-ready check proving the WGC source and encoded output have advanced before the service announces recording.

- Ordinary focus loss and occlusion must keep changing exact-window capture on the supported fixture. Alt-Tab is not a benchmark invalidation for the recorder, although League benchmark collection still requires controlled focus for comparable PresentMon data.
- Same-HWND size, DPI, and monitor-bound changes recreate bounded WGC resources and retain a stable output canvas. Count every recreation and update adapter state. A move to a different adapter is not silently cross-adapter; stop as failed/partial unless a separately proven same-adapter restart contract exists.
- Minimize or a source that stops producing frames is exercised and reported by the dedicated fixture. It may resume after restore within a finite fixture/startup window, but this feature does not add new library lifecycle states or claim general recording-health supervision.
- A closed or replaced HWND, device removal/reset, incompatible direct map, encoder exit, output stall, progress-parser failure, or mux failure ends the attempt deterministically and preserves the fragmented partial file. There is no primary-display retry and no runaway child restart.
- Normal shutdown stops new capture, drains bounded frames, asks FFmpeg to flush, waits under the existing finite timeout, and records terminal counters and finalization outcome. A forced kill is failed/partial, never finalized success.
- Run the Live Client poller independently after the first video epoch. Poller failure remains descriptive-data degradation and must not stop otherwise healthy video; video-path failure must not be blamed on polling.

Exact window-replacement recovery across multiple media segments is not introduced here. If product evidence requires seamless recovery, add a separate feature rather than appending unsafely to the active MP4.

### Metadata, compatibility, and documentation

Extend the existing recording metadata/details contract rather than create a second competing recording manifest. Use a versioned nested capture object with, at minimum:

- capture backend and diagnostics ABI;
- support label;
- capture and encoder adapter LUIDs plus non-authoritative display names for operators;
- encoder backend and interop mode;
- source/output format, resolution, FPS, codec, and profile;
- explicit GPU copy/conversion stages and host-readback false/true;
- FFmpeg version/build/binary hash and QueueBack revision;
- configured bounds and final capture/CFR/encoded/recreation counters;
- first-frame clock source and final bounded-flow counters.

Keep old recording metadata readable through serde defaults or a versioned compatibility reader. New application readers must not assume the nested object exists. Do not rewrite old recordings. Add only an explicitly named developer environment override for legacy-gdi-manual if a compatibility diagnostic is retained; do not alter the deny-unknown-fields user configuration schema.

Update docs/architecture/recorder-lifecycle.md and add docs/architecture/windows-capture.md for the durable frame graph, state transitions, support matrix, adapter rules, observability, and known WGC indicator/minimize limitations. Update recorder/README.md for runtime diagnostics. Update docs/performance/capture-benchmark.md for the generalized manifest and vendor validation protocol.

### Benchmark compatibility and vendor claims

Version the QB-PERF-001 collector/analyzer contract before using it for post-change proof. Schema v2 must record capture backend, diagnostics ABI, adapter LUID, interop path, FFmpeg build hash, expected encoder, and support label. Parameterize target hardware/encoder assertions so AMF and QSV follow-ups can use the identical budgets. Preserve schema-v1 parsing and existing fixtures; do not reinterpret the invalid 2026-08-12 data as valid.

The collector must still prove exactly one recorder and one FFmpeg child, retain all PresentMon and resource gates, reject telemetry gaps/focus contamination, and collect the new structured progress/capture diagnostics. The analyzer must distinguish League/recorder/FFmpeg CPU, GPU 3D, Video Encode, source supersession, CFR drop/duplication, and PresentMon non-displayed frames.

Only NVENC on the current Ryzen 5 5600X/RTX 4060 can become optimized-validated in this feature. AMF and QSV implementations remain optimized-unvalidated until QB-PERF-003 and QB-PERF-004 respectively pass the complete protocol on physical hardware. A failure on those systems does not erase GPU-agnostic architecture; it creates a measured backend-specific optimization feature and keeps that support label unvalidated.

### Rejected alternatives

- Live Client polling optimization: measured recorder idleness and low polling cadence do not explain the capture-correlated frame pacing and system CPU. Preserve and monitor it instead.
- Encoder-preset-only tuning: encoder utilization is not saturated and this leaves GDI acquisition, CPU conversion, and upload intact.
- gdigrab plus NVENC/AMF/QSV: changing only the encoder is not GPU-resident and is the current failing architecture.
- Desktop Duplication as the automatic primary path: it captures an output, not an exact League HWND, and changes privacy/occlusion semantics.
- Per-vendor acquisition APIs: duplicates lifecycle/buffering code and cannot provide the required vendor-neutral core.
- In-game hooks or injection: unnecessary, operationally risky, and contrary to the product's anti-cheat-safe boundary.
- Raw CPU frame pipes or host staging: explicitly recreates the performance problem.
- An unpinned system FFmpeg: required filters and diagnostics vary by version/build; the local 6.1 binary proves executable-name discovery is insufficient.
- Automatic cross-adapter encoding: may hide expensive copies and cannot carry a negligible-impact claim without separate evidence.
- Hiding the Windows capture indicator through restricted capabilities: not required for recording and not worth weakening deployment reliability.

## Milestones

### Milestone 0: Re-open after dependencies

- Confirm QB-CAP-001, QB-PERF-001, and QB-DIST-001 are done with evidence.
- Read their final architecture and verification artifacts.
- Confirm the accepted-invalid pre-change diagnostic report is preserved with its waiver and limitations, and the packaged FFmpeg contract can supply the pinned feature set.
- Confirm the CAP-002 rollback is intact: valid canonical metadata/video bundles retain release-era browsing and playback, and no recording-state authority or strict status UI has been reintroduced.
- Reconcile material differences in this plan before product edits.

Exit: dependency gate is satisfied and the Decision log is current.

### Milestone 1: Pin and prove the runtime contract

- Extend the QB-DIST-001 media-tool capability report with FFmpeg build/hash/filter/hwdevice/encoder/diagnostics-ABI checks.
- Store the minimal gfxcapture diagnostics patch, upstream provenance, hash, and build-verification fixture in the distribution-owned third-party layout established by QB-DIST-001.
- Add deterministic tests for compatible, missing-filter, missing-ABI, wrong-version, and partial-vendor runtime reports.
- Prove ordinary Cargo verification neither downloads nor rebuilds FFmpeg.

Exit: diagnostics can distinguish a compatible optimized runtime from the local FFmpeg 6.1 and from incomplete vendor builds.

### Milestone 2: Preserve HWND and map the physical adapter

Primary files: recorder/src/platform/mod.rs, recorder/src/platform/windows.rs, recorder/src/service.rs, and platform tests.

- Add the versioned target descriptor and stable HWND/monitor/adapter-LUID discovery.
- Implement largest-client-intersection monitor selection, DPI/negative-coordinate handling, stale-HWND checks, and deterministic fake mappings.
- Move Windows capture-plan selection until after target discovery.
- Remove automatic primary-display fallback from optimized production startup.

Exit: tests cover one adapter, iGPU+dGPU, negative-coordinate monitor, DPI/resize, missing output, stale HWND, and monitor move.

### Milestone 3: Add pure capabilities and command planning

Primary files: recorder/src/encoder.rs plus focused modules split from it when needed, such as recorder/src/encoder/capabilities.rs and recorder/src/encoder/windows_graph.rs.

- Implement the pure capability/request/plan types and deterministic same-adapter planner.
- Generate graph arguments for D3D11 gfxcapture, GPU scale/convert, NVENC direct, AMF direct, QSV direct map, bounded buffers, audio, and fragmented MP4.
- Keep macOS behavior unchanged and legacy GDI only behind the explicit developer mode.
- Add golden/snapshot argument tests that reject hwdownload, software scale, ordinary yuv420p conversion, a 1024-frame video queue, implicit adapter choice, and hidden GDI/display fallback.

Exit: NVIDIA, AMD, Intel, missing-encoder, unsupported-codec, direct-map-failure, multi-adapter, and cross-adapter cases resolve deterministically.

### Milestone 4: Prove startup readiness and bounded progress

Primary files: recorder/src/encoder.rs and recorder/src/service.rs. Touch poller/storage only if the first-frame clock or backward-compatible capture metadata actually requires it; do not add a health/state subsystem.

- Drain and parse FFmpeg progress plus QueueBack capture-diagnostics messages without blocking.
- Require first WGC frame and advancing output before declaring startup ready.
- Anchor recording wall/monotonic time and the poller to first-frame QPC.
- Enforce limits, candidate-attempt deadlines, terminal counter collection, and existing partial-file preservation.
- Fold backend/path/counters into the existing metadata contract with backward-compatible readers and no new client completion policy.

Exit: fake-child tests cover delayed first frame, malformed/missing ABI, no output progress, counter invariants, target stall/resume, resize, device loss, encoder error, normal flush, timeout/kill, poller independence, partial preservation, and unchanged legacy bundle visibility/playback.

### Milestone 5: Dedicated Windows media fixture and integration proof

- Add a QueueBack-owned fixture executable that creates a uniquely titled 1920x1080 HWND with deterministic continuously changing pixels, frame/time markers, resize controls, minimize/restore, and clean close. It must never target a user recording or unrelated window.
- Add an opt-in Windows integration runner using a temporary sentinel-marked recording library.
- Exercise available NVENC plus mocked/planner AMF and QSV paths; validate selected adapter/path, changing decoded frames, 60-FPS output, audio/video streams, timing, bounded counters, resize, focus/occlusion, minimize/restore, close, normal stop, and forced interruption.
- Run at least a 30-minute bounded-resource soak on the available optimized path. Record initial, peak, and final process memory, GPU memory where available, texture-budget diagnostics, output growth, and counter progress.

Exit: produced fixture media passes ffprobe, full decode, and decoded-frame-change checks; no bounded-resource or lifecycle invariant fails.

### Milestone 6: Generalize the benchmark without weakening it

Primary files: tools/capture_benchmark/collect.ps1, tools/capture_benchmark/analyze.py, tools/capture_benchmark/tests, and docs/performance/capture-benchmark.md.

- Add schema-v2 capture/runtime/adapter/interop/counter fields and target-encoder parameters.
- Preserve schema-v1 parsing/tests and one-recorder/one-FFmpeg topology checks.
- Reject incompatible backend, adapter mismatch, hidden legacy mode, missing diagnostics, telemetry gaps, focus contamination, non-changing media, and wrong encoder.
- Add AMF/QSV fixtures that prove parameterization changes identity checks, never the common budgets.

Exit: all old and new analyzer/collector tests pass, and a dry preflight identifies the available NVENC path truthfully.

### Milestone 7: Run the complete League proof and close state

- Run one fresh capped B1/C1 baseline/capture pair and one independent uncapped B1/C1 diagnostic pair with the user maintaining the documented benchmark focus and settings. If a pair is noisy, contradictory, or invalid, preserve it and rerun that complete pair in a fresh root; do not require or aggregate three repetitions by default.
- Finalize and media-validate every capture before analysis.
- Require a valid dataset and every capped frame/resource-safety gate to pass. Investigate every substantial CPU, frame, memory, source-drop, CFR, lifecycle, or uncapped finding.
- Update architecture/support documentation, feature-list evidence, and the NVENC support label. Keep AMF/QSV optimized-unvalidated and link QB-PERF-003/QB-PERF-004.
- Run every applicable project-wide command.

Exit: every QB-PERF-002 acceptance criterion has concrete evidence, no known issue contradicts negligible League impact or recording reliability, and the global completion gate passes.

## Verification

Run commands from the repository root unless a dependency's completed documentation supersedes a placeholder path.

Automated recorder checks:

    cargo test --manifest-path recorder/Cargo.toml
    cargo fmt --manifest-path recorder/Cargo.toml -- --check
    cargo clippy --manifest-path recorder/Cargo.toml --all-targets -- -D warnings

Application compatibility checks when metadata or media-tool readers are touched:

    cargo test --manifest-path app/src-tauri/Cargo.toml
    cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
    npm --prefix app run typecheck
    npm --prefix app run build

Benchmark-tool checks:

    python -m unittest discover -s tools/capture_benchmark/tests -v

Pinned runtime inspection must capture commands equivalent to:

    <packaged-ffmpeg> -hide_banner -version
    <packaged-ffmpeg> -hide_banner -filters
    <packaged-ffmpeg> -hide_banner -hwaccels
    <packaged-ffmpeg> -hide_banner -encoders

The implementation milestone must replace the following placeholders with checked-in runner commands and record them in docs/development/VERIFICATION.md:

- Apply/verify the pinned FFmpeg source patch and build/config manifest without downloading during ordinary verification.
- Run the dedicated changing-HWND Windows integration suite.
- Run the 30-minute bounded-resource soak.
- Validate normal and interrupted fixture output with ffprobe, full ffmpeg decode, and decoded-frame-change analysis.

Post-change League analysis:

    python tools/capture_benchmark/analyze.py --input build/perf/qb-perf-002 --output-json docs/performance/results/qb-perf-002-nvenc.json --output-markdown docs/performance/results/qb-perf-002-nvenc.md

Before completion, run every applicable command in docs/development/VERIFICATION.md, validate feature-list.json against feature-list.schema.json, validate feature IDs/dependencies/plan paths, inspect git diff for unrelated changes, and record exact command outcomes rather than claiming unrun checks.

## Performance and reliability requirements

The valid NVENC post-change capped matrix must satisfy all common QB-PERF-001 gates:

- average-FPS loss no more than 2%;
- 1%-low FPS loss no more than 5%;
- p95 frametime increase no more than 5%;
- p99 frametime increase no more than 8%;
- additional non-displayed/dropped-frame rate no more than 0.1 percentage point;
- CPU-at-or-above-90% sample-proportion increase no more than 5 percentage points;
- GPU-3D-at-or-above-95% sample-proportion increase no more than 5 percentage points;
- no sustained recorder/FFmpeg memory growth, output stall, process disappearance, media failure, or encoder/poller/finalization lifecycle failure.

In addition:

- median system and League CPU deltas should fall below the protocol's 2-percentage-point substantial-finding threshold. A larger delta needs an evidence-backed disposition and cannot contradict negligible game impact.
- The optimized graph must contain no full-frame host readback or CPU pixel conversion. Runtime diagnostics and command/source inspection must agree.
- All resource pools and async depths are finite, logged, tested at their boundaries, and included in the calculated texture budget.
- Source supersession, CFR discard, CFR duplication, encoded output, resize/recreation, and health stalls remain separate metrics. PresentMon drops are never substituted for recorder drop counters.
- A file that merely decodes but contains frozen repeated frames does not pass; decoded-frame-change checks and progress counters must prove changing content.
- Focus loss/ordinary occlusion, minimize/restore, resize/DPI, target close, device/encoder failure, normal stop, forced interruption, and poller failure have explicit tested outcomes.
- The exact same budgets apply to later AMF and QSV validation. Vendor-specific thresholds are prohibited.

## Progress

- [x] Attribute the diagnostic performance issue to the current capture path, with the invalid-dataset caveat preserved.
- [x] Map recorder, platform, poller, metadata, packaging, and benchmark assumptions.
- [x] Research and compare WGC, Desktop Duplication, FFmpeg D3D11 capture/conversion, NVENC, AMF, and QSV interop.
- [x] Challenge the design for multi-adapter behavior, buffering, clocks, lifecycle, packaging, support claims, and verification gaps.
- [x] Select the external-FFmpeg WGC/D3D11 architecture and write this ExecPlan.
- [x] Roll back the rejected QB-CAP-002 implementation and remove it from this feature's dependency gate; QB-CAP-001 and QB-PERF-001 remain complete.
- [x] Complete the QB-DIST-001 packaged-runtime dependency gate.
- [ ] Implement Milestones 1 through 6 from a fresh context.
- [ ] Run Milestone 7 and satisfy the global completion gate.

## Deviations and surprises

- The first QB-PERF-001 dataset looked directionally decisive but is invalid for formal comparison because of focus contamination and telemetry gaps. Planning uses it only for bottleneck direction.
- The product owner later accepted that complete single-monitor dataset as sufficient pre-change diagnostic characterization and waived a clean QB-PERF-001 rerun. Its INVALID label remains, and QB-PERF-002 still requires a valid post-change matrix.
- The product owner explicitly replaced the post-change three-capped-pair protocol with one capped baseline/capture pair plus one uncapped baseline/capture pair. All per-run validity, media, resource-safety, and capped performance gates remain unchanged; uncertainty is handled by a fresh pair rerun rather than mandatory median aggregation.
- The locally installed FFmpeg 6.1 cannot exercise the chosen graph. Official FFmpeg 8.1.2, released after that local build, contains gfxcapture and scale_d3d11; the feature therefore depends on a pinned packaged runtime rather than developer PATH state.
- Keeping video inside one FFmpeg child avoids a custom Rust texture IPC or libav integration, but stock gfxcapture lacks the first-frame QPC and bounded-flow counters required for reliable clocking and observability. The chosen response is a small audited patch with a strict ABI, not an in-process capture rewrite.
- GPU-agnostic implementation and GPU-wide measured performance are different claims. Automated capability coverage can be complete on the current machine, while physical AMF/QSV performance remains explicitly unvalidated.
- The attempted QB-CAP-002 implementation regressed real completed-bundle visibility and did not improve capture performance. It was rolled back. PERF-002 starts from the stable release-era app/recorder behavior and owns no recording-state/library-policy redesign.

## Decision log

- 2026-08-12: Live Client polling is not selected for optimization. Its cadence and measured recorder idleness make it implausible as the primary capture-correlated bottleneck.
- 2026-08-12: Select Windows Graphics Capture for exact HWND acquisition and D3D11 hardware frames. Retain Desktop Duplication only as a separately characterized non-production alternative, not an automatic fallback.
- 2026-08-12: Preserve one external FFmpeg child for capture, audio, conversion, encoding, and fragmented MP4. This minimizes lifecycle and benchmark-topology change.
- 2026-08-12: Pin the initial optimized runtime to audited FFmpeg 8.1.2 and require a QueueBack capture-diagnostics ABI. Do not accept arbitrary PATH builds.
- 2026-08-12: Require same-adapter D3D11 capture/conversion/encoding. Cross-adapter paths remain explicit and unvalidated instead of hiding GPU/host transfers.
- 2026-08-12: Use one vendor-neutral D3D11 core; isolate NVENC, AMF, and QSV at the encoder interop boundary. Core decisions do not inspect GPU vendor IDs.
- 2026-08-12: Accept the Windows capture indicator when border suppression is unavailable. Do not rely on restricted capability or game hooks.
- 2026-08-12: Retain GDI only as an explicit developer/legacy diagnostic mode, never as an automatic optimized fallback or support evidence.
- 2026-08-12: Hardware performance claims remain per measured adapter/backend. Create QB-PERF-003 and QB-PERF-004 for physical AMF and QSV validation under the same gates.
- 2026-08-12: Accept the completed QB-PERF-001 diagnostic as the pre-change direction by explicit product decision. Preserve its INVALID label and require QB-PERF-002's post-change matrix to satisfy all original validity rules and budgets.
- 2026-08-12: Remove QB-CAP-002 from the dependency gate and preserve the stable release-era bundle contract. Use only minimal session-local first-frame/output evidence for the new graph; do not reintroduce the rolled-back recording-state or strict client-classification machinery.
- 2026-08-12: By explicit product requirement, schema-v2 performance proof uses four runs total: one capped baseline/capture pair and one uncapped baseline/capture pair. Preserve schema-v1 historical parsing, remove schema-v2 three-run variability gating, and rerun a doubtful pair instead of aggregating mandatory repetitions.

## Completion

The planning pass is complete when this plan is linked from QB-PERF-002, the dependencies and discovered vendor-validation work are canonical in feature-list.json, schema/invariant validation passes, and no product code has changed.

The feature is active and not done. It becomes done only after implementation completes all milestones, every acceptance criterion has evidence, the valid NVENC matrix passes, applicable project verification passes, durable architecture/support documentation is current, no known issue contradicts reliability or negligible League impact, and feature-list.json records the concrete results. AMF/QSV may remain performance-unvalidated only if their common implementation surface is complete and their canonical hardware-validation features remain open and truthfully labeled.
