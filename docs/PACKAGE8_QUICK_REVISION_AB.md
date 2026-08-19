# Package 8: Quick Revision A/B

Date: 2026-08-18

## Question and answer

This comparison asks whether the immediate native-backend audit follow-up has
observable benefits relative to the exact pre-Package-8 code.

Yes, for the large deterministic effects:

- the real tray/service process retains 14 fewer steady threads, from 25 to
  11, in every sampled arm;
- native session startup reached its post-initialization marker 16.30 ms
  sooner on average, a 6.78% reduction, with non-overlapping four-arm ranges;
- the three-second service smoke used 0.54 MiB less mean private memory.

The short native recording arms also moved in the favorable direction for CPU,
wall time, and private memory. Their individual ranges overlap, however, so
they do not prove a whole-recording CPU improvement. A longer experiment is
required for that narrower claim.

## Method

- Baseline: immutable archive of `73c28386e15ff67734e99c0aba1adea361920f73`.
- Current: the Package-8 working tree.
- Both were compiled with the same Cargo release profile and shared dependency
  cache. The copied executables were hash-pinned before testing.
- Fixture: animated 1920x1080 borderless exact-HWND window.
- Runtime: the identical pinned
  `queueback-ffmpeg-8.1.2-windows-x86_64-r6` runtime.
- Native recording order: baseline, current, current, baseline. Each arm ran
  for ten seconds and was sampled every 200 ms.
- Service order: baseline, current, current, baseline. Each tray smoke arm ran
  for three seconds and was sampled every 50 ms.
- Startup order: baseline, current, current, baseline, current, baseline,
  baseline, current. Each arm recorded one exact second; the
  post-`NativeRecorderSession::start` marker was polled every 5 ms.
- Every native output had to reconcile its exact 60 Hz frame count and pass a
  complete decode with the pinned FFmpeg.

Raw ignored evidence root:
`evidence/package8-quick-revision-ab/20260818-181143`.

The first service attempt is retained under `service-runtime-abba`; it exited
before runtime construction because the copied executable did not initially
have a packaged runtime beside it. It produced no measurement. The accepted
`service-runtime-abba-retry` placed the same locked runtime beside both
hash-pinned executables and passed all four arms.

## Native 10-second ABBA

All four arms produced exactly 600 scheduled, submitted, completed, and muxed
frames with exact ten-second media time and full decode.

| Metric | Baseline | Current | Current minus baseline |
| --- | ---: | ---: | ---: |
| Aggregate sampled CPU over two arms | 0.937500 s | 0.859375 s | -0.078125 s (-8.33%) |
| Mean process wall time | 10.4548 s | 10.4004 s | -54.39 ms (-0.52%) |
| Mean maximum probe private memory | 131.676 MiB | 130.170 MiB | -1.506 MiB (-1.14%) |
| Mean maximum mux private memory | 29.293 MiB | 29.295 MiB | +0.002 MiB |

The CPU result is five Windows CPU-accounting quanta in the favorable
direction, but baseline arms ranged from 0.390625 to 0.546875 CPU seconds and
current arms from 0.390625 to 0.468750. The overlap and two-arm sample size make
this corroborative, not proof of lower steady-recording CPU.

## Service-runtime ABBA

Every steady-state sample inside each accepted arm reported the same thread
count for its revision: 25 for baseline and 11 for current.

| Metric | Baseline | Current | Current minus baseline |
| --- | ---: | ---: | ---: |
| Steady process threads | 25 | 11 | -14 (-56.0%) |
| Configured Tokio workers | 16 | 2 | -14 (-87.5%) |
| Mean private memory | 7.478 MiB | 6.940 MiB | -0.538 MiB (-7.19%) |
| Mean three-second CPU | 0.148438 s | 0.085938 s | -0.062500 s (-42.11%) |

The thread result is conclusive for this fixture and exactly matches the
configured-worker delta. The short CPU and memory results support the change,
but should not be extrapolated to active-recording percentages.

## Native startup micro-ABBA

All eight arms produced exactly 60 frames and fully decoded. All four current
startup measurements were below all four baseline measurements.

| Metric | Baseline | Current | Current minus baseline |
| --- | ---: | ---: | ---: |
| Mean post-session-start marker | 240.438 ms | 224.134 ms | -16.304 ms (-6.78%) |
| Median post-session-start marker | 239.492 ms | 224.079 ms | -15.414 ms (-6.44%) |
| Observed range | 232.890-249.877 ms | 217.322-231.058 ms | no overlap |
| Mean total one-second arm | 1264.772 ms | 1251.262 ms | -13.510 ms (-1.07%) |

This result is consistent with removing the temporary production NVENC probe
session. It is still a small local non-League sample, not a general startup
service-level guarantee.

## What the A/B can and cannot prove

The tests prove the 14-thread process reduction and demonstrate a repeatable
short-fixture native startup improvement without any media regression. They
also show no material short-run CPU, memory, or wall-time regression.

The steady-recording changes are intentionally small: four synchronous file
metadata queries per second and normal-path callback lock/broadcast work were
removed. Two ten-second arms per revision do not have enough resolution to
attribute their expected whole-process CPU effect. A useful next gate would be
at least four 60-second arms per revision, preferably interleaved, while also
collecting file-I/O events and callback notification counts.
