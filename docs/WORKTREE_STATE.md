# PC-B repository state

## Provenance

- Baseline source: `build/pc-b-native-m1-m2-m3-replay-scratch`.
- Baseline meaning: the prepared M1/M2 patch plus the audited PC-B M1/M2
  correction patch plus the M3 NV12 patch replayed cleanly.
- Media runtime: r6 is built and staged locally from the pinned FFmpeg 8.1.2
  sources and QueueBack WGC patch under `build/media-runtime/windows-x86_64`.
  Its packaged-runtime integration test and A/B preflight pass.
- Original scratch/evidence directories were not moved, renamed, or deleted.

## Completed provisional milestones

- M1: native prerequisites and exact same-adapter preflight.
- M2: exact-HWND Windows Graphics Capture source.
- M3: reusable GPU BGRA-to-NV12 conversion and fixed four-texture ring.
- M4: direct H.264 NVENC with four registered input/output/event slots,
  non-blocking submission and an ordered completion thread. See
  `docs/M4_NVENC_EVIDENCE.md`.
- M5: 60-Hz CFR scheduling, mux-only FFmpeg/audio integration and valid
  fragmented MP4 output. See `docs/M5_NATIVE_MP4_EVIDENCE.md`.
- M6: truthful functional/failure fixtures plus a 240-second bounded native
  run. See `docs/M6_NATIVE_FIXTURE_EVIDENCE.md`.
- M7 (PC-B portion): developer-only service integration, common lifecycle and
  health evidence, cancellation/worker ownership and publish-after-validation.
  See `docs/M7_NATIVE_LIFECYCLE_EVIDENCE.md`. The audited M7 exit still needs
  one actual League recording later because League must not be run on PC B.
- The integrated M7 lifecycle also passed a persistent 240-second release
  fixture with separately sampled recorder/mux resources, valid 60-FPS H.264 +
  AAC media and full decode. The run exposed and fixed misleading FFmpeg
  stream-copy time telemetry.
- The matched preliminary non-League backend A/B ran in both 240-second orders
  against a 1920x1080 animated exact-HWND fixture. All four arms passed media
  validation and full decode. Native averaged 62.3% less process-attributed
  CPU and 32.1% less peak combined private memory. See
  `docs/PRELIMINARY_BACKEND_AB.md`.

## Current milestone

- M7 service integration is implemented and all available non-League checks
  pass. The preliminary synthetic A/B favors native on CPU and memory, but its
  CFR scheduler counters and visual quality still need validation with actual
  game motion. Keep the developer selector and both paths; do not treat this
  fixture as the M8 migration decision or begin M9 removal.
- The repository-wide recorder review has a dependency-ordered implementation
  addendum at
  `docs/exec-plans/active/qb-perf-005-recorder-review-remediation.md`. It is
  subordinate to QB-PERF-005, changes no milestone status, and separates safe
  PC-B remediation from runtime-segmentation and destructive-retention work
  that require the complete repository.
- Remediation Package 0 is implemented and verified. Its injected-stall
  baseline confirms per-frame processor-state configuration, per-admitted-frame
  snapshot copying, overdue source starvation, and frame-count-derived duration
  loss. See `docs/PACKAGE0_RECORDER_OBSERVABILITY_EVIDENCE.md`.
- Remediation Package 1 is implemented and verified. Invariant D3D11 processor
  state is now configured only at initial creation and accepted size changes.
  Resize/minimize/restore media gates pass, and the matched 60-second check
  reduced recorder CPU time by 36.56% with neutral-to-better GPU and memory.
  See `docs/PACKAGE1_D3D11_PROCESSOR_STATE_EVIDENCE.md`. Package 2 is the next
  implementation gate.

## Gates that remain external

- A working `Win32_PerfRawData_PerfProc_Process` class and performance-counter
  access for the QB-PERF static preflight. See
  `docs/PC_B_NON_LEAGUE_VERIFICATION.md`.
- The R11 empirical `live-client-capture-20260730-110603.json` parser fixture;
  this is the only deferred finding from the supplied recorder review.
- QB-PERF-002: remove only the diagnostic heartbeat from the FFmpeg reference
  backend and pass three independent 240-second captures while retaining the
  synchronized `frame_seq` fix.
- Actual League lifecycle acceptance and League-specific A/B validation, once
  the actual game is available again.

## Verification rule

Only commands actually run against this repository count. Scratch evidence can
establish provenance, but new milestone claims require fresh local results.
The current session handoff is `docs/HANDOFF_PC_B_NATIVE_RECORDER.md`.
