# QB-PERF-005 M7 native lifecycle evidence

## Scope and status

This is provisional PC-B evidence for the non-League portion of audited
QB-PERF-005 Milestone 7. The native graph is integrated into the automatic
recorder service behind the developer-only
`QUEUEBACK_WINDOWS_RECORDER_BACKEND=native` selector. An unset selector, or
the explicit value `ffmpeg`, retains the established FFmpeg/WGC path. Nothing
is persisted as a user preference.

The audited M7 exit requires an actual automatically discovered League target.
That acceptance run is intentionally deferred because League must not be run on
PC B. This document does not claim that external gate passed.

## Lifecycle contract

- `VideoRecordingSession` gives the service one directory, clock anchor,
  evidence, exit, stop and failure-stop contract for either backend.
- Native startup occurs only after the service has resolved and validated its
  exact capture target. The provisional native plan is fixed to High/H.264 and
  the four-slot NVENC ring.
- One named GPU worker owns the thread-affine WinRT, WGC, D3D11 and NVENC graph.
  Normal stop, failed startup and cancellation all signal and join that worker;
  dropping the owner cannot detach it.
- Native source/encode/mux evidence feeds the existing service watchdog through
  a latest-value channel, with publication bounded to four updates per second.
- Native mux output starts as `video.partial.mp4`. Only a successful worker join,
  terminal capture/mux evidence and a nonempty file publish canonical
  `video.mp4`. Explicit or observed failures preserve the partial and never
  publish it as a valid recording.
- Metadata distinguishes the provisional path as
  `native_windows_graphics_capture_d3d11` / `native-provisional` and declares
  D3D11-to-NVENC interop, the two-frame WGC pool, one latest-source snapshot and
  four encoder slots.

## Defects found by the real lifecycle fixture

The first static Notepad run admitted and encoded frames but initially exposed
no mux progress. A 1 MiB buffered stdin could retain a low-entropy Annex-B
stream beyond the 15-second startup deadline. The mux writer now flushes after
256 KiB or 250 ms. This preserves bulk buffering while bounding mux-observability
latency to four time-driven flushes per second.

A normal recording followed immediately by a cancelled second session then
reproduced a Windows access violation at the second generated WGC factory-cache
call. WGC support and free-threaded frame-pool statics are now loaded through
worker-scoped COM interfaces and released before that worker's MTA guard. The
same sequential matrix subsequently passed without a crash.

## Verification performed on PC B

All commands were run from `recorder/` on 2026-08-17:

```text
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test native::lifecycle --lib --all-features
cargo test service --lib --all-features
cargo test --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
```

Results:

- format, all-target/all-feature compile and strict Clippy passed;
- focused lifecycle: 2 passed, 2 environment-gated real tests ignored;
- focused service: 7 passed;
- consolidated non-League suite: 91 passed, 0 failed, 2 explicitly ignored,
  and the unavailable R11 empirical parser fixture filtered out.

The ignored real tests were then run sequentially against an ordinary Notepad
window with the PC-B stock FFmpeg 9 executable:

```text
cargo test --lib --all-features native::lifecycle::tests::real_native_ \
  -- --ignored --test-threads=1 --nocapture
```

Both passed in 7.11 seconds. The first recorded for five seconds, published a
nonempty MP4, reported terminal evidence, encoded at least 300 frames and passed
a full FFmpeg decode. The second immediately exercised pre-start cancellation
and proved the native worker was reaped within its ten-second bound.

## Remaining authority gates

- Re-run against the pinned r5 FFmpeg/ffprobe pair; the PC-B media runtime has
  source/lock contents but not those binaries.
- Rebase against the exact fresh PC-A repository state before further native
  edits when that repository/ZIP is supplied.
- Record one real automatic League lifecycle and perform audited M8 controlled
  League A/B benchmarking later. No League process was started for this
  evidence.
