# League Replay App

Tauri 2 + SolidJS playback and library process for League Replay.

## Development

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

## Checks

```powershell
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
