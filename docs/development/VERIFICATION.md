# Chronobreak verification

This is the authoritative entry point for project-wide verification. Run commands from the repository root unless a command says otherwise. Record actual results in the active feature's evidence; this file defines the checks, not a permanent claim that they pass.

## Prerequisites

- Rust compatible with the manifests (`recorder/Cargo.toml` requires Rust 1.95) and Cargo.
- Node.js and npm; install the locked frontend dependencies with `npm ci --prefix app` when `app/node_modules` is absent.
- Windows 10+ for the supported recorder/Tauri desktop path and interactive validation.
- The staged QueueBack media runtime at `build/media-runtime/windows-x86_64` for recorder/app media-tool and release checks. System `ffmpeg`, system `ffprobe`, and `LEAGUE_REPLAY_FFMPEG` are not production discovery mechanisms.
- Python with `jsonschema` for roadmap validation.
- A dedicated temporary/test output library for destructive storage, retention, recovery, or corruption checks. Never use a real recording library.

## Routine automated checks

Run these for changes in the corresponding component. Run the complete group for cross-cutting changes and before claiming project-wide completion.

```powershell
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo fmt --manifest-path recorder/Cargo.toml -- --check
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run check --prefix app
npm run build --prefix app
```

Recorder Clippy is part of the completion baseline for recorder changes.

## Builds

Build the standalone recorder when recorder packaging or release behavior changes:

```powershell
cargo build --manifest-path recorder/Cargo.toml
cargo build --release --manifest-path recorder/Cargo.toml
```

Build the production frontend separately to isolate TypeScript/Vite failures:

```powershell
npm run build --prefix app
```

Build the Tauri production executable through the Tauri CLI so the frontend is embedded correctly:

```powershell
npm run desktop:build --prefix app
```

The executable is written under `app/src-tauri/target/release/`. A sandboxed Windows environment can reject Vite/esbuild child-process creation with `spawn EPERM`; report that as environment-blocked only after the command is actually attempted. A pre-existing `app/dist` or target executable is not evidence that the current tree built successfully.

## Canonical feature-list validation

The following command validates structure and also checks canonical invariants that JSON Schema cannot express: unique IDs, valid parents/dependencies, epic draft stage, and no dependency cycle.

```powershell
@'
import json
from pathlib import Path
from jsonschema import Draft202012Validator

root = Path('.')
instance = json.loads((root / 'feature-list.json').read_text(encoding='utf-8'))
schema = json.loads((root / 'feature-list.schema.json').read_text(encoding='utf-8'))
Draft202012Validator(schema).validate(instance)

features = instance['features']
by_id = {item['id']: item for item in features}
assert len(by_id) == len(features), 'feature IDs must be unique'
for item in features:
    parent = item.get('parent_id')
    assert parent is None or parent in by_id, f"unknown parent for {item['id']}: {parent}"
    if parent is not None:
        assert by_id[parent]['kind'] == 'epic', f"parent must be an epic: {item['id']}"
    for dependency in item['dependencies']:
        assert dependency in by_id, f"unknown dependency for {item['id']}: {dependency}"
        assert dependency != item['id'], f"self dependency: {item['id']}"
    if item['kind'] == 'epic':
        assert item['stage'] == 'draft', f"epic cannot be ready: {item['id']}"
    if item['stage'] == 'ready':
        assert item['kind'] == 'feature', f"only features can be ready: {item['id']}"
        assert item['acceptance_criteria'], f"ready feature lacks acceptance criteria: {item['id']}"
        assert item['verification'], f"ready feature lacks verification: {item['id']}"
    if item['status'] == 'done':
        assert item['stage'] == 'ready', f"done feature is not ready: {item['id']}"
        assert item['evidence'], f"done feature lacks evidence: {item['id']}"

visiting, visited = set(), set()
def visit(feature_id):
    if feature_id in visiting:
        raise AssertionError(f'dependency cycle at {feature_id}')
    if feature_id in visited:
        return
    visiting.add(feature_id)
    for dependency in by_id[feature_id]['dependencies']:
        visit(dependency)
    visiting.remove(feature_id)
    visited.add(feature_id)
for feature_id in by_id:
    visit(feature_id)
print(f"validated {len(features)} canonical roadmap items")
'@ | python -
```

## Packaged media runtime

The Windows recorder and app use one exact QueueBack FFmpeg/ffprobe pair. Production discovery uses `resources/media-runtime` beside the installed executable; `QUEUEBACK_MEDIA_RUNTIME_DIR` is the only development override and must satisfy the same embedded lock. PATH and single-executable overrides are deliberately ignored.

Run the shared contract, real staged-runtime, atomic preparation, verifier, and sanitized-PATH generated-media checks with:

```powershell
cargo test --manifest-path media-runtime/Cargo.toml
cargo fmt --manifest-path media-runtime/Cargo.toml -- --check
cargo clippy --manifest-path media-runtime/Cargo.toml --all-targets -- -D warnings
$env:QUEUEBACK_TEST_MEDIA_RUNTIME = (Resolve-Path 'build/media-runtime/windows-x86_64').Path
cargo test --manifest-path media-runtime/Cargo.toml --test packaged_runtime -- --ignored --nocapture
Remove-Item Env:QUEUEBACK_TEST_MEDIA_RUNTIME
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/test_prepare.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/prepare.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/verify.ps1 -RuntimeRoot build/media-runtime/windows-x86_64
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/smoke.ps1 -RuntimeRoot build/media-runtime/windows-x86_64
```

Rebuilding FFmpeg is an explicit maintainer action, never part of Cargo/npm/application startup:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/build_ffmpeg.ps1 -Acquire
```

After both release executables exist, stage and verify the portable layout:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/stage_release.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/verify.ps1 -RuntimeRoot build/media-runtime/windows-x86_64 -ReleaseRoot build/release/queueback
```

The generated smoke directory is sentinel-marked and never uses user media. The release smoke must also prove recorder diagnostics resolves `resources/media-runtime` under a PATH with no ffmpeg/ffprobe; missing-runtime checks use only a copied release fixture.

## Recorder diagnostics

Diagnostics validates configuration, ffmpeg discovery, a real short hardware encode, selected codec/profile, and Windows loopback-audio discovery. With `profile = "auto"`, it also runs the recorder's short encoder-selection benchmark. It requires working capture/encode hardware and is not a substitute for the in-game performance benchmark.

```powershell
cargo run --manifest-path recorder/Cargo.toml -- --diagnose
```

The tray smoke test exercises real tray/service startup for three seconds and therefore needs an interactive Windows desktop. It overrides configured storage with a sentinel-marked ephemeral library, so an already-running League process cannot create smoke output in the user library:

```powershell
cargo run --manifest-path recorder/Cargo.toml -- --tray-smoke-test
```

For controlled headless lifecycle testing, set `LEAGUE_REPLAY_PROCESS_NAME` to a dedicated fixture process. The recorder does not implement `LEAGUE_REPLAY_OUTPUT_PATH`; use a complete temporary TOML whose `[storage].output_path` points to a dedicated test library, then select it with `LEAGUE_REPLAY_CONFIG` and isolate logs with `LEAGUE_REPLAY_LOG_DIR`:

```powershell
cargo run --manifest-path recorder/Cargo.toml -- --headless
```

### Native Windows backend

The native WGC/D3D11/NVENC backend is the Windows default. Set
`QUEUEBACK_WINDOWS_RECORDER_BACKEND=ffmpeg` only when intentionally validating the
retained external alternative; there is no automatic cross-backend fallback.

Native probes require the staged r6 runtime and a dedicated target/evidence root.
They never target League or a user recording. Run the matched-backend preflight
before a timed A/B pair:

```powershell
$runtime = (Resolve-Path 'build/media-runtime/windows-x86_64').Path
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_wgc_source_probe.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_preliminary_backend_ab.ps1 -MediaRuntimeRoot $runtime -PreflightOnly
```

Continue to a timed pair only after the preflight prints
`CHRONOBREAK_PRELIMINARY_AB_PREFLIGHT=PASS`. Every run uses a fresh ignored evidence
root. Non-League fixture evidence validates bounded lifecycle/media behavior, not
League performance, hard driver-hang containment, or AMD/Intel hardware support.

## Media integrity checks

Use `ffprobe` against fixture output or a recording the user explicitly selected for non-destructive inspection. Verify that expected audio/video streams exist, duration and frame rate are sensible, and decoding reports no error:

```powershell
ffprobe -v error -show_entries format=duration:stream=index,codec_type,codec_name,avg_frame_rate -of json <media-file>
ffmpeg -v error -i <media-file> -f null NUL
```

For exported clips, also verify H.264/AAC compatibility and the requested duration/size constraints. Discord outputs must remain strictly below 10,000,000 bytes. Tests that deliberately truncate or corrupt media must copy fixtures into a temporary directory first.

For the default native Windows graph, run the sentinel-owned exact-HWND fixture.
These commands target only QueueBack's generated window and write under ignored
`build/perf`:

```powershell
& .\tools\native_backend\run_native_fixture.ps1 -Scenario steady -DurationSeconds 10
& .\tools\native_backend\run_native_fixture.ps1 -Scenario resize -DurationSeconds 10
& .\tools\native_backend\run_native_fixture.ps1 -Scenario minimize_restore -DurationSeconds 10
& .\tools\native_backend\run_native_fixture.ps1 -Scenario occlusion -DurationSeconds 10
& .\tools\native_backend\run_native_fixture.ps1 -Scenario close_window -DurationSeconds 10
& .\tools\native_backend\run_native_fixture.ps1 -Scenario steady -Interruption nvenc_failure -DurationSeconds 10
```

Each run verifies packaged-runtime identity, a physical 1920x1080 target/output,
H.264/AAC streams, exact 60-FPS native accounting, strict single-thread full
decode, changing frame hashes, and finite source/handoff/encoder ownership.
Normal cases require exact scheduled/submitted/completed/muxed reconciliation.
Target closure and injected NVENC failure must return the exact failure while
leaving a recoverable dedicated partial recording.

The retained external FFmpeg alternative has its own runner and evidence. Use
it only when that backend is intentionally in scope:

```powershell
& .\tools\capture_fixture\run_wgc.ps1 -Encoder nvenc -Scenario steady -DurationSeconds 10
```

Do not use the external-alternative runner as evidence for the native default.

The bounded-resource acceptance soak is:

```powershell
& .\tools\native_backend\run_native_fixture.ps1 -Scenario steady -DurationSeconds 1800 -CollectResources -KeepTargetVisible -ResourceSampleSeconds 5
```

`-KeepTargetVisible` makes the generated GDI surface always-on-top and requests Windows' display/system-required execution state for the resource soak. The runner restores the normal execution state in `finally`. This is necessary because Windows may stop WGC when an idle monitor powers down, and may stop compositing a fully hidden GDI test window even though a real actively rendering game continues presenting. It occupies the display for the run. Occlusion/focus behavior is tested separately by the dedicated transition scenario above. Retain the soak's `result.json` and `resource-samples.json`. Review initial/peak/final private memory, GPU dedicated/shared memory, declared texture budget, in-process mux-byte progress, frame/counter advancement, media decode, and terminal evidence. Sampled open-file length is diagnostic on Windows because it can remain unchanged while FFmpeg owns a buffered fragmented MP4. A shorter rehearsal cannot substitute for this 1,800-second run.

## Interactive League and Windows validation

The full recorder procedure is maintained in `recorder/README.md` under live validation. It requires a real League match of at least 25 minutes and checks:

- process-driven start/stop and idle/recording tray states;
- no ffmpeg console window;
- playable normal output; destructive encoder interruption uses the dedicated fixture, not a user recording;
- populated `metadata.json` and synchronized `game_log.json`;
- early/middle/late event alignment within approximately two seconds;
- no progressive polling slowdown or multi-second game-exit I/O freeze.

For app/viewer/export changes, use a dedicated representative library and validate browsing, rapid seeking, event filters, frame-driven state, fullscreen transitions, clip endpoint editing, export playback, A/V synchronization, thumbnails, and source preservation. Data Dragon network failure must degrade to cached assets/placeholders rather than block library playback.

The clean-machine installation, login-startup, upgrade, and uninstall procedure remains future distribution work. `QB-DIST-001` proves the exact portable packaged-media layout separately; it does not claim installer completion.

## Performance and benchmarks

Two different checks exist and are not interchangeable:

- `--diagnose` contains a short synthetic encoder/profile selection benchmark.
- The [capture benchmark protocol](../performance/capture-benchmark.md) defines the reproducible League baseline-versus-recording matrices, performance budget, raw counter semantics, collector/finalizer commands, and sanitized schema-v1/schema-v2 report workflow.

Run the deterministic analyzer/collector fixtures with:

```powershell
python -m unittest discover -s tools/capture_benchmark/tests -v
```

Exercise static collector preflight with an operator-supplied PresentMon 2.x console executable:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture_benchmark/collect.ps1 -PresentMonPath <PresentMon-2.x.exe> -Condition baseline -FrameMode capped -RunNumber 1 -ResultRoot build/perf/preflight -PreflightOnly
```

For the current `QB-PERF-002` proof, use `tools/capture_benchmark/run_qb_perf_002.ps1`. It performs exactly one capped baseline/capture pair and one uncapped baseline/capture pair, preserves failed roots, uses the packaged runtime, and prompts for required League/tray actions. The historical eight-run runner remains `run_qb_perf_001.ps1` and defaults to schema v1.

The complete live matrix remains an interactive Windows/League check; follow the durable protocol exactly. `QB-PERF-001` is complete by explicit product acceptance of its formally invalid single-monitor matrix as diagnostic pre-change characterization, not as a passing performance claim. Do not claim negligible performance impact until `QB-PERF-002` produces its own valid finalized post-change matrix and passes every unchanged frame/resource gate. If evidence is invalid or genuinely doubtful, preserve it and rerun a fresh complete pair rather than appending selective repetitions.

## Environment-blocked and manual results

Record a check as:

- `passed` only when the exact command/procedure ran successfully against the current tree;
- `failed` when the project check ran and found a defect;
- `environment-blocked` when an external restriction such as sandbox child-process denial, missing interactive desktop, League, ffmpeg, or compatible hardware prevented the check;
- `not run` when it was simply not attempted.

Include the command, date, concise result, and relevant environment. Never convert `environment-blocked` or `not run` into passing evidence.
