# End-to-End Recorder Revision A/B

Date: 2026-08-18

## Question and answer

This comparison answers a narrower question than the preliminary backend A/B:
did the complete PC-B remediation change whole-recorder resource use relative
to its Package 0 baseline under the same non-League recording fixture?

For the native backend, the answer is **CPU-neutral within this measurement**.
Across two 60-second baseline arms and two 60-second current arms, sampled
recorder-plus-mux CPU time was exactly 10.84375 seconds in both groups. This
does not erase the Package 1 isolated CPU saving; it says that the entire
remediation set should not be represented as a single end-to-end CPU speedup.
It preserved the same aggregate CPU envelope while adding recovery, bounded
source ownership, transactional tick admission, and persistence correctness.

The FFmpeg-reference backend is also inconclusive: its current aggregate CPU
was 0.515625 seconds higher across about 119 sampled seconds (0.027067 machine
CPU percentage points). Two runs per revision and one-second process sampling
are insufficient to attribute that small shift to the revision rather than
ambient GPU/OS variation. It is neither a performance win nor a release
blocking regression.

## Method

- Baseline: `f885aba` (`Add recorder remediation baseline observability`).
- Current: `8e8d0a6` (`Document recorder remediation acceptance`).
- Order: baseline, current, current, baseline. Every paired run used native
  followed by FFmpeg-reference, with a 15-second NVENC cooldown.
- Fixture: fresh animated 1920x1080 borderless exact-HWND window for each
  paired run; each backend recorded for 60 seconds.
- Runtime: the identical pinned
  `queueback-ffmpeg-8.1.2-windows-x86_64-r6` runtime for every arm.
- Harness: the existing `tools/native_backend/run_preliminary_backend_ab.ps1`.
  It samples the Rust test process and its owned locked-runtime FFmpeg child,
  validates H.264/AAC, 1920x1080, 60 FPS, bounded duration, decoded frame
  count, and preserves ffprobe/resource evidence.

The baseline test build references an intentionally unavailable R11 empirical
capture through `include_str!`, so it cannot compile unchanged in this PC-B
worktree. The temporary detached baseline worktree disabled only that one
unavailable test with `#[cfg(any())]`. The test is not invoked by this lifecycle
benchmark; no recorder production path, lifecycle fixture, runtime, or harness
was changed. The baseline summaries therefore truthfully report
`recorder_dirty: true`. No empirical capture was invented.

## Valid evidence roots

| Sequence position | Revision | Evidence root |
| --- | --- | --- |
| 1 | Baseline | `C:\Users\Hugo\Documents\perso\chrnbrk\chronobreak-recorder-pc-b-baseline-f885aba\evidence\preliminary-backend-ab\20260818-160600` |
| 2 | Current | `evidence/preliminary-backend-ab/20260818-160847` |
| 3 | Current | `evidence/preliminary-backend-ab/20260818-161133` |
| 4 | Baseline | `C:\Users\Hugo\Documents\perso\chrnbrk\chronobreak-recorder-pc-b-baseline-f885aba\evidence\preliminary-backend-ab\20260818-161421` |

All four paired runs reported `CHRONOBREAK_PRELIMINARY_BACKEND_AB=PASS`.
Earlier interrupted/diagnostic roots are excluded from every calculation.

## Results

Metrics below aggregate both valid arms of a revision. Machine CPU is normalized
by the 16 logical processors observed by the harness. Private memory is the
mean of each arm's maximum combined Rust-plus-recording-mux private bytes.

| Backend | Revision | Sampled seconds | Total CPU seconds | Machine CPU | Mean maximum private memory |
| --- | --- | ---: | ---: | ---: | ---: |
| Native | Baseline | 120.216 | 10.84375 | 0.5638% | 160.564 MiB |
| Native | Current | 119.075 | 10.84375 | 0.5692% | 160.891 MiB |
| FFmpeg reference | Baseline | 119.125 | 19.015625 | 0.9978% | 235.111 MiB |
| FFmpeg reference | Current | 119.123 | 19.53125 | 1.0248% | 237.861 MiB |

| Backend | Current minus baseline | Interpretation |
| --- | ---: | --- |
| Native total CPU | 0.000000 s | Exactly neutral at the harness's one-second sampling precision. |
| Native machine CPU | +0.005400 percentage points | Caused by the 1.14-second shorter sampled window with identical CPU time; not a workload increase. |
| Native mean max private memory | +0.326 MiB (0.20%) | Arm ranges overlap; no material memory regression. |
| Native mean output bytes | -0.494% | Encoder-content variation, not a quality/bitrate decision. |
| FFmpeg-reference total CPU | +0.515625 s (2.71%) | Below attribution confidence with two one-second-sampled runs; no conclusion. |
| FFmpeg-reference machine CPU | +0.027067 percentage points | Same inconclusive shift. |
| FFmpeg-reference mean max private memory | +2.750 MiB (1.17%) | Overlapping arm ranges; no material memory regression. |

All eight recording arms produced H.264/AAC, 1920x1080, and a decoded frame
count above the harness's required 55 FPS bound. Native produced 3,677-3,693
decoded frames per arm and FFmpeg-reference produced 3,622-3,632. The native
arm's roughly 61.3-second lifecycle duration is present in both baseline and
current and is the known harness alignment characteristic, not a new change.

## How this changes the value judgement

The package-level measurements remain real:

- Package 1 removed 99.94% of processor-state configurations and reduced its
  matched isolated recorder CPU time by 36.56%.
- Package 2 reduced a service refresh by 24.96%.
- Package 4 removed source-rate-proportional copies (11.72% fewer copies in
  its physical 240-second fixture).
- Package 6 reduced modeled persistence writes by 24.75%, serialized bytes by
  24.25%, and write-service time by 12.97%.

The end-to-end result adds the critical constraint: these savings do not yet
sum to a measured whole-recorder CPU reduction. Some improvements occur on
infrequent control/persistence paths; other packages intentionally spend a
small amount of work to retain every CFR tick and perform bounded cleanup.
Their value is therefore a combination of proven local efficiency and proven
recording integrity, with no measurable native aggregate CPU or memory cost in
this fixture.

This is useful evidence, but it remains non-League and does not satisfy R11,
M7/M8/M9, or backend-removal criteria.
