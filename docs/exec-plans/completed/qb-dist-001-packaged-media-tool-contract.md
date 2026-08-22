# QB-DIST-001 Packaged Media Tool Contract

Status: complete on 2026-08-12.

## Purpose

QueueBack must stop depending on whichever `ffmpeg` happens to be on a machine. The recorder and app will use one packaged, exactly identified Windows FFmpeg/ffprobe pair that already contains the upstream Windows Graphics Capture, D3D11 conversion, and NVIDIA/AMD/Intel encoder surfaces needed by `QB-PERF-002`.

This feature is deliberately narrow. It establishes and packages the runtime contract; it does not change screen capture, start the legacy GDI recorder, build an installer, or claim a performance improvement. Its completion removes the last dependency before the actual capture-performance implementation.

## Relevant current architecture

- `recorder/src/encoder.rs::Ffmpeg::resolve` tries `LEAGUE_REPLAY_FFMPEG`, then `<recorder>/resources/ffmpeg/ffmpeg.exe`, then `ffmpeg.exe` from `PATH`. `command_works` checks only a successful `-version` exit. Recorder diagnostics, encoder selection, capture, and finalization all subsequently use the selected `Ffmpeg.path`.
- `app/src-tauri/src/clip_export.rs::{run_ffmpeg,generate_thumbnail}` spawn bare `ffmpeg`; `app/src-tauri/src/library.rs::probe_duration_ms` spawns bare `ffprobe`. These paths can differ from the recorder's binary.
- `app/src-tauri/src/lib.rs::AppState` has no media-tool state. A runtime failure must not prevent browsing or playback, because those operations do not intrinsically require FFmpeg.
- `app/src-tauri/tauri.conf.json` declares no media resources. Tauri v2 supports Windows-specific configuration and resource mapping into `$RESOURCE`; on Windows installers that directory is under the application's `resources` directory.
- The recorder and Tauri app are separate Cargo packages with separate lockfiles and no root workspace. A small path-dependency crate can still be consumed by both.
- `docs/development/VERIFICATION.md` currently lists system FFmpeg/PATH as a prerequisite and does not define an offline packaged-runtime check.
- `QB-PERF-002` already requires an audited FFmpeg 8.1.2 runtime, rejects arbitrary PATH binaries, and plans to extend the same runtime with a small versioned QueueBack diagnostics patch.

## Scope / non-goals

In scope:

- pin and stage one Windows-x86_64 FFmpeg 8.1.2 distribution;
- define a versioned on-disk layout and immutable runtime lock;
- implement one shared resolver and finite validator for both binaries;
- route every recorder/app FFmpeg or ffprobe call through that resolved pair;
- configure the Windows Tauri resources and a current portable release layout;
- package exact licenses, notices, and source/build provenance;
- prove operation without a system FFmpeg or ffprobe on PATH.

Not in scope:

- Windows installer, upgrade/uninstall, login startup, or recorder supervision (`QB-DIST-002`/`QB-DIST-004`);
- WGC command construction, adapter selection, the QueueBack FFmpeg patch/diagnostics ABI, or any capture/performance claim (`QB-PERF-002`);
- a real League run or any GDI capture during this feature;
- runtime downloads, auto-update, ffplay, code signing, or a vendor SDK installation;
- mutation, migration, decode corruption, or destructive testing of user recordings;
- changing the current non-Windows media-tool policy beyond the minimum compilation seam.

## Exploration findings

- The developer PATH resolves Anaconda FFmpeg 6.1. That build cannot run the planned graph because it lacks `gfxcapture` and `scale_d3d11`; successful `-version` is therefore insufficient.
- Official FFmpeg tag `n8.1.2` contains `libavfilter/vsrc_gfxcapture.c`, including exact `hwnd` capture and D3D11 hardware-frame output, and `libavfilter/vf_scale_d3d11.c`, including D3D11 NV12/P010 conversion and video-encoder-bound output resources.
- FFmpeg publishes source rather than official Windows executables. The initially selected Gyan 8.1.2 build advertised the required surfaces, but real H.264 and HEVC NVENC probes failed on the supported RTX 4060 machine because that build requires NVENC API 13.1 and an NVIDIA 610-or-newer driver. QueueBack must not require a driver update merely to adopt the packaged runtime.
- The compatible replacement is a QueueBack static build from exact source identities: FFmpeg `n8.1.2`/`38b88335f99e76ed89ff3c93f877fdefce736c13`, nv-codec-headers `n12.2.72.0`/`c69278340ab1d5559c7d7bf0edf615dc33ddbba7`, and AMF `v1.4.36`/`16f7d73e0b45c473e903e46981ed0b91efc4c091`, plus exact MSYS2 UCRT64 toolchain package versions.
- Tauri's resource map preserves an explicit target layout, and a platform-specific `tauri.windows.conf.json` avoids imposing Windows assets on macOS configuration.
- A manifest stored beside mutable binaries is not itself an identity boundary. The expected lock must be compiled into both QueueBack binaries and checked against actual file hashes.
- Packaging the static GPLv3 build requires carrying the applicable FFmpeg/x264, NVIDIA codec-header, AMD AMF, and Intel libvpl licenses/notices plus exact source/build provenance. Release evidence inventories the actual shipped files and hashes.

Primary references:

- <https://ffmpeg.org/download.html>
- <https://github.com/FFmpeg/FFmpeg/tree/n8.1.2>
- <https://github.com/FFmpeg/nv-codec-headers/tree/n12.2.72.0>
- <https://github.com/GPUOpen-LibrariesAndSDKs/AMF/tree/v1.4.36>
- <https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.1.2/libavfilter/vsrc_gfxcapture.c>
- <https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.1.2/libavfilter/vf_scale_d3d11.c>
- <https://v2.tauri.app/develop/resources/>

## Chosen design

### One locked directory

Add a `media-runtime` Rust crate plus a checked-in contract/provenance directory. The staged Windows layout is:

    media-runtime/
      runtime-manifest.json
      bin/
        ffmpeg.exe
        ffprobe.exe
      licenses/
        COPYING.GPLv3.txt
        THIRD_PARTY_NOTICES.md
        SOURCE_AND_BUILD.md

The version-two lock records runtime ID, Windows/x86_64 identity, exact FFmpeg version/banner/compiler/configuration, every source URL/tag/40-character commit, exact build-tool package versions in the provenance document, each shipped file path/size/SHA-256, and the distribution-baseline capability set. Paths are relative, slash-normalized, and may not be absolute, empty, contain `..`, or escape the canonical runtime root. `ffplay` is neither built for shipment nor packaged.

The Rust crate embeds the expected lock at compile time. It does not trust an adjacent manifest to choose its own hashes. Tests can inject an expected lock and process-probe seam; production cannot.

### Resolution and validation

Callers pass the production resource root:

- the Tauri app passes `app.path().resource_dir()/media-runtime`;
- the standalone recorder passes `<current executable parent>/resources/media-runtime`;
- `QUEUEBACK_MEDIA_RUNTIME_DIR` is an explicit development/support override for the whole directory and must pass the same embedded lock;
- Windows production does not consult PATH, `LEAGUE_REPLAY_FFMPEG`, or a sibling tool inferred from an individually overridden executable.

Resolution produces one immutable `MediaTools` value containing both canonical paths and a capability/build report. It verifies:

1. safe layout and exact manifest/embedded-lock identity;
2. required regular files and no pair split;
3. SHA-256 and size for both tools and shipped compliance files;
4. bounded, console-hidden `-version` probes for matching FFmpeg/ffprobe build identity;
5. bounded FFmpeg listings for `gfxcapture`, `scale_d3d11`, D3D11 hardware acceleration, `libx264`, and compiled H.264/HEVC NVENC, AMF, and QSV encoders.

Capability presence means compiled availability only. It does not claim that a particular driver/GPU can run that encoder. `QB-PERF-002` adds a stricter requirements profile for the QueueBack diagnostics ABI and live adapter interoperability.

The probe runner has an explicit deadline, kills/reaps on timeout, hides console windows, bounds captured output, and maps errors to stable variants: missing, incomplete, invalid layout/manifest, integrity mismatch, incompatible build, missing capability, blocked execution, and timeout. User-facing text gives one next action: reinstall/repair the package, inspect antivirus quarantine for blocked files, or set the complete development runtime directory when intentionally developing.

### Explicit acquisition and packaging

Add `tools/media_runtime/build_ffmpeg.ps1` as the explicit maintainer-only source acquisition/build command. It clones only the pinned tagged commits when `-Acquire` is supplied, verifies each checked-out commit and each required MSYS2 package version, builds in the ignored workspace tree, and never runs from Cargo/npm/application startup. `tools/media_runtime/prepare.ps1` operates offline on those exact built binaries, validates their hashes, and atomically constructs the staging directory. Generated sources, build products, and release layouts remain ignored; the lock, scripts, license text, notices, and provenance stay checked in.

Add `tools/media_runtime/verify.ps1` to verify both the staging root and a portable release root. Add `app/src-tauri/tauri.windows.conf.json` to map the staged directory to `$RESOURCE/media-runtime`. The portable layout used before `QB-DIST-004` places both QueueBack executables beside `resources/media-runtime`; the later installer consumes the same resource map instead of redefining it.

### Recorder and app integration

The recorder resolves `MediaTools` once before diagnostics or service startup. `Ffmpeg` is constructed from the resolved ffmpeg path; encoder detection, benchmark, capture command creation, and finalization retain their current ownership model. The selected runtime ID/path appears in diagnostics and startup logs. No hash, process probe, or capability query occurs per frame or on a health tick.

The Tauri app resolves once during setup and stores either `Arc<MediaTools>` or the typed error in `AppState`. Library browsing, saved state, and playback server startup continue if the runtime is missing. Duration probing uses the packaged ffprobe when available and otherwise retains existing metadata/fallback behavior. Clip encoding and thumbnail generation require the packaged ffmpeg and return the stored concise error when unavailable. They never downgrade a valid recording to `Unknown` merely because a tool is missing.

## Milestones

### Milestone 1: Pin provenance and implement offline staging

- Acquire only the exact FFmpeg, nv-codec-header, and AMF commits through explicit maintainer action and reject source/toolchain drift.
- Build the QueueBack 8.1.2 static runtime with pinned compatible NVIDIA headers plus current AMD/Intel interfaces; record actual `-version`/`-buildconf` output and every staged file hash.
- Add the checked-in lock, license/notices/provenance, safe PowerShell prepare script, generated-output ignores, and offline verifier.
- Prove a corrupted built tool fails validation and a partial staging operation never replaces a valid runtime.

Exit: `build/media-runtime/windows-x86_64` can be reproduced from pinned sources/toolchain and verified offline without floating inputs or source-controlled binaries.

### Milestone 2: Implement the shared contract

- Create the `media-runtime` crate with manifest/lock types, safe-path checks, paired resolution, hashing, finite process probing, capability parsing, and stable errors.
- Add injected temporary-layout/process-probe tests for every error class, no-PATH behavior, override behavior, mixed-pair rejection, timeouts, output bounds, and capability/build mismatch.
- Preserve a requirements extension point for the later QueueBack diagnostics ABI.

Exit: both consumer crates can receive one validated `MediaTools` pair and an exact report without duplicating discovery logic.

### Milestone 3: Replace every consumer call site

- Add the shared dependency to recorder and app.
- Replace recorder candidate/PATH discovery while retaining the existing `Ffmpeg` session API.
- Store app media-tool state without making app startup/library/playback depend on successful validation.
- Pass explicit ffmpeg/ffprobe paths into clip export, thumbnail generation, and duration probing.
- Add regression tests that search or exercise every production spawn path and prove no Windows bare tool name remains.

Exit: recorder and app use the same atomic pair, missing tools are operation-scoped in the app, and the old environment/PATH routes cannot silently win.

### Milestone 4: Package and prove the Windows layout

- Add the Windows-specific Tauri resource mapping and portable release staging command.
- Build current recorder/app releases with the materialized runtime.
- Run verifier and dedicated temporary-media smoke with a sanitized PATH containing no ffmpeg/ffprobe.
- Verify diagnostics, encode, probe, thumbnail, and export report/use only paths below `resources/media-runtime`.
- Deliberately test missing and blocked fixture copies; do not start League or GDI capture.

Exit: the exact resource pair and compliance files are present in the release layout and all dependent operations work without system media tools.

### Milestone 5: Close the dependency

- Run all feature and project gates.
- Update `docs/development/VERIFICATION.md`, recorder/app operator documentation, and durable architecture documentation for the runtime contract and error recovery.
- Record exact hashes, commands, results, package inventory, limitations, and PERF handoff in `feature-list.json`.
- Mark `QB-DIST-001` done and unblock `QB-PERF-002` only when every acceptance criterion has evidence.

Exit: a fresh PERF implementation context can extend this exact runtime with the diagnostics patch and begin the WGC graph immediately.

## Verification

Core automated checks:

    cargo test --manifest-path media-runtime/Cargo.toml
    cargo fmt --manifest-path media-runtime/Cargo.toml -- --check
    cargo clippy --manifest-path media-runtime/Cargo.toml --all-targets -- -D warnings
    cargo test --manifest-path recorder/Cargo.toml
    cargo fmt --manifest-path recorder/Cargo.toml -- --check
    cargo clippy --manifest-path recorder/Cargo.toml --all-targets -- -D warnings
    cargo test --manifest-path app/src-tauri/Cargo.toml
    cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
    cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
    npm run check --prefix app
    npm run build --prefix app

Runtime materialization and layout checks:

    powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/build_ffmpeg.ps1 -Acquire
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/prepare.ps1
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/verify.ps1 -RuntimeRoot build/media-runtime/windows-x86_64 -ReleaseRoot build/release/queueback

The implementation must add checked-in smoke commands to `docs/development/VERIFICATION.md` for:

- the portable Windows release staging step;
- recorder `--diagnose` under a sanitized PATH;
- a dedicated generated media encode/probe/thumbnail/export check under the same PATH;
- exact `-version`, `-buildconf`, `-filters`, `-hwaccels`, and `-encoders` inventory from the packaged path;
- package file/hash/license inventory.

The final proof must show that PATH contains no ffmpeg/ffprobe, every reported tool path is under the staged resource root, input/output fixtures live under a sentinel temporary directory, and no QueueBack user media is mutated.

Acceptance mapping:

- Criteria 1-5: lock/manifest unit tests, explicit source-build/offline-staging scripts, source/toolchain/tool hashes, provenance and capability inventory.
- Criteria 6-8: recorder/app integration tests, static bare-spawn rejection, resource map, staged release smoke, and app-degradation tests.
- Criterion 9: sanitized-PATH temporary-media smoke and deliberate missing/blocked copies.
- Criterion 10: shared requirements-extension tests, project gates, documentation, and completed evidence.

## Performance / reliability

- Validation happens once per QueueBack process before any recording starts. It performs no per-frame, per-second, health-tick, or Live Client work.
- Full executable hashing is bounded by two known files and occurs outside the active recording path. The resolved value/report is cached for the process lifetime.
- Every child probe has finite output and time bounds and is killed/reaped on timeout; Windows console windows stay hidden.
- The pair is selected atomically. A partial update, antivirus quarantine, wrong version, or mixed ffmpeg/ffprobe cannot degrade silently to PATH.
- Runtime preparation uses temporary extraction/staging and replace-after-validation semantics so interrupted maintainer work cannot publish a half-runtime.
- App failure is operation-scoped: library/playback remain usable. Recorder failure is fail-fast and actionable before a bundle is created.
- The staged FFmpeg is static, so the PATH-free proof does not rely on adjacent third-party DLLs. Required Windows system libraries remain an operating-system prerequisite.
- No actual GDI capture or League session is required here; the first capture/performance check using this runtime belongs to `QB-PERF-002`.

## Progress

- [x] Inspect recorder/app discovery and packaging call sites.
- [x] Confirm the local 6.1 incompatibility and upstream 8.1.2 Windows capture/conversion availability.
- [x] Reject the driver-incompatible Gyan baseline and build the pinned QueueBack 8.1.2 runtime with nv-codec-headers 12.2, AMF 1.4.36, libvpl, libx264, WGC, D3D11 conversion, and static Windows linkage.
- [x] Challenge PATH, partial-upgrade, antivirus/blocking, Tauri resource, licensing, non-Windows, app-degradation, and PERF-extension failure modes.
- [x] Write and link this ExecPlan.
- [x] Implement Milestones 1 through 4: version-two lock, source build/staging/verifier, shared resolver, recorder/app integration, Tauri mapping, corruption proof, real-runtime probe, sanitized-PATH smoke, and NVENC compatibility probe.
- [x] Run Milestone 5 and satisfy the global completion gate.

## Deviations / surprises

- The initially planned Gyan build cannot encode on the supported RTX 4060 with its installed 566.03 driver: the binary requires NVENC API 13.1/driver 610 or newer. QueueBack therefore moved to an exact-source custom build using nv-codec-headers 12.2.72.0. Real H.264 and HEVC NVENC probes pass without a driver upgrade.
- FFmpeg 8.1.2's optional `amf_capture` filter does not compile against current AMF 1.4.36 headers because of a C/C++ linkage mismatch. QueueBack disables only that unused AMD-specific capture filter; the vendor-neutral `gfxcapture` source and AMF H.264/HEVC encoders remain enabled.
- The app can remain useful without FFmpeg for ordinary browsing/playback. Failing all of Tauri setup on a missing runtime would be a product regression, so only tool-dependent operations are gated.
- The full clean-machine installer belongs to `QB-DIST-004`. This feature produces and validates the exact portable/resource layout that installer will consume; it does not claim install/upgrade/uninstall completion.

## Decision log

- 2026-08-12: Reject the planned Gyan FFmpeg 8.1.2 binary after real encoder probes proved it incompatible with the supported NVIDIA driver. Use the reproducible QueueBack FFmpeg 8.1.2 static build pinned to FFmpeg `38b88335f99e76ed89ff3c93f877fdefce736c13`, nv-codec-headers `c69278340ab1d5559c7d7bf0edf615dc33ddbba7`, and AMF `16f7d73e0b45c473e903e46981ed0b91efc4c091`; do not require a driver update.
- 2026-08-12: Compile the expected lock into QueueBack and hash both tools. An adjacent self-declared manifest or successful `-version` alone is not identity proof.
- 2026-08-12: Resolve an entire directory, never one executable. Remove Windows PATH and legacy single-binary override fallback; retain only an explicit complete-directory developer override that passes the same lock.
- 2026-08-12: Keep downloads explicit and out of ordinary builds/runtime. Generated media binaries stay outside Git.
- 2026-08-12: Make app failure operation-scoped, but make recorder startup fail before bundle creation when the runtime is unavailable.
- 2026-08-12: Verify compiled NVIDIA, AMD, and Intel encoder surfaces now while keeping hardware/performance claims explicitly separate. The current machine can physically validate only NVENC later.
- 2026-08-12: Leave the QueueBack diagnostics patch and its ABI to `QB-PERF-002`, which updates this same lock and requirement profile rather than creating a second runtime mechanism.

## Completion

Complete. The staged runtime ID is `queueback-ffmpeg-8.1.2-windows-x86_64-r1`; `ffmpeg.exe` is 33,892,864 bytes with SHA-256 `3bda8a8ec9517b872a9478efe9fe9b7d2bf165c823412db036cfc43f2bc990fd`, and `ffprobe.exe` is 33,681,920 bytes with SHA-256 `2b3c43c7757236c1152a70d8f37966fcef03b6829c06c6e854c88aa34ae84f4e`.

The final tree passed 9 shared-runtime unit tests plus the real staged-runtime test, 31 recorder library tests plus the tray icon test, 30 Tauri tests, and 57 capture-benchmark regression tests. All three Rust crates passed formatting and strict Clippy; TypeScript and the production Vite build passed; recorder and Tauri release executables built successfully. Atomic corruption/preparation checks, runtime/release verification, and generated encode/probe/clip/thumbnail/full-decode smoke passed. With PATH sanitized so neither ffmpeg nor ffprobe resolved, the staged release recorder selected only `resources/media-runtime/bin/ffmpeg.exe`, reported the exact runtime ID, and completed a real H.264 NVENC diagnostic on driver 566.03. A sentinel fixture with the runtime deliberately absent failed before capture with the actionable repair message while app tests proved browsing/playback remain independent. The canonical 50-item roadmap schema/invariants and diff hygiene passed.

`QB-PERF-002` can now extend this exact source build and shared resolver for the WGC/D3D11 graph. No GDI recording or performance claim was made by this distribution feature.
