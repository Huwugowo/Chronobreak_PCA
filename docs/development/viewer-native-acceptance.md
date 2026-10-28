# Ordinary Viewer acceptance procedure

Use this procedure for the remaining QB-DEV-007 native gate. A completed checklist
is operator evidence; this document alone is not a passing result. Use only the
generated-fixture library below. Do not select a personal recording library.

## Start the ordinary build

Close any earlier isolated test Viewer. From the main checkout in PowerShell:

```powershell
Set-Location C:\Users\Hugo\Documents\perso\Chronobreak
npm.cmd run desktop:build --prefix app
if ($LASTEXITCODE -ne 0) { throw 'Ordinary build failed' }
$fixtureRoot = (Resolve-Path build/perf/qbd7-ui-retry).Path
if (!(Test-Path -LiteralPath (Join-Path $fixtureRoot '.chronobreak-ui.json'))) {
    throw 'Missing generated-fixture ownership receipt'
}
$env:LEAGUE_REPLAY_CONFIG = Join-Path $fixtureRoot 'config.toml'
$env:LEAGUE_REPLAY_APP_DATA_DIR = Join-Path $fixtureRoot 'app-data'
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $fixtureRoot 'webview-profile'
Get-ChildItem Env: | Where-Object {
    $_.Name -like 'LEAGUE_REPLAY_BENCHMARK*' -or
    $_.Name -like 'QUEUEBACK_BENCHMARK*' -or
    $_.Name -in @('LEAGUE_REPLAY_OUTPUT_PATH','QUEUEBACK_MEDIA_RUNTIME_DIR','VITE_UI_MOCKS')
} | ForEach-Object { Remove-Item -LiteralPath ('Env:' + $_.Name) }
$ordinaryExe = (Resolve-Path app/src-tauri/target/release/league-replay-app.exe).Path
Get-FileHash -Algorithm SHA256 -LiteralPath $ordinaryExe
Start-Process -FilePath $ordinaryExe -WorkingDirectory (Split-Path $ordinaryExe)
```

Confirm `config.toml` points to `build\perf\qbd7-ui-retry\library`, with
`auto_delete_days = 0`. The library must show exactly the generated Akali Practice
Tool and Master Yi Swiftplay recordings. If it shows anything else, close it and
stop. Place the window on the right second screen. This is ordinary interaction
acceptance, not a performance trial; do not interact with a measured benchmark
window at the same time.

## Interaction checklist

Record pass/fail, the observed time/rate/range, and any error for each step. A
screen recording or before/after screenshots help reproduce a failure.

1. Open **Master Yi**, wait for video and League details, use **Back to Match
   History**, then reopen it. Repeat with Akali, then return to Master Yi. Check
   that the selected recording, duration and details belong to that recording.
2. Click the video or Play/Pause, then use Space with focus outside an input.
   Confirm pause freezes the picture and Play advances it. Select each **Replay
   speed** value (0.25x, 0.5x, 1x, 2x, 4x, 8x), allowing time to settle. Record
   any explicit unsupported/limited state. Restore 1x. Check mute and volume.
3. Select a timeline event named **Seek to … at m:ss**. A **Zoom into N events**
   button expands a cluster first. Confirm the event's position, picture and
   recorded state follow the jump. On the focused **Replay position** slider,
   rapidly alternate Left/Right, Ctrl+Left/Right, Home/End and Page Up/Down; also
   drag the playhead rapidly in both directions. Release at a known position and
   check the final picture/state settles there without a later stale jump.
4. Start playback, choose **Open fullscreen replay**, then press Escape. Repeat
   while paused, at 2x, and after a seek. Confirm time/rate/paused state and the
   selected recording persist; playback must not restart at zero. Restore 1x.
5. Choose **Clip**. Drag **Clip starts at …** and **Clip ends at …** independently.
   Focus each handle and use Left/Right for frame changes; hold briefly to check
   accelerated edits. Confirm valid ordered endpoints and matching preview.
   Space previews the selection. Enter/exit fullscreen during an endpoint drag
   and check that releasing the pointer ends the gesture. Use **Cancel** (or
   Escape after leaving fullscreen) and confirm ordinary seeking works again.
6. From the Viewer, open **Settings**, then **Back**. Confirm the same replay is
   selected and the intended playback state returns. Record the observed time
   and whether this navigation remounts playback; do not infer DOM persistence
   across navigation that unmounts the Viewer. Repeat after a clip edit.
7. Perform the separate copied-fixture error/retry check below. Check the error is
   visible, navigation remains usable, and retry recovers after restoration.
8. Close the test app. Verify the generated source hashes using the final command
   below. Do not treat a failed or unattempted item as passed.

## Primary element and load continuity

Visual continuity alone does not prove element identity. For an exact native DOM
check, launch only this isolated normal app with WebView2 remote inspection on a
free localhost port (for example set
`$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9223'`
before launch). In Chrome's `chrome://inspect/#devices`, configure
`localhost:9223` and inspect this isolated app's `tauri.localhost` target. This
diagnostic launch is not performance evidence. In its Console, after opening the
Viewer, run:

```javascript
globalThis.qbAcceptancePrimary = document.querySelector('[data-qb-primary-playback="true"]');
if (!qbAcceptancePrimary) throw new Error('Primary playback element not found');
globalThis.qbAcceptanceSource = qbAcceptancePrimary.currentSrc;
globalThis.qbAcceptanceLoadEvents = [];
globalThis.qbAcceptanceLoadObserver = event => qbAcceptanceLoadEvents.push(event.type);
for (const type of ['loadstart', 'emptied']) {
    qbAcceptancePrimary.addEventListener(type, qbAcceptanceLoadObserver);
}
```

Repeat fullscreen/Escape, seeks and clip endpoint/layout changes without leaving
the Viewer, then run:

```javascript
({ count: document.querySelectorAll('[data-qb-primary-playback="true"]').length,
   sameElement: qbAcceptancePrimary === document.querySelector('[data-qb-primary-playback="true"]'),
   sameLoad: qbAcceptanceSource === qbAcceptancePrimary.currentSrc,
   connected: qbAcceptancePrimary.isConnected,
   loadEvents: qbAcceptanceLoadEvents.slice() })
```

Require count 1, all three booleans true and an empty `loadEvents` array during
those in-Viewer operations. An unchanged URL alone does not prove load continuity.
Record output. Back/reopen and Settings navigation may remount by design; capture
a fresh reference after each new mount. If native inspection is unavailable,
record that exact limitation rather than claiming the identity check passed.
Close the diagnostic app and remove `Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
afterward.

## Copied-fixture error and recovery

While the fixture library is visible and Master Yi is closed, temporarily rename
only the copied fixture's video. Run these commands in the same PowerShell, never
against a personal library:

```powershell
$fixtureVideo = Join-Path $fixtureRoot 'library/games/1792431584/video.mp4'
Rename-Item -LiteralPath $fixtureVideo -NewName 'video.acceptance-held.mp4'
```

Click the already displayed **Open Master Yi** button. Require **Recording could
not be opened.**, **Retry recording** and **Return to games**. Restore the file
even if the UI behaves unexpectedly:

```powershell
Rename-Item -LiteralPath (Join-Path (Split-Path $fixtureVideo) 'video.acceptance-held.mp4') -NewName 'video.mp4'
```

Click **Retry recording** and require successful ordinary playback. If library
refresh removed the stale card before the click, restore the file and record the
error scenario as not exercised; do not call that a passing error check. Never
corrupt a recording or delete a file to provoke an error.

After closing the app, verify every source game file against the retained receipt:

```powershell
$receipt = Get-Content -Raw (Join-Path $fixtureRoot '.chronobreak-ui.json') | ConvertFrom-Json
foreach ($entry in $receipt.source_games.PSObject.Properties) {
    $file = Join-Path (Join-Path $fixtureRoot 'library') $entry.Name
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash.ToLowerInvariant() -ne $entry.Value) {
        throw "Generated source changed: $file"
    }
}
'Generated source hashes preserved'
```

Return the executable SHA, date, Windows/WebView version, screen/scale, checklist
results, DOM inspection output, error/retry result and source-hash result. Note
any failed step and the exact sequence. QB-DEV-007 stays open until mandatory
native, control and statistical gates have accepted evidence.
