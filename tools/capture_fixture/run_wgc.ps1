[CmdletBinding()]
param(
    [ValidateSet("nvenc", "amf", "qsv")][string]$Encoder = "nvenc",
    [ValidateSet("steady", "resize", "minimize_restore", "occlusion", "close_window")][string]$Scenario = "steady",
    [ValidateSet("none", "kill_encoder")][string]$Interruption = "none",
    [ValidateRange(2, 7200)][int]$DurationSeconds = 10,
    [switch]$CollectResources,
    [switch]$KeepTargetVisible,
    [ValidateRange(1, 60)][int]$ResourceSampleSeconds = 5,
    [string]$RuntimeRoot,
    [string]$OutputRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-BoundedProcess {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][ValidateRange(1, 3600)][int]$TimeoutSeconds
    )

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $FilePath
    $startInfo.Arguments = (($Arguments | ForEach-Object { '"' + $_.Replace('"', '\"') + '"' }) -join ' ')
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Could not start $FilePath."
    }
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        $process.Kill()
        [void]$process.WaitForExit(5000)
        throw "$FilePath exceeded its $TimeoutSeconds-second verification deadline."
    }
    $exitCode = $process.ExitCode
    $stdoutText = $stdout.GetAwaiter().GetResult()
    $stderrText = $stderr.GetAwaiter().GetResult()
    $process.Dispose()
    return [pscustomobject]@{
        ExitCode = $exitCode
        StandardOutput = $stdoutText
        StandardError = $stderrText
    }
}

function Get-OutputBytes {
    param([string]$Bundle)
    $outputs = @(Get-ChildItem -LiteralPath $Bundle -File -Filter "video*.mp4" -ErrorAction SilentlyContinue)
    if ($outputs.Count -eq 0) { return [uint64]0 }
    return [uint64](($outputs | Measure-Object -Property Length -Maximum).Maximum)
}

function Get-GpuProcessMemory {
    param([int]$ProcessId)
    try {
        $samples = (Get-Counter @(
            "\GPU Process Memory(*)\Dedicated Usage",
            "\GPU Process Memory(*)\Shared Usage"
        ) -ErrorAction Stop).CounterSamples
        $prefix = "pid_${ProcessId}_"
        $dedicated = [uint64]0
        $shared = [uint64]0
        $matched = $false
        foreach ($sample in $samples) {
            if (-not $sample.InstanceName.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { continue }
            $matched = $true
            if ($sample.Path -like "*\Dedicated Usage") { $dedicated += [uint64][Math]::Max(0, $sample.CookedValue) }
            if ($sample.Path -like "*\Shared Usage") { $shared += [uint64][Math]::Max(0, $sample.CookedValue) }
        }
        if (-not $matched) { return $null }
        return [pscustomobject]@{ dedicated_bytes = $dedicated; shared_bytes = $shared }
    }
    catch {
        return $null
    }
}

function Get-MaximumProperty {
    param([object[]]$Samples, [string]$Property)
    $values = @($Samples | ForEach-Object { $_.$Property } | Where-Object { $null -ne $_ })
    if ($values.Count -eq 0) { return $null }
    return ($values | Measure-Object -Maximum).Maximum
}

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$allowedRoot = [System.IO.Path]::GetFullPath((Join-Path $repository "build\perf"))
if ([string]::IsNullOrWhiteSpace($RuntimeRoot)) {
    $RuntimeRoot = Join-Path $repository "build\media-runtime\windows-x86_64"
}
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $allowedRoot "qb-perf-002-fixture"
}
$RuntimeRoot = [System.IO.Path]::GetFullPath($RuntimeRoot)
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
if (-not ($OutputRoot -eq $allowedRoot -or $OutputRoot.StartsWith($allowedRoot + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase))) {
    throw "Fixture output must stay inside $allowedRoot."
}

& (Join-Path $repository "tools\media_runtime\verify.ps1") -RuntimeRoot $RuntimeRoot
if ($LASTEXITCODE -ne 0) {
    throw "The packaged media runtime did not verify."
}

& cargo build --manifest-path (Join-Path $repository "recorder\Cargo.toml") --example wgc_fixture --example wgc_capture
if ($LASTEXITCODE -ne 0) {
    throw "Could not build the dedicated WGC fixture executables."
}

New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
$rootSentinel = Join-Path $OutputRoot ".queueback-wgc-fixture-root"
if (-not (Test-Path -LiteralPath $rootSentinel -PathType Leaf)) {
    [System.IO.File]::WriteAllText($rootSentinel, "QueueBack dedicated WGC fixture outputs only.`n")
}
$runId = (Get-Date -Format "yyyyMMdd-HHmmss") + "-$Scenario-$Interruption-" + [Guid]::NewGuid().ToString("N").Substring(0, 8)
$runRoot = Join-Path $OutputRoot $runId
$bundle = Join-Path $runRoot "bundle"
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null

$fixtureStdout = Join-Path $runRoot "fixture.stdout.log"
$fixtureStderr = Join-Path $runRoot "fixture.stderr.log"
$captureStdout = Join-Path $runRoot "capture.stdout.log"
$captureStderr = Join-Path $runRoot "capture.stderr.log"
$fixtureExe = Join-Path $repository "recorder\target\debug\examples\wgc_fixture.exe"
$captureExe = Join-Path $repository "recorder\target\debug\examples\wgc_capture.exe"
$fixture = $null
$capture = $null
$previousRuntime = $env:QUEUEBACK_MEDIA_RUNTIME_DIR
$expectedCaptureFailure = $Interruption -ne "none" -or $Scenario -eq "close_window"
$resourceSamples = New-Object 'System.Collections.Generic.List[object]'
$resourceFfmpegPid = $null
$executionStateSet = $false

try {
    if ($KeepTargetVisible) {
        Add-Type -TypeDefinition @"
using System.Runtime.InteropServices;
public static class QueueBackFixtureExecutionState {
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern uint SetThreadExecutionState(uint flags);
}
"@
        $continuousDisplayAndSystemRequired = [uint32]::Parse("80000003", [System.Globalization.NumberStyles]::HexNumber)
        if ([QueueBackFixtureExecutionState]::SetThreadExecutionState($continuousDisplayAndSystemRequired) -eq 0) {
            throw "Windows refused the fixture display-required execution state."
        }
        $executionStateSet = $true
    }
    $firstActionSeconds = [Math]::Max(1, [Math]::Floor($DurationSeconds / 3))
    $secondActionSeconds = [Math]::Max($firstActionSeconds + 1, [Math]::Floor($DurationSeconds * 2 / 3))
    if ($Scenario -eq "close_window") {
        # Leave enough time for the capture graph's readiness gate and several
        # complete fragmented-MP4 groups before closing only the target HWND.
        $firstActionSeconds = [Math]::Max(3, [Math]::Floor($DurationSeconds * 0.7))
        $secondActionSeconds = $firstActionSeconds + 1
    }
    $fixtureArguments = @(
        "--duration-seconds", ($DurationSeconds + 45),
        "--scenario", $Scenario,
        "--action-after-seconds", $firstActionSeconds,
        "--restore-after-seconds", $secondActionSeconds
    )
    if ($KeepTargetVisible) { $fixtureArguments += "--always-on-top" }
    $fixture = Start-Process -FilePath $fixtureExe -ArgumentList $fixtureArguments -RedirectStandardOutput $fixtureStdout -RedirectStandardError $fixtureStderr -PassThru
    $target = $null
    for ($attempt = 0; $attempt -lt 150; $attempt++) {
        Start-Sleep -Milliseconds 100
        if (Test-Path -LiteralPath $fixtureStdout) {
            $targetLine = Get-Content -LiteralPath $fixtureStdout | Where-Object { $_ -like "QUEUEBACK_WGC_TARGET*" } | Select-Object -Last 1
            if ($targetLine) {
                # Get-Content decorates strings with provider metadata. Keep the
                # report bounded by stripping those extended properties here.
                $target = [string]$targetLine
            }
            if ($target) { break }
        }
        if ($fixture.HasExited) {
            throw "The WGC fixture exited before publishing its target."
        }
    }
    if (-not $target -or $target -notmatch "hwnd=(\d+) adapter_index=(\d+)") {
        throw "The WGC fixture did not publish a valid target: $target"
    }
    if ($target -notmatch "width=1920 height=1080") {
        throw "The WGC fixture target is not the required 1920x1080 client area: $target"
    }

    $env:QUEUEBACK_MEDIA_RUNTIME_DIR = $RuntimeRoot
    $capture = Start-Process -FilePath $captureExe -ArgumentList "--pid", $fixture.Id, "--output", $bundle, "--duration-seconds", $DurationSeconds, "--encoder", $Encoder -RedirectStandardOutput $captureStdout -RedirectStandardError $captureStderr -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds($DurationSeconds + 30)
    $captureStarted = [DateTime]::UtcNow
    $interruptionDone = $false
    $nextResourceSample = $captureStarted
    while (-not $capture.HasExited -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 200
        $capture.Refresh()
        if ($CollectResources -and [DateTime]::UtcNow -ge $nextResourceSample) {
            if ($null -eq $resourceFfmpegPid) {
                $child = Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId=$($capture.Id)" -ErrorAction SilentlyContinue | Where-Object { $_.Name -ieq "ffmpeg.exe" } | Select-Object -First 1
                if ($null -ne $child) { $resourceFfmpegPid = [int]$child.ProcessId }
            }
            $captureProcess = Get-Process -Id $capture.Id -ErrorAction SilentlyContinue
            $ffmpegProcess = if ($null -ne $resourceFfmpegPid) { Get-Process -Id $resourceFfmpegPid -ErrorAction SilentlyContinue } else { $null }
            if ($captureProcess) { $captureProcess.Refresh() }
            if ($ffmpegProcess) { $ffmpegProcess.Refresh() }
            $gpuMemory = if ($ffmpegProcess) { Get-GpuProcessMemory -ProcessId $resourceFfmpegPid } else { $null }
            $resourceSamples.Add([pscustomobject][ordered]@{
                elapsed_seconds = [Math]::Round(([DateTime]::UtcNow - $captureStarted).TotalSeconds, 3)
                capture_pid = $capture.Id
                capture_working_set_bytes = $(if ($captureProcess) { [uint64]$captureProcess.WorkingSet64 } else { $null })
                capture_private_memory_bytes = $(if ($captureProcess) { [uint64]$captureProcess.PrivateMemorySize64 } else { $null })
                ffmpeg_pid = $resourceFfmpegPid
                ffmpeg_working_set_bytes = $(if ($ffmpegProcess) { [uint64]$ffmpegProcess.WorkingSet64 } else { $null })
                ffmpeg_private_memory_bytes = $(if ($ffmpegProcess) { [uint64]$ffmpegProcess.PrivateMemorySize64 } else { $null })
                ffmpeg_gpu_dedicated_bytes = $(if ($gpuMemory) { [uint64]$gpuMemory.dedicated_bytes } else { $null })
                ffmpeg_gpu_shared_bytes = $(if ($gpuMemory) { [uint64]$gpuMemory.shared_bytes } else { $null })
                output_bytes = Get-OutputBytes -Bundle $bundle
            })
            $nextResourceSample = [DateTime]::UtcNow.AddSeconds($ResourceSampleSeconds)
        }
        if (
            -not $interruptionDone -and
            $Interruption -ne "none" -and
            ([DateTime]::UtcNow - $captureStarted).TotalSeconds -ge ($DurationSeconds / 2)
        ) {
            $ffmpegChild = Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId=$($capture.Id)" | Where-Object { $_.Name -ieq "ffmpeg.exe" } | Select-Object -First 1
            if ($null -eq $ffmpegChild) {
                throw "Could not locate the dedicated FFmpeg child for forced interruption."
            }
            Stop-Process -Id ([int]$ffmpegChild.ProcessId) -Force
            Write-Output "Fixture action: dedicated FFmpeg child forced to exit"
            $interruptionDone = $true
        }
    }
    if (-not $capture.HasExited) {
        Stop-Process -Id $capture.Id -Force
        throw "The WGC capture helper exceeded its bounded deadline."
    }
    # Start-Process may report HasExited before redirected output handles have
    # completed and ExitCode has been populated. WaitForExit is immediate here
    # because the bounded loop above already observed process termination.
    $capture.WaitForExit()
    $capture.Refresh()
    $captureExitCode = $capture.ExitCode
    $captureCompleted = $false
    if (Test-Path -LiteralPath $captureStdout -PathType Leaf) {
        $captureCompleted = [bool](Get-Content -LiteralPath $captureStdout | Where-Object { $_ -like "QUEUEBACK_WGC_RECORDING_COMPLETE *" } | Select-Object -Last 1)
    }
    if (-not $expectedCaptureFailure -and $null -ne $captureExitCode -and $captureExitCode -ne 0) {
        $detail = if (Test-Path -LiteralPath $captureStderr) { Get-Content -Raw -LiteralPath $captureStderr } else { "no stderr" }
        throw "The WGC capture helper failed with exit code ${captureExitCode}: $detail"
    }
    if (-not $expectedCaptureFailure -and -not $captureCompleted) {
        $detail = if (Test-Path -LiteralPath $captureStderr) { Get-Content -Raw -LiteralPath $captureStderr } else { "no stderr" }
        throw "The WGC capture helper did not publish its success marker (exit code ${captureExitCode}): $detail"
    }
    if ($expectedCaptureFailure -and $captureCompleted) {
        throw "The $Scenario/$Interruption fixture was incorrectly reported as a successful recording."
    }
    if ($expectedCaptureFailure) {
        $failureDetail = if (Test-Path -LiteralPath $captureStderr) { Get-Content -Raw -LiteralPath $captureStderr } else { "" }
        if ($failureDetail -notmatch "(?m)^Error:") {
            throw "The $Scenario/$Interruption fixture did not return an explicit failed-partial error."
        }
    }
}
finally {
    $env:QUEUEBACK_MEDIA_RUNTIME_DIR = $previousRuntime
    if ($executionStateSet) {
        $continuousOnly = [uint32]::Parse("80000000", [System.Globalization.NumberStyles]::HexNumber)
        [void][QueueBackFixtureExecutionState]::SetThreadExecutionState($continuousOnly)
    }
    if ($capture -and -not $capture.HasExited) {
        Stop-Process -Id $capture.Id -Force
    }
    if ($fixture -and -not $fixture.HasExited) {
        Stop-Process -Id $fixture.Id -Force
    }
}

$video = Join-Path $bundle "video.mp4"
if (-not (Test-Path -LiteralPath $video -PathType Leaf) -or (Get-Item -LiteralPath $video).Length -le 0) {
    throw "The fixture recording is missing or empty at $video."
}
$ffmpeg = Join-Path $RuntimeRoot "bin\ffmpeg.exe"
$ffprobe = Join-Path $RuntimeRoot "bin\ffprobe.exe"
$probeResult = Invoke-BoundedProcess -FilePath $ffprobe -Arguments @("-v", "error", "-show_entries", "format=duration,size:stream=index,codec_type,codec_name,width,height,r_frame_rate,avg_frame_rate,has_b_frames", "-of", "json", $video) -TimeoutSeconds 30
if ($probeResult.ExitCode -ne 0) { throw "ffprobe rejected the fixture recording: $($probeResult.StandardError)" }
$probeText = $probeResult.StandardOutput
$probe = $probeText | ConvertFrom-Json
Write-Output "Fixture validation: ffprobe passed"
$videoStream = @($probe.streams | Where-Object codec_type -eq "video")
$audioStream = @($probe.streams | Where-Object codec_type -eq "audio")
if ($videoStream.Count -ne 1 -or $audioStream.Count -ne 1) {
    throw "Fixture output must contain exactly one video and one audio stream."
}
if ([string]$videoStream[0].r_frame_rate -ne "60/1") {
    throw "Fixture output nominal frame rate is $($videoStream[0].r_frame_rate), expected 60/1."
}
if ([int]$videoStream[0].width -ne 1920 -or [int]$videoStream[0].height -ne 1080) {
    throw "Fixture output is $($videoStream[0].width)x$($videoStream[0].height), expected 1920x1080."
}
if ($Encoder -eq "nvenc") {
    if ([int]$videoStream[0].has_b_frames -ne 0) {
        throw "NVENC output retained B-frames and therefore did not satisfy the four-surface low-latency contract."
    }
    if (Select-String -LiteralPath $captureStdout -SimpleMatch "increasing used surfaces" -Quiet) {
        throw "NVENC overrode QueueBack's finite surface limit."
    }
}

$decodeTimeoutSeconds = [Math]::Max(60, [Math]::Ceiling($DurationSeconds / 2))
$decodeResult = Invoke-BoundedProcess -FilePath $ffmpeg -Arguments @("-v", "error", "-i", $video, "-f", "null", "NUL") -TimeoutSeconds $decodeTimeoutSeconds
if ($decodeResult.ExitCode -ne 0) { throw "Full fixture decode failed: $($decodeResult.StandardError)" }
Write-Output "Fixture validation: full decode passed"
$frameMd5Path = Join-Path $runRoot "frames.framemd5"
$hashTimeoutSeconds = [Math]::Max(60, $DurationSeconds)
$hashResult = Invoke-BoundedProcess -FilePath $ffmpeg -Arguments @("-v", "error", "-i", $video, "-an", "-f", "framemd5", "-y", $frameMd5Path) -TimeoutSeconds $hashTimeoutSeconds
if ($hashResult.ExitCode -ne 0) { throw "Fixture frame-change analysis failed: $($hashResult.StandardError)" }
Write-Output "Fixture validation: framemd5 generated"
$decodedFrameCount = 0
$uniqueFrameHashes = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::Ordinal)
foreach ($line in [System.IO.File]::ReadLines($frameMd5Path)) {
    if ($line -match '^\d') {
        $decodedFrameCount++
        [void]$uniqueFrameHashes.Add(($line.Split(',')[-1]).Trim())
    }
}
$minimumDecodedFrames = if (-not $expectedCaptureFailure) { $DurationSeconds * 50 } else { $DurationSeconds * 10 }
$minimumUniqueFrames = if (-not $expectedCaptureFailure) { $DurationSeconds * 10 } else { $DurationSeconds * 2 }
if ($decodedFrameCount -lt $minimumDecodedFrames -or $uniqueFrameHashes.Count -lt $minimumUniqueFrames) {
    throw "Fixture media did not prove continuously changing video ($decodedFrameCount frames, $($uniqueFrameHashes.Count) unique hashes)."
}
Write-Output "Fixture validation: $decodedFrameCount frames and $($uniqueFrameHashes.Count) unique hashes passed"

if ($Scenario -ne "steady" -and -not $expectedCaptureFailure) {
    $actionLines = @(Get-Content -LiteralPath $fixtureStdout | Where-Object { $_ -like "QUEUEBACK_WGC_ACTION *" })
    if ($actionLines.Count -lt 2) {
        throw "The $Scenario fixture did not complete both scripted window actions."
    }
    Write-Output "Fixture validation: $Scenario window actions completed"
}

$evidenceLine = Get-Content -LiteralPath $captureStdout | Where-Object { $_ -like "QUEUEBACK_WGC_EVIDENCE *" } | Select-Object -Last 1
if (-not $evidenceLine) {
    throw "The WGC capture helper did not publish terminal capture evidence."
}
$evidence = ([string]$evidenceLine).Substring("QUEUEBACK_WGC_EVIDENCE ".Length) | ConvertFrom-Json
if (
    [int]$evidence.diagnostics_abi -ne 1 -or
    -not [bool]$evidence.capture_ready -or
    $null -ne $evidence.protocol_error -or
    [int]$evidence.frame_pool_capacity -ne 2 -or
    [int]$evidence.output_pool_capacity -ne 8 -or
    [uint64]$evidence.source_frames_surfaced -le 0 -or
    [uint64]$evidence.encoded_frames -le 0 -or
    [uint64]$evidence.muxed_bytes -le 0 -or
    [int64]$evidence.first_qpc -le 0 -or
    [int64]$evidence.latest_qpc -lt [int64]$evidence.first_qpc
) {
    throw "The WGC capture helper published invalid or incomplete bounded-pipeline evidence: $($evidence | ConvertTo-Json -Compress)"
}
if (-not $expectedCaptureFailure -and (-not [bool]$evidence.capture_terminal -or -not [bool]$evidence.progress_end)) {
    throw "The normal fixture did not publish both terminal capture and mux progress evidence."
}
if ($Interruption -eq "kill_encoder" -and ([bool]$evidence.capture_terminal -or [bool]$evidence.progress_end)) {
    throw "Forced encoder termination incorrectly published terminal success evidence."
}
Write-Output "Fixture validation: ABI-1 capture evidence and finite pools passed"

$liveEvidence = @(
    Get-Content -LiteralPath $captureStdout |
        Where-Object { $_ -like "QUEUEBACK_WGC_LIVE *" } |
        ForEach-Object { ([string]$_).Substring("QUEUEBACK_WGC_LIVE ".Length) | ConvertFrom-Json }
)
if ($CollectResources) {
    if ($liveEvidence.Count -lt 2) {
        throw "The resource fixture did not publish at least two live capture-progress points."
    }
    $previous = $null
    foreach ($point in $liveEvidence) {
        if ($null -ne $previous) {
            $gapSeconds = ([uint64]$point.elapsed_ms - [uint64]$previous.elapsed_ms) / 1000.0
            if ($gapSeconds -gt 15) {
                throw "Live capture-progress reporting contained a $gapSeconds-second gap."
            }
            foreach ($field in @("source_frames_surfaced", "encoded_frames", "muxed_bytes", "latest_qpc")) {
                if ([uint64]$point.$field -le [uint64]$previous.$field) {
                    throw "Live capture-progress field $field did not advance."
                }
            }
        }
        $previous = $point
    }
    if ([uint64]$evidence.elapsed_ms - [uint64]$liveEvidence[-1].elapsed_ms -gt 15000) {
        throw "Terminal capture evidence followed the final live progress point by more than 15 seconds."
    }
}

$resourceSummary = $null
if ($CollectResources) {
    $samples = $resourceSamples.ToArray()
    [System.IO.File]::WriteAllText((Join-Path $runRoot "resource-samples.json"), ($samples | ConvertTo-Json -Depth 6) + "`n")
    $completeSamples = @($samples | Where-Object { $null -ne $_.ffmpeg_private_memory_bytes -and [uint64]$_.ffmpeg_private_memory_bytes -gt 0 })
    if ($completeSamples.Count -lt 2) {
        throw "Resource collection did not observe at least two live FFmpeg samples."
    }
    $steadyAfterSeconds = [Math]::Min(30, [Math]::Max(1, $DurationSeconds / 4))
    $steadyInitial = $completeSamples | Where-Object { $_.elapsed_seconds -ge $steadyAfterSeconds } | Select-Object -First 1
    if ($null -eq $steadyInitial) { $steadyInitial = $completeSamples[0] }
    $steadyFinal = $completeSamples[-1]
    $maximumOutputStallSeconds = 0.0
    for ($index = 1; $index -lt $liveEvidence.Count; $index++) {
        $maximumOutputStallSeconds = [Math]::Max(
            $maximumOutputStallSeconds,
            ([uint64]$liveEvidence[$index].elapsed_ms - [uint64]$liveEvidence[$index - 1].elapsed_ms) / 1000.0
        )
    }
    $encoderDepth = 4
    $textureBudgetBytes = ([uint64]1920 * 1080 * 4 * (2 + 8)) + ([uint64]1920 * 1080 * 3 / 2 * (32 + $encoderDepth))
    $resourceSummary = [ordered]@{
        sample_interval_seconds = $ResourceSampleSeconds
        sample_count = $completeSamples.Count
        steady_initial_elapsed_seconds = $steadyInitial.elapsed_seconds
        steady_initial_ffmpeg_private_bytes = $steadyInitial.ffmpeg_private_memory_bytes
        peak_ffmpeg_private_bytes = Get-MaximumProperty -Samples $completeSamples -Property "ffmpeg_private_memory_bytes"
        final_ffmpeg_private_bytes = $steadyFinal.ffmpeg_private_memory_bytes
        final_minus_steady_initial_private_bytes = [int64]$steadyFinal.ffmpeg_private_memory_bytes - [int64]$steadyInitial.ffmpeg_private_memory_bytes
        peak_ffmpeg_gpu_dedicated_bytes = Get-MaximumProperty -Samples $completeSamples -Property "ffmpeg_gpu_dedicated_bytes"
        final_ffmpeg_gpu_dedicated_bytes = $steadyFinal.ffmpeg_gpu_dedicated_bytes
        peak_ffmpeg_gpu_shared_bytes = Get-MaximumProperty -Samples $completeSamples -Property "ffmpeg_gpu_shared_bytes"
        maximum_output_stall_seconds = [Math]::Round($maximumOutputStallSeconds, 3)
        output_progress_source = "ffmpeg_in_process_mux_counter"
        declared_maximum_texture_bytes = $textureBudgetBytes
    }
    if ($maximumOutputStallSeconds -gt 15) {
        throw "Fixture output did not grow for $maximumOutputStallSeconds seconds during the bounded-resource run."
    }
    Write-Output "Fixture validation: bounded resource samples and output growth passed"
}

$report = [ordered]@{
    schema_version = 2
    run_id = $runId
    encoder = $Encoder
    scenario = $Scenario
    interruption = $Interruption
    expected_outcome = $(if (-not $expectedCaptureFailure) { "success" } else { "failed_partial" })
    duration_seconds_requested = $DurationSeconds
    runtime_id = (Get-Content -Raw -LiteralPath (Join-Path $RuntimeRoot "runtime-manifest.json") | ConvertFrom-Json).runtime_id
    target = [string]$target
    video = $video
    video_bytes = (Get-Item -LiteralPath $video).Length
    decoded_frames = $decodedFrameCount
    unique_frame_hashes = $uniqueFrameHashes.Count
    capture_evidence = $evidence
    resources = $resourceSummary
    media = $probe
}
Write-Output "Fixture validation: report object created"
[System.IO.File]::WriteAllText((Join-Path $runRoot "result.json"), ($report | ConvertTo-Json -Depth 12) + "`n")
Write-Output "Fixture validation: report persisted"
Write-Output "WGC fixture passed: $runRoot"
