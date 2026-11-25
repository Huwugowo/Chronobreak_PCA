# Windows setup

Build an unsigned local version from a clean Windows x64 source checkout with
the locked media runtime staged. The maintainer build uses PowerShell 7, Rust,
Node, MSVC with `dumpbin`, Tauri's NSIS toolchain and 7-Zip.

```powershell
pwsh -NoProfile -File tools/windows_installer/build_candidate.ps1
```

The command prints the installer path and writes its SHA-256 checksum beside it.
Signing credentials are optional for this local build. For signed promotion,
sign the desktop and recorder before creating the payload manifest. Sign the
manifest-bound coordinator before compiling the uninstaller, sign the
uninstaller before hashing the envelope, then sign the final installer before
computing its published checksum. With the `signed` envelope mode,
`verify_candidate.ps1` also requires valid Authenticode signatures on the final
installer, desktop, recorder, coordinator, and uninstaller.

Setup installs for the current Windows account on native x64 Windows with a
local NTFS profile. Internet access is needed if WebView2 is missing. Close the
desktop and use **Quit** in the recorder tray
before updating or uninstalling. Run the same or a newer setup again if an
installation stops partway through. Setup will report which ownership or
integrity check blocked recovery if it cannot identify the previous or new
installation safely.

Uninstall from **Settings → Apps → Installed apps → Chronobreak**. Configuration,
recordings and clips stay where they are. For automation that needs the
uninstaller's exit code, use the standard NSIS method of executing a verified
copy from a private scratch directory with `/S _?=<installed directory>`. The
normal Windows launcher copies itself and starts a child; its outer process
returns only the result of starting that child.

Run the isolated Windows fixture suite from the source checkout. Pass
`-ArchivedBeta` with the recovered exact beta payload to include historical
migration and rollback. The suite uses generated directories and registry keys
under the current account, with no access to real recordings or the real
installation.

```powershell
pwsh -NoProfile -File tools/windows_installer/verify_installation.ps1
```

Pass `-ProductionPayload` the `payload` directory extracted by
`verify_candidate.ps1` to test the real desktop, recorder and media tools in
the isolated profile. With `-ArchivedBeta`, this exercises an exact beta
upgrade. Without it, it exercises a fresh install. Both paths check interrupted
repair, generated-media processing, and uninstall.

Public promotion also requires the clean-machine acceptance in QB-DIST-004,
the NVIDIA gameplay performance gate, a signed artifact, and verified public
download links for the installer, checksum and corresponding source.
