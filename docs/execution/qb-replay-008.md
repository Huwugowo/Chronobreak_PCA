# QB-REPLAY-008 execution checkpoint

Feature: `QB-REPLAY-008`
ExecPlan: `docs/exec-plans/completed/qb-replay-008-replay-performance-baseline.md`
Updated: 2026-08-27

## Current milestone

Closed on 2026-08-26. This completed checkpoint preserves detailed implementation, measurement, disposition, and verification history.

## Active unit

None; the feature is complete.

## Completed

- Added sentinel-only production initialization, strict schema/root/fixture identity gates, bounded frontend/Rust event streams, current-generation frame/action attribution, route/request telemetry, export diagnostics, and deterministic fixture/matrix/report tooling.
- Built a five-bundle, approximately 615.7 MiB anonymized sentinel corpus from explicit prior QueueBack outputs and verified identity, AAC presence, hashes, probe, and full single-thread decode.
- Hardened the Windows Job Object collector, bounded optional telemetry, runner finalization, action/media identity across remounts, and matrix/analyzer validity checks.
- Accepted the authoritative observer-control matrix and 67-run/trial supported-path production-WebView baseline.
- Assigned measured opening, playback/rate, decoder-proof, and export findings to their canonical follow-up features.

## In flight

None.

## Remaining

None for QB-REPLAY-008. QB-REPLAY-009, QB-REPLAY-010, and QB-CLIP-004 own the recorded capability and performance dispositions; unsupported HEVC and unavailable decoder telemetry were preserved as limitations, not silently treated as passes.

## Verification

- Observer-control `r18` completed eight immutable minimal/full launches and passed all eleven gates with unchanged corpus hashes, no required loss, and exact semantic outcomes. Full/minimal CPU time was 111250/118156.25 ms; maximum private memory was 492806144/495816704 bytes; median seek latency was 300.15 ms in both arms.
- `docs/performance/evidence/qb-replay-008-baseline-20260826-r1/report.json` and `report.md` aggregate 67 valid immutable production-WebView runs/trials and passed matrix identity/order/cooldown/terminal/corpus verification plus aggregate analysis.
- The library useful-state median/p95 was 192.4/214.04 ms. Cold first-presented medians were 204.2 ms short, 538.5 ms external, 1681.9 ms representative, and 3075.3 ms long; payload backend medians were 1.1, 1.1, 4.3, and 8.0 ms. Representative/long seek medians were 261.15/282.75 ms; scrub-settle medians were 450.3/486.5 ms with zero server errors.
- Replay-benchmark tests passed 72/72; capture-benchmark tests 60/60; Tauri 46/46; frontend 16/16; recorder 125 library plus 3 binary with 5 declared hardware/profile ignores; media-runtime 10/10 plus packaged-runtime 1/1.
- Recorder/app/media-runtime formatting and strict Clippy, recorder check/release build, frontend check/build, Tauri desktop build, analyzer CLI, media-runtime preparation/atomicity/verify/smoke, portable release verification, feature-list schema/invariants for 60 items, and `git diff --check` passed at closure.

## Deviations

- Optional Windows GPU CIM sampling was disabled after a rehearsal query blocked for 5.18 seconds without an enforceable deadline; the baseline does not claim decoder-path proof.
- Component-local action/media IDs duplicated across viewer remounts; they were moved to session scope and the earlier matrix was superseded.
- The first collector shutdown path used slow managed process-module enumeration; Toolhelp/direct PInvoke plus explicit root-exit/finalization evidence replaced it.

## Decisions

- Treat the `r18` observer-control result as authoritative.
- Treat the 67 accepted supported-path trials as the canonical baseline.
- Preserve 8x playback failure, unsupported HEVC, and unavailable decoder telemetry as explicit follow-up inputs rather than weakening the matrix or inferring support.

## Blockers

None; all feature-owned gates were resolved at closure.

## Next action

None for QB-REPLAY-008. Reuse its accepted report and protocol only when a current feature's verification design requires a comparison.
