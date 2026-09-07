# QB-DIST-001 execution checkpoint

Feature: `QB-DIST-001`
ExecPlan: `docs/exec-plans/completed/qb-dist-001-packaged-media-tool-contract.md`
Updated: 2026-08-27

## Current milestone

Closed on 2026-08-12. This completed checkpoint preserves detailed runtime-selection, implementation, and verification history.

## Active unit

None; the feature is complete.

## Completed

- Replaced independent PATH/bare-name media-tool discovery with one shared paired media-runtime contract for recorder and app.
- Rejected the driver-incompatible generic FFmpeg 8.1.2 binary after real NVENC probes.
- Built and pinned QueueBack FFmpeg 8.1.2 from exact FFmpeg, nv-codec-header, AMF, and MSYS2 identities with the required Windows capture/conversion and H.264/HEVC NVENC/AMF/QSV surfaces.
- Embedded exact lock/file identity, finite capability validation, atomic offline staging, Windows Tauri resource mapping, sanitized-PATH smoke, and portable release verification.
- Kept missing-runtime degradation operation-scoped so otherwise valid recordings remain browseable/playable.

## In flight

None.

## Remaining

None for QB-DIST-001. QB-PERF-002 owns the diagnostics ABI, GPU-native capture graph, adapter selection, and performance proof; QB-DIST-002 and QB-DIST-004 own supervision/login startup and installer/clean-machine work.

## Verification

- Media-runtime unit tests passed 9/9 plus the real packaged-runtime contract.
- Corrupt-input/atomic preparation and verifier scripts passed.
- Sanitized-PATH generated encode/probe/clip/thumbnail/full-decode smoke passed at `build/media-runtime/smoke-20260812-132214-27a0cfdb`.
- Recorder diagnostics resolved runtime `queueback-ffmpeg-8.1.2-windows-x86_64-r1` and selected working H.264 NVENC on driver 566.03.
- Media-runtime, recorder, and Tauri formatting and strict Clippy passed; recorder/Tauri release executables and frontend production build passed.
- Portable release inventory/path isolation, an actionable missing-runtime fixture, 57 benchmark-tool regressions, 50-item feature-list schema/invariants, and diff hygiene passed.
- Closure hashes: `ffmpeg.exe` 33,892,864 bytes, SHA-256 `3bda8a8ec9517b872a9478efe9fe9b7d2bf165c823412db036cfc43f2bc990fd`; `ffprobe.exe` 33,681,920 bytes, SHA-256 `2b3c43c7757236c1152a70d8f37966fcef03b6829c06c6e854c88aa34ae84f4e`.

## Deviations

The initially selected Gyan FFmpeg 8.1.2 build advertised the required surfaces but required NVENC API 13.1/NVIDIA driver 610 or newer. Real H.264/HEVC probes failed on the supported RTX 4060 machine's 566.03 driver, so the generic binary was rejected.

## Decisions

Use a reproducible QueueBack source build pinned to FFmpeg `n8.1.2`/`38b88335f99e76ed89ff3c93f877fdefce736c13`, nv-codec-headers `n12.2.72.0`/`c69278340ab1d5559c7d7bf0edf615dc33ddbba7`, and AMF `v1.4.36`/`16f7d73e0b45c473e903e46981ed0b91efc4c091`. Production ignores PATH and the former `LEAGUE_REPLAY_FFMPEG` escape hatch.

## Blockers

None; all feature-owned gates were resolved at closure.

## Next action

None for QB-DIST-001. Extend this same paired runtime contract rather than creating another resolver.
