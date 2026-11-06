param(
    [switch]$RequireAvailable,
    [switch]$ControlledProof,
    [switch]$ControlledTimingProof,
    [ValidateRange(100, 30000)][int]$TimeoutMilliseconds = 15000
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($ControlledProof -and $ControlledTimingProof) { throw 'Choose source separation or timing proof.' }

if ([IntPtr]::Size -ne 8 -or $PSVersionTable.PSEdition -ne 'Desktop') {
    throw 'Use 64-bit Windows PowerShell (powershell.exe), not pwsh, for this .NET Framework probe.'
}

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$runId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
$kind = if ($ControlledTimingProof) { 'timing-' } elseif ($ControlledProof) { 'separation-' } else { 'activation-' }
$run = Join-Path $repo ('build\perf\qb-audio-001\' + $kind + $runId)
New-Item -ItemType Directory -Path $run | Out-Null
$executable = Join-Path $run 'ProcessLoopbackProbe.exe'
$stdout = Join-Path $run 'activation.stdout.json'
$stderr = Join-Path $run 'activation.stderr.log'

$sources = @(
    (Join-Path $PSScriptRoot 'ProcessLoopbackProbe.cs'),
    (Join-Path $PSScriptRoot 'ControlledAudioProof.cs'),
    (Join-Path $PSScriptRoot 'ControlledTimingProof.cs'),
    (Join-Path $PSScriptRoot 'ProbeJob.cs')
)
Add-Type -Path $sources `
    -OutputAssembly $executable -OutputType ConsoleApplication `
    -ReferencedAssemblies 'System.dll', 'System.Web.Extensions.dll', 'System.Windows.Forms.dll', 'System.Drawing.dll'

$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $executable
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
[Reflection.Assembly]::LoadFrom($executable) | Out-Null
$gateName = 'Local\Chronobreak.AudioProbe.' + [Guid]::NewGuid().ToString('N')
$gate = New-Object System.Threading.EventWaitHandle($false, [System.Threading.EventResetMode]::ManualReset, $gateName)
$job = New-Object Chronobreak.AudioProbe.ProbeJob
$start.Arguments = '--start-event ' + $gateName
if ($ControlledProof) { $start.Arguments += ' --controlled-proof' }
if ($ControlledTimingProof) {
    $nativeProbe = (Resolve-Path (Join-Path $repo 'recorder\target\debug\examples\native_nvenc_probe.exe')).Path
    $start.Arguments += ' --timing-proof "' + $nativeProbe + '" "' + $run + '"'
}
$child = New-Object System.Diagnostics.Process
$child.StartInfo = $start
$started = $false
$assigned = $false
try {
    $started = $child.Start()
    if (-not $started) { throw 'Could not start activation probe.' }
    $job.Assign($child.Handle)
    $assigned = $true
    $outputRead = $child.StandardOutput.ReadToEndAsync()
    $errorRead = $child.StandardError.ReadToEndAsync()
    $gate.Set() | Out-Null
    if (-not $child.WaitForExit($TimeoutMilliseconds)) {
        $job.TerminateAndWait()
        $child.WaitForExit(2000) | Out-Null
        $outputRead.GetAwaiter().GetResult() | Set-Content -LiteralPath $stdout -Encoding UTF8
        $errorRead.GetAwaiter().GetResult() | Set-Content -LiteralPath $stderr -Encoding UTF8
        throw "Probe exceeded $TimeoutMilliseconds ms; generated process tree terminated; evidence retained at $run"
    }
    $outputRead.GetAwaiter().GetResult() | Set-Content -LiteralPath $stdout -Encoding UTF8
    $errorRead.GetAwaiter().GetResult() | Set-Content -LiteralPath $stderr -Encoding UTF8
    if ($child.ExitCode -ne 0 -and -not ($ControlledProof -and $child.ExitCode -eq 1)) {
        throw "Audio feasibility probe failed (exit $($child.ExitCode)); evidence retained at $run"
    }
    $result = Get-Content -LiteralPath $stdout -Raw | ConvertFrom-Json
    if ($ControlledTimingProof) {
        $available = [bool]$result.fixture_completed
    } elseif ($ControlledProof) {
        $available = [bool]$result.separation_passed
    } else {
        if ($result.Count -ne 2) { throw 'Probe did not return both activation modes.' }
        $available = @($result | Where-Object { -not $_.activation_available }).Count -eq 0
    }
    $windows = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    $report = [ordered]@{
        schema_version = 1
        feature = 'QB-AUDIO-001'
        observed_at_utc = [DateTime]::UtcNow.ToString('o')
        scope = if ($ControlledTimingProof) { 'controlled timing observations; separate analysis required' } elseif ($ControlledProof) { 'controlled source separation; spectral summaries only' } else { 'activation-only; no audio samples captured' }
        windows_build = [int]$windows.CurrentBuildNumber
        windows_revision = [int]$windows.UBR
        documented_minimum_build = 20348
        meets_documented_build = [int]$windows.CurrentBuildNumber -ge 20348
        prerequisite_passed = $available
        observation = $result
        capture_separation_proven = [bool]($ControlledProof -and $available)
        replay_synchronization_proven = $false
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $run 'result.json') -Encoding UTF8
    $state = if ($available) {
        if ($ControlledTimingProof) { 'TIMING FIXTURE COMPLETED (analysis required)' } elseif ($ControlledProof) { 'SEPARATION PROVEN (generated tones only)' } else { 'AVAILABLE (activation only)' }
    } else {
        if ($ControlledProof) { 'SEPARATION CHECK FAILED' } else { 'ACTIVATION UNAVAILABLE' }
    }
    Write-Host "PROCESS_LOOPBACK=$state"
    Write-Host "WINDOWS_BUILD=$($report.windows_build).$($report.windows_revision)"
    Write-Host "EVIDENCE=$run"
    if (($RequireAvailable -or $ControlledProof) -and -not $available) { exit 1 }
}
finally {
    try {
        if ($assigned) {
            $job.TerminateAndWait()
            Write-Host 'FIXTURE_PROCESS_TREE=EMPTY'
        }
        elseif ($started -and -not $child.HasExited) {
            $child.Kill()
            $child.WaitForExit(2000) | Out-Null
        }
    }
    finally {
        $job.Dispose()
        $gate.Dispose()
        $child.Dispose()
    }
}
