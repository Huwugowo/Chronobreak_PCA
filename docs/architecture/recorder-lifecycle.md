# Recorder lifecycle and synchronization

## Startup and steady state

`recorder/src/main.rs` loads configuration, initializes daily non-blocking logs, and selects diagnostics, headless, or tray mode. Production tray mode runs the UI event loop while the recording service runs on Tokio.

The service resolves the immutable packaged media runtime and detects audio once. It then checks League process presence every two seconds. Windows capture planning deliberately waits for the real League HWND because its monitor and DXGI adapter are part of the recording contract.

League process presence remains the automatic lifecycle authority:

1. appearance makes the process eligible while its visible capture HWND is discovered;
2. an owned cancellable startup task starts the native recorder inside one 15-second deadline;
3. PID replacement cancels/reaps startup or finalizes the old session before a new generation can start;
4. disappearance gracefully finalizes an active recording;
5. application shutdown cancels startup and finalizes an active recording before emitting completion.

Live Client readiness and `GameEnd` are not start/stop signals. A process with no ready window is retried on the next watcher tick. Retryable startup failures follow the bounded 2/5/10/30-second schedule only after cleanup, while terminal incompatibility and an unexpected active-backend exit suppress runaway retries for that process generation.

## Capture and finalization

On Windows, the sole production video path is in-process exact-HWND Windows Graphics Capture on an explicit D3D11 adapter, D3D11 resize/BGRA-to-NV12 conversion, and direct NVENC H.264 submission. There is no external capture-backend selector and no automatic GDI, primary-display, software, cross-adapter, codec, or vendor fallback. See [windows-capture.md](windows-capture.md) for the complete supported surface and frame/resource contract.

The service owns one native implementation: a supervised GPU worker, bounded completion thread, four NVENC slots, diagnostics receiver, and one FFmpeg child restricted to audio encoding and fragmented-MP4 muxing. Startup is announced only after a real first WGC frame and advancing encode/mux evidence. Cooperative cleanup performs the blocking worker join off the async executor; child pipes are continuously drained and reaped under finite deadlines.

The service validates the captured HWND/PID/adapter identity while active. A closed or replaced HWND while the League process is still present produces a failed/partial outcome. Focus loss, occlusion, and temporary minimize do not change identity; WGC may pause while minimized and resume after restore.

Normal stop ends native capture, drains bounded GPU/encoder/mux work, flushes the muxer, and reaps every owned task/thread/child under finite deadlines. A clean result requires an intentional stop boundary, terminal WGC/encode/mux evidence, consistent frame accounting, successful mux exit, and nonempty output. The common finalizer validates the private candidate before renaming it to canonical `video.mp4`; unsuccessful fragments are preserved and never overwrite it. Completed MP4 fragments are flushed during capture so forced-encoder fixtures remain recoverable.

## Live Client synchronization

`PollerSession` starts only after video readiness. Its monotonic epoch is the first Windows Graphics Capture `SystemRelativeTime`/QPC frame, mapped into the Rust clock, rather than FFmpeg spawn time. Calibration probes `/gamestats` until it observes five strictly advancing, clock-consistent samples and records a midpoint-derived affine schema-v2 game-to-replay calibration.

After calibration, independent loops:

- poll cumulative `/eventdata` every second, deduplicate by event ID, normalize fields, and sort integer-microsecond game-clock observations chronologically;
- poll game, active-player, and player-list data every ten seconds, storing snapshots and deriving item/level changes. Only the one initial aggregate snapshot may also seed cumulative event data; the one-second event loop is the sole steady-state event authority.

Requests are concurrent within a snapshot. Three consecutive failures stop only the affected polling task; successful requests reset the failure count. API loss never stops video. Poller cancellation and recorder-backend shutdown begin together, and diagnostic/benchmark evidence attributes their CPU and errors separately. This isolation is why the accepted pre-change data identified GDI acquisition/conversion—not polling—as the primary performance issue.

## Persistence

`game_log.json` is rewritten atomically during capture through a temporary sibling and rename. Schema-v2 `metadata.json` is the canonical library metadata. It carries the validated media timeline and immutable `media_id`, which must match the game log before the app exposes a playable recording.

New recordings use only the strict schema-v2 contract; old timing fields, schema-v1 parsing, approximate fallback, and migration are intentionally absent. The optional capture object records backend/ABI, adapter identities, direct interop, GPU stages, finite bounds, runtime ID, first/latest QPC values, final counters, and terminal evidence. Recording bundle IDs are Unix seconds with an optional canonical `-1` through `-999` collision suffix; application browsing, playback, save/delete, clip association, and asset routing all accept that same safe contract.

Live Client poller degradation does not invalidate otherwise healthy video. Encoder/capture failure remains an explicit recorder error while the fragmented partial output and latest atomically written game log stay recoverable. The app does not infer clean completion from the new diagnostics object.

## Performance constraints

Watcher cadence is two seconds; event polling is one second; snapshots are ten seconds. Tokio missed ticks use delay semantics to avoid burst catch-up. Capture diagnostics are aggregate first/progress/terminal messages, not per-frame logs. Video remains in a bounded GPU-resident graph, and no full-video pass occurs at match end.

`QB-PERF-002` applies the common benchmark budget: capped average-FPS loss <=2%, 1%-low loss <=5%, p95 increase <=5%, p99 increase <=8%, additional non-displayed rate <=0.1 percentage point, plus CPU/GPU saturation and memory/output/lifecycle safety gates. Physical validation is per adapter/encoder even though the implementation contract is vendor-neutral.
