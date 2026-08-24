# Recorder lifecycle and synchronization

## Startup and steady state

`recorder/src/main.rs` loads configuration, initializes daily non-blocking logs, and selects diagnostics, headless, or tray mode. Production tray mode runs the UI event loop while the recording service runs on Tokio.

The service resolves the immutable packaged media runtime and detects audio once. It then checks League process presence every two seconds. Windows capture planning deliberately waits for the real League HWND because its monitor and DXGI adapter are part of the recording contract.

League process presence remains the automatic lifecycle authority:

1. appearance makes the process eligible while its visible capture HWND is discovered;
2. an owned cancellable startup task starts the selected backend inside one 15-second deadline; the explicit FFmpeg backend tries each eligible same-adapter encoder once;
3. PID replacement cancels/reaps startup or finalizes the old session before a new generation can start;
4. disappearance gracefully finalizes an active recording;
5. application shutdown cancels startup and finalizes an active recording before emitting completion.

Live Client readiness and `GameEnd` are not start/stop signals. A process with no ready window is retried on the next watcher tick. Retryable startup failures follow the bounded 2/5/10/30-second schedule only after cleanup, while terminal incompatibility and an unexpected active-backend exit suppress runaway retries for that process generation.

## Capture and finalization

On Windows, the default production video path is in-process exact-HWND Windows Graphics Capture on an explicit D3D11 adapter, D3D11 resize/BGRA-to-NV12 conversion, and direct NVENC H.264 submission. `QUEUEBACK_WINDOWS_RECORDER_BACKEND=ffmpeg` selects the retained external WGC/D3D11 NVENC/AMF/QSV backend explicitly. A native failure never silently changes backend. Neither path has automatic GDI, primary-display, software, or cross-adapter fallback. See [windows-capture.md](windows-capture.md) for the complete frame/resource contract.

The service wraps the selected implementation in `VideoRecordingSession`. The FFmpeg implementation owns its capture/encode/mux child. The native implementation owns a supervised GPU worker, a bounded completion thread, four NVENC slots, a diagnostics receiver, and one FFmpeg audio/mux child. Startup is announced only after a real first WGC frame and advancing encode/mux evidence. Cooperative native cleanup performs the blocking worker join off the async executor; child pipes are continuously drained and reaped under finite deadlines.

The service validates the captured HWND/PID/adapter identity while active. A closed or replaced HWND while the League process is still present produces a failed/partial outcome. Focus loss, occlusion, and temporary minimize do not change identity; WGC may pause while minimized and resume after restore.

Normal stop asks the selected backend to end capture, drain bounded native/FFmpeg work, flush the muxer, and reap every owned task/thread/child under finite deadlines. A clean result requires an intentional stop boundary, terminal WGC/encode/mux evidence, consistent frame accounting, successful mux exit, and nonempty output. The successful private candidate is then renamed to canonical `video.mp4`; unsuccessful fragments are preserved and never overwrite it. Completed MP4 fragments are flushed during capture so forced-encoder fixtures remain recoverable.

## Live Client synchronization

`PollerSession` starts only after video readiness. Its monotonic epoch is the first Windows Graphics Capture `SystemRelativeTime`/QPC frame, mapped into the Rust clock, rather than FFmpeg spawn time. Calibration probes `/gamestats` until it observes five strictly advancing, clock-consistent samples and derives `game_start_video_offset_ms` from monotonic receive time and the video epoch.

After calibration, independent loops:

- poll cumulative `/eventdata` every second, deduplicate by event ID, normalize fields, sort chronologically, and precompute `video_time_ms`;
- poll game, active-player, and player-list data every ten seconds, storing snapshots and deriving item/level changes. Only the one initial aggregate snapshot may also seed cumulative event data; the one-second event loop is the sole steady-state event authority.

Requests are concurrent within a snapshot. Three consecutive failures stop only the affected polling task; successful requests reset the failure count. API loss never stops video. Poller cancellation and recorder-backend shutdown begin together, and diagnostic/benchmark evidence attributes their CPU and errors separately. This isolation is why the accepted pre-change data identified GDI acquisition/conversion—not polling—as the primary performance issue.

## Persistence and compatibility

`game_log.json` is rewritten atomically during capture through a temporary sibling and rename. After stop, release-era `metadata.json` is still the canonical library metadata; no recording-state authority or strict Unknown-state UI was reintroduced. Existing recordings are not rewritten.

New recordings add backward-compatible flat path fields and an optional nested `capture` object containing backend/ABI, adapter identities, direct interop, GPU stages, finite bounds, runtime ID, first/latest QPC values, final counters, and terminal evidence. Recording bundle IDs are Unix seconds with an optional canonical `-1` through `-999` collision suffix; application browsing, playback, save/delete, clip association, and asset routing all accept that same safe contract. Older app readers ignore additive metadata fields and continue to browse/play valid `metadata.json` plus `video.mp4` bundles.

Live Client poller degradation does not invalidate otherwise healthy video. Encoder/capture failure remains an explicit recorder error while the fragmented partial output and latest atomically written game log stay recoverable. The app does not infer clean completion from the new diagnostics object.

## Performance constraints

Watcher cadence is two seconds; event polling is one second; snapshots are ten seconds. Tokio missed ticks use delay semantics to avoid burst catch-up. Capture diagnostics are aggregate first/progress/terminal messages, not per-frame logs. Video remains in a bounded GPU-resident graph, and no full-video pass occurs at match end.

`QB-PERF-002` applies the common benchmark budget: capped average-FPS loss <=2%, 1%-low loss <=5%, p95 increase <=5%, p99 increase <=8%, additional non-displayed rate <=0.1 percentage point, plus CPU/GPU saturation and memory/output/lifecycle safety gates. Physical validation is per adapter/encoder even though the implementation contract is vendor-neutral.
