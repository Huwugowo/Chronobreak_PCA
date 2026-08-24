# QB-PERF-005 M6 native fixture evidence

Date: 2026-08-17

This is PC-B, non-League evidence for the standalone native recorder. The
tests used the installed Gyan FFmpeg 9.0 winget package only as the H.264
stream-copy/audio/MP4 utility. It is not the pinned media-runtime r5 binary,
so these results do not replace QB-PERF-002 attribution or the final PC-A
rebase and League comparison.

## Result

The M6 functional/failure matrix and required 240-second bounded fixture pass
on the provisional PC-B tree. The final path preserves the audited
performance invariant: a full four-slot encoder ring causes an immediate,
accounted recorder drop. The GPU submission worker does not wait for encoder
completion and no fifth texture, frame queue, or full-frame host copy exists.

## Functional matrix

- Animated steady/normal stop:
  `evidence/m6-native-mp4-post-fault-normal-20260817-144504`
  passed 600/600 frames, zero drops, maximum in-flight 3 and full A/V decode.
- Resize:
  `evidence/m5-native-mp4-animated-resize-r2-20260817-142416`
  passed 600/600 frames, zero drops, three WGC recreations and exactly four
  source-snapshot allocations for the initial size plus three accepted size
  changes.
- Focus loss/full occlusion:
  `evidence/m6-native-mp4-focus-occlusion-20260817-143747`
  passed 600/600 frames, 600 unique decoded hashes, zero drops and full A/V
  decode while a foreground Notepad window covered the fixture.
- Minimize/restore stress:
  `evidence/m6-native-mp4-accounted-visibility-stress-20260817-145237`
  passed 900/900 frames and full decode through five cycles. Four WGC pool
  recreations required only two converter recreations and three total
  persistent source-snapshot allocations. The scenario declared a five-frame
  recorder-drop ceiling; the observed count was zero. An earlier retained run
  demonstrates the audited overload result with one explicitly accounted
  slot drop rather than a GPU-worker wait.
- Target close:
  `evidence/m6-native-mp4-target-close-20260817-143836`
  returned an explicit 167-of-1200 short-run result 0.148 seconds after target
  close and preserved a fully decodable 167-frame MP4.
- Mux child/pipe failure:
  `evidence/m6-native-mp4-mux-child-kill-r3-20260817-144429`
  returned the exact FFmpeg sink-loss cause in 0.106 seconds and preserved a
  fully decodable 120-frame fragmented MP4. The encoder retains a 1 MiB
  syscall-saving buffer but separately observes progress-pipe closure on every
  output frame.
- Feature-gated terminal NVENC failure:
  `evidence/m6-native-mp4-injected-nvenc-failure-20260817-144951`
  injected failure before tick 480, returned the exact cause after 8.487
  seconds total and preserved a fully decodable 479-frame MP4. The seam is
  absent unless Cargo feature `native-failure-injection` is enabled and does
  not call an undocumented or invalid driver entry point.

## 240-second release fixture

Evidence:
`evidence/m6-native-mp4-steady-240-release-20260817-145635`

- Optimized release example, 1920x1080 at 60 FPS.
- Scheduled/submitted/completed/muxed: 14,400/14,400/14,400/14,400.
- Slot and unstaged tick drops: 0/0.
- Maximum NVENC in-flight submissions: 3 of the fixed maximum 4.
- Source arrivals: 3,784; source snapshot copies: 3,780.
- Decoded video frames: 14,400; unique decoded hashes: 3,803, or 15.846
  changing frames/second and 1.005 unique hashes per source arrival.
- Video/audio start: 0.000/0.000 seconds.
- Video/audio duration: 240.000/240.000 seconds.
- Full video and audio decode: pass.
- Output: 18,120,689 bytes.
- Recorder CPU: 1.562% of one core, 0.098% of the 16-logical-processor
  machine.
- Mux-only FFmpeg CPU: 0.137% of one core, 0.009% machine-wide.
- Recorder per-process GPU-engine sum: 60.042% average, 73% peak. Mux-only
  FFmpeg GPU-engine sum: 0% average and peak.
- Recorder working-set peak: 66,891,776 bytes.
- Recorder private-bytes peak: 138,870,784 bytes.
- Handles: 426-434; threads: 21-26.
- A single 4.62 MiB private-commit step occurred at 191 seconds alongside the
  second bounded source/converter recreation. From 195 through 239 seconds,
  private-memory slope was 0 bytes/second and working set remained
  63.789-63.793 MiB. This is a bounded recreation/cache step, not sustained
  growth.

`resources.csv`, `resource-summary.json`, `ffprobe.json`,
`full-decode.stderr.log` and `media-summary.json` preserve the raw and derived
evidence. The WMI GPU query was slow enough to cluster some nominal one-second
resource samples; endpoint CPU totals, extrema, process bounds and five-second
GPU observations remain usable, but this fixture is not a League performance
comparison.

## Static verification

- Default native unit subset: 30 passed.
- Native subset with all features: 30 passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- Release native MP4 example build: passed.
- Whole recorder suite: 80 passed, 1 environmental fixture failure.

The whole recorder suite retains one previously reviewed environmental failure:
the ignored empirical League client fixture is absent/empty on PC B. No League
capture data was fabricated.

## Follow-up

FFmpeg 9.0's progress pipe reported increasing frame and byte counts but kept
`out_time_us` at 128,000 even though ffprobe and full decode prove exact
240-second A/V streams. Native watchdog integration must use the independently
advancing source QPC, encoded-frame and mux-byte counters, and final media
timing remains an ffprobe gate. Investigate or relabel this auxiliary FFmpeg
field before treating it as a timeline metric.
