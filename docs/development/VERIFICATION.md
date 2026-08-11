# Chronobreak verification

This is the authoritative entry point for project-wide verification. Run commands from the repository root unless a command says otherwise. Record actual results in the active feature's evidence; this file defines the checks, not a permanent claim that they pass.

## Prerequisites

- Rust compatible with the manifests (`recorder/Cargo.toml` requires Rust 1.95) and Cargo.
- Node.js and npm; install the locked frontend dependencies with `npm ci --prefix app` when `app/node_modules` is absent.
- Windows 10+ for the supported recorder/Tauri desktop path and interactive validation.
- `ffmpeg` and `ffprobe` on `PATH`, or `LEAGUE_REPLAY_FFMPEG` pointing to ffmpeg, until packaged media tools are implemented.
- Python with `jsonschema` for roadmap validation.
- A dedicated temporary/test output library for destructive storage, retention, recovery, or corruption checks. Never use a real recording library.

## Routine automated checks

Run these for changes in the corresponding component. Run the complete group for cross-cutting changes and before claiming project-wide completion.

```powershell
cargo test --manifest-path recorder/Cargo.toml
cargo fmt --manifest-path recorder/Cargo.toml -- --check
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run check --prefix app
npm run build --prefix app
```

The recorder has no separate Clippy command in the current completion baseline. For recorder changes, running the analogous strict check is encouraged and any failure must be reported truthfully:

```powershell
cargo clippy --manifest-path recorder/Cargo.toml --all-targets -- -D warnings
```

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

## Recorder diagnostics

Diagnostics validates configuration, ffmpeg discovery, a real short hardware encode, selected codec/profile, and Windows loopback-audio discovery. With `profile = "auto"`, it also runs the recorder's short encoder-selection benchmark. It requires working capture/encode hardware and is not a substitute for the in-game performance benchmark.

```powershell
cargo run --manifest-path recorder/Cargo.toml -- --diagnose
```

The tray smoke test exercises real tray/service startup for three seconds and therefore needs an interactive Windows desktop:

```powershell
cargo run --manifest-path recorder/Cargo.toml -- --tray-smoke-test
```

For controlled headless lifecycle testing, set `LEAGUE_REPLAY_PROCESS_NAME` to a dedicated fixture process and set `LEAGUE_REPLAY_OUTPUT_PATH` to a temporary library before running:

```powershell
cargo run --manifest-path recorder/Cargo.toml -- --headless
```

## Media integrity checks

Use `ffprobe` against fixture output or a recording the user explicitly selected for non-destructive inspection. Verify that expected audio/video streams exist, duration and frame rate are sensible, and decoding reports no error:

```powershell
ffprobe -v error -show_entries format=duration:stream=index,codec_type,codec_name,avg_frame_rate -of json <media-file>
ffmpeg -v error -i <media-file> -f null NUL
```

For exported clips, also verify H.264/AAC compatibility and the requested duration/size constraints. Discord outputs must remain strictly below 10,000,000 bytes. Tests that deliberately truncate or corrupt media must copy fixtures into a temporary directory first.

## Interactive League and Windows validation

The full recorder procedure is maintained in `recorder/README.md` under “Phase 2 live validation.” It requires a real League match of at least 25 minutes and checks:

- process-driven start/stop and idle/recording tray states;
- no ffmpeg console window;
- playable normal and forcibly interrupted fragmented MP4 output;
- populated `metadata.json` and synchronized `game_log.json`;
- early/middle/late event alignment within approximately two seconds;
- no progressive polling slowdown or multi-second game-exit I/O freeze.

For app/viewer/export changes, use a dedicated representative library and validate browsing, rapid seeking, event filters, frame-driven state, fullscreen transitions, clip endpoint editing, export playback, A/V synchronization, thumbnails, and source preservation. Data Dragon network failure must degrade to cached assets/placeholders rather than block library playback.

The clean-machine installation, login-startup, bundled-tool, upgrade, and uninstall procedure remains future distribution work. It cannot be claimed from a developer machine that already has Rust, Node, or ffmpeg installed.

## Performance and benchmarks

Two different checks exist:

- `--diagnose` contains a short synthetic encoder/profile selection benchmark.
- `recorder/README.md` documents manual observations during a long League match.

The repository does not yet contain a reproducible baseline-versus-recording FPS/frametime benchmark or an accepted performance budget. Until `QB-PERF-001` is completed, do not claim “negligible performance impact” from diagnostics, resource usage, or phase prose alone.

## Environment-blocked and manual results

Record a check as:

- `passed` only when the exact command/procedure ran successfully against the current tree;
- `failed` when the project check ran and found a defect;
- `environment-blocked` when an external restriction such as sandbox child-process denial, missing interactive desktop, League, ffmpeg, or compatible hardware prevented the check;
- `not run` when it was simply not attempted.

Include the command, date, concise result, and relevant environment. Never convert `environment-blocked` or `not run` into passing evidence.
