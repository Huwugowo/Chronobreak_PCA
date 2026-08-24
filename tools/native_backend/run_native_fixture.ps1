[CmdletBinding()]
param(
    [ValidateSet("steady", "resize", "minimize_restore", "occlusion", "close_window")]
    [string]$Scenario = "steady",
    [ValidateSet("none", "nvenc_failure")]
    [string]$Interruption = "none",
    [ValidateRange(2, 7200)]
    [int]$DurationSeconds = 10,
    [switch]$CollectResources,
    [switch]$KeepTargetVisible,
    [ValidateRange(1, 60)]
    [int]$ResourceSampleSeconds = 5,
    [string]$RuntimeRoot,
    [string]$OutputRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-BoundedProcess {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][ValidateRange(1, 7200)][int]$TimeoutSeconds
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

function Write-Json {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][object]$Value)

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText(
        $Path,
        (ConvertTo-Json -InputObject $Value -Depth 12),
        $encoding
    )
}

function Get-GpuProcessMemory {
    param([Parameter(Mandatory = $true)][int]$ProcessId)

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
            if (-not $sample.InstanceName.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
                continue
            }
            $matched = $true
            if ($sample.Path -like "*\Dedicated Usage") {
                $dedicated += [uint64][Math]::Max(0, $sample.CookedValue)
            }
            if ($sample.Path -like "*\Shared Usage") {
                $shared += [uint64][Math]::Max(0, $sample.CookedValue)
            }
        }
        if (-not $matched) {
            return $null
        }
        return [pscustomobject]@{
            dedicated_bytes = $dedicated
            shared_bytes = $shared
        }
    }
    catch {
        return $null
    }
}

function Get-Median {
    param([object[]]$Values)

    $numbers = @($Values | Where-Object { $null -ne $_ } | Sort-Object)
    if ($numbers.Count -eq 0) {
        return $null
    }
    $middle = [Math]::Floor($numbers.Count / 2)
    if (($numbers.Count % 2) -eq 1) {
        return [double]$numbers[$middle]
    }
    return ([double]$numbers[$middle - 1] + [double]$numbers[$middle]) / 2.0
}

function Get-Maximum {
    param([object[]]$Values)

    $numbers = @($Values | Where-Object { $null -ne $_ })
    if ($numbers.Count -eq 0) {
        return $null
    }
    return ($numbers | Measure-Object -Maximum).Maximum
}

function Get-NativeTelemetry {
    param([string]$Line)

    if ([string]::IsNullOrWhiteSpace($Line)) {
        return $null
    }
    $values = [ordered]@{}
    foreach ($match in [Regex]::Matches($Line, '(?<key>[a-z0-9_]+)=(?<value>[^\s]+)')) {
        $values[$match.Groups['key'].Value] = $match.Groups['value'].Value
    }
    return [pscustomobject]$values
}

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$allowedRoot = [System.IO.Path]::GetFullPath((Join-Path $repository "build\perf"))
if ([string]::IsNullOrWhiteSpace($RuntimeRoot)) {
    $RuntimeRoot = Join-Path $repository "build\media-runtime\windows-x86_64"
}
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $allowedRoot "qb-perf-002-native-fixture"
}
$RuntimeRoot = [System.IO.Path]::GetFullPath($RuntimeRoot)
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
if (-not ($OutputRoot -eq $allowedRoot -or $OutputRoot.StartsWith(
    $allowedRoot + [System.IO.Path]::DirectorySeparatorChar,
    [StringComparison]::OrdinalIgnoreCase
))) {
    throw "Native fixture output must stay inside $allowedRoot."
}

& (Join-Path $repository "tools\media_runtime\verify.ps1") -RuntimeRoot $RuntimeRoot
if ($LASTEXITCODE -ne 0) {
    throw "The packaged media runtime did not verify."
}

New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
$rootSentinel = Join-Path $OutputRoot ".queueback-native-fixture-root"
if (-not (Test-Path -LiteralPath $rootSentinel -PathType Leaf)) {
    [System.IO.File]::WriteAllText($rootSentinel, "QueueBack dedicated native fixture outputs only.`n")
}
$runId = (Get-Date -Format "yyyyMMdd-HHmmss") + "-$Scenario-$Interruption-" + [Guid]::NewGuid().ToString("N").Substring(0, 8)
$runRoot = Join-Path $OutputRoot $runId
New-Item -ItemType Directory -Path $runRoot | Out-Null

$buildLog = Join-Path $runRoot "cargo-build.log"
$previousPreference = $ErrorActionPreference
try {
    $ErrorActionPreference = "Continue"
    $buildOutput = & cargo build --manifest-path (Join-Path $repository "recorder\Cargo.toml") `
        --release --example wgc_fixture --example native_mp4_probe `
        --features native-failure-injection 2>&1
    $buildExitCode = $LASTEXITCODE
}
finally {
    $ErrorActionPreference = $previousPreference
}
[System.IO.File]::WriteAllLines($buildLog, [string[]]$buildOutput)
if ($buildExitCode -ne 0) {
    throw "Could not build the release native fixture executables; see $buildLog."
}

$fixtureStdout = Join-Path $runRoot "fixture.stdout.log"
$fixtureStderr = Join-Path $runRoot "fixture.stderr.log"
$captureStdout = Join-Path $runRoot "capture.stdout.log"
$captureStderr = Join-Path $runRoot "capture.stderr.log"
$video = Join-Path $runRoot "video.mp4"
$fixtureExe = Join-Path $repository "recorder\target\release\examples\wgc_fixture.exe"
$captureExe = Join-Path $repository "recorder\target\release\examples\native_mp4_probe.exe"
$ffmpeg = Join-Path $RuntimeRoot "bin\ffmpeg.exe"
$ffprobe = Join-Path $RuntimeRoot "bin\ffprobe.exe"
$fixture = $null
$capture = $null
$executionStateSet = $false
$expectedFailure = $Scenario -eq "close_window" -or $Interruption -ne "none"
$resourceSamples = New-Object 'System.Collections.Generic.List[object]'
$resourceFfmpegPid = $null

try {
    if ($KeepTargetVisible) {
        Add-Type -TypeDefinition @"
using System.Runtime.InteropServices;
public static class QueueBackNativeFixtureExecutionState {
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern uint SetThreadExecutionState(uint flags);
}
"@
        $continuousDisplayAndSystemRequired = [uint32]::Parse("80000003", [System.Globalization.NumberStyles]::HexNumber)
        if ([QueueBackNativeFixtureExecutionState]::SetThreadExecutionState($continuousDisplayAndSystemRequired) -eq 0) {
            throw "Windows refused the fixture display-required execution state."
        }
        $executionStateSet = $true
    }

    $firstActionSeconds = [Math]::Max(1, [Math]::Floor($DurationSeconds / 3))
    $secondActionSeconds = [Math]::Max($firstActionSeconds + 1, [Math]::Floor($DurationSeconds * 2 / 3))
    if ($Scenario -eq "close_window") {
        $firstActionSeconds = [Math]::Max(3, [Math]::Floor($DurationSeconds * 0.7))
        $secondActionSeconds = $firstActionSeconds + 1
    }
    $fixtureArguments = @(
        "--duration-seconds", ($DurationSeconds + 45),
        "--scenario", $Scenario,
        "--action-after-seconds", $firstActionSeconds,
        "--restore-after-seconds", $secondActionSeconds
    )
    if ($KeepTargetVisible) {
        $fixtureArguments += "--always-on-top"
    }
    $fixture = Start-Process -FilePath $fixtureExe -ArgumentList $fixtureArguments `
        -RedirectStandardOutput $fixtureStdout -RedirectStandardError $fixtureStderr -PassThru
    [void]$fixture.Handle

    $target = $null
    for ($attempt = 0; $attempt -lt 150; $attempt++) {
        Start-Sleep -Milliseconds 100
        if (Test-Path -LiteralPath $fixtureStdout -PathType Leaf) {
            $targetLine = Get-Content -LiteralPath $fixtureStdout |
                Where-Object { $_ -like "QUEUEBACK_WGC_TARGET*" } |
                Select-Object -Last 1
            if ($targetLine) {
                $target = [string]$targetLine
                break
            }
        }
        if ($fixture.HasExited) {
            throw "The native fixture target exited before publishing its HWND."
        }
    }
    if (-not $target -or $target -notmatch "hwnd=(\d+) adapter_index=(\d+)") {
        throw "The native fixture did not publish a valid target: $target"
    }
    if ($target -notmatch "width=1920 height=1080") {
        throw "The native fixture target is not the required physical 1920x1080 client area: $target"
    }

    $captureArguments = @(
        "--pid", $fixture.Id,
        "--ffmpeg", $ffmpeg,
        "--output", $video,
        "--duration-seconds", $DurationSeconds
    )
    if ($Interruption -eq "nvenc_failure") {
        $failureTick = [Math]::Max(60, [Math]::Min(120, $DurationSeconds * 30))
        $captureArguments += @("--fail-nvenc-after-ticks", $failureTick)
    }

    $capture = Start-Process -FilePath $captureExe -ArgumentList $captureArguments `
        -RedirectStandardOutput $captureStdout -RedirectStandardError $captureStderr `
        -WindowStyle Hidden -PassThru
    [void]$capture.Handle
    $captureStarted = [DateTime]::UtcNow
    $deadline = $captureStarted.AddSeconds($DurationSeconds + 45)
    $nextResourceSample = $captureStarted

    while (-not $capture.HasExited -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 200
        $capture.Refresh()
        # Do not inspect ProcessThreadCollection after the probe has exited;
        # Windows can leave a process object briefly addressable while its
        # thread collection is already unavailable.
        if ($capture.HasExited) {
            break
        }
        if ($CollectResources -and [DateTime]::UtcNow -ge $nextResourceSample) {
            if ($null -eq $resourceFfmpegPid) {
                $child = Get-CimInstance -ClassName Win32_Process `
                    -Filter "ParentProcessId=$($capture.Id)" -ErrorAction SilentlyContinue |
                    Where-Object { $_.Name -ieq "ffmpeg.exe" } |
                    Select-Object -First 1
                if ($null -ne $child) {
                    $resourceFfmpegPid = [int]$child.ProcessId
                }
            }
            $captureProcess = Get-Process -Id $capture.Id -ErrorAction SilentlyContinue
            $ffmpegProcess = if ($null -ne $resourceFfmpegPid) {
                Get-Process -Id $resourceFfmpegPid -ErrorAction SilentlyContinue
            } else {
                $null
            }
            if ($captureProcess) {
                $captureProcess.Refresh()
            }
            if ($ffmpegProcess) {
                $ffmpegProcess.Refresh()
            }
            $gpuMemory = if ($captureProcess) {
                Get-GpuProcessMemory -ProcessId $capture.Id
            } else {
                $null
            }
            $outputBytes = if (Test-Path -LiteralPath $video -PathType Leaf) {
                [uint64](Get-Item -LiteralPath $video).Length
            } else {
                [uint64]0
            }
            $resourceSamples.Add([pscustomobject][ordered]@{
                elapsed_seconds = [Math]::Round(([DateTime]::UtcNow - $captureStarted).TotalSeconds, 3)
                capture_pid = $capture.Id
                capture_cpu_seconds = $(if ($captureProcess) { [double]$captureProcess.CPU } else { $null })
                capture_working_set_bytes = $(if ($captureProcess) { [uint64]$captureProcess.WorkingSet64 } else { $null })
                capture_private_memory_bytes = $(if ($captureProcess) { [uint64]$captureProcess.PrivateMemorySize64 } else { $null })
                capture_handles = $(if ($captureProcess) { [uint32]$captureProcess.HandleCount } else { $null })
                capture_threads = $(if ($captureProcess) { [uint32]$captureProcess.Threads.Count } else { $null })
                capture_gpu_dedicated_bytes = $(if ($gpuMemory) { [uint64]$gpuMemory.dedicated_bytes } else { $null })
                capture_gpu_shared_bytes = $(if ($gpuMemory) { [uint64]$gpuMemory.shared_bytes } else { $null })
                ffmpeg_pid = $resourceFfmpegPid
                ffmpeg_cpu_seconds = $(if ($ffmpegProcess) { [double]$ffmpegProcess.CPU } else { $null })
                ffmpeg_working_set_bytes = $(if ($ffmpegProcess) { [uint64]$ffmpegProcess.WorkingSet64 } else { $null })
                ffmpeg_private_memory_bytes = $(if ($ffmpegProcess) { [uint64]$ffmpegProcess.PrivateMemorySize64 } else { $null })
                ffmpeg_handles = $(if ($ffmpegProcess) { [uint32]$ffmpegProcess.HandleCount } else { $null })
                ffmpeg_threads = $(if ($ffmpegProcess) { [uint32]$ffmpegProcess.Threads.Count } else { $null })
                output_bytes = $outputBytes
            })
            $nextResourceSample = [DateTime]::UtcNow.AddSeconds($ResourceSampleSeconds)
        }
    }
    if (-not $capture.HasExited) {
        Stop-Process -Id $capture.Id -Force -ErrorAction SilentlyContinue
        throw "The native capture probe exceeded its bounded deadline."
    }
    [void]$capture.WaitForExit()
    $capture.Refresh()
    $captureExitCode = $capture.ExitCode
}
finally {
    if ($executionStateSet) {
        $continuousOnly = [uint32]::Parse("80000000", [System.Globalization.NumberStyles]::HexNumber)
        [void][QueueBackNativeFixtureExecutionState]::SetThreadExecutionState($continuousOnly)
    }
    if ($capture -and -not $capture.HasExited) {
        Stop-Process -Id $capture.Id -Force -ErrorAction SilentlyContinue
    }
    if ($fixture -and -not $fixture.HasExited) {
        Stop-Process -Id $fixture.Id -Force -ErrorAction SilentlyContinue
    }
}

$captureOutput = if (Test-Path -LiteralPath $captureStdout -PathType Leaf) {
    Get-Content -Raw -LiteralPath $captureStdout
} else {
    ""
}
$captureError = if (Test-Path -LiteralPath $captureStderr -PathType Leaf) {
    Get-Content -Raw -LiteralPath $captureStderr
} else {
    ""
}

# Persist long-run resource evidence before media validation. A post-capture
# decoder/prober failure must never discard the already collected soak series.
if ($CollectResources) {
    # Windows PowerShell's binder can reject @($genericList) with an
    # ArgumentException. Materialize a normal object array explicitly.
    [object[]]$resourceSampleArray = $resourceSamples.ToArray()
    Write-Json -Path (Join-Path $runRoot "resource-samples.json") -Value $resourceSampleArray
    $resourceSampleArray | Export-Csv -LiteralPath (Join-Path $runRoot "resource-samples.csv") -NoTypeInformation -Encoding UTF8
}

$passLine = Get-Content -LiteralPath $captureStdout -ErrorAction SilentlyContinue |
    Where-Object { $_ -like "CHRONOBREAK_NATIVE_MP4_PASS *" } |
    Select-Object -Last 1
if (-not $expectedFailure) {
    if ($captureExitCode -ne 0 -or -not $passLine) {
        throw "The native fixture failed unexpectedly with exit ${captureExitCode}: $captureError"
    }
} else {
    if ($captureExitCode -eq 0 -or $passLine) {
        throw "The $Scenario/$Interruption native fixture incorrectly reported success."
    }
    $requiredFailure = if ($Scenario -eq "close_window") {
        "native WGC target closed while recording"
    } else {
        "injected terminal native NVENC failure"
    }
    if ($captureError -notlike "*$requiredFailure*") {
        throw "The $Scenario/$Interruption native fixture did not return its exact expected failure: $captureError"
    }
}

if (-not (Test-Path -LiteralPath $video -PathType Leaf) -or (Get-Item -LiteralPath $video).Length -le 0) {
    throw "The native fixture output is missing or empty at $video."
}

$probeResult = Invoke-BoundedProcess -FilePath $ffprobe -Arguments @(
    "-v", "error",
    "-show_entries", "format=duration,size:stream=index,codec_type,codec_name,width,height,r_frame_rate,avg_frame_rate,start_time,duration",
    "-of", "json", $video
) -TimeoutSeconds 60
if ($probeResult.ExitCode -ne 0) {
    throw "ffprobe rejected the native fixture output: $($probeResult.StandardError)"
}
[System.IO.File]::WriteAllText((Join-Path $runRoot "ffprobe.json"), $probeResult.StandardOutput)
$probe = $probeResult.StandardOutput | ConvertFrom-Json
$videoStreams = @($probe.streams | Where-Object codec_type -eq "video")
$audioStreams = @($probe.streams | Where-Object codec_type -eq "audio")
if ($videoStreams.Count -ne 1 -or $audioStreams.Count -ne 1) {
    throw "Native fixture output must contain exactly one video and one audio stream."
}
$videoStream = $videoStreams[0]
if ([string]$videoStream.codec_name -ne "h264" -or [string]$audioStreams[0].codec_name -ne "aac") {
    throw "Native fixture output must use H.264 video and AAC audio."
}
if ([int]$videoStream.width -ne 1920 -or [int]$videoStream.height -ne 1080 -or [string]$videoStream.r_frame_rate -ne "60/1") {
    throw "Native fixture output is not the required 1920x1080 at 60/1 FPS."
}

$decodeTimeout = [Math]::Max(60, [Math]::Ceiling($DurationSeconds / 2))
$decodeResult = Invoke-BoundedProcess -FilePath $ffmpeg -Arguments @(
    "-v", "error", "-xerror", "-threads", "1", "-i", $video, "-f", "null", "NUL"
) -TimeoutSeconds $decodeTimeout
if ($decodeResult.ExitCode -ne 0) {
    throw "Full native fixture decode failed: $($decodeResult.StandardError)"
}

$frameMd5Path = Join-Path $runRoot "frames.framemd5"
$hashResult = Invoke-BoundedProcess -FilePath $ffmpeg -Arguments @(
    "-v", "error", "-xerror", "-threads", "1", "-i", $video, "-an", "-f", "framemd5", "-y", $frameMd5Path
) -TimeoutSeconds ([Math]::Max(60, $DurationSeconds))
if ($hashResult.ExitCode -ne 0) {
    throw "Native fixture frame-change analysis failed: $($hashResult.StandardError)"
}
$decodedFrames = 0
$uniqueHashes = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::Ordinal)
foreach ($line in [System.IO.File]::ReadLines($frameMd5Path)) {
    if ($line -match '^\d') {
        $decodedFrames++
        [void]$uniqueHashes.Add(($line.Split(',')[-1]).Trim())
    }
}
if (-not $expectedFailure) {
    $expectedFrames = $DurationSeconds * 60
    if ($decodedFrames -ne $expectedFrames) {
        throw "Native fixture decoded $decodedFrames frames instead of the exact $expectedFrames CFR contract."
    }
    $minimumUnique = if ($Scenario -eq "minimize_restore") {
        $DurationSeconds * 5
    } else {
        $DurationSeconds * 10
    }
    if ($uniqueHashes.Count -lt $minimumUnique) {
        throw "Native fixture did not prove changing media ($decodedFrames frames, $($uniqueHashes.Count) unique hashes)."
    }
} elseif ($decodedFrames -lt 60 -or $uniqueHashes.Count -lt 10) {
    throw "Native failed-partial output is too short or static ($decodedFrames frames, $($uniqueHashes.Count) unique hashes)."
}

$actionLines = @(Get-Content -LiteralPath $fixtureStdout -ErrorAction SilentlyContinue |
    Where-Object { $_ -like "QUEUEBACK_WGC_ACTION *" })
if ($Scenario -notin @("steady", "close_window") -and $actionLines.Count -lt 2) {
    throw "The native $Scenario fixture did not complete both scripted window actions."
}
if ($Scenario -eq "close_window" -and $actionLines.Count -lt 1) {
    throw "The native close-window fixture did not close its target."
}

$resourceSummary = $null
if ($CollectResources) {
    if ($resourceSampleArray.Count -lt 2) {
        throw "Native resource collection retained fewer than two samples."
    }
    $windowCount = [int][Math]::Min(6, [Math]::Floor($resourceSampleArray.Count / 2))
    $firstWindow = @($resourceSampleArray | Select-Object -First $windowCount)
    $lastWindow = @($resourceSampleArray | Select-Object -Last $windowCount)
    $firstPrivate = Get-Median @($firstWindow | ForEach-Object { $_.capture_private_memory_bytes })
    $lastPrivate = Get-Median @($lastWindow | ForEach-Object { $_.capture_private_memory_bytes })
    $sampleSpan = [double]$resourceSampleArray[-1].elapsed_seconds - [double]$resourceSampleArray[0].elapsed_seconds
    $resourceSummary = [ordered]@{
        sample_count = $resourceSampleArray.Count
        sample_interval_seconds = $ResourceSampleSeconds
        sampled_span_seconds = $sampleSpan
        capture_private_first_window_median_bytes = $firstPrivate
        capture_private_last_window_median_bytes = $lastPrivate
        capture_private_window_growth_bytes = $lastPrivate - $firstPrivate
        capture_private_window_growth_bytes_per_second = $(if ($sampleSpan -gt 0) { ($lastPrivate - $firstPrivate) / $sampleSpan } else { $null })
        capture_private_max_bytes = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.capture_private_memory_bytes })
        capture_working_set_max_bytes = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.capture_working_set_bytes })
        capture_handles_max = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.capture_handles })
        capture_threads_max = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.capture_threads })
        capture_gpu_dedicated_max_bytes = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.capture_gpu_dedicated_bytes })
        capture_gpu_shared_max_bytes = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.capture_gpu_shared_bytes })
        ffmpeg_private_max_bytes = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.ffmpeg_private_memory_bytes })
        ffmpeg_working_set_max_bytes = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.ffmpeg_working_set_bytes })
        ffmpeg_handles_max = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.ffmpeg_handles })
        ffmpeg_threads_max = Get-Maximum @($resourceSampleArray | ForEach-Object { $_.ffmpeg_threads })
        first_output_bytes = [uint64]$resourceSampleArray[0].output_bytes
        last_output_bytes = [uint64]$resourceSampleArray[-1].output_bytes
    }
    Write-Json -Path (Join-Path $runRoot "resource-summary.json") -Value $resourceSummary
}

$telemetry = Get-NativeTelemetry -Line ([string]$passLine)
$result = [ordered]@{
    schema = 1
    scope = "generated non-League native-backend fixture"
    backend = "native_windows_graphics_capture_d3d11_nvenc"
    scenario = $Scenario
    interruption = $Interruption
    expected_failure = $expectedFailure
    observed_exit_code = $captureExitCode
    duration_seconds_requested = $DurationSeconds
    runtime_id = (Get-Content -Raw -LiteralPath (Join-Path $RuntimeRoot "runtime-manifest.json") | ConvertFrom-Json).runtime_id
    target = [string]$target
    action_lines = @($actionLines | ForEach-Object { [string]$_ })
    decoded_frames = $decodedFrames
    unique_frame_hashes = $uniqueHashes.Count
    video_bytes = (Get-Item -LiteralPath $video).Length
    video_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $video).Hash.ToLowerInvariant()
    ffprobe = $probe
    native_telemetry = $telemetry
    expected_failure_detail = $(if ($expectedFailure) { $captureError.Trim() } else { $null })
    resource_summary = $resourceSummary
    full_decode = "pass"
    result = "pass"
}
Write-Json -Path (Join-Path $runRoot "result.json") -Value $result

Write-Output "CHRONOBREAK_NATIVE_FIXTURE=PASS"
Write-Output "EVIDENCE=$runRoot"
Write-Output ($result | ConvertTo-Json -Depth 4)
