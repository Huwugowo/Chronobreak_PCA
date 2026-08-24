# Package 7: Final Non-League Recorder Acceptance

Date: 2026-08-18

Scope: PC-B recorder remediation through revision `12ec1de`, using the pinned
FFmpeg r6 media runtime. This report closes the locally available non-League
work. It does not claim League impact, M7 formal completion, an M8 backend
winner, or permission to remove either recorder backend.

## Decision

Keep Packages 0-6. The complete available static suite passed, fresh native
WGC failure and lifecycle fixtures exited deterministically, a 240-second
release recording reconciled every scheduler/encoder/mux count, and matched
preliminary A/B runs passed in both orders. Package 6 was correctly triggered
by its profile gate and remains deliberately bounded to write coalescing.

The current exact-CFR recording contract remains the replay baseline. The
replay ideas strengthen the need for explicit segment offsets and
timestamp-aware mux experiments, but do not justify changing timestamps, GOP
cadence, or media format during this remediation.

## Static acceptance

The following commands passed from the repository root:

```powershell
cargo fmt --manifest-path recorder/Cargo.toml --all -- --check
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
git diff --check
```

The test result was 113 library tests and one binary test passed, five
environment/profile tests ignored, and only the unavailable R11 empirical
fixture filtered. All examples compiled. No R11 result was fabricated.

## Fresh native lifecycle and failure matrix

Ignored runtime evidence root:
`evidence/package7-final-20260818-152833`.

| Fixture | Result | Important observations |
| --- | --- | --- |
| Resize, minimize/restore, occlusion | Pass, 30.000 s | 1,800 due/submitted/completed/muxed ticks; four WGC state configurations, three converter recreations, zero slot-admission retries |
| Target close after about 4 s | Expected failure | Explicit `native WGC target closed while recording`; bounded exit; partial MP4 decodes with 227 video frames and 3.783333 s aligned audio/video |
| Injected 500-ms encoder stall | Pass, 10.000 s | 600/600/600/600 counts; 21 no-slot retries; maximum scheduler batch two; exact A/V duration |
| Injected NVENC failure after 120 frames | Expected failure | Exact injected cause surfaced; bounded exit; partial MP4 decodes with 120 frames and 2.000 s aligned audio/video |
| Mux child killed after about 4 s | Expected failure | Precise output/mux-sink error and bounded shutdown; the 1,277-byte partial fragment is identifiable but not playable, as expected for this forced kill |

The combined resize fixture commanded a 1600x900 resize, a three-second
minimize, restore to 900x700, and five seconds of occlusion. It recorded 1,619
admitted source frames plus the initial handoff, replaced 170 pending frames,
and used a maximum of one pending frame. Four stale worker frames were closed
and accounted. These figures preserve the two-surface source bound and the
newest-source-per-tick rule.

## Media and replay baseline

The 30-second lifecycle file and 10-second stall file both start at zero, use
video time base `1/15360` and audio time base `1/48000`, contain exactly 60
video frames per second, and have no PTS or DTS regressions. Their video and
audio streams fully decode.

The lifecycle file contains 1,800 frames, 15 keyframes at a fixed 120-frame
interval, 15 `moof` and 15 `mdat` boxes, and one `moov`. The stall file
contains 600 frames, five keyframes with the same cadence, five `moof` and
five `mdat` boxes, and one `moov`. Sampled first/middle/last decoded hashes are
different in both files.

The pinned r6 mux process logs an FFmpeg deprecation warning about unset raw
H.264 packet timestamps. It does not invalidate these files, whose decoded
timeline and counts are exact, but it is a real future compatibility risk.
Explicit packet timestamps belong in the separate timestamp-aware mux/replay
experiment rather than an unattributed change here.

## 240-second release soak

The animated 1920x1080 fixture produced `soak-240.mp4`:

- 14,400 due, submitted, completed, and muxed frames;
- exact 240.000-second video and audio durations, zero PTS/DTS regressions,
  120 keyframes at a 120-frame interval, and full video/audio decode;
- 14,372 unique decoded frame hashes;
- zero no-slot retries, zero unstaged completions, maximum three in-flight
  encoder submissions, maximum scheduler batch two, and 20.5724-ms maximum
  lateness;
- 14,367 source arrivals, 14,366 admitted after the initial handoff, 1,844
  pending replacements, zero worker discards, no WGC recreation, one state
  configuration, and 12,522 source copies.

Over approximately 240 sampled seconds, the recorder used 16.3594 CPU seconds
(6.824% of one core, or 0.4265% of this 16-logical-processor machine). Its
private memory averaged 131.951 MiB in the first 30 samples and 133.420 MiB in
the last 30, peaked at 133.422 MiB, and varied only 0.027 MiB across the last
30 samples. Working set finished at 61.983 MiB and peaked at 61.984 MiB.
Handles and threads did not grow: last-30 averages were lower than first-30
averages.

The mux child used 0.4531 CPU seconds (0.189% of one core), finished near
16.653 MiB private memory, and peaked transiently at 29.227 MiB private and
27.180 MiB working set. Its handle and thread counts were stable.

The first background GPU collection attempt was invalid because its sampler
did not complete; no result is attributed to it. A fresh 35-second release arm
then produced 2,100 exactly reconciled frames and six valid five-second GPU
samples. The recorder's summed GPU-engine counter averaged 61.619% and peaked
at 67.755%; the mux child registered zero. Counter semantics and the small
sample count make this corroborative rather than a performance winner claim.

## Matched preliminary A/B

Both backends used the same animated 1920x1080 borderless exact-HWND fixture,
60-second arms, a 15-second cooldown, and the pinned r6 runtime. Both run roots
reported `PASS`:

- `evidence/preliminary-backend-ab/20260818-package7-ffmpeg-native-154200`
- `evidence/preliminary-backend-ab/20260818-package7-native-ffmpeg-154600`

| Order | Backend | Machine CPU | Max combined private memory | Output bytes |
| --- | --- | ---: | ---: | ---: |
| FFmpeg then native | FFmpeg | 0.943953% | 239.691 MiB | 6,140,583 |
| FFmpeg then native | Native | 0.615188% | 160.723 MiB | 4,878,248 |
| Native then FFmpeg | Native | 0.593740% | 160.984 MiB | 4,873,384 |
| Native then FFmpeg | FFmpeg | 1.024856% | 235.500 MiB | 6,149,938 |

Native was 0.328765 and 0.431116 machine-CPU percentage points lower in these
two matched preliminary runs. That is useful PC-B evidence, not a League
result or an M8 selection. Both backends remain required.

The native harness files extend about 1.3 seconds past their requested arm,
while standalone `run_for` fixtures are exact. The same harness/lifecycle
offset exists in the earlier 10-second and 240-second baselines, so it is not a
new remediation regression; it remains an A/B harness alignment limitation.

## Baseline-to-current revision A/B

A later ABBA comparison measured the complete remediation set against Package
0's baseline under the identical 1920x1080 fixture and pinned runtime. Native
aggregate sampled CPU time was exactly unchanged across two 60-second arms per
revision; memory was within 0.326 MiB (0.20%). This confirms no measurable
whole-recorder native resource regression, but it does **not** establish a
whole-recorder CPU speedup. The FFmpeg-reference change is also inconclusive
at this sample size. See `docs/archive/pc-b-recorder-development/PACKAGE7_END_TO_END_REVISION_AB.md` for the
method, raw roots, baseline R11-build accommodation, and full interpretation.

## Package disposition

| Package | Exact delta and preserved contract | Verification and measured result | Disposition |
| --- | --- | --- | --- |
| 0: observability | Added source, scheduler, encoder, and mux counters without changing media behavior | Baseline fixtures made later count reconciliation possible | Keep |
| 1: invariant D3D11 state | Hoisted invariant video-processor state from the frame loop; preserved format, color state, and GPU-only conversion | Static/conversion tests and matched profiling passed; lower direct conversion CPU was recorded in its evidence | Keep |
| 2: control plane | Cached stable target/monitor identity and performs expensive monitor/DXGI discovery only on relevant bounds change | Resize/minimize/restore passed; close and cross-adapter changes remain explicit terminal outcomes | Keep |
| 3: startup recovery | Classifies pre-publication dynamic startup failures, cleans up, and retries; ownership and timestamp failures remain terminal | Recovery tests and lifecycle fixtures passed | Keep; active-session restart remains deferred |
| 4: source coalescing | Added one worker-owned latest-frame slot and copies only at a due tick; retained initial-frame tick-zero authority and two-surface bound | 240-second arm: 14,367 arrivals, 1,844 replacements, 12,522 copies, zero recreations/discards | Keep |
| 5: transactional ticks | Due inspection is read-only; the tick commits only after NVENC accepts it; retry is 1 ms, catch-up is at most two per pass, final catch-up is bounded | 100/250/500-ms arms and fresh 500-ms arm retained every tick and exact duration | Keep |
| 6: poller persistence | One bounded writer coalesces newest revisions for at most 250 ms and force-flushes final state outside the game-log lock | Gate triggered: 50-minute model went from 400 writes and 200.161x amplification to 301 writes and 151.612x with identical final JSON | Keep; residual amplification needs real-match evidence before a format/journal change |
| 7: acceptance | No media-contract change | Complete static, lifecycle/failure, media, resource, soak, and both-order A/B gates passed | Pass for available non-League scope |

## Remaining gates and risks

- R11's empirical focused snapshot fixture and real League lifecycle/load
  evidence are unavailable in this PC-B repository. They remain mandatory
  before M7/M8/M9 conclusions.
- A window straddling PC B's differently driven displays can switch the
  largest-intersection monitor to the other GPU adapter. The recorder then
  fails explicitly instead of silently crossing adapters. This uncommon case
  is safe but not seamless; adapter rebinding requires an explicit immutable
  media-segment contract, match-relative offsets, and replay behavior.
- Active-session backend/device restart is deferred for the same reason. A
  retry cannot silently create unrelated zero-based media files.
- The pinned mux runtime's unset-packet-timestamp warning should be resolved in
  a measured timestamp-aware mux experiment, not by changing this accepted
  CFR baseline.
- Package 6's modeled 151.612x rewrite amplification is improved but still
  substantial. Real League log shape, storage latency, and crash-loss policy
  must decide whether journaling or a different persistence format is worth
  its additional complexity.
- GOP/fragment changes, replay indexes, retention, runtime replacement, and
  backend removal remain separate work with their own acceptance gates.

## Final boundary

The PC-B implementation plan is complete through Package 7. The remaining
work is repository/product integration and external League validation, not an
unimplemented local recorder-remediation package.
