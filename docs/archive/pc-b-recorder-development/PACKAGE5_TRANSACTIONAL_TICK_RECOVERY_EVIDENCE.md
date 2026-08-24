# Recorder remediation Package 5 evidence

Date: 2026-08-18

## Transactional timing contract

Package 5 separates inspecting a due CFR tick from committing it. `peek_due`
is read-only: repeated peeks return the same tick and do not change scheduled,
lateness, duplicate, source-age, or catch-up telemetry. The GPU worker commits
that tick only after it has leased a fixed NV12 slot and NVENC has accepted the
submission.

A full four-slot ring is now an admission failure, not an output-tick drop. The
clock remains on the same tick, the worker waits one millisecond (while still
receiving/coalescing source ownership), and the next scheduler pass retries.
The worker also checks slot availability before copying a pending WGC surface
into the persistent snapshot, so backpressure does not create extra full-frame
copies. No overdue-frame queue was added.

Catch-up is capped at two accepted submissions per scheduler pass. Returning
to the outer loop between batches restores stop, target, source, and evidence
checks. A duration-bounded recording must commit every tick whose deadline is
before its recording deadline. A normal external stop records the observed
monotonic stop instant and gets a finite five-second final catch-up deadline.
Expiry is a reported timing failure that preserves partial output; it cannot
publish a silently shortened MP4.

Successful shutdown now enforces:

```text
committed CFR ticks = NVENC submitted = NVENC completed = muxed frames
unstaged tick admission failures = 0
maximum catch-up batch <= 2
converter no-slot failures <= session no-slot failures
```

Telemetry and probe output use `no_slot_admission_failures` instead of the
misleading `slot_tick_drops`. The old probe option remains as a compatibility
alias, but it configures an admission-retry ceiling and no longer subtracts
frames from the expected output count.

## Deterministic verification

Clock tests prove that:

- peeking an overdue tick twice returns the same index and leaves the clock at
  zero committed ticks with no recorded lateness;
- committing the accepted tick advances exactly once and records its actual
  commit lateness;
- a stale/already committed tick is rejected without advancing again;
- exact rational 60-Hz boundaries, source selection, duplicates, and source
  age retain their earlier behavior.

The fixed converter-ring tests continue to prove that a fifth slot cannot be
acquired and submitted ownership is released only by NVENC completion.

## Live GPU evidence

All live runs used a disposable animated non-League window and the pinned r6
FFmpeg runtime. No user recording was used. Accepted evidence is in:

```text
evidence/package5-transactional-stalls-20260818-150714
```

### Steady 10-second control

The no-stall control committed, submitted, completed, and muxed all 600 ticks.
It reported zero catch-up ticks, catch-up batch zero, zero no-slot admission
failures, and zero unstaged admission failures. Output time was exactly 10.000
seconds, maximum NVENC in-flight ownership was two, and WGC pending high-water
remained one.

### Injected worker stalls

Each arm recorded five seconds with one injected stall after tick 60:

| Stall | Committed/submitted/completed/muxed | Maximum lateness | Catch-up ticks | Maximum batch | No-slot retries | Video/audio duration |
| --- | --- | --- | --- | --- | --- | --- |
| 100 ms | 300 / 300 / 300 / 300 | 108.1630 ms | 7 | 2 | 1 | 5.000 / 5.000 s |
| 250 ms | 300 / 300 / 300 / 300 | 256.6683 ms | 19 | 2 | 6 | 5.000 / 5.000 s |
| 500 ms | 300 / 300 / 300 / 300 | 503.3893 ms | 36 | 2 | 17 | 5.000 / 5.000 s |

The increasing retry counts expose real ring pressure without consuming the
corresponding timeline tick. All arms kept maximum in-flight NVENC ownership
at four, pending-frame high-water at one, and unstaged admission failures at
zero. The one-millisecond blocked-admission wait prevents a tight retry spin;
all capture, handoff, pending, conversion, and encode storage remains fixed
size.

For every arm, `ffprobe` found exactly 300 video packets and 300 decoded
frames, no missing video PTS/DTS, and no non-increasing PTS or DTS. Full video
decode completed without an FFmpeg error. Frames 0, 150, and 299 had three
distinct full-frame MD5 values in every arm, demonstrating changing content
both during and after recovery.

## Static verification

The accepted tree passed:

- `cargo fmt --all -- --check`;
- `cargo check --all-targets --all-features`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- library suite: 110 passed, 3 environment-gated ignored, and only the known
  absent R11 empirical fixture filtered;
- recorder binary suite: 1 passed.

The unfiltered library invocation again failed only at the documented absent
R11 fixture (`missing field samples`). No placeholder fixture was fabricated.
Earlier GUI-fixture launch attempts and the first zero-ceiling probe are
preserved in the evidence directory but are not presented as accepted arms.
