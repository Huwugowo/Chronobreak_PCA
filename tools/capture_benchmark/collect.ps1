[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PresentMonPath,

    [Parameter(Mandatory = $true)]
    [ValidateSet("baseline", "capture")]
    [string]$Condition,

    [Parameter(Mandatory = $true)]
    [ValidateSet("capped", "uncapped")]
    [string]$FrameMode,

    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 3)]
    [int]$RunNumber,

    [ValidateSet("1", "2")]
    # Keep the historical collector invocation on schema v1. PERF-002's
    # guided runner opts in to schema v2 explicitly.
    [string]$ManifestSchemaVersion = "1",

    [ValidateSet("nvenc", "amf", "qsv")]
    [string]$TargetEncoder = "nvenc",

    [Parameter(Mandatory = $true)]
    [string]$ResultRoot,

    [switch]$PreflightOnly,
    [switch]$PrepareCapture,
    [switch]$FinalizeCapture,
    [switch]$ProtocolAttestation,

    [string]$LeagueConfigPath,
    [string]$RecorderSourceConfig,
    [string]$RecorderPath,
    [string]$FfmpegPath,
    [string]$MediaRuntimeRoot,
    [string]$FfprobePath = "ffprobe.exe",
    [int]$RecorderPid,

    [ValidateRange(1, 3600)]
    [int]$WarmupSeconds = 60,

    [ValidateRange(1, 3600)]
    [int]$MeasurementSeconds = 180,

    [ValidateRange(1, 60)]
    [int]$SampleIntervalSeconds = 1,

    [switch]$TestOnlyAllowNonProtocolTiming
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$script:Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$script:CollectorPath = [System.IO.Path]::GetFullPath($MyInvocation.MyCommand.Path)
$script:RepositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$script:ResultSentinel = ".queueback-perf-results.json"
$script:LibrarySentinel = ".queueback-benchmark-library.json"
$script:SchemaVersion = $ManifestSchemaVersion

function Stop-WithCode {
    param([string]$Code, [string]$Message)
    throw "QB-PERF-$Code`: $Message"
}

function Signal-MeasurementComplete {
    Write-Host "MEASUREMENT COMPLETE - it is now safe to Alt-Tab."
    try {
        [Console]::Beep(784, 180)
        [Console]::Beep(988, 180)
        [Console]::Beep(1319, 260)
    }
    catch {
        # Some redirected or non-interactive consoles do not support Console.Beep.
    }
}

function Set-BenchmarkExecutionState {
    param([bool]$Active)
    if (-not ("QueueBackBenchmarkExecutionState" -as [type])) {
        Add-Type -TypeDefinition @"
using System.Runtime.InteropServices;
public static class QueueBackBenchmarkExecutionState {
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern uint SetThreadExecutionState(uint flags);
}
"@
    }
    $hexFlags = if ($Active) { "80000003" } else { "80000000" }
    $flags = [uint32]::Parse($hexFlags, [System.Globalization.NumberStyles]::HexNumber)
    $result = [QueueBackBenchmarkExecutionState]::SetThreadExecutionState($flags)
    if ($Active -and $result -eq 0) {
        Stop-WithCode "EXECUTION_STATE" "Windows refused the display-required execution state; the no-input benchmark could be interrupted by display sleep."
    }
}

function Get-AbsolutePath {
    param([string]$Path, [string]$BasePath = (Get-Location).Path)
    if ([string]::IsNullOrWhiteSpace($Path)) {
        Stop-WithCode "PATH_REQUIRED" "A required path was empty."
    }
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $BasePath $Path))
}

function Write-Utf8Text {
    param([string]$Path, [string]$Text)
    [System.IO.File]::WriteAllText($Path, $Text, $script:Utf8NoBom)
}

function Write-JsonFile {
    param([string]$Path, $Value)
    $json = $Value | ConvertTo-Json -Depth 12
    Write-Utf8Text -Path $Path -Text ($json + "`n")
}

function Get-Sha256 {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Stop-WithCode "FILE_MISSING" "Required file does not exist: $Path"
    }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-StringSha256 {
    param([string]$Value)
    $bytes = $script:Utf8NoBom.GetBytes($Value)
    $algorithm = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $algorithm.Dispose()
    }
}

function Get-RunId {
    return "$FrameMode-$Condition-$RunNumber"
}

function Get-ExpectedInterop {
    switch ($TargetEncoder) {
        "nvenc" { return "d3d11-nvenc-direct" }
        "amf" { return "d3d11-amf-direct" }
        "qsv" { return "d3d11-qsv-direct-map" }
    }
}

function Get-RunDirectory {
    param([string]$Root)
    return Join-Path (Join-Path (Join-Path $Root $FrameMode) $Condition) "run-$RunNumber"
}

function Assert-UncappedRunNumber {
    if ($script:SchemaVersion -eq "2" -and $RunNumber -ne 1) {
        Stop-WithCode "RUN_NUMBER" "Schema-v2 uses one capped pair and one uncapped pair; every run number must be 1."
    }
    if ($FrameMode -eq "uncapped" -and $RunNumber -ne 1) {
        Stop-WithCode "RUN_NUMBER" "The protocol has only uncapped run 1."
    }
}

function Assert-ResultRoot {
    param([string]$Root, [bool]$Create)
    $sentinel = Join-Path $Root $script:ResultSentinel
    if (Test-Path -LiteralPath $Root) {
        if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
            Stop-WithCode "UNSAFE_RESULT_ROOT" "ResultRoot exists but is not a directory: $Root"
        }
        $entries = @(Get-ChildItem -LiteralPath $Root -Force)
        if ($entries.Count -gt 0 -and -not (Test-Path -LiteralPath $sentinel -PathType Leaf)) {
            Stop-WithCode "UNSAFE_RESULT_ROOT" "Refusing a non-empty result root without $script:ResultSentinel. Choose a new benchmark-only directory."
        }
        if (Test-Path -LiteralPath $sentinel -PathType Leaf) {
            $rootContract = Get-Content -LiteralPath $sentinel -Raw -Encoding UTF8 | ConvertFrom-Json
            if ([string]$rootContract.schema_version -ne $script:SchemaVersion) {
                Stop-WithCode "SCHEMA_MISMATCH" "ResultRoot uses schema $($rootContract.schema_version), not requested schema $script:SchemaVersion."
            }
            if ($script:SchemaVersion -eq "2" -and [string]$rootContract.target_encoder -ne $TargetEncoder) {
                Stop-WithCode "ENCODER_MISMATCH" "ResultRoot targets $($rootContract.target_encoder), not requested $TargetEncoder."
            }
        }
    }
    elseif ($Create) {
        [void](New-Item -ItemType Directory -Path $Root)
    }

    if ($Create -and -not (Test-Path -LiteralPath $sentinel)) {
        Write-JsonFile -Path $sentinel -Value ([ordered]@{
            schema_version = $script:SchemaVersion
            purpose = $(if ($script:SchemaVersion -eq "1") { "QueueBack QB-PERF-001 raw benchmark results" } else { "QueueBack QB-PERF-002 raw benchmark results" })
            target_encoder = $(if ($script:SchemaVersion -eq "2") { $TargetEncoder } else { "nvenc" })
            safety = "This tool never deletes this directory."
        })
    }

    $probe = $Root
    while (-not (Test-Path -LiteralPath $probe -PathType Container)) {
        $parent = Split-Path -Parent $probe
        if ([string]::IsNullOrWhiteSpace($parent) -or $parent -eq $probe) { break }
        $probe = $parent
    }
    try {
        $driveName = [System.IO.Path]::GetPathRoot($probe).TrimEnd('\').TrimEnd(':')
        $drive = Get-PSDrive -Name $driveName -ErrorAction Stop
        if ($null -ne $drive.Free -and [uint64]$drive.Free -lt 10GB) {
            Stop-WithCode "RESULT_SPACE" "The result volume has less than 10 GiB free; choose a benchmark volume with enough space for all raw runs/media."
        }
    }
    catch {
        if ($_.Exception.Message -like "QB-PERF-RESULT_SPACE:*") { throw }
        Stop-WithCode "RESULT_VOLUME" "Could not inspect free space for the benchmark result root."
    }
}

function Get-ExpectedSequence {
    if ($script:SchemaVersion -eq "2") {
        return @(
            "capped-baseline-1",
            "capped-capture-1",
            "uncapped-baseline-1",
            "uncapped-capture-1"
        )
    }
    return @(
        "capped-baseline-1",
        "capped-capture-1",
        "capped-baseline-2",
        "capped-capture-2",
        "capped-baseline-3",
        "capped-capture-3",
        "uncapped-baseline-1",
        "uncapped-capture-1"
    )
}

function Get-RunDirectoryForId {
    param([string]$Root, [string]$RunId)
    $parts = $RunId.Split("-")
    return Join-Path (Join-Path (Join-Path $Root $parts[0]) $parts[1]) "run-$($parts[2])"
}

function Assert-RunOrder {
    param([string]$Root, [bool]$CurrentMayExist)
    $sequence = @(Get-ExpectedSequence)
    $runId = Get-RunId
    $index = [Array]::IndexOf($sequence, $runId)
    if ($index -lt 0) {
        Stop-WithCode "RUN_ID" "Run $runId is not part of the protocol."
    }
    for ($position = 0; $position -lt $index; $position++) {
        $prior = Get-RunDirectoryForId -Root $Root -RunId $sequence[$position]
        $manifestPath = Join-Path $prior "manifest.json"
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
            Stop-WithCode "RUN_ORDER" "Complete $($sequence[$position]) before $runId."
        }
        $priorManifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($priorManifest.condition -eq "capture" -and $priorManifest.capture.finalized -ne $true) {
            Stop-WithCode "RUN_ORDER" "Finalize $($sequence[$position]) before $runId."
        }
    }
    $current = Get-RunDirectoryForId -Root $Root -RunId $runId
    if ((Test-Path -LiteralPath $current) -and -not $CurrentMayExist) {
        Stop-WithCode "RUN_EXISTS" "Run directory already exists; raw runs are immutable: $current"
    }
}

function Quote-NativeArgument {
    param([string]$Value)
    if ($Value -notmatch '[\s"]') {
        return $Value
    }
    return '"' + $Value.Replace('"', '\"') + '"'
}

function Start-NativeProcess {
    param([string]$FilePath, [string[]]$Arguments)
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $FilePath
    $startInfo.Arguments = (($Arguments | ForEach-Object { Quote-NativeArgument $_ }) -join " ")
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        Stop-WithCode "PROCESS_START" "Could not start $FilePath."
    }
    return $process
}

function Invoke-NativeCapture {
    param([string]$FilePath, [string[]]$Arguments, [int]$TimeoutSeconds = 120)
    $process = Start-NativeProcess -FilePath $FilePath -Arguments $Arguments
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        try { $process.Kill() } catch {}
        Stop-WithCode "PROCESS_TIMEOUT" "$FilePath exceeded its $TimeoutSeconds-second timeout."
    }
    $stdout = $stdoutTask.Result
    $stderr = $stderrTask.Result
    return [ordered]@{
        exit_code = $process.ExitCode
        stdout = $stdout
        stderr = $stderr
    }
}

function Resolve-Executable {
    param([string]$Value, [string]$Label)
    if ([System.IO.Path]::IsPathRooted($Value) -or $Value.Contains("\") -or $Value.Contains("/")) {
        $resolved = Get-AbsolutePath $Value
        if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
            Stop-WithCode "${Label}_MISSING" "$Label executable was not found: $resolved"
        }
        return $resolved
    }
    $command = Get-Command $Value -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        Stop-WithCode "${Label}_MISSING" "$Label executable '$Value' was not found on PATH."
    }
    return $command.Source
}

function Get-PresentMonInfo {
    param([string]$Path)
    $resolved = Resolve-Executable -Value $Path -Label "PRESENTMON"
    $helpResult = Invoke-NativeCapture -FilePath $resolved -Arguments @("--help") -TimeoutSeconds 30
    $help = [string]$helpResult.stdout + "`n" + [string]$helpResult.stderr
    $requiredFlags = @(
        "--process_id",
        "--output_file",
        "--timed",
        "--terminate_after_timed",
        "--qpc_time_ms",
        "--track_gpu_video"
    )
    foreach ($flag in $requiredFlags) {
        if (-not $help.Contains($flag)) {
            Stop-WithCode "PRESENTMON_INCOMPATIBLE" "PresentMon help does not expose required flag $flag. Supply the 2.x console application (minimum 2.1.1)."
        }
    }
    $file = Get-Item -LiteralPath $resolved
    $versionText = [string]$file.VersionInfo.ProductVersion
    if ($versionText -notmatch '(\d+\.\d+(?:\.\d+)?)') {
        if ($help -match '(?i)PresentMon[^\r\n]*?(\d+\.\d+(?:\.\d+)?)') {
            $versionText = $Matches[1]
        }
        else {
            Stop-WithCode "PRESENTMON_VERSION" "PresentMon version could not be determined from file metadata or --help."
        }
    }
    $version = [version]$Matches[1]
    if ($version.Major -ne 2 -or $version -lt [version]"2.1.1") {
        Stop-WithCode "PRESENTMON_VERSION" "PresentMon $version is unsupported; use console version 2.1.1 or newer within major version 2."
    }
    return [ordered]@{
        path = $resolved
        version = $version.ToString()
        sha256 = Get-Sha256 $resolved
        supports_v2_flag = $help.Contains("--v2_metrics")
    }
}

function Assert-CimClass {
    param([string]$ClassName, [string[]]$Properties)
    try {
        $sample = Get-CimInstance -ClassName $ClassName -ErrorAction Stop | Select-Object -First 1
    }
    catch {
        $message = $_.Exception.Message
        if ($message -match '(?i)access.*denied') {
            Stop-WithCode "CIM_ACCESS_DENIED" "Windows performance data denied access to $ClassName. Run from an approved interactive shell as a Performance Log Users member or administrator."
        }
        Stop-WithCode "CIM_UNAVAILABLE" "Windows performance class $ClassName is unavailable: $message"
    }
    if ($null -eq $sample) {
        Stop-WithCode "CIM_EMPTY" "Windows performance class $ClassName returned no instances."
    }
    $names = @($sample.CimInstanceProperties.Name)
    foreach ($property in $Properties) {
        if ($names -notcontains $property) {
            Stop-WithCode "CIM_COUNTER_MISSING" "$ClassName does not expose required property $property."
        }
    }
}

function Test-CounterPreflight {
    Assert-CimClass "Win32_PerfRawData_PerfOS_Processor" @("Name", "PercentProcessorTime", "Timestamp_Sys100NS")
    Assert-CimClass "Win32_PerfRawData_PerfOS_Memory" @("AvailableBytes", "CommittedBytes")
    Assert-CimClass "Win32_PerfRawData_PerfProc_Process" @(
        "IDProcess", "PercentProcessorTime", "Timestamp_Sys100NS", "PrivateBytes", "WorkingSet",
        "IOReadBytesPersec", "IOWriteBytesPersec", "IOOtherBytesPersec", "Timestamp_PerfTime", "Frequency_PerfTime"
    )
    Assert-CimClass "Win32_PerfRawData_GPUPerformanceCounters_GPUEngine" @("Name", "UtilizationPercentage", "Timestamp_Sys100NS")
    Assert-CimClass "Win32_PerfRawData_GPUPerformanceCounters_GPUAdapterMemory" @("Name", "DedicatedUsage", "SharedUsage")
    try {
        Assert-CimClass "Win32_PerfRawData_GPUPerformanceCounters_GPUProcessMemory" @("Name", "DedicatedUsage", "SharedUsage")
    }
    catch {
        Write-Warning "Optional GPU Process Memory counters are unavailable: $($_.Exception.Message)"
    }
}

function Get-RunningLeagueProcesses {
    try {
        return @(Get-CimInstance -ClassName Win32_Process -Filter "Name='League of Legends.exe'" -ErrorAction Stop)
    }
    catch {
        Stop-WithCode "LEAGUE_PROCESS_QUERY" "Could not query League processes: $($_.Exception.Message)"
    }
}

function Get-RecorderProcesses {
    return @(Get-Process -Name "recorder" -ErrorAction SilentlyContinue)
}

function Assert-RecorderIsolation {
    $recorders = @(Get-RecorderProcesses)
    if ($Condition -eq "baseline" -and $recorders.Count -gt 0) {
        Stop-WithCode "BASELINE_CONTAMINATED" "A recorder.exe process started during the baseline. Quit it gracefully and repeat in a fresh result root; the collector will not terminate it."
    }
    if ($Condition -eq "capture" -and ($recorders.Count -ne 1 -or [int]$recorders[0].Id -ne $RecorderPid)) {
        Stop-WithCode "RECORDER_PROCESS_COUNT" "Recorder isolation changed during capture. Expected only prepared recorder PID $RecorderPid, found $($recorders.Count) recorder.exe processes; quit them gracefully and repeat."
    }
}

function Assert-LiveState {
    $league = @(Get-RunningLeagueProcesses)
    if ($league.Count -eq 0) {
        Stop-WithCode "LEAGUE_NOT_RUNNING" "Start the League Practice Tool scenario, then rerun collection. The collector does not launch League."
    }
    if ($league.Count -ne 1) {
        Stop-WithCode "LEAGUE_PROCESS_COUNT" "Expected exactly one League of Legends.exe process, found $($league.Count)."
    }
    Assert-RecorderIsolation
    if ($Condition -eq "capture") {
        if ($RecorderPid -le 0) {
            Stop-WithCode "RECORDER_PID_REQUIRED" "Capture collection requires -RecorderPid for the prepared benchmark recorder."
        }
        $recorder = Get-Process -Id $RecorderPid -ErrorAction SilentlyContinue
        if ($null -eq $recorder) {
            Stop-WithCode "RECORDER_NOT_RUNNING" "Prepared recorder PID $RecorderPid is not running. Start it with the generated launcher."
        }
    }
    return $league[0]
}

function Get-GitInfo {
    $revisionOutput = @(& git -C $script:RepositoryRoot rev-parse HEAD 2>$null)
    $revisionExitCode = $LASTEXITCODE
    $revision = $revisionOutput | Select-Object -First 1
    if ($revisionExitCode -ne 0 -or [string]::IsNullOrWhiteSpace($revision)) {
        Stop-WithCode "GIT_INFO" "Could not determine the recorder revision."
    }
    $status = @(& git -C $script:RepositoryRoot status --porcelain 2>$null)
    return [ordered]@{
        revision = $revision.Trim()
        dirty = $status.Count -gt 0
    }
}

function Get-SystemEnvironment {
    param($PresentMon, $LeagueProcess)
    try {
        $os = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop
        $cpu = Get-CimInstance -ClassName Win32_Processor -ErrorAction Stop | Select-Object -First 1
        $computer = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction Stop
        $gpus = @(Get-CimInstance -ClassName Win32_VideoController -ErrorAction Stop)
    }
    catch {
        Stop-WithCode "HARDWARE_QUERY" "Could not query benchmark hardware/OS metadata: $($_.Exception.Message)"
    }
    if (([string]$cpu.Name).Trim() -notmatch '(?i)^AMD\s+Ryzen\s+5\s+5600X(?:\s+6-Core\s+Processor)?$') {
        Stop-WithCode "TARGET_CPU" "QB-PERF-001 target validation requires an AMD Ryzen 5 5600X (not a suffix variant)."
    }
    $display = $gpus | Where-Object {
        $_.CurrentHorizontalResolution -eq 1920 -and
        $_.CurrentVerticalResolution -eq 1080 -and
        ([string]$_.Name).Trim() -match '(?i)^(?:NVIDIA\s+)?(?:GeForce\s+)?RTX\s+4060$'
    } | Select-Object -First 1
    if ($null -eq $display) {
        Stop-WithCode "DISPLAY_MODE" "No active 1920x1080 display was reported. Configure the target display before collection."
    }
    if ($null -eq $display.CurrentRefreshRate -or [int]$display.CurrentRefreshRate -le 0) {
        Stop-WithCode "DISPLAY_REFRESH" "The active RTX 4060 display refresh rate could not be read."
    }
    $leaguePath = [string]$LeagueProcess.ExecutablePath
    $leagueVersionSource = "process_executable"
    if ([string]::IsNullOrWhiteSpace($leaguePath)) {
        try {
            $leaguePath = [string](Get-Process -Id ([int]$LeagueProcess.ProcessId) -ErrorAction Stop).Path
        }
        catch {}
    }
    if ([string]::IsNullOrWhiteSpace($leaguePath)) {
        # Some League processes deny image-path metadata even to an elevated
        # shell. The operator-supplied game.cfg identifies the same installed
        # game whose target PID was already selected by exact process name.
        $configDirectory = Split-Path -Parent (Get-AbsolutePath $LeagueConfigPath)
        $installDirectory = Split-Path -Parent $configDirectory
        $configuredGameExecutable = Join-Path $installDirectory "Game\League of Legends.exe"
        if (Test-Path -LiteralPath $configuredGameExecutable -PathType Leaf) {
            $leaguePath = $configuredGameExecutable
            $leagueVersionSource = "configured_installation_executable"
        }
    }
    if ([string]::IsNullOrWhiteSpace($leaguePath) -or -not (Test-Path -LiteralPath $leaguePath)) {
        Stop-WithCode "LEAGUE_VERSION" "League executable/version could not be read from the target PID or the installation containing the supplied game.cfg."
    }
    $leagueVersion = (Get-Item -LiteralPath $leaguePath).VersionInfo.ProductVersion
    if ([string]::IsNullOrWhiteSpace($leagueVersion)) {
        Stop-WithCode "LEAGUE_VERSION" "League product version is unavailable."
    }
    $git = Get-GitInfo
    $gpuSummary = @($gpus | ForEach-Object {
        [ordered]@{
            name = [string]$_.Name
            driver_version = [string]$_.DriverVersion
        }
    })
    $fingerprintInput = [ordered]@{
        os_version = [string]$os.Version
        os_build = [string]$os.BuildNumber
        cpu = [string]$cpu.Name
        logical_processors = [int]$computer.NumberOfLogicalProcessors
        total_memory_bytes = [uint64]$computer.TotalPhysicalMemory
        gpus = $gpuSummary
        display_width = 1920
        display_height = 1080
        display_name = [string]$display.Name
        display_refresh_hz = [int]$display.CurrentRefreshRate
    } | ConvertTo-Json -Compress -Depth 6
    return [ordered]@{
        system_fingerprint_sha256 = Get-StringSha256 $fingerprintInput
        os = [ordered]@{
            caption = [string]$os.Caption
            version = [string]$os.Version
            build = [string]$os.BuildNumber
        }
        cpu = [ordered]@{
            name = ([string]$cpu.Name).Trim()
            logical_processors = [int]$computer.NumberOfLogicalProcessors
            total_memory_bytes = [uint64]$computer.TotalPhysicalMemory
        }
        gpus = $gpuSummary
        display = [ordered]@{
            width = 1920
            height = 1080
            refresh_hz = [int]$display.CurrentRefreshRate
            adapter_name = [string]$display.Name
            video_mode = [string]$display.VideoModeDescription
        }
        league_version = [string]$leagueVersion
        league_version_source = $leagueVersionSource
        recorder = $git
        collector = [ordered]@{
            schema_version = $script:SchemaVersion
            sha256 = Get-Sha256 $script:CollectorPath
        }
        presentmon = [ordered]@{
            version = $PresentMon.version
            sha256 = $PresentMon.sha256
        }
    }
}

function Get-LeagueConfigHash {
    if ([string]::IsNullOrWhiteSpace($LeagueConfigPath)) {
        Stop-WithCode "LEAGUE_CONFIG_REQUIRED" "Pass -LeagueConfigPath so the untouched League graphics configuration can be fingerprinted."
    }
    $resolved = Get-AbsolutePath $LeagueConfigPath
    if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
        Stop-WithCode "LEAGUE_CONFIG_MISSING" "League configuration file was not found: $resolved"
    }
    return [ordered]@{ path = $resolved; sha256 = Get-Sha256 $resolved; name = [System.IO.Path]::GetFileName($resolved) }
}

function Get-PreparedState {
    param([string]$RunDirectory)
    $path = Join-Path $RunDirectory "prepared.json"
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Stop-WithCode "CAPTURE_NOT_PREPARED" "Run PrepareCapture first; prepared.json is missing."
    }
    return Get-Content -LiteralPath $path -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Escape-TomlString {
    param([string]$Value)
    return $Value.Replace("\", "/").Replace('"', '\"')
}

function Set-IsolatedStorageOutputPath {
    param([string]$ConfigText, [string]$OutputPath)
    $newline = $(if ($ConfigText.Contains("`r`n")) { "`r`n" } else { "`n" })
    $lines = @([regex]::Split($ConfigText, "`r?`n"))
    $inStorage = $false
    $storageSections = 0
    $outputKeys = 0
    $replacement = "output_path = `"$(Escape-TomlString $OutputPath)`""
    for ($index = 0; $index -lt $lines.Count; $index++) {
        if ($lines[$index] -match '^\s*\[([^\]]+)\]\s*(?:#.*)?$') {
            $inStorage = $Matches[1].Trim() -eq "storage"
            if ($inStorage) { $storageSections++ }
            continue
        }
        if ($inStorage -and $lines[$index] -match '^\s*output_path\s*=') {
            if ($lines[$index] -notmatch '^\s*output_path\s*=\s*"(?:[^"\\]|\\.)*"\s*(?:#.*)?$') {
                Stop-WithCode "RECORDER_CONFIG_FORMAT" "[storage].output_path must be a basic quoted TOML string so isolation can be verified."
            }
            $lines[$index] = $replacement
            $outputKeys++
        }
    }
    if ($storageSections -ne 1 -or $outputKeys -ne 1) {
        Stop-WithCode "RECORDER_CONFIG_FORMAT" "Expected exactly one [storage] table with one output_path key in the source TOML."
    }
    $result = $lines -join $newline
    $storagePattern = '(?ms)^\s*\[storage\]\s*(?:#.*)?\r?$.*?^\s*output_path\s*=\s*"' + [regex]::Escape((Escape-TomlString $OutputPath)) + '"\s*$'
    if ($result -notmatch $storagePattern) {
        Stop-WithCode "RECORDER_CONFIG_ISOLATION" "Generated config does not resolve [storage].output_path to the benchmark library."
    }
    return $result
}

function Prepare-CaptureRun {
    param([string]$Root, [string]$RunDirectory, $PresentMon)
    if ($Condition -ne "capture") {
        Stop-WithCode "PREPARE_CONDITION" "-PrepareCapture is valid only with -Condition capture."
    }
    Assert-RunOrder -Root $Root -CurrentMayExist $false
    if (@(Get-RecorderProcesses).Count -gt 0) {
        Stop-WithCode "RECORDER_ALREADY_RUNNING" "Quit every existing recorder gracefully before preparing an isolated capture run."
    }
    if ([string]::IsNullOrWhiteSpace($RecorderSourceConfig)) {
        Stop-WithCode "RECORDER_CONFIG_REQUIRED" "Pass the current recorder config with -RecorderSourceConfig; it is copied and the storage path alone is isolated."
    }
    if ([string]::IsNullOrWhiteSpace($RecorderPath)) {
        Stop-WithCode "RECORDER_PATH_REQUIRED" "Pass the prebuilt release recorder with -RecorderPath."
    }
    if ([string]::IsNullOrWhiteSpace($FfmpegPath)) {
        Stop-WithCode "FFMPEG_PATH_REQUIRED" "Pass the exact ffmpeg executable with -FfmpegPath."
    }
    $sourceConfig = Get-AbsolutePath $RecorderSourceConfig
    $recorder = Resolve-Executable -Value $RecorderPath -Label "RECORDER"
    $ffmpeg = Resolve-Executable -Value $FfmpegPath -Label "FFMPEG"
    $runtimeRoot = $null
    if ($script:SchemaVersion -eq "2") {
        if ([string]::IsNullOrWhiteSpace($MediaRuntimeRoot)) {
            $runtimeRoot = Split-Path -Parent (Split-Path -Parent $ffmpeg)
        }
        else {
            $runtimeRoot = Get-AbsolutePath $MediaRuntimeRoot
        }
        $expectedFfmpeg = [System.IO.Path]::GetFullPath((Join-Path $runtimeRoot "bin\ffmpeg.exe"))
        if (-not $ffmpeg.Equals($expectedFfmpeg, [StringComparison]::OrdinalIgnoreCase)) {
            Stop-WithCode "RUNTIME_LAYOUT" "Schema-v2 ffmpeg must be the packaged runtime binary at $expectedFfmpeg."
        }
        if (-not (Test-Path -LiteralPath (Join-Path $runtimeRoot "runtime-manifest.json") -PathType Leaf)) {
            Stop-WithCode "RUNTIME_MANIFEST" "Schema-v2 packaged runtime manifest is missing under $runtimeRoot."
        }
    }
    if (-not (Test-Path -LiteralPath $sourceConfig -PathType Leaf)) {
        Stop-WithCode "RECORDER_CONFIG_MISSING" "Recorder source config was not found: $sourceConfig"
    }
    [void](New-Item -ItemType Directory -Path $RunDirectory)
    $library = Join-Path $RunDirectory "library"
    $logs = Join-Path $RunDirectory "logs"
    [void](New-Item -ItemType Directory -Path $library)
    [void](New-Item -ItemType Directory -Path $logs)
    Write-JsonFile -Path (Join-Path $library $script:LibrarySentinel) -Value ([ordered]@{
        schema_version = $script:SchemaVersion
        run_id = Get-RunId
        safety = "Dedicated benchmark library. The collector never deletes it."
    })

    $configText = [System.IO.File]::ReadAllText($sourceConfig)
    $benchmarkConfigText = Set-IsolatedStorageOutputPath -ConfigText $configText -OutputPath $library
    $benchmarkConfig = Join-Path $RunDirectory "recorder-config.toml"
    Write-Utf8Text -Path $benchmarkConfig -Text $benchmarkConfigText

    $launcher = Join-Path $RunDirectory "start-recorder.ps1"
    $quote = {
        param([string]$Value)
        return "'" + $Value.Replace("'", "''") + "'"
    }
    $audioOverride = [Environment]::GetEnvironmentVariable("LEAGUE_REPLAY_AUDIO_DEVICE", "Process")
    $audioLine = $(if ([string]::IsNullOrWhiteSpace($audioOverride)) {
        'Remove-Item Env:LEAGUE_REPLAY_AUDIO_DEVICE -ErrorAction SilentlyContinue'
    } else {
        '$env:LEAGUE_REPLAY_AUDIO_DEVICE = ' + (& $quote $audioOverride)
    })
    $runtimeLine = $(if ($script:SchemaVersion -eq "2") {
        '$env:QUEUEBACK_MEDIA_RUNTIME_DIR = ' + (& $quote $runtimeRoot)
    } else {
        '$env:LEAGUE_REPLAY_FFMPEG = ' + (& $quote $ffmpeg)
    })
    $launcherLines = @(
        '$ErrorActionPreference = "Stop"',
        ('$env:LEAGUE_REPLAY_CONFIG = ' + (& $quote $benchmarkConfig)),
        ('$env:LEAGUE_REPLAY_LOG_DIR = ' + (& $quote $logs)),
        $runtimeLine,
        '$env:LEAGUE_REPLAY_PROCESS_NAME = "League of Legends.exe"',
        $audioLine,
        '$env:RUST_LOG = "info"',
        ('& ' + (& $quote $recorder))
    )
    Write-Utf8Text -Path $launcher -Text (($launcherLines -join "`r`n") + "`r`n")
    $prepared = [ordered]@{
        schema_version = $script:SchemaVersion
        run_id = Get-RunId
        source_config_sha256 = Get-Sha256 $sourceConfig
        recorder_config_sha256 = Get-Sha256 $benchmarkConfig
        recorder_sha256 = Get-Sha256 $recorder
        ffmpeg_sha256 = Get-Sha256 $ffmpeg
        presentmon_sha256 = $PresentMon.sha256
        recorder_path = $recorder
        ffmpeg_path = $ffmpeg
        media_runtime_root = $runtimeRoot
        target_encoder = $TargetEncoder
        config_path = $benchmarkConfig
        library_path = $library
        logs_path = $logs
        launcher_path = $launcher
        audio_override = $(if ([string]::IsNullOrWhiteSpace($audioOverride)) { $null } else { $audioOverride })
    }
    Write-JsonFile -Path (Join-Path $RunDirectory "prepared.json") -Value $prepared
    Write-Host "Prepared isolated capture run $(Get-RunId)."
    Write-Host "Start the recorder in a separate PowerShell window with:"
    Write-Host "  & '$launcher'"
    Write-Host "Then note recorder.exe's PID and rerun this command without -PrepareCapture, adding -RecorderPid <PID> -LeagueConfigPath <game.cfg> -ProtocolAttestation."
}

function Get-ChildFfmpeg {
    param([int]$ParentPid, [string]$ExpectedPath)
    $expectedName = [System.IO.Path]::GetFileName($ExpectedPath)
    $children = @(Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId=$ParentPid" -ErrorAction Stop | Where-Object { $_.Name -ieq $expectedName })
    return $children
}

function Get-CaptureOutput {
    param([string]$Library, [string]$FfmpegCommandLine)
    $games = Join-Path $Library "games"
    if (-not (Test-Path -LiteralPath $games -PathType Container)) {
        return $null
    }
    $videos = @(Get-ChildItem -LiteralPath $games -Recurse -File -Filter "video*.mp4")
    $bound = @($videos | Where-Object {
        $FfmpegCommandLine.IndexOf($_.FullName, [System.StringComparison]::OrdinalIgnoreCase) -ge 0
    })
    if ($bound.Count -gt 1) {
        Stop-WithCode "CAPTURE_OUTPUT_COUNT" "The prepared FFmpeg command is ambiguously bound to multiple isolated video files."
    }
    if ($bound.Count -eq 1) { return $bound[0] }
    return $null
}

function Convert-ToRelativePath {
    param([string]$Base, [string]$Path)
    $baseUri = New-Object System.Uri(([System.IO.Path]::GetFullPath($Base).TrimEnd('\') + '\'))
    $pathUri = New-Object System.Uri([System.IO.Path]::GetFullPath($Path))
    return [System.Uri]::UnescapeDataString($baseUri.MakeRelativeUri($pathUri).ToString()).Replace("/", "\")
}

function Wait-ForCaptureReadiness {
    param([string]$RunDirectory, $Prepared)
    $library = [string]$Prepared.library_path
    $librarySentinel = Join-Path $library $script:LibrarySentinel
    if (-not (Test-Path -LiteralPath $librarySentinel -PathType Leaf)) {
        Stop-WithCode "LIBRARY_SENTINEL" "Prepared capture library sentinel is missing; refusing to inspect another library."
    }
    if ((Get-Sha256 ([string]$Prepared.config_path)) -ne [string]$Prepared.recorder_config_sha256) {
        Stop-WithCode "RECORDER_CONFIG_CHANGED" "Prepared recorder config changed after isolation."
    }
    $recorder = Get-Process -Id $RecorderPid -ErrorAction SilentlyContinue
    if ($null -eq $recorder) {
        Stop-WithCode "RECORDER_NOT_RUNNING" "Prepared recorder PID $RecorderPid is not running."
    }
    $recorderPath = $null
    try { $recorderPath = [string]$recorder.Path } catch {}
    if ([string]::IsNullOrWhiteSpace($recorderPath)) {
        Stop-WithCode "RECORDER_IDENTITY" "Recorder PID $RecorderPid executable path is unavailable; run from an approved elevated interactive shell."
    }
    if (
        -not [System.IO.Path]::GetFullPath($recorderPath).Equals(
            [System.IO.Path]::GetFullPath([string]$Prepared.recorder_path),
            [System.StringComparison]::OrdinalIgnoreCase
        ) -or
        (Get-Sha256 $recorderPath) -ne [string]$Prepared.recorder_sha256
    ) {
        Stop-WithCode "RECORDER_IDENTITY" "Recorder PID $RecorderPid does not match the prepared executable path/hash."
    }
    $ffmpegChildren = @(Get-ChildFfmpeg -ParentPid $RecorderPid -ExpectedPath ([string]$Prepared.ffmpeg_path))
    if ($ffmpegChildren.Count -ne 1) {
        Stop-WithCode "FFMPEG_PROCESS_COUNT" "Expected one long-lived prepared ffmpeg child after recorder startup, found $($ffmpegChildren.Count). Wait for recording startup and retry."
    }
    $ffmpegChild = $ffmpegChildren[0]
    $childPath = [string]$ffmpegChild.ExecutablePath
    if ([string]::IsNullOrWhiteSpace($childPath)) {
        Stop-WithCode "FFMPEG_IDENTITY" "The long-lived ffmpeg child executable path is unavailable."
    }
    if (
        -not [System.IO.Path]::GetFullPath($childPath).Equals(
            [System.IO.Path]::GetFullPath([string]$Prepared.ffmpeg_path),
            [System.StringComparison]::OrdinalIgnoreCase
        ) -or
        (Get-Sha256 $childPath) -ne [string]$Prepared.ffmpeg_sha256
    ) {
        Stop-WithCode "FFMPEG_IDENTITY" "The long-lived ffmpeg child does not match the prepared executable path/hash."
    }
    $normalizedCommandLine = ([string]$ffmpegChild.CommandLine).Replace('/', '\')
    if ([string]::IsNullOrWhiteSpace($normalizedCommandLine)) {
        Stop-WithCode "FFMPEG_OUTPUT_IDENTITY" "The prepared ffmpeg command line is unavailable."
    }
    $video = Get-CaptureOutput -Library $library -FfmpegCommandLine $normalizedCommandLine
    if ($null -eq $video) {
        Stop-WithCode "CAPTURE_OUTPUT_MISSING" "No video.mp4 exists in the prepared library. Wait until the recorder reports recording started."
    }
    $before = $video.Length
    Start-Sleep -Seconds 2
    $video.Refresh()
    if ($video.Length -le $before) {
        Stop-WithCode "CAPTURE_NOT_GROWING" "Prepared video.mp4 did not grow during readiness check."
    }
    $normalizedVideoPath = $video.FullName.Replace('/', '\')
    if (
        [string]::IsNullOrWhiteSpace($normalizedCommandLine) -or
        $normalizedCommandLine.IndexOf($normalizedVideoPath, [System.StringComparison]::OrdinalIgnoreCase) -lt 0
    ) {
        Stop-WithCode "FFMPEG_OUTPUT_IDENTITY" "The prepared ffmpeg command line is not bound to the isolated benchmark video."
    }
    return [ordered]@{
        ffmpeg_pid = [int]$ffmpegChildren[0].ProcessId
        video_path = $video.FullName
        active_video_relative_path = Convert-ToRelativePath -Base $RunDirectory -Path $video.FullName
        video_relative_path = Convert-ToRelativePath -Base $RunDirectory -Path (Join-Path $video.Directory.FullName "video.mp4")
    }
}

function New-ProcessCounterRow {
    param($Counter, [string]$Role)
    return [ordered]@{
        role = $Role
        pid = [int]$Counter.IDProcess
        processor_time_raw = [uint64]$Counter.PercentProcessorTime
        timestamp_100ns = [uint64]$Counter.Timestamp_Sys100NS
        working_set_bytes = [uint64]$Counter.WorkingSet
        private_bytes = [uint64]$Counter.PrivateBytes
        io_read_bytes_raw = [uint64]$Counter.IOReadBytesPersec
        io_write_bytes_raw = [uint64]$Counter.IOWriteBytesPersec
        io_other_bytes_raw = [uint64]$Counter.IOOtherBytesPersec
        timestamp_perf = [uint64]$Counter.Timestamp_PerfTime
        frequency_perf = [uint64]$Counter.Frequency_PerfTime
    }
}

function Get-RawTelemetrySample {
    param(
        [int]$Sequence,
        [double]$ElapsedSeconds,
        [int]$LeaguePid,
        [Nullable[int]]$CaptureRecorderPid,
        [Nullable[int]]$CaptureFfmpegPid,
        [uint64]$TotalMemoryBytes,
        [string]$OutputPath,
        [string]$OutputRelativePath
    )
    $status = [ordered]@{
        system_cpu = $false
        system_memory = $false
        processes = $false
        gpu_engine = $false
        gpu_adapter_memory = $false
        gpu_process_memory = $false
    }
    $errors = New-Object System.Collections.Generic.List[string]
    $system = [ordered]@{}
    $processRows = @()
    $gpuEngineRows = @()
    $gpuAdapterRows = @()
    $gpuProcessRows = @()
    try {
        $processor = Get-CimInstance -ClassName Win32_PerfRawData_PerfOS_Processor -ErrorAction Stop | Where-Object { $_.Name -eq "_Total" } | Select-Object -First 1
        if ($null -eq $processor) { throw "_Total processor instance missing" }
        $system.processor_idle_raw = [uint64]$processor.PercentProcessorTime
        $system.processor_timestamp_100ns = [uint64]$processor.Timestamp_Sys100NS
        $status.system_cpu = $true
    }
    catch { $errors.Add("system_cpu:$($_.Exception.GetType().Name)") }
    try {
        $memory = Get-CimInstance -ClassName Win32_PerfRawData_PerfOS_Memory -ErrorAction Stop | Select-Object -First 1
        if ($null -eq $memory) { throw "memory instance missing" }
        $system.available_memory_bytes = [uint64]$memory.AvailableBytes
        $system.committed_memory_bytes = [uint64]$memory.CommittedBytes
        $system.total_memory_bytes = $TotalMemoryBytes
        $status.system_memory = $true
    }
    catch { $errors.Add("system_memory:$($_.Exception.GetType().Name)") }
    try {
        $processCounters = @(Get-CimInstance -ClassName Win32_PerfRawData_PerfProc_Process -ErrorAction Stop)
        $roles = [ordered]@{ league = $LeaguePid; collector = $PID }
        if ($null -ne $CaptureRecorderPid) { $roles.recorder = [int]$CaptureRecorderPid }
        if ($null -ne $CaptureFfmpegPid) { $roles.ffmpeg = [int]$CaptureFfmpegPid }
        foreach ($role in $roles.Keys) {
            $counter = $processCounters | Where-Object { [int]$_.IDProcess -eq [int]$roles[$role] } | Select-Object -First 1
            if ($null -ne $counter) {
                $processRows += New-ProcessCounterRow -Counter $counter -Role $role
            }
        }
        $status.processes = $true
    }
    catch { $errors.Add("processes:$($_.Exception.GetType().Name)") }
    try {
        $gpuEngines = @(Get-CimInstance -ClassName Win32_PerfRawData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop)
        $gpuEngineRows = @($gpuEngines | ForEach-Object {
            [ordered]@{
                name = [string]$_.Name
                utilization_raw = [uint64]$_.UtilizationPercentage
                timestamp_100ns = [uint64]$_.Timestamp_Sys100NS
            }
        })
        $status.gpu_engine = $true
    }
    catch { $errors.Add("gpu_engine:$($_.Exception.GetType().Name)") }
    try {
        $gpuAdapters = @(Get-CimInstance -ClassName Win32_PerfRawData_GPUPerformanceCounters_GPUAdapterMemory -ErrorAction Stop)
        $gpuAdapterRows = @($gpuAdapters | ForEach-Object {
            [ordered]@{
                name = [string]$_.Name
                dedicated_bytes = [uint64]$_.DedicatedUsage
                shared_bytes = [uint64]$_.SharedUsage
            }
        })
        $status.gpu_adapter_memory = $true
    }
    catch { $errors.Add("gpu_adapter_memory:$($_.Exception.GetType().Name)") }
    try {
        $gpuProcesses = @(Get-CimInstance -ClassName Win32_PerfRawData_GPUPerformanceCounters_GPUProcessMemory -ErrorAction Stop)
        $gpuProcessRows = @($gpuProcesses | ForEach-Object {
            $match = [regex]::Match([string]$_.Name, '(?i)pid_(\d+)')
            if ($match.Success) {
                [ordered]@{
                    name = [string]$_.Name
                    pid = [int]$match.Groups[1].Value
                    dedicated_bytes = [uint64]$_.DedicatedUsage
                    shared_bytes = [uint64]$_.SharedUsage
                }
            }
        } | Where-Object { $null -ne $_ })
        $status.gpu_process_memory = $true
    }
    catch { $errors.Add("gpu_process_memory:$($_.Exception.GetType().Name)") }
    $output = $null
    if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
        if (Test-Path -LiteralPath $OutputPath -PathType Leaf) {
            $file = Get-Item -LiteralPath $OutputPath
            $output = [ordered]@{
                relative_path = $OutputRelativePath
                size_bytes = [uint64]$file.Length
            }
        }
    }
    return [ordered]@{
        schema_version = $script:SchemaVersion
        sequence = $Sequence
        utc = [DateTime]::UtcNow.ToString("o")
        elapsed_seconds = [Math]::Round($ElapsedSeconds, 6)
        query_status = $status
        query_errors = @($errors)
        system = $system
        processes = @($processRows)
        gpu_engines = @($gpuEngineRows)
        gpu_adapter_memory = @($gpuAdapterRows)
        gpu_process_memory = @($gpuProcessRows)
        output = $output
    }
}

function Wait-Warmup {
    Write-Host "Warmup started. Keep the fixed camera and provide no input for $WarmupSeconds seconds."
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $nextNotice = 15
    while ($watch.Elapsed.TotalSeconds -lt $WarmupSeconds) {
        $remaining = $WarmupSeconds - $watch.Elapsed.TotalSeconds
        if ($watch.Elapsed.TotalSeconds -ge $nextNotice) {
            Write-Host ("Warmup: {0:N0}s remaining" -f [Math]::Max(0, $remaining))
            $nextNotice += 15
        }
        Start-Sleep -Milliseconds ([Math]::Max(20, [Math]::Min(250, [int]($remaining * 1000))))
    }
}

function Wait-ForLeagueRefocus {
    Write-Host "Refocus the League window now. After the countdown, keep the fixed mid-lane camera and provide no input."
    for ($remaining = 5; $remaining -ge 1; $remaining--) {
        Write-Host "Warmup begins in $remaining..."
        Start-Sleep -Seconds 1
    }
}

function Collect-Run {
    param([string]$Root, [string]$RunDirectory, $PresentMon)
    if (-not $ProtocolAttestation) {
        Stop-WithCode "PROTOCOL_ATTESTATION" "Pass -ProtocolAttestation only after verifying Practice Tool/default-skin Garen/fixed mid camera/no input, 1920x1080 borderless, VSync off, and the requested frame mode."
    }
    if (-not $TestOnlyAllowNonProtocolTiming) {
        if ($WarmupSeconds -ne 60 -or $MeasurementSeconds -ne 180 -or $SampleIntervalSeconds -ne 1) {
            Stop-WithCode "PROTOCOL_TIMING" "Production runs require 60s warmup, 180s measurement, and 1s telemetry samples."
        }
    }
    $currentMayExist = $Condition -eq "capture"
    Assert-RunOrder -Root $Root -CurrentMayExist $currentMayExist
    if ($Condition -eq "baseline") {
        [void](New-Item -ItemType Directory -Path $RunDirectory)
    }
    elseif (-not (Test-Path -LiteralPath $RunDirectory -PathType Container)) {
        Stop-WithCode "CAPTURE_NOT_PREPARED" "Capture run directory does not exist. Run -PrepareCapture first."
    }
    foreach ($name in @("manifest.json", "presentmon.csv", "telemetry.ndjson")) {
        if (Test-Path -LiteralPath (Join-Path $RunDirectory $name)) {
            Stop-WithCode "RUN_IMMUTABLE" "$name already exists; collected runs are never overwritten."
        }
    }

    $leagueConfig = Get-LeagueConfigHash
    $league = Assert-LiveState
    $captureState = $null
    $prepared = $null
    if ($Condition -eq "capture") {
        $prepared = Get-PreparedState $RunDirectory
        $captureState = Wait-ForCaptureReadiness -RunDirectory $RunDirectory -Prepared $prepared
    }
    $environment = Get-SystemEnvironment -PresentMon $PresentMon -LeagueProcess $league
    Wait-ForLeagueRefocus
    Wait-Warmup
    if ((Get-Sha256 $leagueConfig.path) -ne $leagueConfig.sha256) {
        Stop-WithCode "CONFIG_CHANGED" "League configuration changed during warmup. Restore the intended settings and repeat the run."
    }
    $liveLeague = Assert-LiveState
    if ([int]$liveLeague.ProcessId -ne [int]$league.ProcessId) {
        Stop-WithCode "LEAGUE_PID_CHANGED" "League PID changed during warmup. Repeat the run."
    }
    if ($Condition -eq "capture") {
        $warmChildren = @(Get-ChildFfmpeg -ParentPid $RecorderPid -ExpectedPath ([string]$prepared.ffmpeg_path))
        if ($warmChildren.Count -ne 1 -or [int]$warmChildren[0].ProcessId -ne [int]$captureState.ffmpeg_pid) {
            Stop-WithCode "FFMPEG_PROCESS_COUNT" "Prepared ffmpeg child identity changed during warmup. Quit the recorder gracefully and repeat in a fresh result root."
        }
    }

    $presentmonCsv = Join-Path $RunDirectory "presentmon.csv"
    $telemetryPath = Join-Path $RunDirectory "telemetry.ndjson"
    $presentmonStdout = Join-Path $RunDirectory "presentmon.stdout.log"
    $presentmonStderr = Join-Path $RunDirectory "presentmon.stderr.log"
    $sessionName = "QueueBack-$((Get-RunId).Replace('-', '_'))-$PID"
    $arguments = @(
        "--process_id", [string]$league.ProcessId,
        "--output_file", $presentmonCsv,
        "--qpc_time_ms",
        "--track_gpu_video",
        "--timed", [string]$MeasurementSeconds,
        "--terminate_after_timed",
        "--terminate_on_proc_exit",
        "--no_console_stats",
        "--session_name", $sessionName
    )
    if ($PresentMon.supports_v2_flag) { $arguments += "--v2_metrics" }

    $pm = Start-NativeProcess -FilePath $PresentMon.path -Arguments $arguments
    $stdoutTask = $null
    $stderrTask = $null
    $stream = $null
    $measurementStartUtc = $null
    $watch = $null
    $sequence = 0
    $nextSample = 0.0
    $sampleOverruns = 0
    $recorderDisappeared = $false
    $ffmpegDisappeared = $false
    $collectionComplete = $false
    try {
        $stdoutTask = $pm.StandardOutput.ReadToEndAsync()
        $stderrTask = $pm.StandardError.ReadToEndAsync()
        $stream = New-Object System.IO.StreamWriter($telemetryPath, $false, $script:Utf8NoBom)
        $stream.NewLine = "`n"
        $measurementStartUtc = [DateTime]::UtcNow
        $watch = [System.Diagnostics.Stopwatch]::StartNew()
        while ($nextSample -le $MeasurementSeconds) {
            while ($watch.Elapsed.TotalSeconds -lt $nextSample) {
                $remainingMs = [int](($nextSample - $watch.Elapsed.TotalSeconds) * 1000)
                Start-Sleep -Milliseconds ([Math]::Max(1, [Math]::Min(100, $remainingMs)))
            }
            $recorderValue = [Nullable[int]]$null
            $ffmpegValue = [Nullable[int]]$null
            $outputPath = $null
            $outputRelativePath = $null
            Assert-RecorderIsolation
            if ($Condition -eq "capture") {
                $recorderValue = [Nullable[int]]$RecorderPid
                $ffmpegValue = [Nullable[int]]([int]$captureState.ffmpeg_pid)
                $outputPath = [string]$captureState.video_path
                $outputRelativePath = [string]$captureState.active_video_relative_path
                if ($null -eq (Get-Process -Id $RecorderPid -ErrorAction SilentlyContinue)) { $recorderDisappeared = $true }
                if ($null -eq (Get-Process -Id ([int]$captureState.ffmpeg_pid) -ErrorAction SilentlyContinue)) { $ffmpegDisappeared = $true }
            }
            $sample = Get-RawTelemetrySample `
                -Sequence $sequence `
                -ElapsedSeconds $watch.Elapsed.TotalSeconds `
                -LeaguePid ([int]$league.ProcessId) `
                -CaptureRecorderPid $recorderValue `
                -CaptureFfmpegPid $ffmpegValue `
                -TotalMemoryBytes ([uint64]$environment.cpu.total_memory_bytes) `
                -OutputPath $outputPath `
                -OutputRelativePath $outputRelativePath
            $stream.WriteLine(($sample | ConvertTo-Json -Compress -Depth 10))
            $stream.Flush()
            $sequence++
            $nextSample += $SampleIntervalSeconds
            while ($nextSample -le $MeasurementSeconds -and $nextSample -lt $watch.Elapsed.TotalSeconds) {
                $nextSample += $SampleIntervalSeconds
                $sampleOverruns++
            }
        }
        $collectionComplete = $true
    }
    finally {
        if ($null -ne $stream) { $stream.Dispose() }
        if (-not $collectionComplete -and -not $pm.HasExited) {
            try {
                $pm.Kill()
                $pm.WaitForExit()
            }
            catch {}
        }
    }
    $measurementEndUtc = [DateTime]::UtcNow
    $measurementElapsedSeconds = $watch.Elapsed.TotalSeconds
    $watch.Stop()
    Signal-MeasurementComplete
    $presentMonTimedOut = $false
    if (-not $pm.WaitForExit(15000)) {
        $presentMonTimedOut = $true
        try {
            $pm.Kill()
            $pm.WaitForExit()
        }
        catch {}
    }
    Write-Utf8Text -Path $presentmonStdout -Text $stdoutTask.Result
    Write-Utf8Text -Path $presentmonStderr -Text $stderrTask.Result
    if ($presentMonTimedOut) {
        Stop-WithCode "PRESENTMON_TIMEOUT" "PresentMon did not exit after its timed trace. It was stopped; inspect PresentMon logs and repeat in a fresh result root."
    }
    if ($pm.ExitCode -ne 0) {
        Stop-WithCode "PRESENTMON_EXIT" "PresentMon exited with code $($pm.ExitCode). Inspect presentmon.stderr.log and repeat in a fresh result root."
    }
    if (-not (Test-Path -LiteralPath $presentmonCsv -PathType Leaf) -or (Get-Item -LiteralPath $presentmonCsv).Length -eq 0) {
        Stop-WithCode "PRESENTMON_EMPTY" "PresentMon did not produce frame data. Verify Performance Log Users/admin access and the exact League PID."
    }
    $leagueConfigAfter = Get-Sha256 $leagueConfig.path
    if ($leagueConfigAfter -ne $leagueConfig.sha256) {
        Stop-WithCode "CONFIG_CHANGED" "League configuration changed during measurement; repeat the run."
    }
    $leagueAfter = @(Get-RunningLeagueProcesses)
    if ($leagueAfter.Count -ne 1 -or [int]$leagueAfter[0].ProcessId -ne [int]$league.ProcessId) {
        Stop-WithCode "LEAGUE_PID_CHANGED" "League exited or its PID changed during measurement; repeat the run."
    }

    $configuredLimit = $null
    if ($FrameMode -eq "capped") { $configuredLimit = 144 }
    $manifest = [ordered]@{
        schema_version = $script:SchemaVersion
        feature_id = $(if ($script:SchemaVersion -eq "2") { "QB-PERF-002" } else { "QB-PERF-001" })
        benchmark_contract = $(if ($script:SchemaVersion -eq "2") {
            [ordered]@{
                target_encoder = $TargetEncoder
                capture_backend = "windows_graphics_capture_d3d11"
                diagnostics_abi = 1
                encoder_interop = Get-ExpectedInterop
                support_labels = @("optimized-unvalidated", "optimized-validated")
                required_pair_count = 1
            }
        } else { $null })
        run_id = Get-RunId
        frame_mode = $FrameMode
        condition = $Condition
        run_number = $RunNumber
        timing = [ordered]@{
            warmup_seconds = $WarmupSeconds
            measurement_seconds = $MeasurementSeconds
            measurement_elapsed_seconds = [Math]::Round($measurementElapsedSeconds, 6)
            sample_interval_seconds = $SampleIntervalSeconds
            measurement_started_utc = $measurementStartUtc.ToString("o")
            measurement_ended_utc = $measurementEndUtc.ToString("o")
        }
        target = [ordered]@{
            process_name = "League of Legends.exe"
            configured_fps_limit = $configuredLimit
            protocol_attested = $true
        }
        configuration = [ordered]@{
            league_config_name = $leagueConfig.name
            league_config_sha256 = $leagueConfig.sha256
            league_config_sha256_after = $leagueConfigAfter
            width = 1920
            height = 1080
            display_mode = "borderless"
            vsync = $false
        }
        processes = [ordered]@{
            league = [int]$league.ProcessId
            recorder = $(if ($Condition -eq "capture") { $RecorderPid } else { $null })
            ffmpeg = $(if ($Condition -eq "capture") { [int]$captureState.ffmpeg_pid } else { $null })
        }
        environment = $environment
        telemetry = [ordered]@{
            format = "raw-cim-ndjson-v1"
            sample_interval_seconds = $SampleIntervalSeconds
            required_sources = @("system_cpu", "system_memory", "processes", "gpu_engine", "gpu_adapter_memory")
            optional_sources = @("gpu_process_memory", "per_process_gpu_engine")
            process_cpu_normalization = "raw_percent / logical_processors"
            gpu_aggregation = "sum contexts per physical engine, clamp to 100, then select busiest matching engine"
            sample_overruns = $sampleOverruns
        }
        artifacts = [ordered]@{
            presentmon = "presentmon.csv"
            telemetry = "telemetry.ndjson"
            presentmon_stdout = "presentmon.stdout.log"
            presentmon_stderr = "presentmon.stderr.log"
        }
        artifact_sha256 = [ordered]@{
            presentmon = Get-Sha256 $presentmonCsv
            telemetry = Get-Sha256 $telemetryPath
            presentmon_stdout = Get-Sha256 $presentmonStdout
            presentmon_stderr = Get-Sha256 $presentmonStderr
        }
        capture = $null
    }
    if ($Condition -eq "capture") {
        $manifest.capture = [ordered]@{
            finalized = $false
            finalized_utc = $null
            recording_relative_path = [string]$captureState.video_relative_path
            measurement_output_relative_path = [string]$captureState.active_video_relative_path
            source_config_sha256 = [string]$prepared.source_config_sha256
            recorder_config_sha256 = [string]$prepared.recorder_config_sha256
            recorder_binary_sha256 = [string]$prepared.recorder_sha256
            ffmpeg_binary_sha256 = [string]$prepared.ffmpeg_sha256
            media_sha256 = $null
            media_size_bytes = $null
            media_validation = $null
            recording_metadata = $null
            diagnostics = [ordered]@{
                recorder_disappeared = $recorderDisappeared
                ffmpeg_disappeared = $ffmpegDisappeared
                encoder_errors = @()
                poller_errors = @()
                poller = [ordered]@{}
                capture_target_fallback = $false
                recording_pid = $null
                audio_source = $null
            }
        }
    }
    Write-JsonFile -Path (Join-Path $RunDirectory "manifest.json") -Value $manifest
    Write-Host "Collected $(Get-RunId)."
    if ($Condition -eq "capture") {
        Write-Host "Keep Practice Tool open. Quit only the prepared recorder gracefully with its tray menu, then run this command again with -FinalizeCapture -FfprobePath <ffprobe.exe>."
    }
}

function Get-RecorderDiagnostics {
    param([string]$LogsDirectory, [int]$ExpectedLeaguePid)
    $files = @(Get-ChildItem -LiteralPath $LogsDirectory -File -Filter "recorder.log*")
    if ($files.Count -eq 0) {
        Stop-WithCode "RECORDER_LOG_MISSING" "No isolated recorder log exists for capture finalization."
    }
    $text = ($files | Sort-Object FullName | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 }) -join "`n"
    $encoderCodes = New-Object System.Collections.Generic.List[string]
    $patterns = [ordered]@{
        "ffmpeg_exited" = '(?i)ffmpeg exited'
        "recording_start_failed" = '(?i)recording (?:could not|failed to) start'
        "recording_stop_error" = '(?i)recording stopped with an error'
        "ffmpeg_inspection_failed" = '(?i)could not inspect ffmpeg'
    }
    foreach ($code in $patterns.Keys) {
        if ($text -match $patterns[$code]) { $encoderCodes.Add($code) }
    }
    $pollerCodes = New-Object System.Collections.Generic.List[string]
    $pollerPatterns = [ordered]@{
        "poller_stopped_error" = '(?i)Live Client poller stopped with an error'
        "poller_task_panicked" = '(?i)Live Client poller task panicked'
        "final_game_log_flush_failed" = '(?i)could not perform final game log flush'
    }
    foreach ($code in $pollerPatterns.Keys) {
        if ($text -match $pollerPatterns[$code]) { $pollerCodes.Add($code) }
    }
    $recordingStartLines = @($text -split "`r?`n" | Where-Object { $_ -match '(?i)starting recording' })
    if ($recordingStartLines.Count -eq 0) {
        Stop-WithCode "CAPTURE_TARGET" "The isolated recorder log has no starting-recording event."
    }
    $windowTargetObserved = @($recordingStartLines | Where-Object { $_ -match '(?i)target=.*window' }).Count -gt 0
    $primaryTargetObserved = @($recordingStartLines | Where-Object { $_ -match '(?i)target=.*primary display' }).Count -gt 0
    $fallback = (
        $text -match '(?i)primary-display fallback|retrying the primary-display fallback' -or
        $primaryTargetObserved -or
        -not $windowTargetObserved
    )
    $audioSources = New-Object System.Collections.Generic.List[string]
    foreach ($line in $recordingStartLines) {
        $audioMatch = [regex]::Match($line, '(?:^|\s)audio=(.+?)\s+encoder=')
        if ($audioMatch.Success) {
            $audio = $audioMatch.Groups[1].Value.Trim().Trim('"')
            $audioSources.Add($audio)
        }
    }
    $uniqueAudio = @($audioSources | Sort-Object -Unique)
    if ($uniqueAudio.Count -ne 1) {
        Stop-WithCode "AUDIO_DIAGNOSTICS" "Expected one stable audio source in the recorder start log, found $($uniqueAudio.Count)."
    }
    $startedLines = @($text -split "`r?`n" | Where-Object { $_ -match '(?i)\brecording started\b' })
    $recordingPids = New-Object System.Collections.Generic.List[int]
    foreach ($line in $startedLines) {
        $pidMatch = [regex]::Match($line, '(?:^|\s)pid=(\d+)')
        if ($pidMatch.Success) { $recordingPids.Add([int]$pidMatch.Groups[1].Value) }
    }
    $uniquePids = @($recordingPids | Sort-Object -Unique)
    if ($uniquePids.Count -ne 1 -or $uniquePids[0] -ne $ExpectedLeaguePid) {
        Stop-WithCode "CAPTURE_PID" "Recorder log target PID does not match measured League PID $ExpectedLeaguePid."
    }
    $diagnosticLines = @($text -split "`r?`n" | Where-Object { $_ -match 'Live Client poller diagnostics' })
    if ($diagnosticLines.Count -ne 1) {
        Stop-WithCode "POLLER_DIAGNOSTICS" "Expected exactly one final Live Client poller diagnostics log event, found $($diagnosticLines.Count). Quit the recorder gracefully before finalizing."
    }
    $poller = [ordered]@{}
    foreach ($field in @(
        "calibration_requests", "event_requests", "snapshot_requests", "successful_responses",
        "average_response_latency_ms", "maximum_response_latency_ms", "captured_events",
        "captured_snapshots", "game_log_bytes", "json_writes", "slowest_json_write_ms",
        "event_failure_reason", "snapshot_failure_reason"
    )) {
        $match = [regex]::Match($diagnosticLines[0], "(?:^|\s)$field=(?:`"([^`"]*)`"|(\S+))")
        if ($match.Success) {
            $value = $match.Groups[1].Value
            if ([string]::IsNullOrEmpty($value)) { $value = $match.Groups[2].Value }
            $poller[$field] = $value
        }
    }
    $requiredPollerFields = @(
        "calibration_requests", "event_requests", "snapshot_requests", "successful_responses",
        "average_response_latency_ms", "maximum_response_latency_ms", "captured_events",
        "captured_snapshots", "game_log_bytes", "json_writes", "slowest_json_write_ms",
        "event_failure_reason", "snapshot_failure_reason"
    )
    $missingPollerFields = @($requiredPollerFields | Where-Object { -not $poller.Contains($_) })
    if ($missingPollerFields.Count -gt 0) {
        Stop-WithCode "POLLER_DIAGNOSTICS" "Poller diagnostics event is missing fields: $($missingPollerFields -join ', ')."
    }
    $captureProgress = New-Object System.Collections.Generic.List[object]
    $progressPattern = 'queueback_capture_progress elapsed_ms=(\d+) source_frames_surfaced=(\d+) source_frames_superseded=(\d+) encoded_frames=(\d+) muxed_bytes=(\d+) latest_qpc_100ns=(-?\d+) cfr_duplicates=(\d+) cfr_discards=(\d+) pool_recreations=(\d+) terminal=(true|false)'
    foreach ($line in ($text -split "`r?`n")) {
        $match = [regex]::Match($line, $progressPattern)
        if (-not $match.Success) { continue }
        $captureProgress.Add([pscustomobject][ordered]@{
            elapsed_ms = [uint64]$match.Groups[1].Value
            source_frames_surfaced = [uint64]$match.Groups[2].Value
            source_frames_superseded = [uint64]$match.Groups[3].Value
            encoded_frames = [uint64]$match.Groups[4].Value
            muxed_bytes = [uint64]$match.Groups[5].Value
            latest_qpc_100ns = [int64]$match.Groups[6].Value
            cfr_duplicates = [uint64]$match.Groups[7].Value
            cfr_discards = [uint64]$match.Groups[8].Value
            pool_recreations = [uint64]$match.Groups[9].Value
            terminal = [bool]::Parse($match.Groups[10].Value)
        })
    }
    if ($script:SchemaVersion -eq "2" -and $captureProgress.Count -lt 2) {
        Stop-WithCode "CAPTURE_PROGRESS_DIAGNOSTICS" "Schema-v2 requires periodic and terminal in-process capture-progress evidence."
    }
    return [ordered]@{
        encoder_errors = @($encoderCodes | Sort-Object -Unique)
        poller_errors = @($pollerCodes | Sort-Object -Unique)
        poller = $poller
        capture_target_fallback = $fallback
        recording_pid = [int]$uniquePids[0]
        audio_source = [string]$uniqueAudio[0]
        capture_progress = $captureProgress.ToArray()
        log_files = @($files | ForEach-Object { "logs\$($_.Name)" })
    }
}

function Finalize-CaptureRun {
    param([string]$RunDirectory)
    if ($Condition -ne "capture") {
        Stop-WithCode "FINALIZE_CONDITION" "-FinalizeCapture is valid only with -Condition capture."
    }
    $manifestPath = Join-Path $RunDirectory "manifest.json"
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        Stop-WithCode "MANIFEST_MISSING" "Collect the capture run before finalization."
    }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($manifest.capture.finalized -eq $true) {
        Stop-WithCode "ALREADY_FINALIZED" "Capture run is already finalized and immutable."
    }
    if ($null -ne (Get-Process -Id ([int]$manifest.processes.ffmpeg) -ErrorAction SilentlyContinue)) {
        Stop-WithCode "FFMPEG_STILL_RUNNING" "The measured ffmpeg PID is still running. Leave Practice Tool and quit the prepared recorder gracefully; the finalizer will not terminate it."
    }
    if ($null -ne (Get-Process -Id ([int]$manifest.processes.recorder) -ErrorAction SilentlyContinue)) {
        Stop-WithCode "RECORDER_STILL_RUNNING" "The prepared recorder is still running. Use tray Quit (or headless Ctrl+C) so logs and metadata flush before finalization."
    }
    $prepared = Get-PreparedState $RunDirectory
    $ffprobe = Resolve-Executable -Value $FfprobePath -Label "FFPROBE"
    $decodeFfmpeg = Resolve-Executable -Value ([string]$prepared.ffmpeg_path) -Label "FFMPEG"
    $recordingRelative = [string]$manifest.capture.recording_relative_path
    if ([System.IO.Path]::IsPathRooted($recordingRelative) -or $recordingRelative.Contains("..")) {
        Stop-WithCode "RECORDING_PATH" "Manifest recording path is not a safe relative path."
    }
    $video = [System.IO.Path]::GetFullPath((Join-Path $RunDirectory $recordingRelative))
    $runFull = [System.IO.Path]::GetFullPath($RunDirectory).TrimEnd('\') + '\'
    if (-not $video.StartsWith($runFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        Stop-WithCode "RECORDING_PATH" "Manifest recording path escapes the run directory."
    }
    if (-not (Test-Path -LiteralPath $video -PathType Leaf)) {
        Stop-WithCode "RECORDING_MISSING" "Measured video.mp4 is missing."
    }
    $bundle = Split-Path -Parent $video
    $metadataPath = Join-Path $bundle "metadata.json"
    if (-not (Test-Path -LiteralPath $metadataPath -PathType Leaf)) {
        Stop-WithCode "METADATA_MISSING" "metadata.json is missing. Ensure the recorder was stopped gracefully."
    }
    $metadata = Get-Content -LiteralPath $metadataPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($script:SchemaVersion -eq "2") {
        if ([string]$metadata.encoder_used -ne $TargetEncoder) {
            Stop-WithCode "ENCODER_IDENTITY" "Recorder used $($metadata.encoder_used), expected $TargetEncoder."
        }
        if ($null -eq $metadata.capture) {
            Stop-WithCode "CAPTURE_METADATA" "Schema-v2 recording metadata has no GPU capture contract."
        }
    }
    $resolutionMatch = [regex]::Match([string]$metadata.recording_resolution, '^(\d+)x(\d+)$')
    if (-not $resolutionMatch.Success) {
        Stop-WithCode "METADATA_RESOLUTION" "Recorder metadata has an invalid resolution."
    }

    $ffprobeResult = Invoke-NativeCapture -FilePath $ffprobe -Arguments @(
        "-v", "error", "-show_entries",
        "format=duration:stream=index,codec_type,codec_name,profile,width,height,avg_frame_rate",
        "-of", "json", $video
    ) -TimeoutSeconds 120
    $ffprobeOutput = Join-Path $RunDirectory "ffprobe.json"
    Write-Utf8Text -Path $ffprobeOutput -Text ([string]$ffprobeResult.stdout)
    if ($ffprobeResult.exit_code -ne 0) {
        Stop-WithCode "FFPROBE_FAILED" "ffprobe rejected the finalized recording: $($ffprobeResult.stderr)"
    }
    try { $probe = [string]$ffprobeResult.stdout | ConvertFrom-Json }
    catch { Stop-WithCode "FFPROBE_JSON" "ffprobe returned malformed JSON." }
    $videoStreamRows = @($probe.streams | Where-Object { $_.codec_type -eq "video" })
    $audioStreamRows = @($probe.streams | Where-Object { $_.codec_type -eq "audio" })
    $videoStreams = $videoStreamRows.Count
    $audioStreams = $audioStreamRows.Count
    if ($videoStreams -ne 1 -or $audioStreams -lt 1) {
        Stop-WithCode "FFPROBE_STREAMS" "Finalized media must have exactly one video stream and at least one audio stream."
    }
    $videoStream = $videoStreamRows[0]
    $duration = 0.0
    if (-not [double]::TryParse([string]$probe.format.duration, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$duration)) {
        Stop-WithCode "FFPROBE_DURATION" "ffprobe did not report a numeric media duration."
    }
    $rateParts = ([string]$videoStream.avg_frame_rate).Split('/')
    $rateNumerator = 0.0
    $rateDenominator = 0.0
    if (
        $rateParts.Count -ne 2 -or
        -not [double]::TryParse($rateParts[0], [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$rateNumerator) -or
        -not [double]::TryParse($rateParts[1], [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$rateDenominator) -or
        $rateNumerator -le 0 -or $rateDenominator -le 0
    ) {
        Stop-WithCode "FFPROBE_FRAME_RATE" "ffprobe did not report a positive average video frame rate."
    }
    $actualFrameRate = $rateNumerator / $rateDenominator
    if (
        [int]$videoStream.width -ne [int]$resolutionMatch.Groups[1].Value -or
        [int]$videoStream.height -ne [int]$resolutionMatch.Groups[2].Value -or
        ([string]$videoStream.codec_name).ToLowerInvariant() -ne ([string]$metadata.recording_codec).ToLowerInvariant() -or
        [Math]::Abs($actualFrameRate - [double]$metadata.recording_fps) -gt 0.5
    ) {
        Stop-WithCode "MEDIA_METADATA_MISMATCH" "ffprobe codec/resolution/frame rate disagrees with recorder metadata."
    }
    $decodeResult = Invoke-NativeCapture -FilePath $decodeFfmpeg -Arguments @(
        "-v", "error", "-i", $video, "-f", "null", "NUL"
    ) -TimeoutSeconds ([Math]::Max(600, [int]($duration * 2)))
    $decodeLog = Join-Path $RunDirectory "ffmpeg-decode.log"
    Write-Utf8Text -Path $decodeLog -Text ([string]$decodeResult.stderr)
    if ($decodeResult.exit_code -ne 0) {
        Stop-WithCode "MEDIA_DECODE_FAILED" "Full ffmpeg decode failed. Inspect ffmpeg-decode.log."
    }

    $recorderDiagnostics = Get-RecorderDiagnostics `
        -LogsDirectory ([string]$prepared.logs_path) `
        -ExpectedLeaguePid ([int]$manifest.processes.league)
    $diagnosticPath = Join-Path $RunDirectory "recorder-diagnostics.json"
    Write-JsonFile -Path $diagnosticPath -Value $recorderDiagnostics
    $manifest.capture.finalized = $true
    $manifest.capture.finalized_utc = [DateTime]::UtcNow.ToString("o")
    $manifest.capture.media_sha256 = Get-Sha256 $video
    $manifest.capture.media_size_bytes = [uint64](Get-Item -LiteralPath $video).Length
    $manifest.capture.media_validation = [ordered]@{
        ffprobe_ok = $true
        decode_ok = $true
        video_streams = $videoStreams
        audio_streams = $audioStreams
        duration_seconds = $duration
        codec = ([string]$videoStream.codec_name).ToLowerInvariant()
        codec_profile = [string]$videoStream.profile
        width = [int]$videoStream.width
        height = [int]$videoStream.height
        average_frame_rate = $actualFrameRate
    }
    $manifest.capture.recording_metadata = [ordered]@{
        encoder_used = [string]$metadata.encoder_used
        codec = [string]$metadata.recording_codec
        profile = [string]$metadata.recording_profile
        width = [int]$resolutionMatch.Groups[1].Value
        height = [int]$resolutionMatch.Groups[2].Value
        fps = [int]$metadata.recording_fps
        duration_ms = [uint64]$metadata.duration_ms
        saved = [bool]$metadata.saved
        capture = $(if ($script:SchemaVersion -eq "2") { $metadata.capture } else { $null })
    }
    $manifest.capture.diagnostics.encoder_errors = @($recorderDiagnostics.encoder_errors)
    $manifest.capture.diagnostics.poller_errors = @($recorderDiagnostics.poller_errors)
    $manifest.capture.diagnostics.poller = $recorderDiagnostics.poller
    $manifest.capture.diagnostics.capture_target_fallback = [bool]$recorderDiagnostics.capture_target_fallback
    $manifest.capture.diagnostics.recording_pid = [int]$recorderDiagnostics.recording_pid
    $manifest.capture.diagnostics.audio_source = [string]$recorderDiagnostics.audio_source
    if ($script:SchemaVersion -eq "2") {
        $manifest.capture.diagnostics | Add-Member -NotePropertyName "capture_progress" -NotePropertyValue @($recorderDiagnostics.capture_progress) -Force
    }
    $manifest.artifacts | Add-Member -NotePropertyName "ffprobe" -NotePropertyValue "ffprobe.json" -Force
    $manifest.artifacts | Add-Member -NotePropertyName "recording" -NotePropertyValue $recordingRelative -Force
    $manifest.artifacts | Add-Member -NotePropertyName "decode_log" -NotePropertyValue "ffmpeg-decode.log" -Force
    $manifest.artifacts | Add-Member -NotePropertyName "recorder_diagnostics" -NotePropertyValue "recorder-diagnostics.json" -Force
    $manifest.artifact_sha256 | Add-Member -NotePropertyName "ffprobe" -NotePropertyValue (Get-Sha256 $ffprobeOutput) -Force
    $manifest.artifact_sha256 | Add-Member -NotePropertyName "recording" -NotePropertyValue ([string]$manifest.capture.media_sha256) -Force
    $manifest.artifact_sha256 | Add-Member -NotePropertyName "decode_log" -NotePropertyValue (Get-Sha256 $decodeLog) -Force
    $manifest.artifact_sha256 | Add-Member -NotePropertyName "recorder_diagnostics" -NotePropertyValue (Get-Sha256 $diagnosticPath) -Force
    Write-JsonFile -Path $manifestPath -Value $manifest
    Write-Host "Finalized $(Get-RunId): ffprobe and full decode passed."
}

try {
    Assert-UncappedRunNumber
    if ($env:OS -ne "Windows_NT") {
        Stop-WithCode "WINDOWS_REQUIRED" "The collector supports Windows only."
    }
    if ($PSVersionTable.PSVersion.Major -lt 5) {
        Stop-WithCode "POWERSHELL_VERSION" "PowerShell 5.1 or newer is required."
    }
    $selectedModes = @($PreflightOnly.IsPresent, $PrepareCapture.IsPresent, $FinalizeCapture.IsPresent)
    $modeCount = @($selectedModes | Where-Object { $_ }).Count
    if ($modeCount -gt 1) {
        Stop-WithCode "MODE" "Choose at most one of -PreflightOnly, -PrepareCapture, or -FinalizeCapture."
    }
    $root = Get-AbsolutePath $ResultRoot
    Assert-ResultRoot -Root $root -Create (-not $PreflightOnly)
    $presentMon = Get-PresentMonInfo $PresentMonPath
    Test-CounterPreflight
    if ($PreflightOnly) {
        Write-Host "QB-PERF-PREFLIGHT-OK: PresentMon $($presentMon.version), raw CPU/process/GPU counters, and result-root safety checks passed."
        Write-Host "LIVE-CHECKS-DEFERRED: League PID, exact display/config fingerprint, recorder/ffmpeg identity, output growth, and media validation require the live Practice Tool run."
        exit 0
    }
    $runDirectory = Get-RunDirectory $root
    if ($PrepareCapture) {
        Prepare-CaptureRun -Root $root -RunDirectory $runDirectory -PresentMon $presentMon
    }
    elseif ($FinalizeCapture) {
        Finalize-CaptureRun -RunDirectory $runDirectory
    }
    else {
        Set-BenchmarkExecutionState -Active $true
        try {
            Collect-Run -Root $root -RunDirectory $runDirectory -PresentMon $presentMon
        }
        finally {
            Set-BenchmarkExecutionState -Active $false
        }
    }
}
catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 2
}
