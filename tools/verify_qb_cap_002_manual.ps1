[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BundleDirectory,

    [Parameter(Mandatory = $true)]
    [ValidateSet("normal", "forced")]
    [string]$Mode,

    [string]$FfmpegPath = "ffmpeg.exe",
    [string]$FfprobePath = "ffprobe.exe",
    [int]$MinimumNormalDurationSeconds = 1500
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-Condition {
    param(
        [bool]$Condition,
        [string]$Message
    )
    if (-not $Condition) {
        throw "[QB-CAP-002] $Message"
    }
}

function Resolve-Executable {
    param(
        [string]$Value,
        [string]$Label
    )
    if ([System.IO.Path]::IsPathRooted($Value) -or $Value.Contains([System.IO.Path]::DirectorySeparatorChar)) {
        $resolved = Resolve-Path -LiteralPath $Value -ErrorAction SilentlyContinue
        Assert-Condition ($null -ne $resolved -and (Test-Path -LiteralPath $resolved.Path -PathType Leaf)) "$Label executable was not found: $Value"
        return $resolved.Path
    }
    $command = Get-Command -Name $Value -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    Assert-Condition ($null -ne $command) "$Label executable was not found on PATH: $Value"
    return $command.Source
}

function ConvertTo-NativeArgument {
    param(
        [AllowEmptyString()]
        [string]$Value
    )
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') {
        return $Value
    }

    $quoted = [System.Text.StringBuilder]::new()
    $null = $quoted.Append('"')
    $backslashCount = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $backslashCount += 1
            continue
        }
        if ($character -eq '"') {
            $null = $quoted.Append(('\' * (($backslashCount * 2) + 1)))
            $null = $quoted.Append('"')
            $backslashCount = 0
            continue
        }
        if ($backslashCount -gt 0) {
            $null = $quoted.Append(('\' * $backslashCount))
            $backslashCount = 0
        }
        $null = $quoted.Append($character)
    }
    if ($backslashCount -gt 0) {
        $null = $quoted.Append(('\' * ($backslashCount * 2)))
    }
    $null = $quoted.Append('"')
    return $quoted.ToString()
}

function Invoke-Captured {
    param(
        [string]$Executable,
        [string[]]$Arguments,
        [int]$TimeoutSeconds,
        [string]$Label
    )
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Executable
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    if ($null -ne $startInfo.PSObject.Properties["ArgumentList"]) {
        foreach ($argument in $Arguments) {
            $null = $startInfo.ArgumentList.Add($argument)
        }
    } else {
        $startInfo.Arguments = (($Arguments | ForEach-Object {
                    ConvertTo-NativeArgument -Value $_
                }) -join " ")
    }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    Assert-Condition $process.Start() "Could not start $Label."
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        try { $process.Kill() } catch {}
        throw "[QB-CAP-002] $Label exceeded its $TimeoutSeconds-second deadline."
    }
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    return [pscustomobject]@{
        ExitCode = $process.ExitCode
        Stdout = $stdout
        Stderr = $stderr
    }
}

function Get-RequiredProperty {
    param(
        [object]$Object,
        [string]$Name,
        [string]$Context
    )
    $property = $Object.PSObject.Properties[$Name]
    Assert-Condition ($null -ne $property) "$Context is missing required property '$Name'."
    return $property.Value
}

$bundle = Resolve-Path -LiteralPath $BundleDirectory -ErrorAction SilentlyContinue
Assert-Condition ($null -ne $bundle -and (Test-Path -LiteralPath $bundle.Path -PathType Container)) "Bundle directory was not found: $BundleDirectory"
$bundlePath = [System.IO.Path]::GetFullPath($bundle.Path)
$bundleInfo = Get-Item -LiteralPath $bundlePath
Assert-Condition ($bundleInfo.Name -match '^\d+(?:-\d+)?$') "Bundle ID is not a recorder-safe digits or digits-suffix identifier: $($bundleInfo.Name)"
$gamesDirectory = $bundleInfo.Parent
Assert-Condition ($gamesDirectory.Name -eq "games") "Bundle must be directly inside a games directory."
$libraryDirectory = $gamesDirectory.Parent.FullName
$sentinel = Join-Path $libraryDirectory ".queueback-cap-002-manual-library"
Assert-Condition (Test-Path -LiteralPath $sentinel -PathType Leaf) "Refusing a non-sentinel library: $libraryDirectory"

$statePath = Join-Path $bundlePath "recording-state.json"
$metadataPath = Join-Path $bundlePath "metadata.json"
$pendingMetadataPath = Join-Path $bundlePath ".metadata.json.pending"
$gameLogPath = Join-Path $bundlePath "game_log.json"
$videoPath = Join-Path $bundlePath "video.mp4"
Assert-Condition (Test-Path -LiteralPath $statePath -PathType Leaf) "recording-state.json is missing."
Assert-Condition (-not (Test-Path -LiteralPath $pendingMetadataPath)) "Pending metadata remains after terminal finalization."

$state = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json
Assert-Condition ((Get-RequiredProperty $state "schema_version" "recording state") -eq 1) "recording-state schema is not version 1."
Assert-Condition ((Get-RequiredProperty $state "bundle_id" "recording state") -eq $bundleInfo.Name) "recording-state bundle_id does not match its directory."
$health = Get-RequiredProperty $state "health" "recording state"
$finalization = Get-RequiredProperty $state "finalization" "recording state"
Assert-Condition ($null -ne $finalization) "Terminal recording state has no finalization evidence."
$terminalReasonProperty = $finalization.PSObject.Properties["terminal_reason"]
$terminalReason = if ($null -eq $terminalReasonProperty) { $null } else { $terminalReasonProperty.Value }
$declaredBytes = [uint64](Get-RequiredProperty $finalization "output_bytes" "finalization")
$declaredPresent = [bool](Get-RequiredProperty $finalization "output_present" "finalization")
$videoExists = Test-Path -LiteralPath $videoPath -PathType Leaf
$actualBytes = if ($videoExists) { [uint64](Get-Item -LiteralPath $videoPath).Length } else { [uint64]0 }
Assert-Condition ($declaredPresent -eq $videoExists) "Declared output presence disagrees with video.mp4."
Assert-Condition ($declaredBytes -eq $actualBytes) "Declared output bytes ($declaredBytes) disagree with video.mp4 ($actualBytes)."

if ($Mode -eq "normal") {
    Assert-Condition ($state.lifecycle -eq "finalized") "Normal run lifecycle is '$($state.lifecycle)', expected finalized."
    Assert-Condition ($health.current -eq "finalized") "Normal run current health is '$($health.current)', expected finalized."
    Assert-Condition ($health.verdict -eq "clean") "Normal run verdict is '$($health.verdict)', expected clean."
    Assert-Condition ([uint64]$health.incident_count -eq 0) "Normal run recorded one or more health incidents."
    Assert-Condition ([uint64]$health.total_stall_ms -eq 0 -and [uint64]$health.longest_stall_ms -eq 0) "Normal run recorded nonzero stall aggregates."
    Assert-Condition ($finalization.result -eq "graceful") "Normal run result is '$($finalization.result)', expected graceful."
    Assert-Condition ([int]$finalization.exit_code -eq 0) "Normal run FFmpeg exit code is not zero."
    Assert-Condition (-not [bool]$finalization.forced) "Normal run required forced FFmpeg termination."
    Assert-Condition ([bool]$finalization.terminal_progress) "Normal run did not receive terminal FFmpeg progress."
    Assert-Condition ($null -eq $terminalReason) "Normal run has an unexpected terminal reason: $terminalReason"
    Assert-Condition ($videoExists -and $actualBytes -gt 0) "Normal run has no nonempty video.mp4."
    Assert-Condition (Test-Path -LiteralPath $metadataPath -PathType Leaf) "Normal run did not publish canonical metadata.json."
    Assert-Condition (Test-Path -LiteralPath $gameLogPath -PathType Leaf) "Normal run did not preserve game_log.json."
} else {
    Assert-Condition ($state.lifecycle -eq "failed_partial") "Forced run lifecycle is '$($state.lifecycle)', expected failed_partial with preserved media."
    Assert-Condition ($health.current -eq "failed_partial") "Forced run current health is '$($health.current)', expected failed_partial."
    Assert-Condition ($health.verdict -eq "failed") "Forced run verdict is '$($health.verdict)', expected failed."
    Assert-Condition ($finalization.result -eq "failed") "Forced run result is '$($finalization.result)', expected failed."
    Assert-Condition ([string]$terminalReason -in @("league_process_terminated", "league_exit_unobservable")) "Forced run reason '$terminalReason' is not a supported League-exit failure."
    Assert-Condition ($videoExists -and $actualBytes -gt 0) "Forced run did not preserve a nonempty partial video.mp4."
    Assert-Condition (-not (Test-Path -LiteralPath $metadataPath)) "Forced run incorrectly published successful metadata.json."
}

$ffprobe = Resolve-Executable -Value $FfprobePath -Label "ffprobe"
$probeResult = Invoke-Captured -Executable $ffprobe -Arguments @(
    "-v", "error",
    "-show_entries", "format=duration:stream=index,codec_type,codec_name,width,height,avg_frame_rate",
    "-of", "json",
    $videoPath
) -TimeoutSeconds 120 -Label "ffprobe"
Assert-Condition ($probeResult.ExitCode -eq 0) "ffprobe rejected video.mp4: $($probeResult.Stderr.Trim())"
$probe = $probeResult.Stdout | ConvertFrom-Json
$videoStreams = @($probe.streams | Where-Object { $_.codec_type -eq "video" })
$audioStreams = @($probe.streams | Where-Object { $_.codec_type -eq "audio" })
Assert-Condition ($videoStreams.Count -eq 1) "Expected exactly one video stream, found $($videoStreams.Count)."
Assert-Condition ($audioStreams.Count -eq 1) "Expected exactly one audio stream, found $($audioStreams.Count)."
$durationSeconds = [double]$probe.format.duration
Assert-Condition (-not [double]::IsNaN($durationSeconds) -and -not [double]::IsInfinity($durationSeconds) -and $durationSeconds -gt 0) "ffprobe reported an invalid duration."

if ($Mode -eq "normal") {
    $metadata = Get-Content -Raw -LiteralPath $metadataPath | ConvertFrom-Json
    $minimumDuration = [double]$MinimumNormalDurationSeconds
    Assert-Condition ($durationSeconds -ge $minimumDuration) "Normal video duration is $durationSeconds seconds; expected at least $minimumDuration."
    $metadataDurationSeconds = [double]$metadata.duration_ms / 1000.0
    Assert-Condition ([math]::Abs($metadataDurationSeconds - $durationSeconds) -le 2.0) "Metadata and ffprobe duration differ by more than two seconds."
    $resolutionMatch = [regex]::Match([string]$metadata.recording_resolution, '^(\d+)x(\d+)$')
    Assert-Condition $resolutionMatch.Success "Metadata recording_resolution is malformed."
    Assert-Condition ([int]$videoStreams[0].width -eq [int]$resolutionMatch.Groups[1].Value -and [int]$videoStreams[0].height -eq [int]$resolutionMatch.Groups[2].Value) "ffprobe resolution disagrees with metadata."
    Assert-Condition ([string]$videoStreams[0].codec_name -eq [string]$metadata.recording_codec) "ffprobe codec disagrees with metadata."
    $rateParts = ([string]$videoStreams[0].avg_frame_rate).Split('/')
    Assert-Condition ($rateParts.Count -eq 2 -and [double]$rateParts[1] -ne 0) "ffprobe average frame rate is malformed."
    $averageFps = [double]$rateParts[0] / [double]$rateParts[1]
    Assert-Condition ([math]::Abs($averageFps - [double]$metadata.recording_fps) -lt 0.01) "ffprobe frame rate disagrees with metadata."
    $gameLog = Get-Content -Raw -LiteralPath $gameLogPath | ConvertFrom-Json
    Assert-Condition ($null -ne $gameLog.game_start_video_offset_ms) "Normal run has no calibrated game/video offset."
    Assert-Condition (@($gameLog.snapshots).Count -gt 0) "Normal run has no Live Client snapshots."
    Assert-Condition (@($gameLog.events).Count -gt 0) "Normal run has no Live Client events."
}

$ffmpeg = Resolve-Executable -Value $FfmpegPath -Label "ffmpeg"
$decodeResult = Invoke-Captured -Executable $ffmpeg -Arguments @(
    "-v", "error",
    "-i", $videoPath,
    "-map", "0:v:0",
    "-map", "0:a:0",
    "-f", "null",
    "NUL"
) -TimeoutSeconds 1800 -Label "full media decode"
Assert-Condition ($decodeResult.ExitCode -eq 0) "Full media decode failed: $($decodeResult.Stderr.Trim())"

[pscustomobject]@{
    bundle = $bundleInfo.Name
    mode = $Mode
    lifecycle = [string]$state.lifecycle
    health = [string]$health.current
    verdict = [string]$health.verdict
    terminal_reason = $terminalReason
    output_bytes = $actualBytes
    duration_seconds = [math]::Round($durationSeconds, 3)
    video_codec = [string]$videoStreams[0].codec_name
    audio_codec = [string]$audioStreams[0].codec_name
    ffprobe = "passed"
    full_decode = "passed"
} | ConvertTo-Json -Depth 4
