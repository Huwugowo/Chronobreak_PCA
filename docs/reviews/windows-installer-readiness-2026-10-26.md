# Windows installer readiness inspection — 2026-10-26

This is build and inspection evidence for `QB-DIST-004`, not installed-workflow
certification. Canonical lifecycle and dependencies remain in `feature-list.json`.
The inspected source revision is `28be415`; its working tree was clean when the
build started. Existing dependency/build caches were present. No installer was
executed, no registry setting was changed, and no user media was used.

## Build and payload evidence

`npm run tauri --prefix app -- build` passed on the development Windows machine.
It ran the frontend type check/production build and optimized Rust build, then
produced both configured installer formats. Tauri acquired and hash-validated its
WiX 3.14.1 and NSIS 3.11 build tools. This was not a fresh tool-free Windows VM.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `app/src-tauri/target/release/bundle/msi/League Replay_0.1.0_x64_en-US.msi` | 40,030,208 | `5c8bc8485e5ced3e130ff2442e524195fed8d0cfc5f64e4e0a7ede1fe380c132` |
| `app/src-tauri/target/release/bundle/nsis/League Replay_0.1.0_x64-setup.exe` | 29,459,471 | `d8cbb06d6825b7059a0c5a5dfaeb11bc29e631e0227c601cb1c4dc3b442d6e97` |

The MSI database was opened read-only through Windows Installer APIs. Its embedded
cabinet was expanded with Windows `expand.exe`, without invoking install actions.
The inspection retained File, Component, Directory, Media, RemoveFile, Registry
and Property tables and the extracted files under
`build/verification/qb-dist-004/payload-20261026/`; `result.json` contains the full
inventory, artifact hashes and table rows. The bounded inspection helper is
`build/verification/qb-dist-004/inspect_msi.py`.

The MSI contains 11 payload files: the main executable, its DLL, runtime manifest,
FFmpeg, ffprobe and six license/provenance files. All eight locked runtime files
matched `media-runtime/runtime-lock.json` by size and SHA-256; the embedded runtime
manifest matched that lock. The runtime identity is
`queueback-ffmpeg-8.1.2-windows-x86_64-r6`.
The separately staged runtime also passed
`powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/verify.ps1 -RuntimeRoot build/media-runtime/windows-x86_64`.

**`recorder.exe` is absent from the MSI payload and both generated installer
sources.** `tauri.windows.conf.json` maps only the staged media-runtime resource;
it has no recorder sidecar/copy rule. The separate portable release staging script
includes a recorder, but `tauri build` does not invoke that script. A passing
installer build therefore does not establish installed recording capability.
The NSIS payload was not independently extracted/hash-checked; its authored
payload rules and final artifact hash were inspected.

## Upgrade and uninstall ownership

Generated sources inspected:

- `app/src-tauri/target/release/wix/x64/main.wxs`
- `app/src-tauri/target/release/nsis/x64/installer.nsi` and `utils.nsh`

The MSI Directory table roots the payload under Program Files / `League Replay`.
Its RemoveFile table has only empty-directory removal rows for `INSTALLDIR`,
DesktopFolder and the product Start Menu directory. Its Registry table contains
four product install/shortcut values under HKCU
`Software\leaguereplay\League Replay`; it contains no login-startup registration.
The generated WiX source declares a major upgrade.

NSIS uninstall deletes the enumerated installed executable/DLL/runtime files and
uninstaller, then attempts nonrecursive install-directory removal
(`installer.nsi:766–795`). It removes verified product shortcuts and uninstall
registration (`797–833`) and the HKCU Run value named by `${PRODUCTNAME}`
(`835–841`). There is no implemented registration contract from `QB-DIST-002`
against which that last rule can yet be checked.

NSIS also exposes a “Delete app data” checkbox (`419–457`). When selected outside
update mode it recursively removes the roaming/local
`com.leaguereplay.desktop` directories (`843–859`). The generated source does not
preselect it. Same-format upgrades pass `/UPDATE` to the old uninstaller and skip
that deletion; migration from WiX uses the detected MSI uninstaller instead
(`307–354`). Source inspection is not execution evidence for any of these paths.

The shared configuration is under `LeagueReplay/config`, and the default media
root is `~/LeagueReplays`. Default uninstall rules do not explicitly target them.
However, the output root is user-selectable: preservation is not certified if a
library lies under an optionally deleted app-data directory. Release verification
must cover that overlap or change the app-data deletion policy. No destructive
test was performed against any existing profile or library.

## Unmet release gates

`QB-DIST-002` still owns installed recorder supervision, exact single-instance
behavior, intentional recording finalization, login registration and cleanup.
`QB-DIST-003` still owns the complete validated installed settings contract and
safe lifecycle application. Both are canonically not started. Their contracts
must be settled before finalizing the integration ExecPlan; this inspection does
not implement either dependency.

The installer integration must include the recorder in the approved installed
layout, validate one shared locked runtime, and check startup/uninstall ownership
against those dependency contracts. Then use a dedicated clean supported Windows
test machine/profile and sentinel-owned library to exercise fresh install,
repeated launch/login, a real match through browse/seek/export, an upgrade from
the beta, failure handling and uninstall. Compare configuration/media hashes and
registration before/after. A real clean-machine workflow and all upgrade/uninstall
procedures remain **not run**. The generated packages are inspection artifacts,
not release-certified installers.
