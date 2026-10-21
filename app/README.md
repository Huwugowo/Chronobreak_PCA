# League Replay App

Tauri 2 + SolidJS playback and library process for League Replay.

## Development

For the actual desktop app with isolated real League data and synthetic video,
see [desktop preprod](../fixtures/preprod-library/README.md). From the repository
root, `powershell -NoProfile -ExecutionPolicy Bypass -File tools/preprod_launch.ps1 -Dev`
starts ordinary hot reload with frontend mocks disabled. Omit `-Dev` for a fresh release build.

```powershell
npm install
npm run tauri dev
```

## Production executable

```powershell
npm run desktop:build
```

The executable is written to `src-tauri/target/release/league-replay-app.exe` on
Windows. Use this command instead of invoking `cargo build --release` directly:
the Tauri build command embeds the production frontend in the executable.

The app scans `~/LeagueReplays/games` by default. Set the task-specific
`LEAGUE_REPLAY_OUTPUT_PATH` environment variable before launch to point at a different
output root.

`LEAGUE_REPLAY_APP_DATA_DIR` optionally selects an absolute app-data directory
for native caches and app-owned files. An invalid override fails explicitly.
Without it, the normal Tauri app-local-data directory is used.

## Checks

```powershell
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
