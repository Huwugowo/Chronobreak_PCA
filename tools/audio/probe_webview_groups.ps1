param([Parameter(Mandatory=$true)][string]$PlaywrightModule)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$runId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$run = Join-Path $repo ('build\perf\qb-audio-001\webview-' + $runId)
& python (Join-Path $PSScriptRoot 'prepare_webview_fixture.py') $run
if ($LASTEXITCODE -ne 0) { throw 'Generated WebView fixture preparation failed.' }
# Reserve a free port for this isolated WebView instance, then hand it to Chromium.
$listener = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()
$env:CHRONOBREAK_DISABLE_RECORDER_SUPERVISION = '1'
$env:LEAGUE_REPLAY_CONFIG = Join-Path $run 'config.toml'
$env:LEAGUE_REPLAY_OUTPUT_PATH = Join-Path $run 'library'
$env:LEAGUE_REPLAY_APP_DATA_DIR = Join-Path $run 'app-data'
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $run 'webview2'
$env:QUEUEBACK_MEDIA_RUNTIME_DIR = Join-Path $repo 'build\media-runtime\windows-x86_64'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=' + $port
@{debug_port=$port;supervision_disabled=$true;isolated_profile=$true} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $run 'launch.json') -Encoding UTF8
$launcher = Join-Path $run 'AudioWebViewProbe.exe'
Add-Type -Path @((Join-Path $PSScriptRoot 'ProbeLauncher.cs'), (Join-Path $PSScriptRoot 'ProbeJob.cs')) -OutputAssembly $launcher -OutputType ConsoleApplication -ReferencedAssemblies 'System.dll'
[Reflection.Assembly]::LoadFrom($launcher) | Out-Null
$gateName = 'Local\Chronobreak.AudioWebView.' + [Guid]::NewGuid().ToString('N')
$gate = New-Object Threading.EventWaitHandle($false, [Threading.EventResetMode]::ManualReset, $gateName)
$job = New-Object Chronobreak.AudioProbe.ProbeJob
$child = $null
try {
    $app = Join-Path $repo 'app\src-tauri\target\release\league-replay-app.exe'
    $child = Start-Process -FilePath $launcher -ArgumentList @($gateName, ('"' + $app + '"')) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $run 'app.stdout.log') -RedirectStandardError (Join-Path $run 'app.stderr.log')
    $job.Assign($child.Handle)
    $gate.Set() | Out-Null
    & node (Join-Path $PSScriptRoot 'probe_webview_groups.mjs') $PlaywrightModule ('http://127.0.0.1:' + $port) $run
    if ($LASTEXITCODE -ne 0) { throw "WebView channel proof failed; evidence retained at $run" }
    Write-Host "WEBVIEW_GROUPS=PASSED"
    Write-Host "EVIDENCE=$run"
}
finally {
    try { $job.TerminateAndWait(); Write-Host 'FIXTURE_PROCESS_TREE=EMPTY' }
    finally { $job.Dispose(); $gate.Dispose(); if ($null -ne $child) { $child.Dispose() } }
}
