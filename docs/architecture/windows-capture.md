# Windows capture architecture

QueueBack targets the exact League of Legends top-level HWND with Windows Graphics Capture (WGC). It does not capture the desktop, inject into League, hook DirectX, intercept input, or modify League settings. The optimized paths require Windows 10 version 1903 or newer and the pinned QueueBack media runtime.

## Production backend

Windows has one production recorder backend:
`native-wgc-d3d11-nvenc`. There is no environment selector and no automatic or
operator-selected cross-backend fallback. The supported recording plan is H.264 High,
1920x1080 at 60 FPS on a same-adapter NVIDIA NVENC device. Unsupported codec/profile
configuration fails during initialization rather than being silently remapped.

HEVC, non-High recording profiles, AMD/AMF, and Intel/QSV were available only through
the retired FFmpeg-driven WGC comparison backend and had no validated production
combination. They are unsupported by the current product architecture. Future vendor
or codec support must extend and validate the native recorder contract rather than
revive a parallel capture backend.

## Native frame path

```text
League HWND
  -> WGC free-threaded frame pool (2 D3D11 BGRA frames)
  -> nonblocking callback-to-worker handoff (capacity 1)
  -> one D3D11 GPU worker and one pending source frame
  -> D3D11 video-processor resize and BGRA-to-NV12 conversion
  -> four owned NV12/NVENC slots
  -> direct NVENC H.264 submission and bounded completion thread
  -> FFmpeg video-bitstream pipe + native WASAPI PCM / intentional-silence AAC
  -> fragmented MP4
```

The WGC callback never waits for the GPU worker, encoder, muxer, disk, or runtime work. If the handoff is full, the incoming frame is closed and counted as a handoff drop. Tick admission is transactional: a CFR tick commits only after successful NVENC submission. Scheduled, submitted, completed, and muxed frame accounting is reconciled before a successful final result.

Full video frames remain GPU-resident through acquisition, resize/color conversion, and encoder submission. There is no per-frame full-surface readback, CPU pixel conversion, or raw-frame transfer. WGC, conversion, and encoder surfaces may still require bounded GPU-to-GPU copies, so the path is described as GPU-resident rather than universally zero-copy.

A duplicate CFR tick can reuse NV12 pixels when the selected free encoder slot still contains the current source snapshot. NVENC unmaps the input before releasing the slot. Staging a new snapshot or reconfiguring the video processor invalidates reuse. Each tick still submits its own frame to NVENC.

## FFmpeg boundary

The native recorder starts one supervised FFmpeg child, but FFmpeg performs no video
capture, filtering, scaling, or encoding. It receives the already encoded native
Annex-B H.264 packet sequence and native WASAPI f32le 48-kHz stereo PCM (or explicit
intentional silence), encodes AAC, and muxes fragmented MP4. Packaged ffprobe performs the
bounded common finalization probe. FFmpeg remains the correct implementation for
muxing, probing, export/transcoding, and fixture tooling; those uses are not recorder
backends.

The generated silence input follows video EOF via `-shortest`. System PCM can
close early while video continues, and video-only capture opens no audio input.

System audio has two independent owners: an MTA WASAPI capture thread, started
before WGC without waiting for endpoint activation, and an event-driven PCM writer.
Packet storage is pooled (2 MiB plus one 32-KiB coalescing buffer); pre-epoch queued
coverage is bounded to one second and available pool capacity, and post-epoch PCM
to 500 ms. Overwrite and missing startup coverage are reported. QPC timestamps map
to signed 48-kHz sample slots against the original first-WGC QPC/Instant anchor;
contiguous device positions preserve a run. The fixed delivery allowance is 250 ms.
Silence inferred after that allowance, confirmed silent packets, discarded/late
valid frames and timing residuals are distinct counters; late sound is degraded
coverage, never a promise that waiting guarantees packet availability.
An active endpoint with no packets counts as inferred idle, even before its first
packet. Known unavailable fill and lost startup coverage degrade the saved audio
outcome. Intentional capture shutdown retains the last availability state for the
final PCM drain.

The writer opens a private 32-KiB named pipe and supplies one complete 4,096-frame
prefix before video production. Its minimum post-epoch readiness is about 335 ms;
NVENC initialization overlaps that preparation and only residual time is awaited.
The existing CFR epoch/catch-up behavior is retained. Failed bootstrap reaps the
A/V child before one video-only retry within the original startup deadline.
Audio reserved/emitted lead is bounded to 24,000 frames plus `800*M`, where M is
FFmpeg's existing video progress. A cap-stall EOF escape requires the same actual
video pipe write pending, unchanged M and an active cap for one second. User-space
buffering, slow initialization and ordinary pauses cannot arm it.

Endpoint failure/retry remains upstream of a healthy PCM pump, which writes counted
fill. Default endpoint/state/format notifications are coalesced on the audio owner;
retries run at most once per second for five seconds, then every 30 seconds or on
notification. Stop freezes the video boundary before encoder drain and gives audio
500 ms including pending I/O. Cancellation retains live buffers until OS completion;
an outstanding owner retains no GPU/video resources and disables further audio until
relaunch. Cooperative Windows cancellation is not a hard kernel-call guarantee.
Raw PCM overhang is bounded to 500 ms, with up to two AAC frames of codec padding;
replay/export duration stays video-authoritative. Managed complete-prefix EOF has
scoped shipped-runtime evidence; arbitrary pipe/FFmpeg/storage corruption remains
a shared-mux failure. Audio absence is permitted for system capture, but present
audio still needs valid facts and nonzero MP4 sample runs. See the feature checkpoint
for hardware validation results and remaining limitations.

## Runtime contract

The recorder resolves `queueback-ffmpeg-8.1.2-windows-x86_64-r6` through the shared
immutable media-runtime contract. Startup verifies file hashes, exact version/build
identity, and the FFmpeg/ffprobe pair. The currently pinned artifact still contains
the historical WGC/D3D11 patch and compiled hardware encoders as reproducible build
provenance, but the production recorder neither selects nor invokes those capture
filters or FFmpeg video encoders.

The r6 build pins FFmpeg 8.1.2, nv-codec-headers 12.2.72.0, AMF 1.4.36, the QueueBack
WGC patch, and its exact MSYS2 toolchain. Ordinary builds and startup do not rebuild
the runtime. Removing unused compiled capabilities requires a separately locked,
rebuilt runtime revision; it is not part of retiring the recorder backend.

The fragmented MP4 muxer flushes completed packets. Successful recording publishes only a validated private candidate as `video.mp4`. Failed startup or active-session failure preserves a recoverable partial candidate and an actionable error rather than claiming canonical success.

## Ownership, timing, and lifecycle

Native ownership is explicit and bounded. A lifecycle owner supervises the GPU worker; cooperative startup cancellation and stop execute blocking joins through Tokio's blocking pool. A fail-safe Drop join remains for invariant-breaking unwinds. An in-process worker cannot forcibly terminate a GPU-driver call that never returns, so hard driver-hang containment remains an open supervised-process boundary rather than a solved claim.

Native construction checks the stop flag and shared absolute deadline before source creation, initial content recovery, conversion, mux and NVENC construction, and completion. A completed blocking call cannot authorize the next stage after cancellation or expiry. Every check after source creation stays inside the initialization result scope so dependent resources drop before explicit callback-safe source close. Reported protocol failures take precedence over a simultaneous generic startup timeout; cancellation retains priority.

Startup is ready only after a real WGC frame with a positive `SystemRelativeTime`/QPC value, encoded output, mux progress, and a positive output timestamp. The first copied WGC source QPC timestamp maps to Rust monotonic and wall clocks and becomes both the recording epoch and Live Client poller anchor. Process-spawn time is not the video epoch. The one-frame worker pending slot selects the newest eligible timestamp among offered frames, otherwise the earliest future timestamp. Once pending is eligible, both pre-deadline waiting and tick polling leave the following frame in the existing handoff; delivery before its presentation timestamp must not cause its premature discard. The worker waits only until its next deadline. Duplicate or older timestamps are closed and counted as worker discards; the CFR clock keeps the last good GPU snapshot rather than failing the recording or moving its source clock backwards. The watchdog and published recording QPC evidence use the CFR clock's first/latest successfully copied source timestamps, so incoming unusable frames cannot mask a frozen image. Sustained lack of copied source-QPC advancement still triggers the capture-progress watchdog when the target is visible.

Capture diagnostics separately expose surfaced and superseded source frames, copied snapshots, worker discards, CFR discards (pending replacements) and duplications, catch-up ticks, maximum source age, encoded/completed/muxed frames, pool recreations, bounded depths, first/latest copied QPC and latest callback source QPC, output progress, writer/flush timing, and terminal errors. The callback QPC is the frame's source timestamp, not its wall receipt time; the source-only probe's `--timing-only` mode records both relative source and receipt times without requiring NVENC. Counters are monotonic and compositor frames never surfaced by Windows remain outside the claim.

## Window and failure behavior

- Focus loss and ordinary occlusion continue exact-window capture.
- Minimize can pause WGC. Hidden or detached sources pause only source freshness; encoder/mux watchdogs remain active. Restore or replacement rearms source freshness.
- Same-HWND size changes recreate the bounded WGC pool while retaining the configured output canvas. Transient content sizes that cannot form a nonempty chroma-aligned NV12 crop (including 1x1 frames) are discarded before pool recreation, retaining the last valid GPU snapshot. The first unusable size is logged once per session; native session/probe telemetry records its dimensions and the discard count. Resize preparation errors also close and account for their owned frame before propagating the underlying error.
- Teardown stops frame admission, attempts callback revocation and proves frame callbacks idle before draining queued frames or closing the free-threaded pool, then the session. It proves handler-owner release before destroying capture/device/MTA resources; explicit close and implicit source Drop use the same rule. A failed proof retains the complete source stack; cleanup API errors still fail the recording but do not retain the stack once callback release is proven. NVENC/mux output drains before source teardown. These are application ownership guarantees, not a vendor guarantee of synchronous revocation or hard driver-hang containment.
- Target discovery and visibility/bounds queries temporarily use per-monitor-v2
  thread DPI awareness, then restore the caller's prior context. Window sizes
  therefore remain physical pixels even when the app or fixture process was
  initialized under a DPI-virtualized context.
- WGC Closed or typed HWND disappearance detaches only the WGC leaf after callback quiescence. The same GPU worker/device/MTA, converter, last image, encoder, mux and CFR clock survive. Same-process, same-adapter replacement resumes fresh frames; attachment attempts are limited to one per two seconds. There is no lost-source timeout; healthy CFR continues until original-process exit or user stop.
- Other WGC/driver/callback failures and unsupported adapter changes are terminal. Unproven callback cleanup retains the guarded context and disables new capture until recorder relaunch. Cumulative leaf counters and the first error survive replacement.
- Confirmed termination of the exact original process requests graceful finalization. A different watcher selection or reused PID cannot replace a live session owner.
- Before creating a recording bundle or initializing WGC/D3D11/NVENC, automatic capture requires two identical usable window observations spanning at least two seconds: PID/HWND, physical client dimensions, DPI and monitor. Hidden/minimized/missing/tiny windows clear evidence; geometry/identity changes or observation gaps above five seconds restart stability. DXGI adapter resolution occurs only after stability, followed by an exact snapshot/cancellation recheck within 500 ms. Expected window absence defers; actual observation/adapter errors propagate. Live Client availability, game clock, map and mode never gate video capture. Process identity includes PID and exact Windows creation FILETIME, pinned by a limited-rights owned process handle. Window stability is admission evidence, not proof of driver safety or absence of League interference.
- Target changes after admission retain the bounded 2, 5, 10, and 30-second pre-native retry schedule. Actual readiness-query/adapter errors and any native initialization failure suppress further initialization for that process identity; native cleanup is joined first. A new process must earn fresh window stability. Initial item/frame content must cover the admitted physical client dimensions and support an NV12 crop; smaller item sizes use validated HWND dimensions for the same pool. Before audio/conversion/encoding, that source waits for suitable content with a five-second inactivity limit and the shared absolute 15-second startup deadline, measured before worker spawn. Tiny-frame progress cannot extend the absolute limit. Expiry stops and joins the worker terminally, without GPU retries; it cannot forcibly interrupt a stuck driver call or cleanup join. One terminal recovery event retains elapsed time, unusable count, first/latest/expected dimensions and failure reason. Partial initialization explicitly closes the source, retaining its device/MTA if callback quiescence fails. Active-session failure suppression remains unchanged. Generated service fixtures use the same window admission and unavailable Live source as production, without fake gameplay-readiness injection.
- There is no automatic primary-display, Desktop Duplication, GDI, software, cross-adapter, or cross-backend fallback.

WGC may show an operating-system capture indicator. QueueBack requests cursor exclusion and `IsBorderRequired=false` before capture starts. Border suppression is best-effort: older Windows builds may lack the API, and Windows policy or another capturing app may retain the indicator. Recording correctness never depends on restricted capabilities or manipulating the user's pointer.

## Support and validation matrix

| Combination | Production path | State | Physical performance claim |
| --- | --- | --- | --- |
| Windows 10 1903+ / exact HWND / D3D11 | native WGC/D3D11/NVENC | implemented with synthetic lifecycle/media fixtures | measured adapter/backend only |
| NVIDIA / NVENC / H.264 High 1080p60 | native production backend | implemented and non-League fixture-validated on RTX 3050 Ti Laptop GPU | pending valid QB-PERF-002 League matrix on the target RTX 4060 system |
| HEVC or non-High recording profile | none | unsupported | none |
| AMD / AMF | none | unsupported by the native recorder | none |
| Intel / QSV | none | unsupported by the native recorder | none |
| Cross-adapter encoder | none | deliberately unsupported | cross-adapter-unvalidated |
| Missing WGC/runtime/direct NVENC interop | none | unsupported with diagnostics | unsupported |

The supported native combination is not a vendor-wide performance claim. Every
future physical combination must first exist in the native architecture and pass the
same frame-pacing, lifecycle, media-integrity, and resource budgets before receiving
a hardware-validated label.
