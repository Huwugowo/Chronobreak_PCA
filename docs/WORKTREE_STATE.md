# PC-B repository state

## Provenance

- Baseline source: `build/pc-b-native-m1-m2-m3-replay-scratch`.
- Baseline meaning: the prepared M1/M2 patch plus the audited PC-B M1/M2
  correction patch plus the M3 NV12 patch replayed cleanly.
- Media runtime: r5 source/lock contents only. The expected pinned `ffmpeg.exe`
  and `ffprobe.exe` binaries are absent.
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
- A matched 240-second preliminary non-League backend A/B harness is prepared
  in commit `9ef92f8`. It compares both backends against one 1920x1080
  animated exact-HWND fixture and validates process ownership, media and full
  decode. It has not run because the exact r5 binaries are absent. See
  `docs/PRELIMINARY_BACKEND_AB.md`.

## Current milestone

- M7 service integration is implemented and all available non-League checks
  pass. The next performance milestone is M8, but its controlled League A/B
  benchmark is intentionally external to PC B. Until the exact PC-A tree and
  League fixture are available, keep the developer selector and both paths;
  do not make the migration decision or begin M9 removal.
- Before M8, the prepared non-League A/B may be run as preliminary engineering
  evidence once the exact locked r5 runtime is staged. A useful preliminary
  result still does not complete M8.

## Gates that remain external

- Fresh whole PC-A repository/ZIP.
- Pinned r5 `ffmpeg.exe` and `ffprobe.exe` binaries.
- A working `Win32_PerfRawData_PerfProc_Process` class and performance-counter
  access for the QB-PERF static preflight. See
  `docs/PC_B_NON_LEAGUE_VERIFICATION.md`.
- The R11 empirical `live-client-capture-20260730-110603.json` parser fixture;
  this is the only deferred finding from the supplied recorder review.
- QB-PERF-002: remove only the diagnostic heartbeat from r5 and pass three
  independent 240-second captures while retaining the synchronized `frame_seq`
  fix.
- Actual League lifecycle acceptance and League-specific A/B validation (run
  later by the user, not on PC B).

## Verification rule

Only commands actually run against this repository count. Scratch evidence can
establish provenance, but new milestone claims require fresh local results.
The current session handoff is `docs/HANDOFF_PC_B_NATIVE_RECORDER.md`.
