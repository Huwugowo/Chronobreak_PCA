# Package 11: Full-ring synchronization gate

Date: 2026-08-19

Decision: **not triggered**. No production synchronization change is
authorized or implemented.

## Gate

The active plan required four 60-second unstalled native arms and both of the
following before replacing the fixed one-millisecond full-ring retry:

1. repeated normal writer/flush calls longer than one 60-Hz output interval,
   or recurring full-ring admission failures; and
2. attributable CPU/wakeup, catch-up, or stop-latency cost outside normal run
   variance.

The first condition failed in every accepted arm, so the conjunctive gate is
closed without a synchronization redesign.

## Accepted evidence

Pinned runtime:
`queueback-ffmpeg-8.1.2-windows-x86_64-r6`.

Evidence roots:

- `evidence/package11-full-ring-gate-20260819-124906` (`normal-1` and
  `normal-2`);
- `evidence/package11-full-ring-gate-supplement-20260819-125402`
  (`animated-normal-1` and `animated-normal-2`).

Every arm used the optimized feature-gated native probe with no injection,
sampled the Rust and pinned FFmpeg processes approximately every 250 ms, and
used a five-second inter-arm cooldown. The first pair captured a static
visible non-League Notepad HWND; the second pair captured the repository's
visible animated non-League fixture, giving both low- and higher-output-rate
normal workloads.

| Arm | Writer max | Flush max | Slow writes | Full-ring failures | Catch-up ticks | Rust + FFmpeg CPU | Wall overhead |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| normal-1 | 77.1 us | 11.5 us | 0 | 0 | 38 | 2.766 s | 0.406 s |
| normal-2 | 77.1 us | 9.6 us | 0 | 0 | 0 | 3.094 s | 0.563 s |
| animated-normal-1 | 320.9 us | 16.2 us | 0 | 0 | 2 | 2.219 s | 0.398 s |
| animated-normal-2 | 100.0 us | 17.8 us | 0 | 0 | 2 | 2.438 s | 0.747 s |

The largest normal writer call was 0.321 ms, over fifty times below one exact
60-Hz interval. All 14,400 scheduled ticks were submitted, completed and
muxed; maximum catch-up batch remained 2; unstaged admission failures were
zero; maximum in flight remained within 4; and the pending-source high-water
mark remained 1.

Each output was 1920x1080 H.264 at 60/1 FPS with AAC, exactly 3,600 decoded
video frames and 60.000 seconds of video/audio. Full-file decode diagnostics
were empty. Total sampled process CPU ranged from 2.219 to 3.094 seconds and
total process wall overhead from 0.398 to 0.747 seconds; neither was associated
with a full ring or a slow output call. The 38 catch-up ticks in `normal-1`
occurred with zero full-ring failures and a 77.1-us writer maximum, so they are
not attributable to mux backpressure.

## Excluded attempts

The same first root also preserves `normal-3` and the failed `normal-4`
attempt. During `normal-3`, the Notepad source stopped refreshing for about
15.6 seconds; its HWND then disappeared before `normal-4` could start.
Although `normal-3` preserved exact output accounting and decoded media, it is
excluded from the normal gate because source freshness was not continuous.
`normal-4` failed target resolution before recording. Neither attempt was
overwritten or counted.

## Conclusion

Package 9's short unstalled smoke had 12 transient no-slot retries, but the
required longer gate showed zero in all four accepted arms, including the
higher-bitrate animated fixture. There is no evidence that normal recording
repeatedly fills the ring or that fixed retry waiting causes material CPU,
catch-up, or stop cost. Adding a notification protocol now would increase
lost-wakeup and shutdown complexity without an evidenced benefit.

Package 11 is complete as an evidence-only `not triggered` decision. The
one-millisecond retry remains unchanged. This is non-League evidence and does
not satisfy QB-PERF-002 or M8.
