param(
    [int]$DurationSeconds = 10,
    [switch]$KeepTargetVisible
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$cargo = Join-Path $repo "recorder\Cargo.toml"
$fixture = Join-Path $repo "recorder\target\debug\examples\wgc_fixture.exe"
$probe = Join-Path $repo "recorder\target\debug\examples\native_wgc_source_probe.exe"
$outRoot = Join-Path $repo "build\perf\qb-perf-005-native-source"
New-Item -ItemType Directory -Force $outRoot | Out-Null

Write-Host "=== QB-PERF-005 native source probe ==="
& cargo build --manifest-path $cargo --example wgc_fixture --example native_wgc_source_probe
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$run = Join-Path $outRoot ([DateTime]::Now.ToString("yyyyMMdd-HHmmss"))
New-Item -ItemType Directory -Force $run | Out-Null
$fixtureOut = Join-Path $run "fixture.stdout.log"
$fixtureErr = Join-Path $run "fixture.stderr.log"
$probeOut = Join-Path $run "probe.stdout.log"

$fixtureArgs = @("--duration-seconds", [string]($DurationSeconds + 10), "--scenario", "steady")
if ($KeepTargetVisible) { $fixtureArgs += "--always-on-top" }
$fixtureProcess = Start-Process -FilePath $fixture -ArgumentList $fixtureArgs -PassThru `
    -RedirectStandardOutput $fixtureOut -RedirectStandardError $fixtureErr

try {
    $ready = $null
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-Path $fixtureOut) {
            $ready = Get-Content $fixtureOut | Where-Object { $_ -match '^QUEUEBACK_WGC_FIXTURE_READY pid=(\d+) hwnd=(\d+)$' } | Select-Object -First 1
            if ($ready) { break }
        }
        if ($fixtureProcess.HasExited) { throw "WGC fixture exited before readiness" }
        Start-Sleep -Milliseconds 100
    }
    if (-not $ready) { throw "timed out waiting for WGC fixture readiness" }
    if ($ready -notmatch '^QUEUEBACK_WGC_FIXTURE_READY pid=(\d+) hwnd=(\d+)$') { throw "malformed fixture readiness line" }
    $pidValue = [int]$Matches[1]

    & $probe --pid $pidValue --duration-seconds $DurationSeconds 2>&1 | Tee-Object -FilePath $probeOut
    $probeExit = $LASTEXITCODE
    if ($probeExit -ne 0) {
        throw "native WGC source probe failed with exit code $probeExit"
    }
    $pass = Select-String -LiteralPath $probeOut -SimpleMatch "CHRONOBREAK_NATIVE_WGC_PASS" -Quiet
    if (-not $pass) { throw "native source probe exited without PASS evidence" }

    Write-Host ""
    Write-Host "QB_PERF_005_NATIVE_SOURCE=PASS"
    Write-Host "EVIDENCE=$run"
}
finally {
    if (-not $fixtureProcess.HasExited) {
        Stop-Process -Id $fixtureProcess.Id -Force -ErrorAction SilentlyContinue
    }
}

