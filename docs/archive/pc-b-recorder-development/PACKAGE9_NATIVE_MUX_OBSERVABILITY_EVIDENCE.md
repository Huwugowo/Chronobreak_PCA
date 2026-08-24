# Package 9: Native mux/output observability

Date: 2026-08-19

Scope: measure completion-thread output blocking and add a bounded,
feature-gated writer-stall seam. This package does not change the callback,
queue capacities, CFR clock, NVENC configuration, FFmpeg arguments, media
metadata, or the fixed full-ring retry.

## Implementation

`NativeMuxVideoWriter` now accumulates writer and explicit-flush call counts,
total durations, maxima, and writer calls at least one exact rational output
interval. The counters remain local to the completion thread. Writer Drop
publishes them once to shared telemetry after the completion loop's explicit
flush; a release/acquire publication flag prevents a live snapshot from
observing a partially published group. Statistics use relaxed atomic loads and
stores and perform no per-frame atomic read-modify-write.

The `native-failure-injection` feature adds a one-shot stall selected by
positive writer-call index and duration in `1 ns..=5 s`. The configuration
uses release/acquire ordering because it is a command crossing threads. The
stall itself runs on the existing completion/output thread and is reported in
the terminal writer statistics. `native_mp4_probe` exposes paired
`--stall-mux-after-writes` and `--stall-mux-ms` arguments and prints every new
field without renaming existing evidence fields.

## Static acceptance

The following passed from the canonical repository:

```powershell
cargo fmt --manifest-path recorder/Cargo.toml --all -- --check
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
cargo build --manifest-path recorder/Cargo.toml --release --bin recorder
cargo build --manifest-path recorder/Cargo.toml --release --example native_mp4_probe --features native-failure-injection
git diff --check
```

The broad suite passed 117 library tests and three binary tests. Five explicit
environment/profile tests remained ignored. Only the unavailable R11
empirical fixture was filtered. Focused tests cover local-to-terminal
publication, the exact 60-Hz rational threshold, bounded one-shot injection,
monotonic progress, and early mux exit.

## Hardware evidence

Pinned runtime preflight passed as
`queueback-ffmpeg-8.1.2-windows-x86_64-r6`. The successful ignored evidence
root is:

`evidence/native-mux-observability-20260819-123744`

Four ten-second native arms captured the same visible non-League Notepad HWND.
Each produced 600 scheduled, submitted, completed, and muxed frames; 1920x1080
H.264 at 60/1 FPS; ten seconds of AAC audio; and a full decode with zero
diagnostics.

| Arm | Maximum writer call | Accounted injected stall | Slow calls | No-slot retries |
| --- | ---: | ---: | ---: | ---: |
| normal | 68.1 us | 0 | 0 | 12 |
| 500-ms stall | 500.4845 ms | 500.4576 ms | 1 | 83 |
| 2-s stall | 2.0004689 s | 2.0004498 s | 1 | 389 |
| 5-s stall | 5.0002081 s | 5.0001708 s | 1 | 1,026 |

Every arm preserved maximum in-flight 4, pending-source high-water mark 1,
zero unstaged-tick admission failures, and maximum catch-up batch 2. The stall
arms therefore demonstrate bounded containment and truthful pressure
accounting; they do not by themselves trigger a synchronization redesign.

The preceding root `evidence/native-mux-observability-20260819-123652` is a
preserved failed fixture attempt. Windows reported the hidden WinForms fixture
as non-visible, so target resolution failed before recording began.

## Decision boundary

Package 9 is complete. Package 11 remains gated: one ten-second unstalled arm
is neither the required four interleaved 60-second arms nor CPU/wakeup/stop
latency evidence. The observed normal no-slot retries justify running that
gate later, but do not establish that event-driven waiting is materially
better. Package 10 lifecycle work is next.
