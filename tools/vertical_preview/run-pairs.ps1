param([int]$StartPair=1)
$ErrorActionPreference='Stop'
$clipRoot=(Resolve-Path build/perf/qb-clip-010/editor).Path
$env:LEAGUE_REPLAY_CONFIG="$clipRoot/config.toml"
$env:LEAGUE_REPLAY_OUTPUT_PATH="$clipRoot/library"
$env:LEAGUE_REPLAY_APP_DATA_DIR="$clipRoot/app-data"
$env:WEBVIEW2_USER_DATA_FOLDER="$clipRoot/webview2"
$env:QUEUEBACK_MEDIA_RUNTIME_DIR=(Resolve-Path build/media-runtime/windows-x86_64).Path
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9224'
$env:CLIP_CPU_WORKER=(Resolve-Path build/perf/qb-clip-010/admin-queue-v4).Path
foreach ($pair in $StartPair..5) {
 foreach ($arm in @('a','b')) {
  $label="$arm-$pair"
  $exe=if($arm -eq 'a'){'build/perf/qb-clip-010/baseline/league-replay-app.exe'}else{'app/src-tauri/target/release/league-replay-app.exe'}
  $measurementApp=Start-Process $exe -WindowStyle Normal -RedirectStandardOutput "$clipRoot/$label.stdout.log" -RedirectStandardError "$clipRoot/$label.stderr.log" -PassThru
  try {
   Start-Sleep -Seconds 3
   node tools/vertical_preview/trial.mjs C:/Users/Hugo/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright http://127.0.0.1:9224 "$clipRoot/library" $measurementApp.Id $label
   if ($LASTEXITCODE -ne 0) { throw "Trial $label runner failed" }
   $result=Get-Content "build/perf/qb-clip-010/trials/$label/result.json" -Raw | ConvertFrom-Json
   if (-not $result.valid) { throw "Trial $label invalid" }
  } finally { if(-not $measurementApp.HasExited){Stop-Process -Id $measurementApp.Id} }
  Start-Sleep -Seconds 2
 }
}
