[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Spec,
    [Parameter(Mandatory = $true)]
    [string]$Manifest,
    [string]$MediaRuntimeRoot,
    [string]$FfmpegPath,
    [string]$FfprobePath,
    [string]$MediaRuntimeId,
    [ValidateRange(30, 86400)]
    [int]$DecodeTimeoutSeconds = 7200,
    [switch]$PreflightOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:SentinelName = ".chronobreak-replay-benchmark"
$script:AllowedTopLevel = @(
    "schema_version", "run_id", "sentinel_root", "library_root", "config_path",
    "app_data_root", "result_root", "scratch_root", "observer_profile", "ddragon",
    "fixtures", "scenarios", "app_binary", "analyzer_path", "python_path", "timeout_seconds"
)
$script:RequiredTopLevel = @(
    "schema_version", "run_id", "sentinel_root", "library_root", "config_path",
    "app_data_root", "result_root", "scratch_root", "observer_profile", "ddragon",
    "fixtures", "scenarios"
)
$script:AllowedScenarioFields = @(
    "id", "kind", "fixture_ids", "trial_id", "seed", "warmup_seconds",
    "duration_seconds", "idle_seconds", "target_times_ms", "rates", "request_rate_hz",
    "iterations", "distance_classes", "export_presets", "music_modes", "gain_modes",
    "endpoint_alignment", "observer_control", "seek_reason", "seek_playback_mode", "clip_start_ms", "clip_end_ms",
    "expected_duration_ms", "duration_tolerance_ms", "expected_video_codec",
    "expected_audio_codec", "music_mode", "gain_mode", "built_in_music_filename"
)
$script:ScenarioKinds = @(
    "app_idle", "cold_open", "warm_open", "play_pause", "rate", "seek", "scrub",
    "layout", "lifecycle", "export"
)

function Stop-Benchmark {
    param([string]$Code, [string]$Message)
    throw "REPLAY-BENCHMARK-$Code`: $Message"
}

function Write-Utf8Text {
    param([string]$Path, [string]$Text)
    [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Write-NewUtf8Text {
    param([string]$Path, [string]$Text)
    $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($Text)
    $stream = [System.IO.File]::Open(
        $Path,
        [System.IO.FileMode]::CreateNew,
        [System.IO.FileAccess]::Write,
        [System.IO.FileShare]::None
    )
    try { $stream.Write($bytes, 0, $bytes.Length) }
    finally { $stream.Dispose() }
}

function Write-JsonFile {
    param([string]$Path, [object]$Value)
    $json = $Value | ConvertTo-Json -Depth 100
    Write-Utf8Text -Path $Path -Text ($json + "`n")
}

function Get-RequiredProperty {
    param([object]$Object, [string]$Name, [string]$Context)
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        Stop-Benchmark "MANIFEST" "$Context is missing required property '$Name'."
    }
    return $property.Value
}

function Assert-OnlyProperties {
    param([object]$Object, [string[]]$Allowed, [string]$Context)
    $unexpected = @($Object.PSObject.Properties.Name | Where-Object { $_ -notin $Allowed })
    if ($unexpected.Count -gt 0) {
        Stop-Benchmark "MANIFEST" "$Context contains unsupported properties: $($unexpected -join ', ')."
    }
}

function Get-FullAbsolutePath {
    param([string]$Value, [string]$Label)
    if ([string]::IsNullOrWhiteSpace($Value) -or $Value -notmatch '^[A-Za-z]:[\\/]') {
        Stop-Benchmark "PATH" "$Label must be an absolute drive-qualified Windows path."
    }
    try {
        return [System.IO.Path]::GetFullPath($Value)
    }
    catch {
        Stop-Benchmark "PATH" "$Label is not a valid path: $($_.Exception.Message)"
    }
}

function Test-PathEqual {
    param([string]$Left, [string]$Right)
    return [string]::Equals(
        $Left.TrimEnd('\', '/'),
        $Right.TrimEnd('\', '/'),
        [System.StringComparison]::OrdinalIgnoreCase
    )
}

function Assert-NotBroadRoot {
    param([string]$Path, [string]$Label)
    $full = Get-FullAbsolutePath -Value $Path -Label $Label
    $volume = [System.IO.Path]::GetPathRoot($full)
    if (Test-PathEqual $full $volume) {
        Stop-Benchmark "BROAD_ROOT" "$Label cannot be a volume root."
    }
    $parent = [System.IO.Directory]::GetParent($full)
    if ($null -eq $parent -or (Test-PathEqual $parent.FullName $volume)) {
        Stop-Benchmark "BROAD_ROOT" "$Label cannot be a direct child of a volume root."
    }
    $knownBroad = New-Object System.Collections.Generic.List[string]
    foreach ($folder in @(
        [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile),
        [Environment]::GetFolderPath([Environment+SpecialFolder]::Windows),
        [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles),
        [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
    )) {
        if (-not [string]::IsNullOrWhiteSpace($folder)) { $knownBroad.Add($folder) }
    }
    foreach ($folder in $knownBroad) {
        if (Test-PathEqual $full ([System.IO.Path]::GetFullPath($folder))) {
            Stop-Benchmark "BROAD_ROOT" "$Label cannot be a system or user-profile root."
        }
    }
    return $full
}

function Assert-ReparseFree {
    param([string]$Path, [string]$Label)
    $full = Get-FullAbsolutePath -Value $Path -Label $Label
    $cursor = $full
    while (-not (Test-Path -LiteralPath $cursor)) {
        $parent = [System.IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName
    }
    while (-not [string]::IsNullOrWhiteSpace($cursor)) {
        $item = Get-Item -LiteralPath $cursor -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            Stop-Benchmark "REPARSE_PATH" "$Label traverses reparse point '$($item.FullName)'."
        }
        $parent = [System.IO.Directory]::GetParent($item.FullName)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName
    }
    return $full
}

function Assert-StrictDescendant {
    param([string]$Root, [string]$Path, [string]$Label)
    $rootPrefix = $Root.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    if (-not $Path.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        Stop-Benchmark "PATH_ESCAPE" "$Label must be strictly below sentinel_root."
    }
}

function Assert-DisjointRoots {
    param([object[]]$Entries)
    for ($leftIndex = 0; $leftIndex -lt $Entries.Count; $leftIndex++) {
        for ($rightIndex = $leftIndex + 1; $rightIndex -lt $Entries.Count; $rightIndex++) {
            $left = [string]$Entries[$leftIndex].path
            $right = [string]$Entries[$rightIndex].path
            $leftPrefix = $left.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
            $rightPrefix = $right.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
            if (
                (Test-PathEqual $left $right) -or
                $left.StartsWith($rightPrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
                $right.StartsWith($leftPrefix, [System.StringComparison]::OrdinalIgnoreCase)
            ) {
                Stop-Benchmark "ROOT_OVERLAP" "$($Entries[$leftIndex].label) and $($Entries[$rightIndex].label) must be disjoint."
            }
        }
    }
}

function Get-SafeRelativePath {
    param([string]$Value, [string]$Label)
    if (
        [string]::IsNullOrWhiteSpace($Value) -or
        [System.IO.Path]::IsPathRooted($Value) -or
        $Value.Contains(':')
    ) {
        Stop-Benchmark "RELATIVE_PATH" "$Label must be a non-empty relative path."
    }
    $parts = @($Value -split '[\\/]')
    if ($parts.Count -eq 0 -or @($parts | Where-Object { $_ -in @("", ".", "..") }).Count -gt 0) {
        Stop-Benchmark "RELATIVE_PATH" "$Label contains an empty, current, or parent component."
    }
    return ($parts -join [System.IO.Path]::DirectorySeparatorChar)
}

function Get-Sha256 {
    param([string]$Path)
    $stream = [System.IO.File]::OpenRead($Path)
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $hasher.ComputeHash($stream)
        return ([System.BitConverter]::ToString($bytes) -replace '-', '').ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
        $stream.Dispose()
    }
}

function Get-BenchmarkConfigText {
    param([string]$LibraryRoot)
    $tomlPath = $LibraryRoot.Replace('\', '\\').Replace('"', '\"')
    return (@(
        '[recording]',
        'profile = "auto"',
        'codec = "auto"',
        '',
        '[storage]',
        "output_path = `"$tomlPath`"",
        'auto_delete_days = 0',
        '',
        '[app]',
        'autostart = true',
        'hevc_playback_supported = false',
        ''
    ) -join "`n")
}

function Ensure-BenchmarkConfig {
    param([string]$Path, [string]$LibraryRoot, [switch]$ValidateOnly)
    $expected = Get-BenchmarkConfigText -LibraryRoot $LibraryRoot
    if (Test-Path -LiteralPath $Path) {
        if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
            Stop-Benchmark "CONFIG_PATH" "config_path exists but is not a file."
        }
        [void](Assert-ReparseFree -Path $Path -Label "config_path")
        $actual = [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
        if ($actual -ne $expected) {
            Stop-Benchmark "CONFIG_EXISTS" "Existing config_path is not the canonical benchmark config; preparation never overwrites it."
        }
        return [ordered]@{ created = $false; sha256 = Get-Sha256 $Path }
    }
    if (-not $ValidateOnly) { Write-NewUtf8Text -Path $Path -Text $expected }
    $hashBytes = [System.Text.UTF8Encoding]::new($false).GetBytes($expected)
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = ([System.BitConverter]::ToString($hasher.ComputeHash($hashBytes)) -replace '-', '').ToLowerInvariant()
    }
    finally { $hasher.Dispose() }
    return [ordered]@{ created = (-not $ValidateOnly); sha256 = $hash }
}

function Get-ReparseSafeFiles {
    param([string]$Root, [string]$FixtureId)
    $rootPrefix = $Root.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    $pending = New-Object 'System.Collections.Generic.Queue[string]'
    $files = New-Object 'System.Collections.Generic.List[object]'
    $pending.Enqueue($Root)
    while ($pending.Count -gt 0) {
        $directory = $pending.Dequeue()
        foreach ($item in @(Get-ChildItem -LiteralPath $directory -Force | Sort-Object FullName)) {
            if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                Stop-Benchmark "REPARSE_SOURCE" "Fixture '$FixtureId' contains reparse point '$($item.FullName)'."
            }
            if ($item.PSIsContainer) {
                $pending.Enqueue($item.FullName)
            }
            else {
                $files.Add([pscustomobject]@{
                    source = $item.FullName
                    relative = $item.FullName.Substring($rootPrefix.Length)
                })
            }
        }
    }
    return $files.ToArray()
}

function Get-DirectoryFingerprint {
    param([string]$Root, [string]$Label)
    $lines = New-Object 'System.Collections.Generic.List[string]'
    if (Test-Path -LiteralPath $Root -PathType Container) {
        foreach ($entry in @(Get-ReparseSafeFiles -Root $Root -FixtureId $Label | Sort-Object relative)) {
            $item = Get-Item -LiteralPath ([string]$entry.source)
            $relative = ([string]$entry.relative).Replace('\', '/')
            $lines.Add("$relative|$([uint64]$item.Length)|$(Get-Sha256 ([string]$entry.source))")
        }
    }
    $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes(($lines -join "`n"))
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        return "sha256:" + (([System.BitConverter]::ToString($hasher.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant())
    }
    finally { $hasher.Dispose() }
}

function ConvertTo-NativeArgument {
    param([string]$Value)
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') { return $Value }
    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $slashes++
            continue
        }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * (($slashes * 2) + 1)))
            [void]$builder.Append('"')
        }
        else {
            if ($slashes -gt 0) { [void]$builder.Append(('\' * $slashes)) }
            [void]$builder.Append($character)
        }
        $slashes = 0
    }
    if ($slashes -gt 0) { [void]$builder.Append(('\' * ($slashes * 2))) }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Invoke-NativeTool {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [int]$TimeoutSeconds,
        [string]$Label
    )
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $FilePath
    $start.Arguments = (($Arguments | ForEach-Object { ConvertTo-NativeArgument ([string]$_) }) -join ' ')
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (-not $process.Start()) { Stop-Benchmark "TOOL_START" "$Label did not start." }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        try { $process.Kill() } catch {}
        try { $process.WaitForExit() } catch {}
        Stop-Benchmark "TOOL_TIMEOUT" "$Label exceeded the finite $TimeoutSeconds-second timeout."
    }
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    return [pscustomobject][ordered]@{
        exit_code = [int]$process.ExitCode
        stdout = [string]$stdout
        stderr = [string]$stderr
    }
}

function Get-ToolIdentity {
    param([string]$Path, [string]$Label)
    $full = Assert-ReparseFree -Path $Path -Label $Label
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
        Stop-Benchmark "TOOL_MISSING" "$Label does not exist: $full"
    }
    $version = Invoke-NativeTool -FilePath $full -Arguments @("-version") -TimeoutSeconds 30 -Label "$Label -version"
    if ($version.exit_code -ne 0) {
        Stop-Benchmark "TOOL_VERSION" "$Label -version failed: $($version.stderr.Trim())"
    }
    $firstLine = @($version.stdout -split "`r?`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })[0]
    $item = Get-Item -LiteralPath $full
    return [ordered]@{
        path = $full
        size_bytes = [uint64]$item.Length
        sha256 = Get-Sha256 $full
        version_line = [string]$firstLine
    }
}

function Assert-Scenario {
    param([object]$Scenario, [string[]]$FixtureIds)
    Assert-OnlyProperties -Object $Scenario -Allowed $script:AllowedScenarioFields -Context "scenario"
    $id = [string](Get-RequiredProperty $Scenario "id" "scenario")
    if ($id -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') {
        Stop-Benchmark "MANIFEST" "Scenario id '$id' is unsafe."
    }
    $kind = [string](Get-RequiredProperty $Scenario "kind" "scenario '$id'")
    if ($kind -notin $script:ScenarioKinds) {
        Stop-Benchmark "MANIFEST" "Scenario '$id' has unsupported kind '$kind'."
    }
    $trialId = [string](Get-RequiredProperty $Scenario "trial_id" "scenario '$id'")
    if ($trialId -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') {
        Stop-Benchmark "MANIFEST" "Scenario '$id' trial_id is unsafe."
    }
    $references = @((Get-RequiredProperty $Scenario "fixture_ids" "scenario '$id'"))
    if ($references.Count -eq 0) {
        Stop-Benchmark "MANIFEST" "Scenario '$id' must reference at least one fixture."
    }
    if ($kind -ne "lifecycle" -and $references.Count -ne 1) {
        Stop-Benchmark "MANIFEST" "Only lifecycle scenarios may reference multiple fixtures."
    }
    foreach ($fixtureId in $references) {
        if ([string]$fixtureId -notin $FixtureIds) {
            Stop-Benchmark "MANIFEST" "Scenario '$id' references unknown fixture '$fixtureId'."
        }
    }
    if ($Scenario.PSObject.Properties["seek_reason"] -and [string]$Scenario.seek_reason -notin @("benchmark", "event-jump", "endpoint-edit")) {
        Stop-Benchmark "MANIFEST" "Scenario '$id' has an unsupported seek_reason."
    }
    if ($Scenario.PSObject.Properties["seek_playback_mode"] -and [string]$Scenario.seek_playback_mode -notin @("playing", "paused")) {
        Stop-Benchmark "MANIFEST" "Scenario '$id' has an unsupported seek_playback_mode."
    }
    if ($kind -eq "export") {
        foreach ($field in @(
            "clip_start_ms", "clip_end_ms", "expected_duration_ms", "expected_video_codec",
            "expected_audio_codec", "export_presets"
        )) {
            [void](Get-RequiredProperty $Scenario $field "export scenario '$id'")
        }
        [int64]$clipStartMs = $Scenario.clip_start_ms
        [int64]$clipEndMs = $Scenario.clip_end_ms
        [int64]$expectedDurationMs = $Scenario.expected_duration_ms
        if ($clipStartMs -lt 0 -or $clipEndMs -le $clipStartMs -or ($clipEndMs - $clipStartMs) -lt 5000) {
            Stop-Benchmark "MANIFEST" "Export scenario '$id' must declare a clip of at least five seconds."
        }
        if ($expectedDurationMs -ne ($clipEndMs - $clipStartMs)) {
            Stop-Benchmark "MANIFEST" "Export scenario '$id' expected_duration_ms must equal clip_end_ms minus clip_start_ms."
        }
        $presets = @($Scenario.export_presets)
        if ($presets.Count -lt 1 -or $presets.Count -gt 3 -or @($presets | Select-Object -Unique).Count -ne $presets.Count) {
            Stop-Benchmark "MANIFEST" "Export scenario '$id' must declare one to three unique export_presets."
        }
        if (@($presets | Where-Object { [string]$_ -notin @("horizontal", "vertical", "discord") }).Count -gt 0) {
            Stop-Benchmark "MANIFEST" "Export scenario '$id' has an unsupported export preset."
        }
        if ([string]$Scenario.expected_video_codec -ne "h264" -or [string]$Scenario.expected_audio_codec -ne "aac") {
            Stop-Benchmark "MANIFEST" "Export scenario '$id' must validate the current H.264/AAC export contract."
        }
        if ($Scenario.PSObject.Properties["duration_tolerance_ms"] -and ([int64]$Scenario.duration_tolerance_ms -lt 0 -or [int64]$Scenario.duration_tolerance_ms -gt 5000)) {
            Stop-Benchmark "MANIFEST" "Export scenario '$id' duration_tolerance_ms must be between 0 and 5000."
        }
        if ($Scenario.PSObject.Properties["music_mode"] -and [string]$Scenario.music_mode -notin @("none", "built_in")) {
            Stop-Benchmark "MANIFEST" "Export scenario '$id' has an unsupported music_mode."
        }
        if ($Scenario.PSObject.Properties["gain_mode"] -and [string]$Scenario.gain_mode -notin @("unity", "non_unity")) {
            Stop-Benchmark "MANIFEST" "Export scenario '$id' has an unsupported gain_mode."
        }
        if ($Scenario.PSObject.Properties["built_in_music_filename"] -and [string]$Scenario.built_in_music_filename -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$') {
            Stop-Benchmark "MANIFEST" "Export scenario '$id' built_in_music_filename must be a safe filename."
        }
    }
}

function Read-AndValidateSpec {
    param([string]$Path)
    $full = Get-FullAbsolutePath -Value $Path -Label "Spec"
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
        Stop-Benchmark "SPEC_MISSING" "Preparation specification does not exist: $full"
    }
    $full = Assert-ReparseFree -Path $full -Label "Spec"
    try { $value = Get-Content -LiteralPath $full -Raw -Encoding UTF8 | ConvertFrom-Json }
    catch { Stop-Benchmark "SPEC_JSON" "Preparation specification is not valid JSON: $($_.Exception.Message)" }
    Assert-OnlyProperties -Object $value -Allowed $script:AllowedTopLevel -Context "preparation specification"
    foreach ($name in $script:RequiredTopLevel) {
        [void](Get-RequiredProperty $value $name "preparation specification")
    }
    if ([int]$value.schema_version -ne 1) { Stop-Benchmark "SCHEMA_VERSION" "schema_version must be 1." }
    if ([string]$value.run_id -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') {
        Stop-Benchmark "RUN_ID" "run_id is unsafe."
    }
    if ([string]$value.observer_profile -notin @("minimal", "full")) {
        Stop-Benchmark "OBSERVER_PROFILE" "observer_profile must be minimal or full."
    }
    Assert-OnlyProperties -Object $value.ddragon -Allowed @("mode", "cache_root", "cache_fingerprint") -Context "ddragon"
    if ([string]$value.ddragon.mode -ne "offline") {
        Stop-Benchmark "DDRAGON" "ddragon.mode must be exactly 'offline'."
    }
    $fixtures = @($value.fixtures)
    if ($fixtures.Count -eq 0) { Stop-Benchmark "FIXTURES" "At least one fixture is required." }
    $ids = New-Object System.Collections.Generic.List[string]
    $aliases = New-Object System.Collections.Generic.List[string]
    $gameTimestamps = New-Object System.Collections.Generic.List[string]
    foreach ($fixture in $fixtures) {
        Assert-OnlyProperties -Object $fixture -Allowed @(
            "id", "alias", "game_timestamp", "kind", "source_path", "destination_relative_path", "backend", "codec", "negative"
        ) -Context "fixture"
        foreach ($field in @("id", "alias", "game_timestamp", "kind", "source_path", "destination_relative_path")) {
            [void](Get-RequiredProperty $fixture $field "fixture")
        }
        $fixtureId = [string]$fixture.id
        if ($fixtureId -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$' -or $ids.Contains($fixtureId)) {
            Stop-Benchmark "FIXTURE_ID" "Fixture id '$fixtureId' is unsafe or duplicated."
        }
        if ([string]$fixture.kind -notin @("recording_bundle", "media", "negative_fixture")) {
            Stop-Benchmark "FIXTURE_KIND" "Fixture '$fixtureId' has unsupported kind '$($fixture.kind)'."
        }
        if ([string]$fixture.alias -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') {
            Stop-Benchmark "FIXTURE_ALIAS" "Fixture '$fixtureId' alias is unsafe."
        }
        if ($aliases.Contains([string]$fixture.alias)) {
            Stop-Benchmark "FIXTURE_ALIAS" "Fixture alias '$($fixture.alias)' is duplicated."
        }
        $gameTimestamp = [string]$fixture.game_timestamp
        if ($gameTimestamp -notmatch '^\d+(?:-[1-9]\d{0,2})?$') {
            Stop-Benchmark "FIXTURE_TIMESTAMP" "Fixture '$fixtureId' game_timestamp is not a canonical game id."
        }
        if ($gameTimestamps.Contains($gameTimestamp)) {
            Stop-Benchmark "FIXTURE_TIMESTAMP" "Fixture game_timestamp '$gameTimestamp' is duplicated."
        }
        $destinationRelative = Get-SafeRelativePath -Value ([string]$fixture.destination_relative_path) -Label "fixture '$fixtureId' destination"
        if (
            [string]$fixture.kind -eq "recording_bundle" -and
            $destinationRelative -ne (Join-Path "games" $gameTimestamp)
        ) {
            Stop-Benchmark "FIXTURE_TIMESTAMP" "Recording fixture '$fixtureId' destination must be games\\$gameTimestamp."
        }
        $ids.Add($fixtureId)
        $aliases.Add([string]$fixture.alias)
        $gameTimestamps.Add($gameTimestamp)
    }
    $scenarios = @($value.scenarios)
    if ($scenarios.Count -ne 1) { Stop-Benchmark "SCENARIOS" "A launch manifest must contain exactly one scenario/trial." }
    $scenarioIds = New-Object System.Collections.Generic.List[string]
    foreach ($scenario in $scenarios) {
        Assert-Scenario -Scenario $scenario -FixtureIds $ids.ToArray()
        if ($scenarioIds.Contains([string]$scenario.id)) {
            Stop-Benchmark "SCENARIO_ID" "Scenario id '$($scenario.id)' is duplicated."
        }
        $scenarioIds.Add([string]$scenario.id)
    }
    return [pscustomobject]@{ path = $full; value = $value }
}

function Resolve-MediaTools {
    $resolvedRuntimeId = $MediaRuntimeId
    if (-not [string]::IsNullOrWhiteSpace($MediaRuntimeRoot)) {
        if (-not [string]::IsNullOrWhiteSpace($FfmpegPath) -or -not [string]::IsNullOrWhiteSpace($FfprobePath)) {
            Stop-Benchmark "TOOLS" "Use either -MediaRuntimeRoot or both explicit tool paths, not both forms."
        }
        $runtime = Assert-ReparseFree -Path $MediaRuntimeRoot -Label "MediaRuntimeRoot"
        if (-not (Test-Path -LiteralPath $runtime -PathType Container)) {
            Stop-Benchmark "TOOLS" "MediaRuntimeRoot does not exist: $runtime"
        }
        $runtimeManifest = Assert-ReparseFree -Path (Join-Path $runtime "runtime-manifest.json") -Label "media runtime manifest"
        if (-not (Test-Path -LiteralPath $runtimeManifest -PathType Leaf)) {
            Stop-Benchmark "TOOLS" "MediaRuntimeRoot has no runtime-manifest.json."
        }
        try { $runtimeIdentity = Get-Content -LiteralPath $runtimeManifest -Raw -Encoding UTF8 | ConvertFrom-Json }
        catch { Stop-Benchmark "TOOLS" "Media runtime manifest is malformed: $($_.Exception.Message)" }
        if ([string]::IsNullOrWhiteSpace([string]$runtimeIdentity.runtime_id)) {
            Stop-Benchmark "TOOLS" "Media runtime manifest has no runtime_id."
        }
        if (
            -not [string]::IsNullOrWhiteSpace($resolvedRuntimeId) -and
            $resolvedRuntimeId -ne [string]$runtimeIdentity.runtime_id
        ) {
            Stop-Benchmark "TOOLS" "-MediaRuntimeId disagrees with runtime-manifest.json."
        }
        $resolvedRuntimeId = [string]$runtimeIdentity.runtime_id
        $script:FfmpegPath = Join-Path $runtime "bin\ffmpeg.exe"
        $script:FfprobePath = Join-Path $runtime "bin\ffprobe.exe"
    }
    $hasFfmpeg = -not [string]::IsNullOrWhiteSpace($FfmpegPath)
    $hasFfprobe = -not [string]::IsNullOrWhiteSpace($FfprobePath)
    if ($hasFfmpeg -ne $hasFfprobe) {
        Stop-Benchmark "TOOLS" "Provide both ffmpeg and ffprobe, or neither."
    }
    if (-not $hasFfmpeg) { return $null }
    if ([string]::IsNullOrWhiteSpace($resolvedRuntimeId)) {
        Stop-Benchmark "TOOLS" "Explicit packaged tool paths require -MediaRuntimeId."
    }
    return [ordered]@{
        runtime_id = $resolvedRuntimeId
        ffmpeg = Get-ToolIdentity -Path $FfmpegPath -Label "ffmpeg"
        ffprobe = Get-ToolIdentity -Path $FfprobePath -Label "ffprobe"
    }
}

function Copy-ExplicitFixture {
    param(
        [object]$Fixture,
        [string]$LibraryRoot,
        [object]$MediaTools,
        [System.Collections.Generic.List[object]]$ReceiptFiles
    )
    $fixtureId = [string]$Fixture.id
    $source = Assert-ReparseFree -Path ([string]$Fixture.source_path) -Label "fixture '$fixtureId' source"
    if (-not (Test-Path -LiteralPath $source)) {
        Stop-Benchmark "FIXTURE_MISSING" "Fixture '$fixtureId' source does not exist: $source"
    }
    if (Test-Path -LiteralPath $source -PathType Container) {
        [void](Assert-NotBroadRoot -Path $source -Label "fixture '$fixtureId' source directory")
    }
    $relativeDestination = Get-SafeRelativePath `
        -Value ([string]$Fixture.destination_relative_path) `
        -Label "fixture '$fixtureId' destination"
    $destination = Get-FullAbsolutePath -Value (Join-Path $LibraryRoot $relativeDestination) -Label "fixture destination"
    Assert-StrictDescendant -Root $LibraryRoot -Path $destination -Label "fixture '$fixtureId' destination"
    if (Test-Path -LiteralPath $destination) {
        Stop-Benchmark "FIXTURE_EXISTS" "Fixture '$fixtureId' destination already exists; preparation never overwrites it."
    }

    $sourceFiles = New-Object System.Collections.Generic.List[object]
    if (Test-Path -LiteralPath $source -PathType Leaf) {
        $sourceFiles.Add([pscustomobject]@{ source = $source; relative = [System.IO.Path]::GetFileName($destination) })
    }
    else {
        foreach ($entry in @(Get-ReparseSafeFiles -Root $source -FixtureId $fixtureId)) { $sourceFiles.Add($entry) }
    }
    if ($sourceFiles.Count -eq 0) { Stop-Benchmark "FIXTURE_EMPTY" "Fixture '$fixtureId' contains no files." }

    if (-not $PreflightOnly) {
        if (Test-Path -LiteralPath $source -PathType Container) {
            New-Item -ItemType Directory -Path $destination | Out-Null
        }
        else {
            $destinationParent = Split-Path -Parent $destination
            New-Item -ItemType Directory -Path $destinationParent -Force | Out-Null
        }
    }

    $preparedFiles = New-Object System.Collections.Generic.List[object]
    foreach ($entry in $sourceFiles) {
        $sourceFile = Assert-ReparseFree -Path ([string]$entry.source) -Label "fixture '$fixtureId' file"
        $sourceItem = Get-Item -LiteralPath $sourceFile
        $sourceLastWriteUtc = [DateTime]::SpecifyKind(
            $sourceItem.LastWriteTimeUtc,
            [System.DateTimeKind]::Utc
        )
        $sourceHashBefore = Get-Sha256 $sourceFile
        $destinationFile = if (Test-Path -LiteralPath $source -PathType Leaf) {
            $destination
        }
        else {
            Join-Path $destination ([string]$entry.relative)
        }
        $destinationFile = Get-FullAbsolutePath -Value $destinationFile -Label "fixture '$fixtureId' staged file"
        Assert-StrictDescendant -Root $LibraryRoot -Path $destinationFile -Label "fixture '$fixtureId' staged file"
        if (-not $PreflightOnly) {
            $parent = Split-Path -Parent $destinationFile
            if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
                New-Item -ItemType Directory -Path $parent -Force | Out-Null
            }
            [System.IO.File]::Copy($sourceFile, $destinationFile, $false)
            [System.IO.File]::SetLastWriteTimeUtc($destinationFile, $sourceLastWriteUtc)
        }
        $sourceHashAfter = Get-Sha256 $sourceFile
        if ($sourceHashBefore -ne $sourceHashAfter) {
            Stop-Benchmark "SOURCE_CHANGED" "Fixture '$fixtureId' source changed while it was copied: $sourceFile"
        }
        if ($PreflightOnly) {
            $stagedHash = $sourceHashBefore
            $stagedSize = [uint64]$sourceItem.Length
            $stagedLastWrite = $sourceLastWriteUtc
        }
        else {
            $stagedHash = Get-Sha256 $destinationFile
            $destinationItem = Get-Item -LiteralPath $destinationFile
            $stagedSize = [uint64]$destinationItem.Length
            $stagedLastWrite = [DateTime]::SpecifyKind(
                $destinationItem.LastWriteTimeUtc,
                [System.DateTimeKind]::Utc
            )
            if ($stagedHash -ne $sourceHashBefore -or $stagedSize -ne [uint64]$sourceItem.Length) {
                Stop-Benchmark "COPY_MISMATCH" "Fixture '$fixtureId' staged copy does not match its source."
            }
        }
        $libraryRelative = $destinationFile.Substring($LibraryRoot.TrimEnd('\', '/').Length + 1)
        $preparedFiles.Add([ordered]@{
            relative_path = $libraryRelative
            size_bytes = $stagedSize
            sha256 = $stagedHash
            last_write_utc = $stagedLastWrite.ToString("o")
        })
        $ReceiptFiles.Add([ordered]@{
            fixture_id = $fixtureId
            source_path = $sourceFile
            destination_path = $destinationFile
            source_sha256_before = $sourceHashBefore
            source_sha256_after = $sourceHashAfter
            staged_sha256 = $stagedHash
            size_bytes = $stagedSize
            copied = -not $PreflightOnly
        })
    }

    $mediaValidations = New-Object System.Collections.Generic.List[object]
    $mediaFiles = @($preparedFiles | Where-Object {
        [System.IO.Path]::GetExtension([string]$_.relative_path).ToLowerInvariant() -in @(".mp4", ".mov", ".mkv", ".webm")
    })
    if ($Fixture.kind -ne "negative_fixture" -and $mediaFiles.Count -eq 0) {
        Stop-Benchmark "MEDIA_MISSING" "Positive fixture '$fixtureId' contains no supported media file."
    }
    if ($null -ne $MediaTools -and -not $PreflightOnly) {
        foreach ($file in $mediaFiles) {
            $mediaPath = Join-Path $LibraryRoot ([string]$file.relative_path)
            $probe = Invoke-NativeTool -FilePath ([string]$MediaTools.ffprobe.path) -Arguments @(
                "-v", "error", "-show_entries",
                "format=duration,format_name,start_time,size:stream=index,codec_type,codec_name,profile,width,height,avg_frame_rate,time_base,start_time,duration",
                "-of", "json", $mediaPath
            ) -TimeoutSeconds 300 -Label "ffprobe fixture '$fixtureId'"
            $probeObject = $null
            if ($probe.exit_code -eq 0) {
                try { $probeObject = $probe.stdout | ConvertFrom-Json }
                catch { $probeObject = [ordered]@{ parse_error = $_.Exception.Message } }
            }
            else {
                $probeObject = [ordered]@{ error = $probe.stderr.Trim() }
            }
            $decode = Invoke-NativeTool -FilePath ([string]$MediaTools.ffmpeg.path) -Arguments @(
                "-threads", "1", "-v", "error", "-i", $mediaPath,
                "-map", "0:v?", "-map", "0:a?", "-f", "null", "NUL"
            ) -TimeoutSeconds $DecodeTimeoutSeconds -Label "full decode fixture '$fixtureId'"
            $decodeError = [string]$decode.stderr
            if ($decodeError.Length -gt 16384) { $decodeError = $decodeError.Substring(0, 16384) }
            $negative = $Fixture.kind -eq "negative_fixture" -or (
                $Fixture.PSObject.Properties["negative"] -and [bool]$Fixture.negative
            )
            if (-not $negative -and ($probe.exit_code -ne 0 -or $null -eq $probeObject -or $decode.exit_code -ne 0)) {
                Stop-Benchmark "MEDIA_INVALID" "Positive fixture '$fixtureId' failed ffprobe or full decode."
            }
            $mediaValidations.Add([ordered]@{
                relative_path = [string]$file.relative_path
                ffprobe = $probeObject
                decode_ok = ($decode.exit_code -eq 0)
                decode_stderr = $decodeError
            })
        }
    }

    $backend = if ($Fixture.PSObject.Properties["backend"]) { $Fixture.backend } else { $null }
    $codec = if ($Fixture.PSObject.Properties["codec"]) { $Fixture.codec } else { $null }
    $negativeFlag = if ($Fixture.PSObject.Properties["negative"]) {
        [bool]$Fixture.negative
    }
    else {
        $Fixture.kind -eq "negative_fixture"
    }
    return [ordered]@{
        id = $fixtureId
        alias = [string]$Fixture.alias
        game_timestamp = [string]$Fixture.game_timestamp
        kind = [string]$Fixture.kind
        relative_path = $relativeDestination
        backend = $backend
        codec = $codec
        negative = $negativeFlag
        files = $preparedFiles.ToArray()
        media_validation = $mediaValidations.ToArray()
    }
}

try {
    if ($env:OS -ne "Windows_NT") {
        Stop-Benchmark "WINDOWS_REQUIRED" "Replay benchmark preparation supports Windows only."
    }
    if ($PSVersionTable.PSVersion.Major -lt 5) {
        Stop-Benchmark "POWERSHELL_VERSION" "PowerShell 5.1 or newer is required."
    }

    $parsed = Read-AndValidateSpec -Path $Spec
    $specification = $parsed.value
    $sentinelRoot = Assert-NotBroadRoot -Path ([string]$specification.sentinel_root) -Label "sentinel_root"
    if (-not (Test-Path -LiteralPath $sentinelRoot -PathType Container)) {
        Stop-Benchmark "SENTINEL_ROOT" "sentinel_root must already exist."
    }
    $sentinelRoot = Assert-ReparseFree -Path $sentinelRoot -Label "sentinel_root"
    if ([System.IO.Path]::GetFileName($sentinelRoot) -ne $script:SentinelName) {
        Stop-Benchmark "SENTINEL_NAME" "sentinel_root itself must end with '$script:SentinelName'."
    }

    $libraryRoot = Get-FullAbsolutePath -Value ([string]$specification.library_root) -Label "library_root"
    $configPath = Get-FullAbsolutePath -Value ([string]$specification.config_path) -Label "config_path"
    $appDataRoot = Get-FullAbsolutePath -Value ([string]$specification.app_data_root) -Label "app_data_root"
    $resultRoot = Get-FullAbsolutePath -Value ([string]$specification.result_root) -Label "result_root"
    $scratchRoot = Get-FullAbsolutePath -Value ([string]$specification.scratch_root) -Label "scratch_root"
    $manifestPath = Get-FullAbsolutePath -Value $Manifest -Label "Manifest"
    Assert-DisjointRoots -Entries @(
        [pscustomobject]@{ label = "library_root"; path = $libraryRoot },
        [pscustomobject]@{ label = "config parent"; path = (Split-Path -Parent $configPath) },
        [pscustomobject]@{ label = "app_data_root"; path = $appDataRoot },
        [pscustomobject]@{ label = "result_root"; path = $resultRoot },
        [pscustomobject]@{ label = "scratch_root"; path = $scratchRoot }
    )
    foreach ($pair in @(
        @($libraryRoot, "library_root"), @($configPath, "config_path"),
        @($appDataRoot, "app_data_root"), @($resultRoot, "result_root"),
        @($scratchRoot, "scratch_root"), @($manifestPath, "Manifest")
    )) {
        Assert-StrictDescendant -Root $sentinelRoot -Path $pair[0] -Label $pair[1]
        [void](Assert-ReparseFree -Path $pair[0] -Label $pair[1])
    }
    if ([System.IO.Path]::GetFileName($configPath) -ne "config.toml") {
        Stop-Benchmark "CONFIG_PATH" "config_path must end with config.toml."
    }
    if ([System.IO.Path]::GetFileName($resultRoot) -ne [string]$specification.run_id) {
        Stop-Benchmark "RESULT_ROOT" "result_root leaf must equal run_id."
    }
    if (Test-Path -LiteralPath $resultRoot) {
        Stop-Benchmark "RESULT_EXISTS" "result_root already exists; runs are immutable and preparation never reuses one."
    }
    if (Test-Path -LiteralPath $manifestPath) {
        Stop-Benchmark "MANIFEST_EXISTS" "Manifest already exists; preparation never overwrites it."
    }

    $receiptPath = Join-Path (Split-Path -Parent $manifestPath) `
        (([System.IO.Path]::GetFileNameWithoutExtension($manifestPath)) + ".preparation.json")
    if (Test-Path -LiteralPath $receiptPath) {
        Stop-Benchmark "RECEIPT_EXISTS" "Preparation receipt already exists; preparation never overwrites it."
    }
    $ddragonCache = Join-Path $appDataRoot "ddragon"
    if ($specification.ddragon.PSObject.Properties["cache_root"]) {
        $ddragonCache = Get-FullAbsolutePath -Value ([string]$specification.ddragon.cache_root) -Label "ddragon.cache_root"
        Assert-StrictDescendant -Root $sentinelRoot -Path $ddragonCache -Label "ddragon.cache_root"
        [void](Assert-ReparseFree -Path $ddragonCache -Label "ddragon.cache_root")
    }
    if (-not (Test-PathEqual $ddragonCache (Join-Path $appDataRoot "ddragon"))) {
        Stop-Benchmark "DDRAGON" "ddragon.cache_root must equal app_data_root\ddragon."
    }

    $mediaTools = Resolve-MediaTools
    if ($PreflightOnly) {
        $preflightDdragonFingerprint = Get-DirectoryFingerprint -Root $ddragonCache -Label "ddragon-cache"
        if (
            $specification.ddragon.PSObject.Properties["cache_fingerprint"] -and
            $null -ne $specification.ddragon.cache_fingerprint -and
            [string]$specification.ddragon.cache_fingerprint -ne $preflightDdragonFingerprint
        ) {
            Stop-Benchmark "DDRAGON" "Declared Data Dragon cache fingerprint does not match the prepared cache."
        }
        [void](Ensure-BenchmarkConfig -Path $configPath -LibraryRoot $libraryRoot -ValidateOnly)
        $preflightReceiptFiles = New-Object 'System.Collections.Generic.List[object]'
        $preflightFixtures = New-Object 'System.Collections.Generic.List[object]'
        foreach ($fixture in @($specification.fixtures)) {
            $preflightFixtures.Add((Copy-ExplicitFixture `
                -Fixture $fixture `
                -LibraryRoot $libraryRoot `
                -MediaTools $mediaTools `
                -ReceiptFiles $preflightReceiptFiles))
        }
        Write-Host "QB-REPLAY-PREPARE-PREFLIGHT-OK: schema v1, sentinel/path safety, $($preflightReceiptFiles.Count) explicit source file hash(es), destinations, and optional media tools passed."
        exit 0
    }

    foreach ($directory in @(
        $libraryRoot,
        (Split-Path -Parent $configPath),
        $appDataRoot,
        $scratchRoot,
        (Split-Path -Parent $resultRoot),
        (Split-Path -Parent $manifestPath),
        $ddragonCache
    )) {
        if ([string]::IsNullOrWhiteSpace($directory)) { continue }
        if (-not (Test-Path -LiteralPath $directory -PathType Container)) {
            New-Item -ItemType Directory -Path $directory -Force | Out-Null
        }
        [void](Assert-ReparseFree -Path $directory -Label "prepared directory")
    }

    $configIdentity = Ensure-BenchmarkConfig -Path $configPath -LibraryRoot $libraryRoot
    $ddragonFingerprint = Get-DirectoryFingerprint -Root $ddragonCache -Label "ddragon-cache"
    if (
        $specification.ddragon.PSObject.Properties["cache_fingerprint"] -and
        $null -ne $specification.ddragon.cache_fingerprint -and
        [string]$specification.ddragon.cache_fingerprint -ne $ddragonFingerprint
    ) {
        Stop-Benchmark "DDRAGON" "Declared Data Dragon cache fingerprint does not match the prepared cache."
    }

    $receiptFiles = New-Object 'System.Collections.Generic.List[object]'
    $preparedFixtures = New-Object 'System.Collections.Generic.List[object]'
    foreach ($fixture in @($specification.fixtures)) {
        $preparedFixtures.Add((Copy-ExplicitFixture `
            -Fixture $fixture `
            -LibraryRoot $libraryRoot `
            -MediaTools $mediaTools `
            -ReceiptFiles $receiptFiles))
    }

    $runtimeManifest = [ordered]@{
        schema_version = 1
        run_id = [string]$specification.run_id
        sentinel_root = $sentinelRoot
        library_root = $libraryRoot
        config_path = $configPath
        app_data_root = $appDataRoot
        result_root = $resultRoot
        scratch_root = $scratchRoot
        observer_profile = [string]$specification.observer_profile
        ddragon = [ordered]@{
            mode = "offline"
            cache_root = $ddragonCache
            cache_fingerprint = $ddragonFingerprint
        }
        fixtures = $preparedFixtures.ToArray()
        scenarios = @($specification.scenarios)
    }
    foreach ($optional in @("app_binary", "analyzer_path", "python_path", "timeout_seconds")) {
        if ($specification.PSObject.Properties[$optional]) {
            $runtimeManifest[$optional] = $specification.$optional
        }
    }
    $runtimeManifest["prepared_utc"] = [DateTime]::UtcNow.ToString("o")
    $runtimeManifest["preparation_receipt"] = $receiptPath
    if ($null -ne $mediaTools) { $runtimeManifest["media_tools"] = $mediaTools }

    $receipt = [ordered]@{
        schema_version = 1
        run_id = [string]$specification.run_id
        prepared_utc = [DateTime]::UtcNow.ToString("o")
        specification_path = $parsed.path
        specification_sha256 = Get-Sha256 $parsed.path
        manifest_path = $manifestPath
        sentinel_root = $sentinelRoot
        copy_method = "System.IO.File.Copy; no hard links, symbolic links, moves, or source mutation"
        automatic_cleanup = $false
        benchmark_config = [ordered]@{
            path = $configPath
            created = [bool]$configIdentity.created
            sha256 = [string]$configIdentity.sha256
            output_path = $libraryRoot
            auto_delete_days = 0
        }
        ddragon_cache = [ordered]@{
            path = $ddragonCache
            fingerprint = $ddragonFingerprint
        }
        files = $receiptFiles.ToArray()
        media_tools = $mediaTools
    }
    Write-JsonFile -Path $receiptPath -Value $receipt
    Write-JsonFile -Path $manifestPath -Value $runtimeManifest
    Write-Host "QB-REPLAY-PREPARE-OK: prepared $($preparedFixtures.Count) explicit fixture(s); source hashes matched; nothing was cleaned."
    Write-Host "MANIFEST=$manifestPath"
    Write-Host "RECEIPT=$receiptPath"
}
catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    [Console]::Error.WriteLine("Preparation preserves every copied or partial artifact for inspection; it performs no automatic cleanup.")
    exit 2
}
