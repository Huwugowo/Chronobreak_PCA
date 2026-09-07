# QB-PERF-002 execution checkpoint

Feature: `QB-PERF-002`
ExecPlan: `docs/exec-plans/qb-perf-002-low-overhead-native-windows-capture-v3.md`
Updated: 2026-08-27

## Current milestone

Superseding-plan Milestone 5: valid target League capped/uncapped performance proof, support-matrix disposition, and final completion verification.

## Active unit

No product-code unit is active. `QB-PERF-002` is blocked until the required interactive Ryzen 5 5600X/RTX 4060/1920x1080 League environment is available for one valid capped baseline/capture pair and one independent uncapped baseline/capture pair.

Do not substitute current-workstation generated-window evidence, the historical native/external A/B, the accepted-invalid QB-PERF-001 report, or a different GPU/CPU target for that matrix.

## Completed

- Integrated the in-process exact-HWND WGC/D3D11/NV12/direct-NVENC H.264 High recorder and one FFmpeg audio/mux child.
- Retired the FFmpeg-driven WGC recorder, backend selector/candidate retry, external runner, and unvalidated HEVC/profile/AMF/QSV product claims. There is no automatic or explicit cross-backend fallback.
- Preserved the paired packaged-runtime/build/staging, recorder lifecycle, collision-suffixed bundle discovery, Live Client polling, finalization, and application contracts.
- Passed the recorded packaged-runtime/release checks, historical matched generated-window A/B, native source probe, native lifecycle/failure matrix, and accepted 1,800-second native arm described below.
- Preserved the original immutable dual-backend plan as history and finalized the reviewed self-contained native-only superseding plan `docs/exec-plans/qb-perf-002-low-overhead-native-windows-capture-v3.md`.

## In flight

No partial implementation is in flight. Later QB-REPLAY-012 native mux/marker/finalizer work may share recorder files, but it does not satisfy or alter this feature's missing target League matrix.

## Remaining

- Run one valid capped baseline/capture pair and one valid uncapped baseline/capture pair on the required Ryzen 5 5600X/RTX 4060 League setup.
- Pass every unchanged capped frame/resource gate and disposition every substantial capped or uncapped finding.
- Keep unimplemented native AMD/AMF and Intel/QSV adapters explicitly unsupported; draft `QB-PERF-003` and `QB-PERF-004` own any future implementation plus physical validation.
- Complete the native support matrix, architecture/operator inspection, packaged-runtime/project-wide verification, and canonical evidence update.

## Verification

Restart assumptions from the 2026-08-24 checkpoints:

- Recorder all-target/all-feature tests passed: 125 library tests and 3 binary tests; 5 hardware/profile fixtures explicitly ignored. Recorder formatting, all examples, and strict all-target/all-feature Clippy passed.
- Tauri tests passed 34/34 with strict Clippy; frontend type checking and production build passed.
- Media-runtime tests passed 10/10 with strict Clippy; capture-benchmark tests passed 60/60.
- The audited r6 packaged-runtime contract, corrupt-input/atomic preparation, sanitized-PATH encode/probe/export/full-decode smoke, optimized application/recorder builds, portable release verification, recorder diagnostics, and isolated app startup smoke passed. Evidence remains under ignored `build/media-runtime` and `build/perf` roots.
- Native source probe `build/perf/queueback-native-source/20260824-110114` delivered 588 frames with zero reported source/consumer drops, pool recreations, or conversion failures and returned all four bounded NV12 slots.
- Matched 240-second native/external generated-window A/B evidence at `evidence/preliminary-backend-ab/20260824-113718` produced decodable H.264/AAC media. Native used 0.144873% machine CPU and 157.980 MiB maximum combined private memory; external used 0.396960% and 235.867 MiB. This is directional non-League evidence only.
- Native steady, resize, minimize/restore, and occlusion fixtures produced exact 600-frame/10-second H.264/AAC files; target-close and injected-NVENC-failure fixtures preserved exact decodable partials. Evidence roots are dated `20260824-120516` through `20260824-120744` under ignored `build/perf/qb-perf-002-native-fixture`.
- Accepted 1,800-second native arm `20260824-120801-steady-none-1d73e07c` produced 108,000 reconciled frames and exact 1,800-second streams with bounded counters. The original wrapper lost its in-memory formal resource series and timed out a full count/decode probe; only live resource checkpoints and strict 60-second decode windows are claimed.
- Current-tree diff hygiene passed at the recorded checkpoint.

No valid League matrix, negligible-impact claim, AMD/Intel implementation or hardware validation, or feature-completion claim is recorded.

## Deviations

The original approved plan retained a packaged external FFmpeg WGC backend as an explicit alternative and current AMF/QSV route. The 2026-09-01 product architecture decision and capability audit materially invalidated that design: the external path had no unique validated production obligation, duplicated capture/lifecycle ownership, and exposed only unvalidated codec/vendor combinations. The original plan remains unchanged as history; the versioned v3 plan is the current design.

The first 1,800-second wrapper discarded its in-memory resource samples after a fixed 60-second frame-count probe timed out. The wrapper was corrected, but the product owner accepted the completed arm without repetition and without treating the missing formal series/full-file decode as passed.

## Decisions

- Native WGC/D3D11/direct-NVENC H.264 High 1080p60 is the sole supported Windows production recorder path.
- FFmpeg remains the audio encoder, fragmented-MP4 muxer, packaged probe/export runtime, and fixture tool; it is not a Windows video-capture backend.
- Unimplemented AMD/AMF, Intel/QSV, HEVC, and non-High native combinations are unsupported, not unvalidated current product paths. Their draft follow-ups may change that only after implementation and full evidence.
- The accepted 1,800-second local arm is not rerun solely for the lost sample series. This does not waive the target League matrix or broaden hardware claims.
- QB-PERF-001 remains invalid-but-accepted diagnostic pre-change evidence; it is not a formal performance pass.

## Blockers

The formal comparison requires the specified interactive League environment and Ryzen 5 5600X/RTX 4060 target, which is unavailable on this workstation. This external prerequisite blocks completion; no current repository action can manufacture equivalent evidence.

## Next action

When the specified target environment is available, run `powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture_benchmark/run_qb_perf_002.ps1`, preserve every raw root, validate/finalize all four media outputs, analyze with the v3 command, and checkpoint every gate/finding before any completion claim.
