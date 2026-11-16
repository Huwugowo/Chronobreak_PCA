# QB-DIST-004 execution checkpoint

Feature: `QB-DIST-004`
ExecPlan: `docs/exec-plans/qb-dist-004-windows-installer-and-release.md`
Updated: 2026-11-16

## Current milestone

Planning handoff complete. Milestone 1 is next.

## Active unit

None. The installer implementation has yet to start.

## Completed

- Chose one per-user Windows x64 NSIS setup with a stable sibling payload, explicit historical beta eligibility, and guarded upgrade and uninstall.
- Built a local portable preview from clean source revision `de38126a1832` using the existing release build and stager. Both release executables come from that snapshot. The archive excludes the working tree's uncommitted UI changes.

## In flight

None.

## Remaining

Implement the release build hook and installer, then verify payload, contention, repair, and uninstall in fixture profiles. Complete the required clean-machine real match-to-clip and beta upgrade once both installation dependencies pass. Prepare the download assets and verify the distribution links.

## Verification

2026-11-16 on the Windows development machine:

- `cargo build --release --locked --manifest-path recorder/Cargo.toml` and `npm run desktop:build --prefix app -- --ci -- --locked` passed in the isolated committed snapshot. The frontend type check and production build passed through Tauri, and `npm run test --prefix app` passed all 602 tests.
- `stage_release.ps1` and `verify.ps1` verified both siblings and the full locked r6 media runtime. Extracted ZIP inventory and SHA-256 comparisons passed for all 13 files. From the extracted ZIP, `smoke.ps1` passed generated-media encode/probe/clip/thumbnail/full-decode with system media tools removed from PATH. Isolated `recorder.exe --diagnose` resolved the adjacent r6 runtime, passed H.264 High NVENC diagnostics and found WASAPI loopback.
- `dumpbin /DEPENDENTS /HEADERS` confirmed both images are x64, the desktop has no application DLL dependency, and the preview recorder imports `VCRUNTIME140.dll`. The ZIP documents the installed Microsoft VC++ x64 Redistributable and WebView2 requirements. The installer design puts static CRT linkage and import inspection at the release build gate.
- Local artifact `build/release/preview-de38126a1832/Chronobreak-0.1.0-preview-de38126-windows-x64.zip`, 44,293,319 bytes, SHA-256 `de8f5c0209f2c6585a7b439cfb34df09779e6c78bf91bfb20fe7e4c529cf7e9a`. Generated media smoke evidence is under `build/release/preview-de38126a1832/media-smoke/`.
- The new installer build, interactive preview workflow, clean-machine install, real League match-to-clip, upgrade, and uninstall were not run.

## Deviations

None. The preview was produced with existing tooling before installer implementation.

## Decisions

The installer will preserve the historical default per-user path and positively identify the historical NSIS beta. MSI and ambiguous layouts will receive an explicit safe migration failure. The portable preview remains unsigned and local.

## Blockers

QB-DIST-002 still needs its manual supervision/login acceptance. QB-DIST-003 still needs the installed settings implementation and verification. These block installer acceptance and publication. Same-revision candidate packaging can proceed independently.

## Next action

Implement Milestone 1 beginning with the Windows before-bundle staging hook. Build the recorder from the same clean revision as the desktop, verify the native imports and inventory, then wire the candidate NSIS payload and prove it by extraction.
