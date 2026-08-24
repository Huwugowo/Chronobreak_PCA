# Recorder remediation Package 4 evidence

Date: 2026-08-18

## Scope and ownership

Package 4 moves post-anchor WGC ownership into a worker-only, latest-pending
slot inside `NativeWgcCapture`. This avoids a self-referential session borrow:
the pending value is owned by the source, and `CapturedWgcFrame` still borrows
that source whenever a frame is temporarily taken for staging. Safe code
therefore cannot tear down or recreate WGC while a taken frame remains live.

The hard bounds are unchanged:

- WGC frame pool: 2;
- callback-to-worker handoff: 1, `try_send`, nonblocking;
- worker pending frame: at most 1;
- NV12/NVENC slots: 4.

The first admitted frame is copied once and owns CFR tick zero. After that,
source reception replaces the worker's pending frame without copying pixels.
Replacing a pending value drops and closes the older WGC frame immediately.
Immediately before each due tick, the worker drains any handoff frame, stages
only the freshest pending frame into the persistent BGRA snapshot, closes it,
and then emits/converts the tick. With no new pending frame, CFR reuses the
existing snapshot.

Capture telemetry now distinguishes callback handoff drops, pending-frame
replacements, pending high-water mark, and other worker discards. Successful
session finalization enforces:

```text
admitted = source_snapshot_copies + pending_frame_replacements + worker_frame_discards
cfr_source_discards = pending_frame_replacements
pending_frame_high_water_mark <= 1
source_snapshot_copies <= scheduled_ticks + 1
```

## Resize and teardown correction

Resize/recreate and shutdown pause or detach the callback, wait for active
callback work to quiesce, drain the handoff, and close the pending frame before
recreating or closing the WGC pool. These frames are included in worker-discard
accounting.

The first live resize/minimize fixture exposed a real transition edge: WGC can
briefly report a restored/resized `ContentSize` while the surfaced texture
still has the prior, smaller allocation. Copying a box for the new dimensions
would be invalid. The converter now rejects that transition surface without a
copy, accounts and closes it, and preserves the last valid snapshot until a
dimensionally consistent surface arrives. Reconfiguration waits for that
consistent surface, so it cannot clear the usable snapshot prematurely.

## Deterministic policy verification

The pure latest-pending test drives logical source rates of 60, 144, and 240 Hz
against 60 output ticks. At every tick it selects the newest source whose
logical arrival is at or before the tick, keeps source age within one output
interval, and keeps pending high-water at one. The 144-Hz arm coalesces 82 of
142 admitted sources; the 240-Hz arm coalesces 177 of 237. Each arm performs
60 modeled snapshot copies, independent of source rate.

A separate ownership test proves replacement immediately drops the older
pending value rather than retaining two worker-owned frames.

## Live GPU evidence

All live runs used disposable non-League windows and the pinned r6 FFmpeg mux
runtime. No user recording was used.

### Steady 10-second exact-HWND run

Evidence:

```text
evidence/package4-source-coalescing-20260818-144133
```

The run scheduled, submitted, completed, muxed, and decoded all 600 frames.
Video and audio both start at zero and last exactly 10.000 seconds. Pending
high-water was one, with complete accounting:

```text
601 admitted = 539 snapshot copies + 61 pending replacements + 1 resize discard
```

Even at this display's approximately 60-Hz compositor rate, 62 full-size
source copies were avoided.

### Resize plus minimize/restore

Accepted evidence after the transition-surface correction:

```text
evidence/package4-resize-minimize-r3-20260818-144630
```

The window was resized three times and minimized for three seconds. Results:

- 1,800 scheduled/submitted/completed/muxed frames and zero slot/unstaged
  drops;
- exact 30.000-second H.264/AAC streams, full video decode, and 1,800 decoded
  frames;
- 42 distinct full-frame hashes despite the minimized duplicate interval;
- five WGC pool recreations and four accepted converter recreations;
- pending high-water one;
- `1,620 admitted = 1,400 copies + 215 replacements + 5 worker discards`.

The earlier failed attempts are preserved in separate ignored evidence roots;
they are not presented as passing evidence.

### 240-second animated soak

Evidence:

```text
evidence/package4-240s-soak-r2-20260818-144911
```

The continuously animated 1920x1080 fixture produced:

- 14,400 scheduled, submitted, completed, muxed, and decoded frames;
- zero slot drops, unstaged drops, completion errors, or queue failures;
- exact 240.000-second H.264 and AAC streams, both starting at zero;
- three distinct hashes sampled at frames 0, 7,200, and 14,399;
- pending high-water one and no pool/converter recreation;
- maximum selected-source age 341 QPC 100-ns units (34.1 microseconds);
- `14,400 admitted = 12,712 copies + 1,688 replacements`;
- one callback handoff drop among 14,401 valid arrivals.

Snapshot copies fell 11.72% relative to admitted frames on the physical
approximately-60-Hz fixture. This is not extrapolated as a League-impact
claim; the deterministic tests establish the bounded higher-rate policy.

One-second resource sampling produced 239 Rust and 238 mux-child samples. The
Rust process's average private memory was 126.058 MiB in the first 30 samples,
131.581 MiB around the middle, and 132.102 MiB in the last 30; the last window
equals the run maximum and was flat. Average working set moved from 58.041 to
61.805 MiB. Handles averaged 413.67 initially, peaked at 426, and ended at 415;
threads averaged 24.2 initially and ended at 19. The mux child ended at 16.770
MiB private memory with 205 handles and 37 threads. This is bounded warm-up,
not sustained handle/thread growth. Sampled CPU deltas were 16.844 seconds for
Rust over 238 seconds (7.08% of one logical core) and 0.250 seconds for the mux
child over 237 seconds.

## Static verification

The following passed after the live evidence:

```text
cargo fmt --manifest-path recorder/Cargo.toml --all -- --check
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
git diff --check
```

The known R11 empirical fixture remains the only filtered test. No timestamp,
codec, mux, GOP, audio, replay-index, or backend-selection behavior changed.

## Decision

Keep Package 4. It removes source-rate-proportional full-size GPU copies while
preserving exact CFR output, callback nonblocking behavior, strict resource
bounds, resize/minimize recovery, media decode, and complete ownership
accounting. Package 5 may proceed independently.
