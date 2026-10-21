$ErrorActionPreference='Stop'
$clipRoot=(Resolve-Path build/perf/qb-clip-010/editor).Path
$env:LEAGUE_REPLAY_CONFIG="$clipRoot/config.toml"
$env:LEAGUE_REPLAY_OUTPUT_PATH="$clipRoot/library"
$env:LEAGUE_REPLAY_APP_DATA_DIR="$clipRoot/app-data"
$env:WEBVIEW2_USER_DATA_FOLDER="$clipRoot/webview2"
$env:QUEUEBACK_MEDIA_RUNTIME_DIR=(Resolve-Path build/media-runtime/windows-x86_64).Path
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9224'
$env:CLIP_CPU_WORKER=(Resolve-Path build/perf/qb-clip-010/admin-queue-v4).Path
foreach ($mode in @('gpu-upload-off','gpu-draw-off','gpu-no-flip')) {
 $env:CLIP_DIAGNOSTIC=$mode
 $measurementApp=Start-Process app/src-tauri/target/release/league-replay-app.exe -WindowStyle Normal -PassThru
 try {
  Start-Sleep -Seconds 3
  node tools/vertical_preview/trial.mjs C:/Users/Hugo/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright http://127.0.0.1:9224 "$clipRoot/library" $measurementApp.Id b-1
  if($LASTEXITCODE -ne 0){throw 'Diagnostic failed'}
 } finally {if(-not $measurementApp.HasExited){Stop-Process -Id $measurementApp.Id}}
 Start-Sleep -Seconds 2
}
