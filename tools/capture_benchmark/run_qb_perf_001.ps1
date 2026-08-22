[CmdletBinding()]
param(
    # Defaults match the validated local setup. Every path can still be
    # overridden when the runner is used from a different Windows account.
    [string]$PresentMonPath = (Join-Path $env:USERPROFILE "Downloads\PresentMon-2.5.1-x64.exe"),

    [string]$LeagueConfigPath = (Join-Path $env:SystemDrive "Riot Games\League of Legends\Config\game.cfg"),

    [string]$RecorderSourceConfig = (Join-Path $env:APPDATA "LeagueReplay\config\config.toml"),

    [string]$RecorderPath,

    [ValidateSet("1", "2")]
    [string]$ManifestSchemaVersion = "1",

    [ValidateSet("nvenc", "amf", "qsv")]
    [string]$TargetEncoder = "nvenc",

    [string]$MediaRuntimeRoot,

    [string]$FfmpegPath,

    [string]$FfprobePath,

    [string]$ResultRoot,
    [switch]$PublishResults
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$script:RepositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$script:Collector = Join-Path $script:RepositoryRoot "tools\capture_benchmark\collect.ps1"
$script:Analyzer = Join-Path $script:RepositoryRoot "tools\capture_benchmark\analyze.py"
$script:FeatureId = if ($ManifestSchemaVersion -eq "2") { "QB-PERF-002" } else { "QB-PERF-001" }
$script:FeatureSlug = $script:FeatureId.ToLowerInvariant()

if ($ManifestSchemaVersion -eq "2") {
    if ([string]::IsNullOrWhiteSpace($MediaRuntimeRoot)) {
        $MediaRuntimeRoot = Join-Path $script:RepositoryRoot "build\media-runtime\windows-x86_64"
    }
    if ([string]::IsNullOrWhiteSpace($FfmpegPath)) {
        $FfmpegPath = Join-Path $MediaRuntimeRoot "bin\ffmpeg.exe"
    }
    if ([string]::IsNullOrWhiteSpace($FfprobePath)) {
        $FfprobePath = Join-Path $MediaRuntimeRoot "bin\ffprobe.exe"
    }
}
else {
    if ([string]::IsNullOrWhiteSpace($FfmpegPath)) { $FfmpegPath = "ffmpeg.exe" }
    if ([string]::IsNullOrWhiteSpace($FfprobePath)) { $FfprobePath = "ffprobe.exe" }
}

function Resolve-RequiredFile {
    param([string]$Value, [string]$Label)
    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw "$Label is required."
    }
    if (Test-Path -LiteralPath $Value -PathType Leaf) {
        return [System.IO.Path]::GetFullPath($Value)
    }
    $command = Get-Command -Name $Value -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -ne $command -and (Test-Path -LiteralPath $command.Source -PathType Leaf)) {
        return [System.IO.Path]::GetFullPath($command.Source)
    }
    throw "$Label was not found: $Value"
}

function Read-RequiredValue {
    param([string]$Value, [string]$Label)
    $candidate = if (-not [string]::IsNullOrWhiteSpace($Value)) { $Value } else { Read-Host "$Label" }
    $candidate = $candidate.Trim()
    if ($candidate.Length -ge 2 -and (
        ($candidate.StartsWith('"') -and $candidate.EndsWith('"')) -or
        ($candidate.StartsWith("'") -and $candidate.EndsWith("'"))
    )) {
        $candidate = $candidate.Substring(1, $candidate.Length - 2)
    }
    if ([string]::IsNullOrWhiteSpace($candidate)) {
        throw "$Label is required."
    }
    return $candidate
}

function New-FreshResultRoot {
    if (-not [string]::IsNullOrWhiteSpace($ResultRoot)) {
        $requested = [System.IO.Path]::GetFullPath($ResultRoot)
        if (Test-Path -LiteralPath $requested) {
            throw "Result root already exists and will not be reused: $requested. Choose a new empty path."
        }
        return $requested
    }
    $base = Join-Path $script:RepositoryRoot "build\perf"
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $candidate = Join-Path $base "$($script:FeatureSlug)-$stamp"
    $suffix = 1
    while (Test-Path -LiteralPath $candidate) {
        $candidate = Join-Path $base "$($script:FeatureSlug)-$stamp-$suffix"
        $suffix++
    }
    return $candidate
}

function Invoke-Collector {
    param([string[]]$Arguments)
    $effectiveArguments = @(
        "-ManifestSchemaVersion", $ManifestSchemaVersion,
        "-TargetEncoder", $TargetEncoder
    ) + $Arguments
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $script:Collector @effectiveArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Collector stopped with exit code $LASTEXITCODE. The result root is preserved; do not reuse it."
    }
}

function Wait-ForOperator {
    param([string]$Instruction)
    Write-Host ""
    Write-Host $Instruction -ForegroundColor Yellow
    [void](Read-Host "Press Enter when ready")
}

function Read-RecorderPid {
    while ($true) {
        $value = Read-Host "Enter the one prepared recorder.exe PID"
        $parsed = 0
        if ([int]::TryParse($value, [ref]$parsed) -and $parsed -gt 0) {
            return $parsed
        }
        Write-Host "Enter a positive numeric PID." -ForegroundColor Red
    }
}

$presentMonInput = Read-RequiredValue -Value $PresentMonPath -Label "Paste the PresentMon console EXE path"
$leagueConfigInput = Read-RequiredValue -Value $LeagueConfigPath -Label "Paste the League game.cfg path"
if ([string]::IsNullOrWhiteSpace($RecorderSourceConfig) -and -not [string]::IsNullOrWhiteSpace($env:LEAGUE_REPLAY_CONFIG)) {
    $RecorderSourceConfig = $env:LEAGUE_REPLAY_CONFIG
}
$sourceConfigInput = Read-RequiredValue -Value $RecorderSourceConfig -Label "Paste the current recorder TOML path"
if ([string]::IsNullOrWhiteSpace($RecorderPath)) {
    $RecorderPath = Join-Path $script:RepositoryRoot "recorder\target\release\recorder.exe"
}

$presentMon = Resolve-RequiredFile -Value $presentMonInput -Label "PresentMon"
$leagueConfig = Resolve-RequiredFile -Value $leagueConfigInput -Label "League game.cfg"
$sourceConfig = Resolve-RequiredFile -Value $sourceConfigInput -Label "Recorder source config"
$recorder = Resolve-RequiredFile -Value $RecorderPath -Label "Release recorder"
$ffmpeg = Resolve-RequiredFile -Value $FfmpegPath -Label "ffmpeg"
$ffprobe = Resolve-RequiredFile -Value $FfprobePath -Label "ffprobe"
$root = New-FreshResultRoot

Write-Host "$($script:FeatureId) guided benchmark ($TargetEncoder)" -ForegroundColor Cyan
Write-Host "Fresh result root: $root"
Write-Host "This runner never launches, terminates, or reconfigures League. It never deletes raw results."

Invoke-Collector -Arguments @(
    "-PresentMonPath", $presentMon,
    "-Condition", "baseline",
    "-FrameMode", "capped",
    "-RunNumber", "1",
    "-ResultRoot", $root,
    "-PreflightOnly"
)

$matrix = if ($ManifestSchemaVersion -eq "2") {
    @(
        [pscustomobject]@{ FrameMode = "capped"; Condition = "baseline"; RunNumber = 1 },
        [pscustomobject]@{ FrameMode = "capped"; Condition = "capture"; RunNumber = 1 },
        [pscustomobject]@{ FrameMode = "uncapped"; Condition = "baseline"; RunNumber = 1 },
        [pscustomobject]@{ FrameMode = "uncapped"; Condition = "capture"; RunNumber = 1 }
    )
}
else {
    @(
        [pscustomobject]@{ FrameMode = "capped"; Condition = "baseline"; RunNumber = 1 },
        [pscustomobject]@{ FrameMode = "capped"; Condition = "capture"; RunNumber = 1 },
        [pscustomobject]@{ FrameMode = "capped"; Condition = "baseline"; RunNumber = 2 },
        [pscustomobject]@{ FrameMode = "capped"; Condition = "capture"; RunNumber = 2 },
        [pscustomobject]@{ FrameMode = "capped"; Condition = "baseline"; RunNumber = 3 },
        [pscustomobject]@{ FrameMode = "capped"; Condition = "capture"; RunNumber = 3 },
        [pscustomobject]@{ FrameMode = "uncapped"; Condition = "baseline"; RunNumber = 1 },
        [pscustomobject]@{ FrameMode = "uncapped"; Condition = "capture"; RunNumber = 1 }
    )
}

foreach ($run in $matrix) {
    $modeInstruction = if ($run.FrameMode -eq "capped") { "set the League cap to 144 FPS" } else { "set the League cap to uncapped" }
    Write-Host ""
    Write-Host "=== $($run.FrameMode) $($run.Condition) B/C$($run.RunNumber) ===" -ForegroundColor Cyan
    Wait-ForOperator -Instruction "In Practice Tool: use default-skin Garen on Summoner's Rift, $modeInstruction, keep 1920x1080 borderless/VSync off, use the fixed mid-lane camera, keep League visible, and provide no input during collection."

    if ($run.Condition -eq "baseline") {
        Wait-ForOperator -Instruction "Confirm no recorder.exe is running. The next command collects the baseline after a five-second refocus countdown, 60-second warmup, and 180-second measurement."
        Invoke-Collector -Arguments @(
            "-PresentMonPath", $presentMon,
            "-Condition", "baseline",
            "-FrameMode", $run.FrameMode,
            "-RunNumber", [string]$run.RunNumber,
            "-ResultRoot", $root,
            "-LeagueConfigPath", $leagueConfig,
            "-ProtocolAttestation"
        )
        continue
    }

    $prepareArguments = @(
        "-PresentMonPath", $presentMon,
        "-Condition", "capture",
        "-FrameMode", $run.FrameMode,
        "-RunNumber", [string]$run.RunNumber,
        "-ResultRoot", $root,
        "-PrepareCapture",
        "-RecorderSourceConfig", $sourceConfig,
        "-RecorderPath", $recorder,
        "-FfmpegPath", $ffmpeg
    )
    if ($ManifestSchemaVersion -eq "2") {
        $prepareArguments += @("-MediaRuntimeRoot", $MediaRuntimeRoot)
    }
    Invoke-Collector -Arguments $prepareArguments
    $runDirectory = Join-Path $root "$($run.FrameMode)\capture\run-$($run.RunNumber)"
    $launcher = Join-Path $runDirectory "start-recorder.ps1"
    Wait-ForOperator -Instruction "Open a separate visible PowerShell window and run:`n  & '$launcher'`nWait for the recorder tray to show recording and the isolated video to grow."
    $recorderPid = Read-RecorderPid

    Invoke-Collector -Arguments @(
        "-PresentMonPath", $presentMon,
        "-Condition", "capture",
        "-FrameMode", $run.FrameMode,
        "-RunNumber", [string]$run.RunNumber,
        "-ResultRoot", $root,
        "-RecorderPid", [string]$recorderPid,
        "-LeagueConfigPath", $leagueConfig,
        "-ProtocolAttestation"
    )
    Wait-ForOperator -Instruction "Keep League open. Use the prepared recorder tray Quit action, wait for recorder.exe and ffmpeg to exit, then finalize the recording. Do not use Stop-Process."
    Invoke-Collector -Arguments @(
        "-PresentMonPath", $presentMon,
        "-Condition", "capture",
        "-FrameMode", $run.FrameMode,
        "-RunNumber", [string]$run.RunNumber,
        "-ResultRoot", $root,
        "-FinalizeCapture",
        "-FfprobePath", $ffprobe
    )
}

$reportDirectory = if ($PublishResults) {
    Join-Path $script:RepositoryRoot "docs\performance\results"
}
else {
    $root
}
$jsonReport = Join-Path $reportDirectory "$($script:FeatureSlug)-$TargetEncoder.json"
$markdownReport = Join-Path $reportDirectory "$($script:FeatureSlug)-$TargetEncoder.md"
if ($PublishResults -and ((Test-Path -LiteralPath $jsonReport) -or (Test-Path -LiteralPath $markdownReport))) {
    throw "Refusing to overwrite an existing published report. Review it or select a new publication name first."
}
& python $script:Analyzer "--input" $root "--target-encoder" $TargetEncoder "--output-json" $jsonReport "--output-markdown" $markdownReport
$analysisExitCode = $LASTEXITCODE
Write-Host ""
Write-Host "Analyzer exit code: $analysisExitCode" -ForegroundColor Cyan
Write-Host "JSON report: $jsonReport"
Write-Host "Markdown report: $markdownReport"
if ($analysisExitCode -eq 2) {
    throw "The dataset is invalid. Preserve this root for diagnosis and rerun the entire matrix in a new result root."
}
if ($analysisExitCode -eq 1) {
    Write-Host "The dataset is valid but a gate failed. Preserve the report and investigate the measured performance issue." -ForegroundColor Yellow
}
if ($analysisExitCode -eq 0) {
    Write-Host "The dataset passed. Review the report and record completion evidence before marking $($script:FeatureId) done." -ForegroundColor Green
}
