# QB-DIST-004 execution checkpoint

Feature: `QB-DIST-004`
ExecPlan: `docs/exec-plans/qb-dist-004-windows-installer-and-release.md`
Updated: 2026-11-24

## Current milestone

Milestones 1–3 are complete on the local Windows host. The functional unsigned candidate passed payload, guard, transaction, recovery and uninstall verification. Milestone 4 awaits the clean Windows and public release gates.

## Active unit

No active local implementation or verification unit. The next unit is the clean installed workflow with real sign-in and League gameplay.

## Completed

- Chose one per-user Windows x64 NSIS setup with a stable sibling payload, explicit historical beta eligibility, and guarded upgrade and uninstall.
- Built a local portable preview from clean source revision `de38126a1832` using the existing release build and stager. Both release executables come from that snapshot. The archive excludes the working tree's uncommitted UI changes.
- Added the same-revision build hook, static native CRT checks, inventory-bound coordinator, NSIS candidate template, extraction check and checksum output.
- Built and verified an unsigned Windows x64 package from clean local snapshot `d0aa19b6ba3a626b3b6bcb46b698c210f11325f2`. The package exits before changing installed state.
- Added shared installed-process use and exclusive setup/recorder-lease primitives. The installed application entry points hold shared use for their lifetime.
- Recovered the exact historical beta payload, including the generated uninstaller, without executing the archived setup. The generated installer source records the legacy path, registration and shortcut rules as inspection evidence.
- Implemented per-user setup, existing-version repair, journaled upgrade and rollback, and targeted uninstall. The outer launcher verifies the bound helper before executing it and installs Microsoft WebView2 when required before starting a transaction.
- Demonstrated that a file-ID handle could block an actual executable start while permitting a whole-directory move on local NTFS. A process already running from the executable remained detectable while the barrier was held.
- Exercised fresh installation, repair, verified upgrades, exact-beta migration, undo, and uninstall through compiled fixture NSIS entry points and isolated HKCU keys.
- Built the functional unsigned Windows x64 setup from clean source revision `111869324dbe5e4e5b5e8329e95b18ac26c3c0b7`. The app and recorder reported the same revision and release version, native import inspection passed, and all extracted members matched the exact staged payload and setup envelope.
- Installed the exact production payload in a fresh fixture and upgraded the exact archived beta to the same production payload in a second fixture. Both passed interrupted repair, executable revision and version checks, installed media processing, and uninstall with data preserved.

## In flight

None.

## Remaining

Complete the required clean-machine real match-to-clip and beta upgrade once the relevant dependencies pass. Verify the WebView2 download/install path on a supported system that lacks it, and exercise the separate Windows session and actual sign-in cases. Prepare signed download assets, complete the corresponding-source materials and verify the distribution links during public promotion.

## Verification

2026-11-16 on the Windows development machine:

- `cargo build --release --locked --manifest-path recorder/Cargo.toml` and `npm run desktop:build --prefix app -- --ci -- --locked` passed in the isolated committed snapshot. The frontend type check and production build passed through Tauri, and `npm run test --prefix app` passed all 602 tests.
- `stage_release.ps1` and `verify.ps1` verified both siblings and the full locked r6 media runtime. Extracted ZIP inventory and SHA-256 comparisons passed for all 13 files. From the extracted ZIP, `smoke.ps1` passed generated-media encode/probe/clip/thumbnail/full-decode with system media tools removed from PATH. Isolated `recorder.exe --diagnose` resolved the adjacent r6 runtime, passed H.264 High NVENC diagnostics and found WASAPI loopback.
- `dumpbin /DEPENDENTS /HEADERS` confirmed both images are x64, the desktop has no application DLL dependency, and the preview recorder imports `VCRUNTIME140.dll`. The ZIP documents the installed Microsoft VC++ x64 Redistributable and WebView2 requirements. The installer design puts static CRT linkage and import inspection at the release build gate.
- Local artifact `build/release/preview-de38126a1832/Chronobreak-0.1.0-preview-de38126-windows-x64.zip`, 44,293,319 bytes, SHA-256 `de8f5c0209f2c6585a7b439cfb34df09779e6c78bf91bfb20fe7e4c529cf7e9a`. Generated media smoke evidence is under `build/release/preview-de38126a1832/media-smoke/`.
- The new installer build, interactive preview workflow, clean-machine install, real League match-to-clip, upgrade, and uninstall were not run.
- 2026-11-24: Compiled an isolated NSIS fixture and inspected the expected package paths offline. A disposable executable opened to deny new launches while allowing delete sharing could be renamed on the same volume. The file ran after release. Acquiring the same handle while a fixture image was already running also succeeded, confirming the need for a separate running-image check. The recorder-control suite passed the shared/exclusive maintenance, repair-marker and recorder lease checks.
- 2026-11-24: The historical NSIS artifact matched the recorded `d8cbb06d6825b7059a0c5a5dfaeb11bc29e631e0227c601cb1c4dc3b442d6e97` SHA-256. Extracted archive inventory is in `build/verification/qb-dist-004/beta-49955071/archive-inventory.json`. The historical setup was never executed on the development profile.
- 2026-11-24: Recompiled the historical NSIS source byte-for-byte from the archived app and existing runtime. Reconstructed `uninstall.exe` from the original archive's stub, icon patches and nested uninstaller data, and compared it with NSIS compiler export. Both were exactly 79,377 bytes with SHA-256 `6087acd7ba48cc909fb17ca9e784c254a684a373266f6fc23ae43f96918d457e`. The coordinator accepted the 11 exact historical files and refused an added sentinel without changing it. [NSIS uninstaller writer](https://github.com/NSIS-Dev/nsis/blob/master/Source/exehead/exec.c)
- 2026-11-24: Restart Manager found a running disposable image before the launch barrier was held. Its results became unavailable or reported contention while the barrier was held. A separate disposable-process test identified the existing mapped image by its process path under the barrier, and the fixture cleared only after actual exit. The production process check remains pending.
- 2026-11-24: A separate recorder-control child process holding shared installed use denied exclusive maintenance until it exited. After exit, setup acquired both disposable leases. Recorder-control all-target tests and strict Clippy passed.
- 2026-11-24: `tools/windows_installer/build_candidate.ps1` passed using a clean Windows snapshot, locked dependencies and the existing r6 runtime. PowerShell 7, MSVC `dumpbin` and 7-Zip are local build prerequisites. The two release executables reported the same revision and version. Native dependency scans passed for both executables and the coordinator. The final unsigned NSIS archive had 11 exact payload files plus its manifest, envelope and coordinator. A silent run returned exit code 10 before install. Installed WebView2 and the tool-free workflow remain unverified.
- Local artifact `build/windows-installer/verified-d0aa19b6ba3a/Chronobreak-0.1.1-x64-local-setup.exe`, 33,118,420 bytes, SHA-256 `c2d7e197512bd2fde2fcc27823c0dfa2954dec80a11b4a1f630ca8314292e604`. `smoke.ps1` passed encode, probe, seek, clip, thumbnail and full-decode from the extracted runtime with system tools removed from PATH. The extracted coordinator refused the historical fixture containing extra material. Media smoke output is in the artifact directory under `media-smoke/`.
- 2026-11-24: `verify_installation.ps1 -ArchivedBeta <recovered exact payload>` passed WebView2 prerequisite failure, fresh setup, same-version repair, both lease contention cases, process interruption around the moves and metadata writes, repeated recovery, uninstall retry, typed Run ownership, fixture media/configuration preservation, MSI/custom/foreign refusal, and exact historical beta rollback and upgrade. Initial evidence is under `build/windows-installer/verify-2810b94ba1c345f3b25745adcb33c926`. The isolated compile profile differs from production only for fixture paths, registry identities, and fault hooks.
- 2026-11-24: Final disposable matrix passed missing, modified and extra payload refusals, package identity failures for the helper, uninstaller and manifest, and rejection of an unsigned WebView2 stand-in. It passed fresh install, both maintenance leases, an actual locked executable, interruption before and after renames and metadata writes, rollback and idempotent repair, verified shortcut target, downgrade/MSI/custom/foreign/junction refusal, exact beta rollback and upgrade, and interrupted and completed uninstall with recording, clip, configuration and Run-value preservation. Absent, owned and foreign typed startup registration cases passed. Evidence: `build/windows-installer/verify-dcbc1b70f9cc43a79a0cd81a9c24fcb6` and `build/windows-installer/final-all-fixture-verification.log`.
- 2026-11-24: Coordinator unit tests and strict Clippy for production and fixtures passed. The NTFS test detected a real running image while the file-ID barrier was active, blocked a new launch, allowed moving the whole folder, and verified launch after release. The project-wide `tools/ci/check-rust.ps1` rerun passed all suites, including the desktop library timing test that failed once and then passed when rerun separately. Frontend type checking, all 643 tests, production frontend build and the opt-in locked media runtime test passed.
- 2026-11-24: Final local artifact `build/windows-installer/verified-111869324dbe/Chronobreak-0.1.1-x64-local-setup.exe`, 33,678,932 bytes, SHA-256 `fcccdb13b68d3b68c2178dd8cce483d10b73026d0e009ebf803d39f5baffa058`. The checksum, manifest, setup envelope and exact project-source ZIP are beside it. From the extracted setup, `recorder.exe --diagnose` passed synthetic H.264 High NVENC and WASAPI loopback using packaged FFmpeg with system media tools removed from PATH, and `--tray-smoke-test` exited successfully. Each used an isolated configuration, library and log directory.
- 2026-11-24: `verify_installation.ps1 -ProductionPayload <extracted payload>` passed fresh install, stopped setup after moving the prior tree, observed the stable folder absent, recovered by rerunning the same installer, verified all installed production member hashes and both executable identities, then passed generated-media encode/probe/seek/export/thumbnail/full-decode and uninstall. The same checks passed when started with `-ArchivedBeta <exact payload>`, including legacy registry retirement. The isolated installer shared the production NSIS and Rust paths with fixture-only directories, registry keys and fault hooks. Evidence is in `build/windows-installer/verify-177ca801b2ff4bcd8c7d21830e9d64a6`, `build/windows-installer/verify-2def7871ee494ce28600fee67210d9e4`, and the `real-payload-*.log` files beside them.
- 2026-11-24: The archive verifier passed the final unsigned installer and refused a disposable unsigned archive whose envelope claimed signed promotion. The signing order is documented in `tools/windows_installer/README.md`. Positive verification of an actual signed installer awaits release credentials.

## Deviations

None. The historical preview was produced with existing tooling before installer implementation.

## Decisions

The installer will preserve the historical default per-user path and positively identify the historical NSIS beta. MSI and ambiguous layouts will receive an explicit safe migration failure. The portable preview remains unsigned and local.

## Blockers

QB-DIST-002 still needs manual supervision and login acceptance. QB-DIST-003 local implementation and verification passed and its installed acceptance remains pending. A clean supported Windows installation without the development prerequisites is still needed for the machine-level matrix, actual alternate-session/logon and online bootstrapper checks, and real League/NVIDIA acceptance. The local fixtures exercise exact archived files with isolated registry copies.

## Next action

Use the recorded 0.1.1 artifact on a clean supported Windows x64 test machine and execute the immutable plan's installed workflow after QB-DIST-002 and QB-DIST-003 acceptance. Apply the existing NVIDIA gameplay performance gate, signing and publication checks before public promotion.
