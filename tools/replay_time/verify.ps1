[CmdletBinding()]
param(
    [string]$RuntimeRoot = "build/media-runtime/windows-x86_64",
    [string]$FixtureRoot = "build/replay-time/qb-replay-012",
    [string]$AppBinary = "build/release/queueback/league-replay-app.exe",
    [switch]$PreflightOnly
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$script:SchemaVersion = 2
$script:MaximumResultBytes = 256KB
$script:MaximumCommandOutputBytes = 16MB
$script:Repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..\..")).Path
$script:RunRoot = $null
$script:ResultPath = $null
$script:Commands = [System.Collections.ArrayList]::new()
$script:Gates = [System.Collections.ArrayList]::new()
$script:Scenarios = [System.Collections.ArrayList]::new()
$script:Dependencies = [System.Collections.ArrayList]::new()
$script:StartedUtc = [DateTime]::UtcNow
$script:Utf8NoBom = [System.Text.UTF8Encoding]::new($false, $true)

function Resolve-RepositoryPath {
    param([Parameter(Mandatory = $true)][string]$Value, [Parameter(Mandatory = $true)][string]$Label)
    if ([string]::IsNullOrWhiteSpace($Value) -or $Value.IndexOf([char]0) -ge 0) {
        throw "$Label must be a nonempty path without NUL characters."
    }
    $candidate = if ([System.IO.Path]::IsPathRooted($Value)) {
        $Value
    } else {
        Join-Path $script:Repository $Value
    }
    return [System.IO.Path]::GetFullPath($candidate)
}

function Test-StrictDescendant {
    param([string]$Path, [string]$Root)
    $prefix = $Root.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    return $Path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

function Assert-ReparseFreeExistingChain {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Label)
    $cursor = [System.IO.Path]::GetFullPath($Path)
    while (-not (Test-Path -LiteralPath $cursor)) {
        $parent = [System.IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { throw "$Label has no existing ancestor." }
        $cursor = $parent.FullName
    }
    while ($true) {
        $item = Get-Item -LiteralPath $cursor -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "$Label traverses a reparse point at $cursor."
        }
        $parent = [System.IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName
    }
}

function Assert-TreeReparseFree {
    param([Parameter(Mandatory = $true)][string]$Root, [Parameter(Mandatory = $true)][string]$Label)
    Assert-ReparseFreeExistingChain -Path $Root -Label $Label
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) { throw "$Label is not a directory: $Root" }
    foreach ($item in Get-ChildItem -LiteralPath $Root -Force -Recurse) {
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "$Label contains a reparse point at $($item.FullName)."
        }
    }
}

function Write-NewBytes {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][AllowEmptyCollection()][byte[]]$Bytes)
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
    try { $stream.Write($Bytes, 0, $Bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
}

function Write-NewText {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Text)
    Write-NewBytes -Path $Path -Bytes $script:Utf8NoBom.GetBytes($Text)
}

function Write-NewJson {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][object]$Value,
        [int]$MaximumBytes = 1MB
    )
    $json = ConvertTo-Json -InputObject $Value -Depth 30 -Compress
    $bytes = $script:Utf8NoBom.GetBytes($json + "`n")
    if ($bytes.Length -gt $MaximumBytes) { throw "JSON output exceeds its $MaximumBytes-byte bound: $Path" }
    Write-NewBytes -Path $Path -Bytes $bytes
}

function Assert-NoDuplicateJsonNode {
    param([Parameter(Mandatory = $true)][System.Xml.XmlNode]$Node, [Parameter(Mandatory = $true)][string]$Label)
    $typeAttribute = $Node.Attributes['type']
    if ($null -ne $typeAttribute -and $typeAttribute.Value -ceq 'object') {
        $names = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
        foreach ($child in @($Node.ChildNodes | Where-Object { $_.NodeType -eq [System.Xml.XmlNodeType]::Element })) {
            $itemAttribute = $child.Attributes['item']
            $name = if ($child.LocalName -ceq 'item' -and $null -ne $itemAttribute) {
                $itemAttribute.Value
            } else {
                $child.LocalName
            }
            if (-not $names.Add($name)) { throw "$Label contains duplicate JSON property $name." }
            Assert-NoDuplicateJsonNode -Node $child -Label $Label
        }
    } else {
        foreach ($child in @($Node.ChildNodes | Where-Object { $_.NodeType -eq [System.Xml.XmlNodeType]::Element })) {
            Assert-NoDuplicateJsonNode -Node $child -Label $Label
        }
    }
}

function ConvertFrom-StrictJsonBytes {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][byte[]]$Bytes,
        [Parameter(Mandatory = $true)][string]$Label
    )
    if ($Bytes.Length -eq 0) { throw "$Label is empty." }
    if ($Bytes.Length -ge 3 -and $Bytes[0] -eq 0xef -and $Bytes[1] -eq 0xbb -and $Bytes[2] -eq 0xbf) {
        throw "$Label must be BOM-less UTF-8 JSON."
    }
    try {
        $text = $script:Utf8NoBom.GetString($Bytes)
    } catch [System.Text.DecoderFallbackException] {
        throw "$Label is not strict UTF-8."
    }
    try {
        $quotas = [System.Xml.XmlDictionaryReaderQuotas]::Max
        $reader = [System.Runtime.Serialization.Json.JsonReaderWriterFactory]::CreateJsonReader($Bytes, $quotas)
        try {
            $xml = [System.Xml.XmlDocument]::new()
            $xml.Load($reader)
        } finally {
            $reader.Dispose()
        }
        Assert-NoDuplicateJsonNode -Node $xml.DocumentElement -Label $Label
        return ConvertFrom-Json -InputObject $text
    } catch {
        throw "$Label is not strict JSON: $($_.Exception.Message)"
    }
}

function Read-StrictUtf8Text {
    param([Parameter(Mandatory = $true)][string]$Path, [int]$MaximumBytes = 1MB)
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or $item.Length -le 0 -or $item.Length -gt $MaximumBytes) {
        throw "UTF-8 input is not a bounded nonempty file: $Path"
    }
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xef -and $bytes[1] -eq 0xbb -and $bytes[2] -eq 0xbf) {
        throw "UTF-8 input must be BOM-less: $Path"
    }
    try {
        return $script:Utf8NoBom.GetString($bytes)
    } catch [System.Text.DecoderFallbackException] {
        throw "UTF-8 input contains an invalid byte sequence: $Path"
    }
}

function ConvertFrom-BomlessUtf8Bytes {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][byte[]]$Bytes,
        [Parameter(Mandatory = $true)][string]$Label
    )
    if ($Bytes.Length -ge 3 -and $Bytes[0] -eq 0xef -and $Bytes[1] -eq 0xbb -and $Bytes[2] -eq 0xbf) {
        throw "$Label must be BOM-less UTF-8."
    }
    try {
        return $script:Utf8NoBom.GetString($Bytes)
    } catch [System.Text.DecoderFallbackException] {
        throw "$Label contains an invalid UTF-8 byte sequence."
    }
}

function Read-BoundedJson {
    param([Parameter(Mandatory = $true)][string]$Path, [int]$MaximumBytes = 1MB)
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or $item.Length -le 0 -or $item.Length -gt $MaximumBytes) {
        throw "JSON input is not a bounded nonempty file: $Path"
    }
    return ConvertFrom-StrictJsonBytes -Bytes ([System.IO.File]::ReadAllBytes($Path)) -Label $Path
}

function Assert-ExactJsonFields {
    param(
        [Parameter(Mandatory = $true)][object]$Value,
        [Parameter(Mandatory = $true)][string[]]$Expected,
        [Parameter(Mandatory = $true)][string]$Label
    )
    if ($Value.GetType().FullName -cne 'System.Management.Automation.PSCustomObject') {
        throw "$Label must be a JSON object."
    }
    $actual = @($Value.PSObject.Properties | ForEach-Object { $_.Name })
    $actualSet = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($name in $actual) { [void]$actualSet.Add($name) }
    $missing = @($Expected | Where-Object { -not $actualSet.Contains($_) })
    $expectedSet = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($name in $Expected) { [void]$expectedSet.Add($name) }
    $unexpected = @($actual | Where-Object { -not $expectedSet.Contains($_) })
    if ($missing.Count -ne 0 -or $unexpected.Count -ne 0 -or $actual.Count -ne $Expected.Count) {
        throw "$Label fields are not exact (missing=$($missing -join ','), unexpected=$($unexpected -join ','))."
    }
}

function Test-JsonInteger {
    param([object]$Value)
    return (
        $Value -is [sbyte] -or $Value -is [byte] -or
        $Value -is [int16] -or $Value -is [uint16] -or
        $Value -is [int32] -or $Value -is [uint32] -or
        $Value -is [int64] -or $Value -is [uint64]
    )
}

function Assert-JsonIntegerEquals {
    param([object]$Actual, [int64]$Expected, [string]$Label)
    if (-not (Test-JsonInteger $Actual) -or [int64]$Actual -ne $Expected) {
        throw "$Label must be integer $Expected."
    }
}

function ConvertTo-PowerShellSingleQuotedLiteral {
    param([Parameter(Mandatory = $true)][string]$Value)
    if ($Value.IndexOf([char]0) -ge 0) { throw 'PowerShell child argument contains NUL.' }
    return "'" + $Value.Replace("'", "''") + "'"
}

function New-Utf8PowerShellArguments {
    param([Parameter(Mandatory = $true)][string]$ScriptPath, [string[]]$Arguments = @())
    $tokens = [System.Collections.ArrayList]::new()
    [void]$tokens.Add((ConvertTo-PowerShellSingleQuotedLiteral $ScriptPath))
    foreach ($argument in $Arguments) {
        $token = if ($argument -match '^-[A-Za-z][A-Za-z0-9]*$') {
            $argument
        } else {
            ConvertTo-PowerShellSingleQuotedLiteral $argument
        }
        [void]$tokens.Add($token)
    }
    $command = @(
        '& {'
        '[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false, $true);'
        'try {'
        ('& ' + (@($tokens) -join ' ') + ';')
        'if (-not $?) { exit 1 }'
        '} catch {'
        '[Console]::Error.WriteLine($_.Exception.ToString());'
        'exit 1'
        '}'
        '}'
    ) -join ' '
    return @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $command)
}

function Get-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-RelativeEvidencePath {
    param([Parameter(Mandatory = $true)][string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    if (-not (Test-StrictDescendant -Path $full -Root $script:RunRoot)) {
        throw "Evidence path is outside the immutable run root: $full"
    }
    $prefix = $script:RunRoot.TrimEnd('\') + '\'
    return $full.Substring($prefix.Length).Replace('\', '/')
}

function Add-Gate {
    param([string]$Name, [bool]$Passed, [string]$Detail, [bool]$Required = $true)
    [void]$script:Gates.Add([ordered]@{
        name = $Name
        required = $Required
        passed = $Passed
        detail = $Detail
    })
}

function Add-Dependency {
    param([string]$Name, [string]$Status, [string]$Detail)
    [void]$script:Dependencies.Add([ordered]@{ name = $Name; status = $Status; detail = $Detail })
}

function Get-ExecutablePath {
    param([Parameter(Mandatory = $true)][string]$Name)
    $command = Get-Command $Name -CommandType Application -ErrorAction Stop | Select-Object -First 1
    return [System.IO.Path]::GetFullPath($command.Source)
}

function Invoke-Bounded {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [ValidateRange(1, 7200)][int]$TimeoutSeconds,
        [ValidateRange(1024, 67108864)][int]$OutputLimitBytes = $script:MaximumCommandOutputBytes,
        [string[]]$PathDirectories = @(),
        [hashtable]$Environment = @{},
        [switch]$AllowFailure
    )
    if (-not [System.IO.Path]::IsPathRooted($Executable) -or -not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Bounded command executable must be an existing absolute file: $Executable"
    }
    Assert-ReparseFreeExistingChain -Path $Executable -Label "bounded command $Name executable"
    $safeName = $Name -replace '[^A-Za-z0-9._-]', '_'
    if ($safeName.Length -gt 40) { $safeName = $safeName.Substring(0, 40) }
    $logRoot = Join-Path $script:RunRoot "commands"
    if (-not (Test-Path -LiteralPath $logRoot)) { [System.IO.Directory]::CreateDirectory($logRoot) | Out-Null }
    $ordinal = $script:Commands.Count.ToString('D2', [Globalization.CultureInfo]::InvariantCulture)
    $stdoutPath = Join-Path $logRoot "$ordinal-$safeName.stdout.log"
    $stderrPath = Join-Path $logRoot "$ordinal-$safeName.stderr.log"
    $previousPath = $env:PATH
    $effectiveEnvironment = @{
        LEAGUE_REPLAY_CONFIG = $null
        LEAGUE_REPLAY_OUTPUT_PATH = $null
        QUEUEBACK_MEDIA_RUNTIME_DIR = $null
    }
    foreach ($key in $Environment.Keys) { $effectiveEnvironment[$key] = $Environment[$key] }
    $previousEnvironment = @{}
    foreach ($key in $effectiveEnvironment.Keys) {
        $previousEnvironment[$key] = [System.Environment]::GetEnvironmentVariable([string]$key, 'Process')
    }
    $systemDirectories = @(
        (Join-Path $env:SystemRoot "System32"),
        $env:SystemRoot,
        (Split-Path -Parent $Executable)
    )
    $allDirectories = @(($PathDirectories + $systemDirectories) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
    $runError = $null
    try {
        $env:PATH = $allDirectories -join [System.IO.Path]::PathSeparator
        foreach ($key in $effectiveEnvironment.Keys) {
            $newValue = if ($null -eq $effectiveEnvironment[$key]) { $null } else { [string]$effectiveEnvironment[$key] }
            [System.Environment]::SetEnvironmentVariable([string]$key, $newValue, 'Process')
        }
        $childEnvironment = [System.Collections.Generic.Dictionary[string,string]]::new(
            [StringComparer]::OrdinalIgnoreCase
        )
        foreach ($entry in [System.Environment]::GetEnvironmentVariables('Process').GetEnumerator()) {
            $childEnvironment[[string]$entry.Key] = [string]$entry.Value
        }
        $result = [Chronobreak.ReplayTime.BoundedProcess]::Run(
            $Executable,
            $Arguments,
            $script:Repository,
            $TimeoutSeconds * 1000,
            $OutputLimitBytes,
            $childEnvironment
        )
    } catch {
        $runError = $_.Exception
    } finally {
        $env:PATH = $previousPath
        foreach ($key in $effectiveEnvironment.Keys) {
            [System.Environment]::SetEnvironmentVariable([string]$key, $previousEnvironment[$key], 'Process')
        }
    }
    if ($null -ne $runError) {
        $errorText = $runError.Message
        if ($errorText.Length -gt 2000) { $errorText = $errorText.Substring(0, 2000) }
        Write-NewBytes -Path $stdoutPath -Bytes ([byte[]]@())
        Write-NewText -Path $stderrPath -Text ($errorText + "`n")
        $errorRecord = [ordered]@{
            name = $Name
            executable = $Executable
            arguments = @($Arguments)
            timeout_seconds = $TimeoutSeconds
            output_limit_bytes = $OutputLimitBytes
            exit_code = -1
            timed_out = $false
            output_overflow = $false
            duration_ms = 0
            stdout = Get-RelativeEvidencePath $stdoutPath
            stderr = Get-RelativeEvidencePath $stderrPath
            runner_error = $true
            passed = $false
        }
        [void]$script:Commands.Add($errorRecord)
        if (-not $AllowFailure) { throw "Bounded command $Name could not complete: $errorText" }
        return [pscustomobject]@{ Record=$errorRecord; StandardOutput=''; StandardError=$errorText }
    }
    Write-NewBytes -Path $stdoutPath -Bytes $result.StandardOutput
    Write-NewBytes -Path $stderrPath -Bytes $result.StandardError
    $record = [ordered]@{
        name = $Name
        executable = $Executable
        arguments = @($Arguments)
        timeout_seconds = $TimeoutSeconds
        output_limit_bytes = $OutputLimitBytes
        exit_code = $result.ExitCode
        timed_out = $result.TimedOut
        output_overflow = $result.OutputOverflow
        duration_ms = $result.DurationMilliseconds
        stdout = Get-RelativeEvidencePath $stdoutPath
        stderr = Get-RelativeEvidencePath $stderrPath
        passed = ($result.ExitCode -eq 0 -and -not $result.TimedOut -and -not $result.OutputOverflow)
    }
    [void]$script:Commands.Add($record)
    if (-not $record.passed -and -not $AllowFailure) {
        throw "Bounded command $Name failed (exit=$($record.exit_code), timeout=$($record.timed_out), overflow=$($record.output_overflow)); evidence: $($record.stderr)."
    }
    if ($Executable -ieq $script:PowerShell) {
        $standardOutput = ConvertFrom-BomlessUtf8Bytes -Bytes $result.StandardOutput -Label "bounded PowerShell command $Name stdout"
        $standardError = ConvertFrom-BomlessUtf8Bytes -Bytes $result.StandardError -Label "bounded PowerShell command $Name stderr"
    } else {
        $standardOutput = $script:Utf8NoBom.GetString($result.StandardOutput)
        $standardError = $script:Utf8NoBom.GetString($result.StandardError)
    }
    return [pscustomobject]@{
        Record = $record
        StandardOutput = $standardOutput
        StandardError = $standardError
    }
}

function Assert-HelperResult {
    param([string]$Path, [string]$Command)
    $result = Read-BoundedJson -Path $Path
    if ($result.schema_version -ne 2 -or $result.command -cne $Command -or $result.passed -ne $true) {
        throw "Fixture helper result did not prove schema-v2 $Command success: $Path"
    }
    return $result
}

function Invoke-FixtureHelper {
    param([string]$Name, [string[]]$Arguments, [string]$ResultPath, [string]$ExpectedCommand, [int]$TimeoutSeconds = 300)
    $invocation = Invoke-Bounded -Name $Name -Executable $script:Python -Arguments (@($script:FixtureTools) + $Arguments) -TimeoutSeconds $TimeoutSeconds
    return Assert-HelperResult -Path $ResultPath -Command $ExpectedCommand
}

function Copy-NewReadOnly {
    param([string]$Source, [string]$Destination, [int64]$MaximumBytes = 8GB)
    Assert-ReparseFreeExistingChain -Path $Source -Label "copy source"
    $sourceItem = Get-Item -LiteralPath $Source -Force
    if ($sourceItem.PSIsContainer -or $sourceItem.Length -le 0 -or $sourceItem.Length -gt $MaximumBytes) {
        throw "Copy source is not a bounded nonempty file: $Source"
    }
    $sourceStream = [System.IO.File]::Open($Source, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::Read)
    $destinationStream = [System.IO.File]::Open($Destination, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
    try {
        $sourceStream.CopyTo($destinationStream, 1MB)
        $destinationStream.Flush($true)
    } finally {
        $destinationStream.Dispose()
        $sourceStream.Dispose()
    }
    if ((Get-Item -LiteralPath $Destination).Length -ne $sourceItem.Length) { throw "Read-only evidence copy size mismatch: $Destination" }
    (Get-Item -LiteralPath $Destination).IsReadOnly = $true
}

function Invoke-ProbeAndGrid {
    param([string]$Name, [string]$Media, [ValidateSet('zero','nonzero')][string]$Origin, [string]$Directory)
    $probePath = Join-Path $Directory "$Name.probe.json"
    $gridPath = Join-Path $Directory "$Name.grid.json"
    $probe = Invoke-Bounded -Name "ffprobe-$Name" -Executable $script:Ffprobe -Arguments @(
        '-v','error','-select_streams','v:0','-count_frames','-count_packets',
        '-show_streams','-show_frames','-show_packets','-of','json',$Media
    ) -TimeoutSeconds 300 -OutputLimitBytes 64MB
    Write-NewText -Path $probePath -Text $probe.StandardOutput
    $grid = Invoke-FixtureHelper -Name "grid-$Name" -Arguments @(
        'verify-grid','--probe',$probePath,'--expected-rate','60/1','--origin',$Origin,'--result',$gridPath
    ) -ResultPath $gridPath -ExpectedCommand 'verify-grid'
    return [ordered]@{
        media = Get-RelativeEvidencePath $Media
        media_sha256 = Get-Sha256 $Media
        probe = Get-RelativeEvidencePath $probePath
        grid = Get-RelativeEvidencePath $gridPath
        first_pts = $grid.first_pts
        frame_count = $grid.frame_count
        origin = $grid.origin
    }
}

function Invoke-ExactCargoTest {
    param([string]$Name, [string]$ManifestPath, [string]$TestName, [string]$CargoTarget)
    $invocation = Invoke-Bounded -Name $Name -Executable $script:Cargo -Arguments @(
        'test','--manifest-path',$ManifestPath,'--lib',$TestName,'--','--exact'
    ) -TimeoutSeconds 1800 -PathDirectories @($script:ToolchainPathDirectories + (Split-Path -Parent $script:Ffmpeg)) -Environment @{ CARGO_TARGET_DIR = $CargoTarget }
    $testOutput = $invocation.StandardOutput + "`n" + $invocation.StandardError
    $testLine = '(?m)^test ' + [Regex]::Escape($TestName) + ' \.\.\. ok\r?$'
    if ($testOutput -notmatch $testLine -or $testOutput -notmatch '(?m)^test result: ok\. 1 passed; 0 failed;') {
        throw "Cargo command $Name exited successfully without executing exactly the required test $TestName."
    }
    return $invocation.Record
}
function Invoke-DecodeAndVerifyMedia {
    param([string]$Name, [string]$Media, [string]$Manifest, [int]$ExpectedStart, [int]$ExpectedCount, [string]$Directory)
    $raw = Join-Path $Directory "$Name.yuv"
    $wav = Join-Path $Directory "$Name.wav"
    $resultPath = Join-Path $Directory "$Name.media.json"
    $markerContract = Read-BoundedJson -Path $Manifest -MaximumBytes 1MB
    if ([int]$markerContract.width -ne 320 -or [int]$markerContract.height -ne 240) {
        throw 'Marker media verification requires the controlled 320x240 marker contract.'
    }
    $markerVideoFilter = 'crop=1280:960:320:60,scale=320:240:flags=neighbor'
    Invoke-Bounded -Name "decode-video-$Name" -Executable $script:Ffmpeg -Arguments @(
        '-hide_banner','-nostdin','-v','error','-i',$Media,'-map','0:v:0','-vf',$markerVideoFilter,'-fps_mode','passthrough','-pix_fmt','yuv420p','-f','rawvideo',$raw
    ) -TimeoutSeconds 600 | Out-Null
    Invoke-Bounded -Name "decode-audio-$Name" -Executable $script:Ffmpeg -Arguments @(
        '-hide_banner','-nostdin','-v','error','-i',$Media,'-map','0:a:0','-ac','1','-ar','48000','-c:a','pcm_s16le',$wav
    ) -TimeoutSeconds 600 | Out-Null
    $result = Invoke-FixtureHelper -Name "verify-media-$Name" -Arguments @(
        'verify-media','--video',$raw,'--audio',$wav,'--manifest',$Manifest,
        '--expected-start-frame',$ExpectedStart.ToString([Globalization.CultureInfo]::InvariantCulture),
        '--expected-frame-count',$ExpectedCount.ToString([Globalization.CultureInfo]::InvariantCulture),
        '--result',$resultPath
    ) -ResultPath $resultPath -ExpectedCommand 'verify-media' -TimeoutSeconds 600
    return [ordered]@{
        decoded_video = Get-RelativeEvidencePath $raw
        decoded_audio = Get-RelativeEvidencePath $wav
        result = Get-RelativeEvidencePath $resultPath
        expected_start_frame = $ExpectedStart
        expected_frame_count = $ExpectedCount
        first_source_frame = $result.first_decoded_source_frame
        last_source_frame = $result.last_decoded_source_frame
        marker_count = $result.markers_in_interval
        maximum_av_disagreement_ms = $result.maximum_av_disagreement_ms
        maximum_drift_growth_ms = $result.maximum_drift_growth_ms
    }
}

function Get-RecorderEvidenceRoot {
    param([string]$Text, [string]$AllowedRoot)
    $lines = @($Text -split "`r?`n" | Where-Object { $_.Length -gt 0 })
    $matches = @($lines | Where-Object { $_ -cmatch '^EVIDENCE=(?<path>[A-Za-z]:\\.+)$' })
    if ($matches.Count -ne 1) { throw "Native runner must emit exactly one absolute EVIDENCE= line." }
    [void]($matches[0] -cmatch '^EVIDENCE=(?<path>[A-Za-z]:\\.+)$')
    $full = [System.IO.Path]::GetFullPath($Matches.path)
    if (-not (Test-StrictDescendant -Path $full -Root $AllowedRoot)) {
        throw "Native evidence escaped its dedicated output root."
    }
    Assert-TreeReparseFree -Root $full -Label "native recorder evidence"
    return $full
}

function Assert-OrdinalStringEquals {
    param([object]$Actual, [Parameter(Mandatory = $true)][string]$Expected, [string]$Label)
    if ($Actual -isnot [string] -or [string]$Actual -cne $Expected) {
        throw "$Label does not match the invoked contract."
    }
}

function Assert-Sha256Equals {
    param([object]$Actual, [Parameter(Mandatory = $true)][string]$Expected, [string]$Label)
    if ($Actual -isnot [string] -or [string]$Actual -cnotmatch '^[0-9a-f]{64}$' -or [string]$Actual -cne $Expected) {
        throw "$Label does not match the published bytes."
    }
}

function Assert-NormalizedPathEquals {
    param([object]$Actual, [Parameter(Mandatory = $true)][string]$Expected, [string]$Label)
    if ($Actual -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$Actual)) {
        throw "$Label is not a nonempty path."
    }
    $full = [System.IO.Path]::GetFullPath([string]$Actual)
    if (-not [string]::Equals($full, [System.IO.Path]::GetFullPath($Expected), [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label does not match the actual path."
    }
}

function Get-OnlyPrefixedLine {
    param([string]$Text, [string]$Prefix, [string]$Label)
    $matches = @($Text -split "`r?`n" | Where-Object { ([string]$_).StartsWith($Prefix, [StringComparison]::Ordinal) })
    if ($matches.Count -ne 1) { throw "$Label must contain exactly one $Prefix line." }
    return [string]$matches[0]
}

function Test-JsonValuesEquivalent {
    param(
        [Parameter(Mandatory = $false)][AllowNull()][object]$Actual,
        [Parameter(Mandatory = $false)][AllowNull()][object]$Expected
    )
    if ($null -eq $Actual -or $null -eq $Expected) {
        return $null -eq $Actual -and $null -eq $Expected
    }

    $actualIsObject = $Actual -is [System.Collections.IDictionary] -or
        $Actual.GetType().FullName -ceq 'System.Management.Automation.PSCustomObject'
    $expectedIsObject = $Expected -is [System.Collections.IDictionary] -or
        $Expected.GetType().FullName -ceq 'System.Management.Automation.PSCustomObject'
    if ($actualIsObject -or $expectedIsObject) {
        if (-not ($actualIsObject -and $expectedIsObject)) { return $false }
        $actualNames = @(
            if ($Actual -is [System.Collections.IDictionary]) {
                $Actual.Keys | ForEach-Object { [string]$_ }
            } else {
                $Actual.PSObject.Properties | ForEach-Object { $_.Name }
            }
        )
        $expectedNames = @(
            if ($Expected -is [System.Collections.IDictionary]) {
                $Expected.Keys | ForEach-Object { [string]$_ }
            } else {
                $Expected.PSObject.Properties | ForEach-Object { $_.Name }
            }
        )
        if ($actualNames.Count -ne $expectedNames.Count) { return $false }
        foreach ($name in $actualNames) {
            if (-not ($expectedNames -ccontains $name)) { return $false }
            $actualValue = if ($Actual -is [System.Collections.IDictionary]) {
                $Actual[$name]
            } else {
                $Actual.PSObject.Properties[$name].Value
            }
            $expectedValue = if ($Expected -is [System.Collections.IDictionary]) {
                $Expected[$name]
            } else {
                $Expected.PSObject.Properties[$name].Value
            }
            if (-not (Test-JsonValuesEquivalent -Actual $actualValue -Expected $expectedValue)) {
                return $false
            }
        }
        return $true
    }

    $actualIsArray = $Actual -is [System.Collections.IEnumerable] -and
        $Actual -isnot [string]
    $expectedIsArray = $Expected -is [System.Collections.IEnumerable] -and
        $Expected -isnot [string]
    if ($actualIsArray -or $expectedIsArray) {
        if (-not ($actualIsArray -and $expectedIsArray)) { return $false }
        $actualItems = @($Actual)
        $expectedItems = @($Expected)
        if ($actualItems.Count -ne $expectedItems.Count) { return $false }
        for ($index = 0; $index -lt $actualItems.Count; $index++) {
            if (-not (Test-JsonValuesEquivalent -Actual $actualItems[$index] -Expected $expectedItems[$index])) {
                return $false
            }
        }
        return $true
    }

    return (ConvertTo-Json -InputObject $Actual -Compress) -ceq
        (ConvertTo-Json -InputObject $Expected -Compress)
}

function Assert-JsonObjectsEquivalent {
    param([object]$Actual, [object]$Expected, [string]$Label)
    if (-not (Test-JsonValuesEquivalent -Actual $Actual -Expected $Expected)) {
        throw "$Label does not match the terminal evidence."
    }
}

function Assert-MediaProbeFacts {
    param([object]$Probe, [string]$Label)
    if ($Probe.GetType().FullName -cne 'System.Management.Automation.PSCustomObject') { throw "$Label must be an object." }
    $video = @($Probe.streams | Where-Object { $_.codec_type -ceq 'video' })
    $audio = @($Probe.streams | Where-Object { $_.codec_type -ceq 'audio' })
    if (@($Probe.streams).Count -ne 2 -or $video.Count -ne 1 -or $audio.Count -ne 1) {
        throw "$Label must describe exactly one video and one audio stream."
    }
    if (
        [string]$video[0].codec_name -cne 'h264' -or [string]$audio[0].codec_name -cne 'aac' -or
        -not (Test-JsonInteger $video[0].width) -or [int64]$video[0].width -ne 1920 -or
        -not (Test-JsonInteger $video[0].height) -or [int64]$video[0].height -ne 1080 -or
        [string]$video[0].r_frame_rate -cne '60/1'
    ) {
        throw "$Label contradicts the required recorder media facts."
    }
}

function Assert-RecorderReport {
    param(
        [string]$ReportPath,
        [string]$SourceRoot,
        [string]$SourceVideo,
        [string]$CopiedVideo,
        [string]$RuntimeId,
        [string]$InvocationOutput
    )
    $report = Read-BoundedJson -Path $ReportPath -MaximumBytes 16MB
    $actualHash = Get-Sha256 $CopiedVideo
    $actualBytes = (Get-Item -LiteralPath $CopiedVideo -Force).Length
    $fixtureLog = Join-Path $SourceRoot 'fixture.stdout.log'
    $captureLog = Join-Path $SourceRoot 'capture.stdout.log'
    foreach ($log in @($fixtureLog, $captureLog)) {
        if (-not (Test-Path -LiteralPath $log -PathType Leaf)) {
            throw "Native recorder terminal log is missing: $log"
        }
        Assert-ReparseFreeExistingChain -Path $log -Label "native recorder terminal log"
    }
    $fixtureText = Read-StrictUtf8Text -Path $fixtureLog -MaximumBytes 16MB
    $captureText = Read-StrictUtf8Text -Path $captureLog -MaximumBytes 16MB
    $targetLine = Get-OnlyPrefixedLine -Text $fixtureText -Prefix 'QUEUEBACK_WGC_TARGET ' -Label "native recorder target evidence"

    Assert-ExactJsonFields -Value $report -Expected @(
        'schema','scope','backend','scenario','interruption','expected_failure','observed_exit_code',
        'duration_seconds_requested','runtime_id','target','action_lines','decoded_frames',
        'unique_frame_hashes','video_bytes','video_sha256','ffprobe','native_telemetry',
        'replay_time_markers','expected_failure_detail','mux_failure_trigger','resource_summary','full_decode','result'
    ) -Label 'native recorder report'
    Assert-JsonIntegerEquals -Actual $report.schema -Expected 1 -Label 'native report schema'
    Assert-OrdinalStringEquals $report.scope 'generated non-League native-backend fixture' 'native report scope'
    Assert-OrdinalStringEquals $report.backend 'native_windows_graphics_capture_d3d11_nvenc' 'native report backend'
    Assert-OrdinalStringEquals $report.scenario 'steady' 'native report scenario'
    Assert-OrdinalStringEquals $report.interruption 'none' 'native report interruption'
    if ($report.expected_failure -isnot [bool] -or $report.expected_failure) {
        throw 'Native report expected_failure must be false.'
    }
    Assert-JsonIntegerEquals $report.observed_exit_code 0 'native report observed exit code'
    Assert-JsonIntegerEquals $report.duration_seconds_requested 6 'native report requested duration'
    Assert-OrdinalStringEquals $report.runtime_id $RuntimeId 'native report runtime identity'
    Assert-OrdinalStringEquals $report.target $targetLine 'native report target'
    if (@($report.action_lines).Count -ne 0) {
        throw 'Steady native report must not claim window action lines.'
    }
    Assert-JsonIntegerEquals $report.decoded_frames 360 'native report decoded frame count'
    if (-not (Test-JsonInteger $report.unique_frame_hashes) -or [int64]$report.unique_frame_hashes -lt 60) {
        throw 'Native report has insufficient changing-frame evidence.'
    }
    Assert-JsonIntegerEquals $report.video_bytes $actualBytes 'native report video bytes'
    Assert-Sha256Equals $report.video_sha256 $actualHash 'native report video SHA-256'
    if (
        $null -ne $report.expected_failure_detail -or
        $null -ne $report.mux_failure_trigger -or
        $null -ne $report.resource_summary
    ) {
        throw 'Native steady report contains unexpected failure, mux-trigger, or resource evidence.'
    }
    Assert-OrdinalStringEquals $report.full_decode 'pass' 'native report decode status'
    Assert-OrdinalStringEquals $report.result 'pass' 'native report status'
    Assert-MediaProbeFacts -Probe $report.ffprobe -Label 'native report media probe'
    $savedProbePath = Join-Path $SourceRoot 'ffprobe.json'
    if (-not (Test-Path -LiteralPath $savedProbePath -PathType Leaf)) {
        throw 'Native report source probe is missing.'
    }
    Assert-JsonObjectsEquivalent -Actual $report.ffprobe -Expected (Read-BoundedJson $savedProbePath 16MB) -Label 'native report media probe'

    $passLine = Get-OnlyPrefixedLine -Text $captureText -Prefix 'CHRONOBREAK_NATIVE_MP4_PASS ' -Label 'native terminal capture evidence'
    $remainder = $passLine.Substring('CHRONOBREAK_NATIVE_MP4_PASS '.Length).Trim()
    $matches = [Regex]::Matches($remainder, '(?<key>[a-z0-9_]+)=(?<value>[^\s]+)')
    if (($matches | ForEach-Object { $_.Value }) -join ' ' -cne $remainder) {
        throw 'Native terminal capture evidence contains malformed facts.'
    }
    $terminal = [System.Collections.Generic.Dictionary[string,string]]::new([StringComparer]::Ordinal)
    foreach ($match in $matches) {
        if ($terminal.ContainsKey($match.Groups['key'].Value)) {
            throw "Native terminal capture evidence repeats $($match.Groups['key'].Value)."
        }
        $terminal.Add($match.Groups['key'].Value, $match.Groups['value'].Value)
    }
    Assert-ExactJsonFields -Value $report.native_telemetry -Expected @($terminal.Keys) -Label 'native report terminal telemetry'
    foreach ($property in $report.native_telemetry.PSObject.Properties) {
        Assert-OrdinalStringEquals $property.Value $terminal[$property.Name] "native telemetry $($property.Name)"
    }
    foreach ($fact in @(
        @('ticks','360'), @('media_time_base','1/60'), @('submitted','360'),
        @('completed','360'), @('mux_frames','360'),
        @('output_bytes',$actualBytes.ToString([Globalization.CultureInfo]::InvariantCulture))
    )) {
        if (-not $terminal.ContainsKey($fact[0]) -or $terminal[$fact[0]] -cne $fact[1]) {
            throw "Native terminal fact $($fact[0]) does not match the invoked media."
        }
    }
    foreach ($name in @('first_source_qpc_100ns','latest_source_qpc_100ns','mux_progress_bytes')) {
        [uint64]$positive = 0
        if (-not $terminal.ContainsKey($name) -or -not [uint64]::TryParse($terminal[$name], [ref]$positive) -or $positive -eq 0) {
            throw "Native terminal fact $name is not a positive integer."
        }
    }
    Assert-NormalizedPathEquals $terminal['output'] $SourceVideo 'native terminal output'

    Assert-ExactJsonFields -Value $report.replay_time_markers -Expected @(
        'requested','source_contract','expected_generation','start_signal','verification'
    ) -Label 'native replay-time marker report'
    if (
        $report.replay_time_markers.requested -isnot [bool] -or
        -not $report.replay_time_markers.requested
    ) {
        throw 'Native recorder report did not enable replay-time markers.'
    }
    $markerContract = Get-OnlyPrefixedLine -Text $fixtureText `
        -Prefix 'QUEUEBACK_WGC_MARKER_CONTRACT ' -Label 'native marker source contract'
    Assert-OrdinalStringEquals $report.replay_time_markers.source_contract `
        $markerContract 'native marker source contract'
    if ($markerContract -notmatch 'generation=(\d+) duration_frames=360 sample_rate=48000 ') {
        throw 'Native marker source contract has invalid generation, duration, or sample rate.'
    }
    $markerGeneration = [int]$Matches[1]
    Assert-JsonIntegerEquals $report.replay_time_markers.expected_generation `
        $markerGeneration 'native marker generation'
    [void](Get-OnlyPrefixedLine -Text $fixtureText `
        -Prefix "QUEUEBACK_WGC_MARKER_AUDIO_CONNECTED generation=$markerGeneration" `
        -Label 'native marker shared epoch commit')
    $markerStartSignal = Join-Path $SourceRoot 'marker-start.signal'
    Assert-NormalizedPathEquals $report.replay_time_markers.start_signal `
        $markerStartSignal 'native marker start signal path'
    if (
        -not (Test-Path -LiteralPath $markerStartSignal -PathType Leaf) -or
        (Read-StrictUtf8Text -Path $markerStartSignal -MaximumBytes 64) -cne "start`n"
    ) {
        throw 'Native marker start signal is missing or malformed.'
    }
    $readyLine = Get-OnlyPrefixedLine -Text $captureText `
        -Prefix 'CHRONOBREAK_NATIVE_MP4_READY_FOR_START_SIGNAL ' `
        -Label 'native marker capture readiness'
    if ($readyLine -notmatch '^CHRONOBREAK_NATIVE_MP4_READY_FOR_START_SIGNAL path=(.+)$') {
        throw 'Native marker capture readiness line is malformed.'
    }
    Assert-NormalizedPathEquals $Matches[1] $markerStartSignal `
        'native marker capture readiness path'
    $sourceSignalLine = Get-OnlyPrefixedLine -Text $fixtureText `
        -Prefix 'QUEUEBACK_WGC_MARKER_START_SIGNAL_ACCEPTED ' `
        -Label 'native marker source start signal'
    if ($sourceSignalLine -notmatch '^QUEUEBACK_WGC_MARKER_START_SIGNAL_ACCEPTED path=(.+)$') {
        throw 'Native marker source start-signal line is malformed.'
    }
    Assert-NormalizedPathEquals $Matches[1] $markerStartSignal `
        'native marker source start signal path'
    $publishedLine = Get-OnlyPrefixedLine -Text $captureText `
        -Prefix 'CHRONOBREAK_NATIVE_MP4_START_SIGNAL_PUBLISHED ' `
        -Label 'native marker capture start publication'
    if ($publishedLine -notmatch '^CHRONOBREAK_NATIVE_MP4_START_SIGNAL_PUBLISHED path=(.+)$') {
        throw 'Native marker capture start-publication line is malformed.'
    }
    Assert-NormalizedPathEquals $Matches[1] $markerStartSignal `
        'native marker capture start-publication path'

    $markerVerification = $report.replay_time_markers.verification
    Assert-ExactJsonFields -Value $markerVerification -Expected @(
        'schema_version','command','passed','synthetic','fixture_kind',
        'marker_duration_seconds','expected_generation','decoded_frame_count',
        'decoded_audio_samples','expected_audio_samples','decoded_audio_coverage_delta_samples',
        'probe_time_base','first_video_pts','last_video_pts','video_epoch_time_seconds',
        'pre_epoch_frame_count',
        'first_live_output_frame','first_live_timestamp_ms','last_live_timestamp_ms',
        'duplicate_visual_timestamps','maximum_visual_timestamp_step_ms',
        'invalid_pre_epoch_payload_frames','invalid_post_epoch_payload_frames',
        'invalid_post_epoch_payload_frame_limit','markers_in_interval','measurements',
        'maximum_av_disagreement_ms','maximum_observed_drift_samples',
        'capture_latency_identifiable','drift_gate_applied','timing_claim',
        'strict_drift_authority','input_files','manifest'
    ) -Label 'native marker verification'
    Assert-JsonIntegerEquals $markerVerification.schema_version 2 'native marker result schema'
    Assert-OrdinalStringEquals $markerVerification.command `
        'verify-native-recorder-media' 'native marker command'
    if (
        $markerVerification.passed -isnot [bool] -or -not $markerVerification.passed -or
        $markerVerification.synthetic -isnot [bool] -or $markerVerification.synthetic
    ) {
        throw 'Native marker result must be a passing non-synthetic result.'
    }
    Assert-OrdinalStringEquals $markerVerification.fixture_kind `
        'native-wgc-replay-time' 'native marker fixture kind'
    Assert-JsonIntegerEquals $markerVerification.marker_duration_seconds 6 `
        'native marker duration'
    Assert-JsonIntegerEquals $markerVerification.expected_generation $markerGeneration `
        'native marker result generation'
    Assert-JsonIntegerEquals $markerVerification.decoded_frame_count 360 `
        'native marker decoded frame count'
    Assert-JsonIntegerEquals $markerVerification.expected_audio_samples 288000 `
        'native marker expected audio samples'
    if (
        -not (Test-JsonInteger $markerVerification.decoded_audio_coverage_delta_samples) -or
        [Math]::Abs([int64]$markerVerification.decoded_audio_coverage_delta_samples) -gt 2048
    ) {
        throw 'Native marker decoded audio coverage differs by more than 2048 samples.'
    }
    Assert-JsonIntegerEquals $markerVerification.invalid_pre_epoch_payload_frames 0 `
        'native marker invalid pre-epoch words'
    Assert-JsonIntegerEquals $markerVerification.invalid_post_epoch_payload_frames 0 `
        'native marker invalid post-epoch words'
    Assert-JsonIntegerEquals $markerVerification.invalid_post_epoch_payload_frame_limit 0 `
        'native marker invalid-word limit'
    Assert-JsonIntegerEquals $markerVerification.markers_in_interval 3 `
        'native marker measurement count'
    Assert-JsonIntegerEquals $markerVerification.maximum_av_disagreement_ms 50 `
        'native marker coarse A/V bound'
    if (
        -not (Test-JsonInteger $markerVerification.maximum_observed_drift_samples) -or
        [int64]$markerVerification.maximum_observed_drift_samples -lt 0
    ) {
        throw 'Native marker observed diagnostic drift must be a nonnegative integer.'
    }
    if (
        $markerVerification.capture_latency_identifiable -isnot [bool] -or
        $markerVerification.capture_latency_identifiable -or
        $markerVerification.drift_gate_applied -isnot [bool] -or
        $markerVerification.drift_gate_applied
    ) {
        throw 'Native marker result must identify capture latency as unknown and apply no strict drift gate.'
    }
    Assert-OrdinalStringEquals $markerVerification.timing_claim `
        'diagnostic-coarse-wgc-alignment-only' 'native marker timing claim'
    Assert-OrdinalStringEquals $markerVerification.strict_drift_authority `
        'native-post-capture-mux-replay-time' 'native marker strict drift authority'
    Assert-OrdinalStringEquals $markerVerification.first_video_pts '0' `
        'native marker first video PTS'
    Assert-JsonObjectsEquivalent -Actual $markerVerification.probe_time_base `
        -Expected ([ordered]@{ numerator='1'; denominator='15360' }) `
        -Label 'native marker probe time base'

    $sourceMarkerResult = Join-Path $SourceRoot 'marker-result.json'
    $sourceMarkerManifest = Join-Path $SourceRoot 'marker-manifest.json'
    foreach ($markerArtifact in @($sourceMarkerResult, $sourceMarkerManifest)) {
        if (-not (Test-Path -LiteralPath $markerArtifact -PathType Leaf)) {
            throw "Native marker artifact is missing: $markerArtifact"
        }
        Assert-ReparseFreeExistingChain -Path $markerArtifact -Label 'native marker artifact'
    }
    Assert-JsonObjectsEquivalent -Actual $markerVerification `
        -Expected (Read-BoundedJson $sourceMarkerResult 16MB) `
        -Label 'native marker persisted result'
    Assert-NormalizedPathEquals $markerVerification.manifest $sourceMarkerManifest `
        'native marker manifest path'
    Assert-NormalizedPathEquals $markerVerification.input_files.source_media.path `
        $SourceVideo 'native marker source media path'
    Assert-JsonIntegerEquals $markerVerification.input_files.source_media.bytes `
        $actualBytes 'native marker source media bytes'
    Assert-Sha256Equals $markerVerification.input_files.source_media.sha256 `
        $actualHash 'native marker source media SHA-256'

    $markerArtifacts = @(
        @('decoded_marker_video','marker.gray'),
        @('decoded_audio','marker.wav'),
        @('frame_probe','marker-frames.ffprobe.json')
    )
    foreach ($artifact in $markerArtifacts) {
        $facts = $markerVerification.input_files.($artifact[0])
        $path = Join-Path $SourceRoot $artifact[1]
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Native marker decoded artifact is missing: $path"
        }
        Assert-NormalizedPathEquals $facts.path $path "native marker $($artifact[0]) path"
        Assert-JsonIntegerEquals $facts.bytes (Get-Item -LiteralPath $path -Force).Length `
            "native marker $($artifact[0]) bytes"
        Assert-Sha256Equals $facts.sha256 (Get-Sha256 $path) `
            "native marker $($artifact[0]) SHA-256"
    }

    $measurements = @($markerVerification.measurements)
    if ($measurements.Count -ne 3) {
        throw 'Native marker result must contain exactly three measurements.'
    }
    $expectedMarkerNames = @('early','middle','late')
    for ($index = 0; $index -lt $measurements.Count; $index++) {
        Assert-OrdinalStringEquals $measurements[$index].name $expectedMarkerNames[$index] `
            "native marker measurement $index name"
        if (
            -not (Test-JsonInteger $measurements[$index].av_disagreement_samples) -or
            -not (Test-JsonInteger $measurements[$index].drift_from_early_samples)
        ) {
            throw "Native marker measurement $index diagnostic values are not integers."
        }
        $absolute = [Math]::Abs([int64]$measurements[$index].av_disagreement_samples)
        if ($absolute -gt 2400) {
            throw "Native marker measurement $index exceeds the coarse 50 ms diagnostic bound."
        }
    }
    $outerPass = @($InvocationOutput -split "`r?`n" | Where-Object { $_ -ceq 'CHRONOBREAK_NATIVE_FIXTURE=PASS' })
    if ($outerPass.Count -ne 1) {
        throw 'Native runner did not emit exactly one terminal PASS marker.'
    }
    return $report
}

function Assert-CorpusReceipt {
    param(
        [string]$ReceiptPath,
        [string]$CorpusRoot,
        [string]$CorpusId,
        [string]$RuntimeId,
        [object[]]$FixtureSpecs
    )
    $receipt = Read-BoundedJson -Path $ReceiptPath -MaximumBytes 16MB
    Assert-ExactJsonFields -Value $receipt -Expected @(
        'schema_version','corpus_id','runtime_id','source_policy','publication_policy',
        'automatic_cleanup','fixtures'
    ) -Label 'corpus receipt'
    Assert-JsonIntegerEquals $receipt.schema_version 2 'corpus receipt schema'
    Assert-OrdinalStringEquals $receipt.corpus_id $CorpusId 'corpus receipt identity'
    Assert-OrdinalStringEquals $receipt.runtime_id $RuntimeId 'corpus receipt runtime identity'
    Assert-OrdinalStringEquals $receipt.source_policy 'explicit generated fixture media; sources opened read-only and hash-checked before/after' 'corpus receipt source policy'
    Assert-OrdinalStringEquals $receipt.publication_policy 'fixtures remain below .partial until all media and schema-v2 bundle facts are validated; corpus.json publishes last' 'corpus receipt publication policy'
    if ($receipt.automatic_cleanup -isnot [bool] -or $receipt.automatic_cleanup) {
        throw 'Corpus receipt automatic_cleanup must be false.'
    }

    Assert-TreeReparseFree -Root $CorpusRoot -Label 'published corpus'
    $expected = [System.Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
    foreach ($fixture in $FixtureSpecs) {
        if ($fixture.id -isnot [string] -or $expected.ContainsKey([string]$fixture.id)) {
            throw 'Requested corpus fixture identities are invalid or duplicated.'
        }
        $expected.Add([string]$fixture.id, $fixture)
    }
    if (@($receipt.fixtures).Count -ne $expected.Count) {
        throw 'Corpus receipt has a missing or unexpected fixture count.'
    }
    $expectedRootNames = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    [void]$expectedRootNames.Add('corpus.json')
    foreach ($id in $expected.Keys) { [void]$expectedRootNames.Add($id) }
    $rootEntries = @(Get-ChildItem -LiteralPath $CorpusRoot -Force)
    if ($rootEntries.Count -ne $expectedRootNames.Count) { throw 'Published corpus has missing or unexpected root entries.' }
    foreach ($entry in $rootEntries) {
        if (-not $expectedRootNames.Contains($entry.Name)) { throw "Published corpus contains unexpected entry $($entry.Name)." }
    }

    $seen = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $fixtureFields = @(
        'fixture_id','media_id','producer_backend','source_name','source_sha256_before',
        'source_sha256_after_preflight','source_media','source_grid','source_sha256_after_build',
        'alias','game_timestamp','transform','bundle_relative_path','output_media','output_grid',
        'output_media_sha256','metadata_sha256','game_log_sha256','snapshot_count','event_count',
        'personal_data'
    )
    $mediaFields = @(
        'size_bytes','format_name','video_codec','video_profile','width','height','video_start_pts',
        'video_duration_ts','video_frames','video_packets','has_b_frames','audio_codec',
        'audio_sample_rate','audio_channels','audio_start_pts','audio_duration_ts','audio_packets',
        'exact_container_start','exact_container_duration','exact_frame_rate','exact_video_time_base',
        'exact_audio_time_base'
    )
    foreach ($published in @($receipt.fixtures)) {
        Assert-ExactJsonFields $published $fixtureFields 'corpus receipt fixture'
        if ($published.fixture_id -isnot [string] -or -not $expected.ContainsKey([string]$published.fixture_id)) {
            throw 'Corpus receipt contains an unexpected fixture identity.'
        }
        $id = [string]$published.fixture_id
        if (-not $seen.Add($id)) { throw "Corpus receipt duplicates fixture $id." }
        $spec = $expected[$id]
        $producer = 'replay-corpus-' + ([string]$spec.transform).Replace('_','-')
        Assert-OrdinalStringEquals $published.media_id ([string]$spec.media_id) "receipt $id media identity"
        Assert-OrdinalStringEquals $published.alias ([string]$spec.alias) "receipt $id alias"
        Assert-OrdinalStringEquals $published.game_timestamp ([string]$spec.game_timestamp) "receipt $id game identity"
        Assert-OrdinalStringEquals $published.transform ([string]$spec.transform) "receipt $id transform"
        Assert-OrdinalStringEquals $published.producer_backend $producer "receipt $id producer"
        Assert-OrdinalStringEquals $published.bundle_relative_path $id "receipt $id bundle path"
        Assert-OrdinalStringEquals $published.source_name ([System.IO.Path]::GetFileName([string]$spec.source_video)) "receipt $id source name"
        $sourceHash = Get-Sha256 ([string]$spec.source_video)
        Assert-Sha256Equals $published.source_sha256_before $sourceHash "receipt $id source hash before"
        Assert-Sha256Equals $published.source_sha256_after_preflight $sourceHash "receipt $id source hash after preflight"
        Assert-Sha256Equals $published.source_sha256_after_build $sourceHash "receipt $id source hash after build"
        if ([string]$spec.transform -ceq 'normalize') {
            if ($null -ne $published.source_grid) {
                throw "Normalize receipt $id must not claim a source grid."
            }
        } elseif ([string]$spec.transform -ceq 'copy') {
            Assert-ExactJsonFields $published.source_grid @(
                'frame_count','first_pts','one_past_last_pts','duration_ts','frame_step_pts'
            ) "receipt $id source grid"
            foreach ($name in @('frame_count','first_pts','one_past_last_pts','duration_ts','frame_step_pts')) {
                if (-not (Test-JsonInteger $published.source_grid.PSObject.Properties[$name].Value)) {
                    throw "Receipt $id source-grid fact $name is not an integer."
                }
            }
            if (
                [int64]$published.source_grid.frame_count -ne [int64]$published.source_media.video_frames -or
                [int64]$published.source_grid.first_pts -ne [int64]$published.source_media.video_start_pts -or
                [int64]$published.source_grid.duration_ts -ne [int64]$published.source_media.video_duration_ts
            ) {
                throw "Copy receipt $id source-grid facts contradict its media facts."
            }
        } else {
            throw "Receipt $id uses an unsupported verifier transform."
        }
        Assert-ExactJsonFields $published.source_media $mediaFields "receipt $id source media"
        Assert-ExactJsonFields $published.output_media $mediaFields "receipt $id output media"
        foreach ($mediaFact in @($published.source_media, $published.output_media)) {
            foreach ($name in @(
                'size_bytes','width','height','video_start_pts','video_duration_ts','video_frames',
                'video_packets','has_b_frames','audio_sample_rate','audio_channels','audio_start_pts',
                'audio_duration_ts','audio_packets'
            )) {
                if (-not (Test-JsonInteger $mediaFact.PSObject.Properties[$name].Value)) {
                    throw "Receipt $id media fact $name is not an integer."
                }
            }
            foreach ($name in @('exact_container_start','exact_container_duration','exact_frame_rate','exact_video_time_base','exact_audio_time_base')) {
                Assert-ExactJsonFields $mediaFact.PSObject.Properties[$name].Value @('numerator','denominator') "receipt $id $name"
            }
            Assert-OrdinalStringEquals $mediaFact.video_codec ([string]$spec.expected_video_codec) "receipt $id video codec"
            Assert-OrdinalStringEquals $mediaFact.audio_codec ([string]$spec.expected_audio_codec) "receipt $id audio codec"
            Assert-OrdinalStringEquals $mediaFact.exact_frame_rate.numerator ([string]$spec.expected_frame_rate.numerator) "receipt $id frame-rate numerator"
            Assert-OrdinalStringEquals $mediaFact.exact_frame_rate.denominator ([string]$spec.expected_frame_rate.denominator) "receipt $id frame-rate denominator"
            if ($mediaFact.format_name -isnot [string] -or @(([string]$mediaFact.format_name).Split(',')) -cnotcontains 'mp4') {
                throw "Receipt $id media fact format_name is not MP4."
            }
        }
        Assert-JsonIntegerEquals $published.source_media.size_bytes (Get-Item -LiteralPath ([string]$spec.source_video) -Force).Length "receipt $id source byte count"

        $bundle = [System.IO.Path]::GetFullPath((Join-Path $CorpusRoot $id))
        if (-not (Test-StrictDescendant $bundle $CorpusRoot) -or -not (Test-Path -LiteralPath $bundle -PathType Container)) {
            throw "Receipt $id bundle path is not the requested published directory."
        }
        $expectedBundleNames = @('video.mp4','metadata.json','game_log.json')
        $bundleEntries = @(Get-ChildItem -LiteralPath $bundle -Force)
        if ($bundleEntries.Count -ne $expectedBundleNames.Count) { throw "Published bundle $id has missing or unexpected entries." }
        foreach ($entry in $bundleEntries) {
            if ($entry.PSIsContainer -or $entry.Name -cnotin $expectedBundleNames) { throw "Published bundle $id contains unexpected entry $($entry.Name)." }
        }
        $videoPath = Join-Path $bundle 'video.mp4'
        $metadataPath = Join-Path $bundle 'metadata.json'
        $gameLogPath = Join-Path $bundle 'game_log.json'
        $videoHash = Get-Sha256 $videoPath
        Assert-Sha256Equals $published.output_media_sha256 $videoHash "receipt $id output media hash"
        Assert-Sha256Equals $published.metadata_sha256 (Get-Sha256 $metadataPath) "receipt $id metadata hash"
        Assert-Sha256Equals $published.game_log_sha256 (Get-Sha256 $gameLogPath) "receipt $id game-log hash"
        Assert-JsonIntegerEquals $published.output_media.size_bytes (Get-Item -LiteralPath $videoPath -Force).Length "receipt $id output byte count"

        Assert-ExactJsonFields $published.output_grid @('frame_count','first_pts','one_past_last_pts','duration_ts','frame_step_pts') "receipt $id output grid"
        foreach ($name in @('frame_count','first_pts','one_past_last_pts','duration_ts','frame_step_pts')) {
            if (-not (Test-JsonInteger $published.output_grid.PSObject.Properties[$name].Value)) {
                throw "Receipt $id output-grid fact $name is not an integer."
            }
        }
        if (
            [int64]$published.output_grid.frame_count -ne [int64]$published.output_media.video_frames -or
            [int64]$published.output_grid.first_pts -ne [int64]$published.output_media.video_start_pts -or
            [int64]$published.output_grid.duration_ts -ne [int64]$published.output_media.video_duration_ts -or
            [int64]$published.output_grid.one_past_last_pts -ne
                ([int64]$published.output_grid.first_pts + [int64]$published.output_grid.duration_ts)
        ) {
            throw "Receipt $id output-grid facts contradict its media facts."
        }

        $metadata = Read-BoundedJson -Path $metadataPath -MaximumBytes 16MB
        $gameLog = Read-BoundedJson -Path $gameLogPath -MaximumBytes 16MB
        Assert-JsonIntegerEquals $metadata.schema_version 2 "bundle $id metadata schema"
        Assert-JsonIntegerEquals $gameLog.schema_version 2 "bundle $id game-log schema"
        Assert-OrdinalStringEquals $metadata.media_id ([string]$spec.media_id) "bundle $id metadata identity"
        Assert-OrdinalStringEquals $metadata.media_timeline.media_id ([string]$spec.media_id) "bundle $id timeline identity"
        Assert-OrdinalStringEquals $gameLog.media_id ([string]$spec.media_id) "bundle $id game-log identity"
        Assert-OrdinalStringEquals $metadata.capture_backend $producer "bundle $id producer"
        Assert-OrdinalStringEquals $metadata.media_timeline.producer.backend $producer "bundle $id timeline producer"
        Assert-OrdinalStringEquals $metadata.media_runtime_id $RuntimeId "bundle $id runtime"
        Assert-OrdinalStringEquals $metadata.media_timeline.producer.media_runtime_id $RuntimeId "bundle $id timeline runtime"
        Assert-OrdinalStringEquals $metadata.recording_codec ([string]$spec.expected_video_codec) "bundle $id recording codec"
        Assert-JsonIntegerEquals $published.snapshot_count @($gameLog.snapshots).Count "receipt $id snapshot count"
        Assert-JsonIntegerEquals $published.event_count @($gameLog.events).Count "receipt $id event count"
        if ($published.personal_data -isnot [bool] -or $published.personal_data) {
            throw "Receipt $id must identify the generated bundle as non-personal."
        }
    }
    if ($seen.Count -ne $expected.Count) { throw 'Corpus receipt is missing a requested fixture.' }
    return $receipt
}

function New-ResultDocument {
    param([string]$Status, [string]$Failure)
    $requiredFailures = @($script:Gates | Where-Object { $_.required -and -not $_.passed } | ForEach-Object { $_.name })
    return [ordered]@{
        schema_version = 2
        verifier = 'QB-REPLAY-012'
        status = $Status
        preflight_only = [bool]$PreflightOnly
        repository = $script:Repository
        runtime_root = $script:RuntimeRoot
        fixture_root = $script:FixtureRoot
        app_binary = $script:AppBinary
        sentinel_root = $script:SentinelRoot
        run_id = if ($script:RunRoot) { Split-Path -Leaf $script:RunRoot } else { $null }
        started_utc = $script:StartedUtc.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
        finished_utc = [DateTime]::UtcNow.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
        automatic_cleanup = $false
        artifacts_preserved = $true
        synthetic_media_provenance = 'synthetic_offline_reference_only'
        recorder_provenance_policy = 'native label requires successful dedicated native recorder runner evidence'
        interfaces = [ordered]@{
            corpus_builder = 'python tools/replay_benchmark/build_corpus.py --spec <v2-spec> --media-runtime-root <runtime> [--preflight-only]'
            native_recorder = 'powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -DurationSeconds 6 -ReplayTimeMarkers -RuntimeRoot <runtime> -OutputRoot build/perf/qb-replay-012-native'
            fixture_helper = 'python tools/replay_time/fixture_tools.py <generate|verify-grid|verify-media|verify-native-recorder-media|verify-bundle|make-negative> ...'
            media_tools = '<RuntimeRoot>/bin/ffmpeg.exe and ffprobe.exe only'
            production_export = 'powershell tools/replay_benchmark/prepare.ps1 followed by tools/replay_benchmark/run.ps1 with one packaged Tauri/WebView export scenario'
        }
        production_export_interface = [ordered]@{
            available = $true
            required = $true
            detail = 'The verifier prepares isolated marker bundles and invokes the existing packaged Tauri/WebView replay-benchmark export path.'
        }
        required_gate_failures = $requiredFailures
        failure = $Failure
        dependencies = @($script:Dependencies)
        gates = @($script:Gates)
        scenarios = @($script:Scenarios)
        commands = @($script:Commands)
    }
}

function Write-FinalResult {
    param([string]$Status, [string]$Failure)
    if ($null -eq $script:ResultPath) { return }
    Assert-TreeReparseFree -Root $script:RunRoot -Label 'immutable verifier run'
    $document = New-ResultDocument -Status $Status -Failure $Failure
    foreach ($file in Get-ChildItem -LiteralPath $script:RunRoot -File -Recurse -Force) {
        $file.IsReadOnly = $true
    }
    Write-NewJson -Path $script:ResultPath -Value $document -MaximumBytes $script:MaximumResultBytes
    (Get-Item -LiteralPath $script:ResultPath).IsReadOnly = $true
    $summary = [ordered]@{
        schema_version = 2
        verifier = 'QB-REPLAY-012'
        status = $Status
        result = $script:ResultPath
        required_gate_failures = @($document.required_gate_failures)
        automatic_cleanup = $false
    } | ConvertTo-Json -Depth 5 -Compress
    Write-Output $summary
}

$exitCode = 1
try {
    $script:RuntimeRoot = Resolve-RepositoryPath -Value $RuntimeRoot -Label 'RuntimeRoot'
    $script:FixtureRoot = Resolve-RepositoryPath -Value $FixtureRoot -Label 'FixtureRoot'
    $script:AppBinary = Resolve-RepositoryPath -Value $AppBinary -Label 'AppBinary'
    $allowedFixtureParent = [System.IO.Path]::GetFullPath((Join-Path $script:Repository 'build\replay-time'))
    $buildPerf = [System.IO.Path]::GetFullPath((Join-Path $script:Repository 'build\perf'))

    if (-not (Test-StrictDescendant -Path $script:FixtureRoot -Root $allowedFixtureParent)) {
        throw "FixtureRoot must be a strict descendant of $allowedFixtureParent."
    }
    if ($script:FixtureRoot -eq $allowedFixtureParent) { throw 'FixtureRoot cannot be the broad replay-time build root.' }
    if ((Test-StrictDescendant -Path $script:RuntimeRoot -Root $script:FixtureRoot) -or
        (Test-StrictDescendant -Path $script:FixtureRoot -Root $script:RuntimeRoot) -or
        $script:RuntimeRoot -eq $script:FixtureRoot) {
        throw 'RuntimeRoot and FixtureRoot must be disjoint.'
    }
    if (-not (Test-StrictDescendant -Path $script:AppBinary -Root $script:Repository)) {
        throw 'AppBinary must be a project-owned file below the repository root.'
    }
    $knownLibraryRoots = @(
        (Join-Path $env:LOCALAPPDATA 'LeagueReplay'),
        (Join-Path $env:LOCALAPPDATA 'QueueBack'),
        (Join-Path $env:APPDATA 'LeagueReplay'),
        (Join-Path $HOME 'Videos\LeagueReplay')
    ) | ForEach-Object { [System.IO.Path]::GetFullPath($_) }
    foreach ($libraryRoot in $knownLibraryRoots) {
        if ($script:FixtureRoot -eq $libraryRoot -or (Test-StrictDescendant -Path $script:FixtureRoot -Root $libraryRoot) -or (Test-StrictDescendant -Path $libraryRoot -Root $script:FixtureRoot)) {
            throw "FixtureRoot intersects a known user recording/library root: $libraryRoot"
        }
    }
    Assert-ReparseFreeExistingChain -Path $script:RuntimeRoot -Label 'RuntimeRoot'
    Assert-ReparseFreeExistingChain -Path $script:FixtureRoot -Label 'FixtureRoot'
    if (-not (Test-Path -LiteralPath $script:RuntimeRoot -PathType Container)) { throw "Packaged RuntimeRoot is missing: $($script:RuntimeRoot)" }

    if (Test-Path -LiteralPath $script:FixtureRoot) {
        $existingEntries = @(Get-ChildItem -LiteralPath $script:FixtureRoot -Force)
        $expectedSentinel = Join-Path $script:FixtureRoot '.chronobreak-replay-time'
        if (Test-Path -LiteralPath $expectedSentinel -PathType Container) {
            if ($existingEntries.Count -ne 1 -or -not [string]::Equals($existingEntries[0].FullName, $expectedSentinel, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'An existing FixtureRoot with a sentinel may contain only that sentinel directory.'
            }
        } elseif ($existingEntries.Count -ne 0) {
            throw 'An existing FixtureRoot without the replay-time sentinel must be truly empty.'
        }
    }
    $script:SentinelRoot = Join-Path $script:FixtureRoot '.chronobreak-replay-time'
    if (-not (Test-Path -LiteralPath $script:SentinelRoot)) {
        [System.IO.Directory]::CreateDirectory($script:SentinelRoot) | Out-Null
        Write-NewText -Path (Join-Path $script:SentinelRoot 'sentinel.json') -Text "{`"schema_version`":2,`"purpose`":`"QB-REPLAY-012 immutable verifier evidence only`",`"automatic_cleanup`":false}`n"
    }
    $sentinelContract = Join-Path $script:SentinelRoot 'sentinel.json'
    if (-not (Test-Path -LiteralPath $sentinelContract -PathType Leaf)) { throw 'Replay-time sentinel contract is missing.' }
    $expectedSentinelContract = "{`"schema_version`":2,`"purpose`":`"QB-REPLAY-012 immutable verifier evidence only`",`"automatic_cleanup`":false}`n"
    $actualSentinelBytes = [System.IO.File]::ReadAllBytes($sentinelContract)
    $expectedSentinelBytes = $script:Utf8NoBom.GetBytes($expectedSentinelContract)
    if ([Convert]::ToBase64String($actualSentinelBytes) -cne [Convert]::ToBase64String($expectedSentinelBytes)) {
        throw 'Replay-time sentinel contract content is not exact.'
    }
    Assert-TreeReparseFree -Root $script:SentinelRoot -Label 'replay-time sentinel root'
    $runsRoot = Join-Path $script:SentinelRoot 'runs'
    if (-not (Test-Path -LiteralPath $runsRoot)) { [System.IO.Directory]::CreateDirectory($runsRoot) | Out-Null }
    $runId = 'r-' + [DateTime]::UtcNow.ToString('yyMMddHHmmssfff', [Globalization.CultureInfo]::InvariantCulture) + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
    $script:RunRoot = Join-Path $runsRoot $runId
    if (Test-Path -LiteralPath $script:RunRoot) { throw "Fresh run directory collision: $($script:RunRoot)" }
    New-Item -ItemType Directory -Path $script:RunRoot -ErrorAction Stop | Out-Null
    $script:ResultPath = Join-Path $script:RunRoot 'result.json'

    Add-Type -AssemblyName System.Runtime.Serialization
    Add-Type -Path (Join-Path $PSScriptRoot 'BoundedProcess.cs')
    $script:PowerShell = Join-Path $PSHOME 'powershell.exe'
    $script:Python = Get-ExecutablePath -Name 'python.exe'
    $script:Cargo = Get-ExecutablePath -Name 'cargo.exe'
    $script:ToolchainDirectories = [System.Collections.ArrayList]::new()
    [void]$script:ToolchainDirectories.Add((Split-Path -Parent $script:Cargo))
    foreach ($toolName in @('rustc.exe', 'link.exe', 'cl.exe', 'cmake.exe', 'ninja.exe')) {
        $toolCommand = Get-Command $toolName -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($null -ne $toolCommand) {
            $toolDirectory = Split-Path -Parent ([System.IO.Path]::GetFullPath($toolCommand.Source))
            if (-not $script:ToolchainDirectories.Contains($toolDirectory)) { [void]$script:ToolchainDirectories.Add($toolDirectory) }
        }
    }
    $script:ToolchainPathDirectories = [string[]]$script:ToolchainDirectories
    $script:FixtureTools = Join-Path $PSScriptRoot 'fixture_tools.py'
    $corpusBuilder = Join-Path $script:Repository 'tools\replay_benchmark\build_corpus.py'
    $nativeRunner = Join-Path $script:Repository 'tools\native_backend\run_native_fixture.ps1'
    $runtimeVerifier = Join-Path $script:Repository 'tools\media_runtime\verify.ps1'
    $benchmarkPrepare = Join-Path $script:Repository 'tools\replay_benchmark\prepare.ps1'
    $benchmarkRunner = Join-Path $script:Repository 'tools\replay_benchmark\run.ps1'
    $benchmarkAnalyzer = Join-Path $script:Repository 'tools\replay_benchmark\analyze.py'
    foreach ($requiredFile in @($script:PowerShell, $script:Python, $script:Cargo, $script:FixtureTools, $corpusBuilder, $nativeRunner, $runtimeVerifier, $benchmarkPrepare, $benchmarkRunner, $benchmarkAnalyzer, $script:AppBinary)) {
        if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) { throw "Required interface is missing: $requiredFile" }
        Assert-ReparseFreeExistingChain -Path $requiredFile -Label 'required interface'
    }
    Add-Dependency -Name 'windows' -Status 'required' -Detail 'Windows job objects, WGC, and PowerShell 5.1 are required.'
    Add-Dependency -Name 'python' -Status 'available' -Detail $script:Python
    Add-Dependency -Name 'rust-cargo' -Status 'available' -Detail $script:Cargo
    Add-Dependency -Name 'native-recorder' -Status 'environment-dependent' -Detail 'Requires an available native capture/NVENC path and an interactive desktop.'
    Add-Dependency -Name 'production-export-webview' -Status 'available' -Detail "Packaged app $($script:AppBinary) is driven only through tools/replay_benchmark/run.ps1."

    Assert-TreeReparseFree -Root $script:RuntimeRoot -Label 'packaged media runtime'
    $script:Ffmpeg = Join-Path $script:RuntimeRoot 'bin\ffmpeg.exe'
    $script:Ffprobe = Join-Path $script:RuntimeRoot 'bin\ffprobe.exe'
    foreach ($tool in @($script:Ffmpeg, $script:Ffprobe)) {
        if (-not (Test-Path -LiteralPath $tool -PathType Leaf)) { throw "Packaged media tool is missing: $tool" }
        Assert-ReparseFreeExistingChain -Path $tool -Label 'packaged media tool'
    }
    if (Test-Path -LiteralPath (Join-Path $script:RuntimeRoot 'bin\ffplay.exe')) { throw 'Packaged runtime must not contain ffplay.exe.' }
    $runtimeManifest = Read-BoundedJson -Path (Join-Path $script:RuntimeRoot 'runtime-manifest.json') -MaximumBytes 1MB
    if ([string]::IsNullOrWhiteSpace([string]$runtimeManifest.runtime_id)) { throw 'Packaged runtime manifest has no runtime_id.' }
    $runtimePowerShellArguments = New-Utf8PowerShellArguments -ScriptPath $runtimeVerifier -Arguments @(
        '-RuntimeRoot',$script:RuntimeRoot
    )
    $runtimeCheck = Invoke-Bounded -Name 'runtime-verifier' -Executable $script:PowerShell -Arguments $runtimePowerShellArguments `
        -TimeoutSeconds 300 -PathDirectories @((Split-Path -Parent $script:Ffmpeg))
    Add-Gate -Name 'packaged-runtime-identity-and-capabilities' -Passed $runtimeCheck.Record.passed -Detail "runtime_id=$($runtimeManifest.runtime_id); exact manifest/hash/banner/config/capability verifier passed"
    Add-Gate -Name 'sentinel-and-reparse-safety' -Passed $true -Detail 'All mutable artifacts are under a strict build/replay-time descendant and a reparse-free .chronobreak-replay-time sentinel.'
    Add-Gate -Name 'bounded-process-and-output' -Passed $true -Detail 'All child commands use a kill-on-close Windows job, finite timeout, and combined output cap; final JSON is capped at 256 KiB.'
    Add-Gate -Name 'production-export-interface' -Passed $true -Detail 'Packaged Tauri/WebView replay-benchmark prepare/run interfaces and the project-owned app binary are available.'

    if ($PreflightOnly) {
        Add-Gate -Name 'preflight-no-generation' -Passed $true -Detail 'Preflight performed no recorder, build, test, corpus, production-app, or media-generation command.'
        Write-FinalResult -Status 'passed' -Failure $null
        $exitCode = 0
        exit 0
    }

    $syntheticRoot = Join-Path $script:RunRoot 'synthetic'
    [System.IO.Directory]::CreateDirectory($syntheticRoot) | Out-Null
    $rawVideo = Join-Path $syntheticRoot 'markers-360.yuv'
    $rawAudio = Join-Path $syntheticRoot 'markers-360.wav'
    $manifest = Join-Path $syntheticRoot 'markers.manifest.json'
    $generateResult = Join-Path $syntheticRoot 'generate.json'
    $generated = Invoke-FixtureHelper -Name 'generate-markers' -Arguments @(
        'generate','--video',$rawVideo,'--audio',$rawAudio,'--manifest',$manifest,
        '--width','320','--height','240','--frames','360','--rate','60/1','--result',$generateResult
    ) -ResultPath $generateResult -ExpectedCommand 'generate' -TimeoutSeconds 600
    if ($generated.synthetic -ne $true -or $generated.frame_count -ne 360) {
        throw 'Fixture generator did not identify the deterministic 360-frame source as synthetic.'
    }
    $markerEncodeFilter = 'scale=1280:960:flags=neighbor,pad=1920:1080:320:60:color=black'

    $zeroMedia = Join-Path $syntheticRoot 'zero-start.mp4'
    Invoke-Bounded -Name 'encode-zero-start' -Executable $script:Ffmpeg -Arguments @(
        '-hide_banner','-nostdin','-v','error','-f','rawvideo','-pix_fmt','yuv420p','-video_size','320x240','-framerate','60','-i',$rawVideo,
        '-i',$rawAudio,'-map','0:v:0','-map','1:a:0','-vf',$markerEncodeFilter,'-c:v','libx264','-preset','medium','-crf','12','-bf','0','-g','60',
        '-c:a','aac','-b:a','192k','-ar','48000','-ac','1','-video_track_timescale','60000','-frames:v','360','-shortest',$zeroMedia
    ) -TimeoutSeconds 900 | Out-Null
    $nonzeroMedia = Join-Path $syntheticRoot 'nonzero-start.mp4'
    Invoke-Bounded -Name 'encode-nonzero-start' -Executable $script:Ffmpeg -Arguments @(
        '-hide_banner','-nostdin','-v','error','-f','rawvideo','-pix_fmt','yuv420p','-video_size','320x240','-framerate','60','-i',$rawVideo,
        '-i',$rawAudio,'-filter_complex',"[0:v]${markerEncodeFilter},setpts=PTS+5/TB[v];[1:a]asetpts=PTS+5/TB[a]",
        '-map','[v]','-map','[a]','-c:v','libx264','-preset','medium','-crf','12','-bf','0','-g','60',
        '-c:a','aac','-b:a','192k','-ar','48000','-ac','1','-video_track_timescale','60000','-frames:v','360','-vsync','0',$nonzeroMedia
    ) -TimeoutSeconds 900 | Out-Null
    $zeroGrid = Invoke-ProbeAndGrid -Name 'synthetic-zero' -Media $zeroMedia -Origin 'zero' -Directory $syntheticRoot
    $nonzeroGrid = Invoke-ProbeAndGrid -Name 'synthetic-nonzero' -Media $nonzeroMedia -Origin 'nonzero' -Directory $syntheticRoot
    $zeroMediaProof = Invoke-DecodeAndVerifyMedia -Name 'synthetic-zero' -Media $zeroMedia -Manifest $manifest -ExpectedStart 0 -ExpectedCount 360 -Directory $syntheticRoot
    $nonzeroMediaProof = Invoke-DecodeAndVerifyMedia -Name 'synthetic-nonzero' -Media $nonzeroMedia -Manifest $manifest -ExpectedStart 0 -ExpectedCount 360 -Directory $syntheticRoot
    [void]$script:Scenarios.Add([ordered]@{
        name = 'synthetic-grid-nonzero-av'
        status = 'passed'
        provenance = 'synthetic_offline_reference'
        generated = Get-RelativeEvidencePath $generateResult
        zero_grid = $zeroGrid
        nonzero_grid = $nonzeroGrid
        zero_av = $zeroMediaProof
        nonzero_av = $nonzeroMediaProof
    })
    Add-Gate -Name 'synthetic-exact-grid-nonzero-start-av' -Passed $true -Detail '360 deterministic 60 fps markers proved exact zero/nonzero grids and bounded A/V marker disagreement/drift.'

    $exportsRoot = Join-Path $syntheticRoot 'reference-exports'
    [System.IO.Directory]::CreateDirectory($exportsRoot) | Out-Null
    $exportRecords = [System.Collections.ArrayList]::new()
    foreach ($range in @(
        [ordered]@{ name='boundary'; start=0; end=120 },
        [ordered]@{ name='middle'; start=120; end=240 },
        [ordered]@{ name='end'; start=240; end=360 }
    )) {
        $exportMedia = Join-Path $exportsRoot "$($range.name).mp4"
        $audioStart = $range.start * 800
        $audioEnd = $range.end * 800
        $filter = "[0:v]trim=start_frame=$($range.start):end_frame=$($range.end),setpts=PTS-STARTPTS[v];[0:a]atrim=start_sample=${audioStart}:end_sample=${audioEnd},asetpts=PTS-STARTPTS[a]"
        Invoke-Bounded -Name "reference-export-$($range.name)" -Executable $script:Ffmpeg -Arguments @(
            '-hide_banner','-nostdin','-v','error','-i',$zeroMedia,'-filter_complex',$filter,'-map','[v]','-map','[a]',
            '-c:v','libx264','-preset','medium','-crf','12','-bf','0','-g','60','-c:a','aac','-b:a','192k','-ar','48000','-ac','1',
            '-video_track_timescale','60000','-frames:v','120',$exportMedia
        ) -TimeoutSeconds 900 | Out-Null
        $grid = Invoke-ProbeAndGrid -Name "reference-$($range.name)" -Media $exportMedia -Origin 'zero' -Directory $exportsRoot
        $mediaProof = Invoke-DecodeAndVerifyMedia -Name "reference-$($range.name)" -Media $exportMedia -Manifest $manifest -ExpectedStart $range.start -ExpectedCount 120 -Directory $exportsRoot
        [void]$exportRecords.Add([ordered]@{
            name = $range.name
            source_interval = "[$($range.start),$($range.end))"
            provenance = 'synthetic_offline_reference_not_production_export'
            grid = $grid
            media = $mediaProof
        })
    }
    [void]$script:Scenarios.Add([ordered]@{
        name = 'synthetic-reference-exports'
        status = 'passed-reference-only'
        provenance = 'synthetic_offline_reference_not_production_export'
        intervals = @($exportRecords)
    })
    Add-Gate -Name 'synthetic-reference-export-boundary-middle-end' -Passed $true -Detail 'Offline references exactly cover [0,120), [120,240), and [240,360); they do not satisfy the production-export gate.' -Required $false

    $productionCorpusId = 'pm-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
    $productionCorpusRoot = Join-Path $script:RunRoot $productionCorpusId
    $productionCorpusSpecPath = Join-Path $script:RunRoot 'pm.json'
    $productionMarkerFixture = [ordered]@{
        id = 'pm-v2'
        media_id = [Guid]::NewGuid().ToString('D').ToLowerInvariant()
        alias = 'pm-v2'
        game_timestamp = '1900000010'
        source_video = $zeroMedia
        transform = 'copy'
        expected_video_codec = 'h264'
        expected_audio_codec = 'aac'
        expected_frame_rate = [ordered]@{ numerator='60'; denominator='1' }
        minimum_duration_seconds = 5
    }
    $productionCorpusSpec = [ordered]@{
        schema_version = 2
        corpus_id = $productionCorpusId
        sentinel_root = $script:SentinelRoot
        output_root = $productionCorpusRoot
        fixtures = @($productionMarkerFixture)
    }
    Write-NewJson -Path $productionCorpusSpecPath -Value $productionCorpusSpec
    $productionBuilderArguments = @(
        $corpusBuilder,'--spec',$productionCorpusSpecPath,
        '--media-runtime-root',$script:RuntimeRoot
    )
    Invoke-Bounded -Name 'production-marker-corpus-preflight' -Executable $script:Python `
        -Arguments @($productionBuilderArguments + '--preflight-only') -TimeoutSeconds 1800 `
        -PathDirectories @((Split-Path -Parent $script:Ffmpeg)) | Out-Null
    Invoke-Bounded -Name 'production-marker-corpus-build' -Executable $script:Python `
        -Arguments $productionBuilderArguments -TimeoutSeconds 1800 `
        -PathDirectories @((Split-Path -Parent $script:Ffmpeg)) | Out-Null
    $productionCorpusReceiptPath = Join-Path $productionCorpusRoot 'corpus.json'
    $productionCorpusReceipt = Assert-CorpusReceipt -ReceiptPath $productionCorpusReceiptPath `
        -CorpusRoot $productionCorpusRoot -CorpusId $productionCorpusId `
        -RuntimeId ([string]$runtimeManifest.runtime_id) -FixtureSpecs @($productionMarkerFixture)
    $productionMarkerBundle = Join-Path $productionCorpusRoot $productionMarkerFixture.id
    if (@($productionCorpusReceipt.fixtures).Count -ne 1) {
        throw 'Production marker corpus did not publish exactly one fixture.'
    }

    $productionExportsRoot = Join-Path $script:RunRoot 'pe'
    [System.IO.Directory]::CreateDirectory($productionExportsRoot) | Out-Null
    $productionExportRecords = [System.Collections.ArrayList]::new()
    foreach ($range in @(
        [ordered]@{ name='boundary'; slug='b'; start_frame=0; end_frame=300; start_ms=0; end_ms=5000; marker_count=2 },
        [ordered]@{ name='middle'; slug='m'; start_frame=30; end_frame=330; start_ms=500; end_ms=5500; marker_count=3 },
        [ordered]@{ name='end'; slug='e'; start_frame=60; end_frame=360; start_ms=1000; end_ms=6000; marker_count=3 }
    )) {
        $benchmarkRoot = Join-Path $productionExportsRoot $range.slug
        [System.IO.Directory]::CreateDirectory($benchmarkRoot) | Out-Null
        $benchmarkSentinel = Join-Path $benchmarkRoot '.chronobreak-replay-benchmark'
        [System.IO.Directory]::CreateDirectory($benchmarkSentinel) | Out-Null
        $benchmarkRunId = "pe-$($range.slug)"
        $benchmarkSpecPath = Join-Path $benchmarkRoot 'p.json'
        $benchmarkManifestPath = Join-Path $benchmarkSentinel "manifests\$benchmarkRunId.json"
        $benchmarkLibraryRoot = Join-Path $benchmarkSentinel 'library'
        $benchmarkResultRoot = Join-Path $benchmarkSentinel "results\$benchmarkRunId"
        $benchmarkSpec = [ordered]@{
            schema_version = 1
            run_id = $benchmarkRunId
            sentinel_root = $benchmarkSentinel
            library_root = $benchmarkLibraryRoot
            config_path = Join-Path $benchmarkSentinel 'config\config.toml'
            app_data_root = Join-Path $benchmarkSentinel 'app-data'
            result_root = $benchmarkResultRoot
            scratch_root = Join-Path $benchmarkSentinel 'scratch'
            observer_profile = 'full'
            ddragon = [ordered]@{
                mode = 'offline'
                cache_root = Join-Path $benchmarkSentinel 'app-data\ddragon'
                cache_fingerprint = $null
            }
            fixtures = @(
                [ordered]@{
                    id = $productionMarkerFixture.id
                    alias = $productionMarkerFixture.alias
                    game_timestamp = $productionMarkerFixture.game_timestamp
                    kind = 'recording_bundle'
                    source_path = $productionMarkerBundle
                    destination_relative_path = "games\$($productionMarkerFixture.game_timestamp)"
                    backend = 'replay-corpus-copy'
                    codec = 'h264'
                    negative = $false
                }
            )
            scenarios = @(
                [ordered]@{
                    id = "production-export-$($range.name)"
                    kind = 'export'
                    fixture_ids = @($productionMarkerFixture.id)
                    trial_id = "$($range.name)-01"
                    seed = 20260831
                    clip_start_ms = $range.start_ms
                    clip_end_ms = $range.end_ms
                    expected_duration_ms = 5000
                    duration_tolerance_ms = 50
                    expected_video_codec = 'h264'
                    expected_audio_codec = 'aac'
                    export_presets = @('horizontal')
                    music_mode = 'none'
                    gain_mode = 'unity'
                }
            )
            app_binary = $script:AppBinary
            analyzer_path = $benchmarkAnalyzer
            python_path = $script:Python
            timeout_seconds = 300
        }
        Write-NewJson -Path $benchmarkSpecPath -Value $benchmarkSpec
        $preparePreflightArguments = New-Utf8PowerShellArguments -ScriptPath $benchmarkPrepare -Arguments @(
            '-Spec',$benchmarkSpecPath,'-Manifest',$benchmarkManifestPath,
            '-MediaRuntimeRoot',$script:RuntimeRoot,'-PreflightOnly'
        )
        Invoke-Bounded -Name "prepare-production-$($range.name)-preflight" `
            -Executable $script:PowerShell -Arguments $preparePreflightArguments `
            -TimeoutSeconds 1800 -PathDirectories @($script:ToolchainPathDirectories + (Split-Path -Parent $script:Ffmpeg)) | Out-Null
        $prepareArguments = New-Utf8PowerShellArguments -ScriptPath $benchmarkPrepare -Arguments @(
            '-Spec',$benchmarkSpecPath,'-Manifest',$benchmarkManifestPath,
            '-MediaRuntimeRoot',$script:RuntimeRoot
        )
        Invoke-Bounded -Name "prepare-production-$($range.name)" `
            -Executable $script:PowerShell -Arguments $prepareArguments `
            -TimeoutSeconds 1800 -PathDirectories @($script:ToolchainPathDirectories + (Split-Path -Parent $script:Ffmpeg)) | Out-Null

        $preparedManifestHash = Get-Sha256 $benchmarkManifestPath
        $stagedSourceMedia = Join-Path $benchmarkLibraryRoot "games\$($productionMarkerFixture.game_timestamp)\video.mp4"
        $sourceHashBefore = Get-Sha256 $stagedSourceMedia
        if ($sourceHashBefore -cne (Get-Sha256 $zeroMedia)) {
            throw "Prepared production $($range.name) source does not match the controlled marker media."
        }
        $runArguments = New-Utf8PowerShellArguments -ScriptPath $benchmarkRunner -Arguments @(
            '-Manifest',$benchmarkManifestPath
        )
        Invoke-Bounded -Name "run-production-$($range.name)" -Executable $script:PowerShell `
            -Arguments $runArguments -TimeoutSeconds 900 -OutputLimitBytes 16MB `
            -PathDirectories @($script:ToolchainPathDirectories + (Split-Path -Parent $script:Ffmpeg)) | Out-Null

        Assert-TreeReparseFree -Root $benchmarkSentinel -Label "production $($range.name) benchmark sentinel"
        $resultManifestPath = Join-Path $benchmarkResultRoot 'manifest.json'
        $runnerResultPath = Join-Path $benchmarkResultRoot 'runner-result.json'
        $exportValidationPath = Join-Path $benchmarkResultRoot 'export-validation.json'
        foreach ($evidencePath in @($resultManifestPath, $runnerResultPath, $exportValidationPath)) {
            if (-not (Test-Path -LiteralPath $evidencePath -PathType Leaf)) {
                throw "Production $($range.name) export evidence is missing: $evidencePath"
            }
        }
        if ((Get-Sha256 $resultManifestPath) -cne $preparedManifestHash) {
            throw "Production $($range.name) result manifest differs from its prepared launch."
        }
        $runnerResult = Read-BoundedJson -Path $runnerResultPath -MaximumBytes 1MB
        $exportValidation = Read-BoundedJson -Path $exportValidationPath -MaximumBytes 16MB
        if (
            [int]$runnerResult.schema_version -ne 1 -or
            [string]$runnerResult.run_id -cne $benchmarkRunId -or
            [string]$runnerResult.status -cne 'complete' -or
            [int]$runnerResult.app_exit_code -ne 0 -or
            $runnerResult.fixture_hashes_match -ne $true -or
            [int]$runnerResult.analyzer_exit_code -ne 0
        ) {
            throw "Production $($range.name) runner result is incomplete or invalid."
        }
        if (
            [int]$exportValidation.schema_version -ne 1 -or
            [string]$exportValidation.run_id -cne $benchmarkRunId -or
            [string]$exportValidation.scenario_id -cne "production-export-$($range.name)" -or
            [int]$exportValidation.expected_output_count -ne 1 -or
            [int]$exportValidation.expected_duration_ms -ne 5000 -or
            [string]$exportValidation.expected_streams.video_codec -cne 'h264' -or
            [string]$exportValidation.expected_streams.audio_codec -cne 'aac' -or
            $exportValidation.all_valid -ne $true -or
            @($exportValidation.outputs).Count -ne 1
        ) {
            throw "Production $($range.name) export validation contract did not pass."
        }
        $output = @($exportValidation.outputs)[0]
        if (
            [string]$output.preset -cne 'horizontal' -or
            [int]$output.duration_ms -ne 5000 -or
            $output.full_single_thread_decode_ok -ne $true -or
            $output.valid -ne $true -or
            [string]$output.ffprobe.streams[0].codec_name -cne 'h264' -or
            [string]$output.ffprobe.streams[1].codec_name -cne 'aac'
        ) {
            throw "Production $($range.name) output media record is invalid."
        }
        $outputMedia = [System.IO.Path]::GetFullPath((Join-Path $benchmarkLibraryRoot ([string]$output.relative_path)))
        $outputThumbnail = [System.IO.Path]::GetFullPath((Join-Path $benchmarkLibraryRoot ([string]$output.thumbnail_relative_path)))
        foreach ($publishedPath in @($outputMedia, $outputThumbnail)) {
            if (-not (Test-StrictDescendant -Path $publishedPath -Root $benchmarkLibraryRoot) -or
                -not (Test-Path -LiteralPath $publishedPath -PathType Leaf)) {
                throw "Production $($range.name) export escaped its isolated benchmark library."
            }
            Assert-ReparseFreeExistingChain -Path $publishedPath -Label "production $($range.name) output"
        }
        if ((Get-Sha256 $outputMedia) -cne [string]$output.sha256 -or
            (Get-Sha256 $outputThumbnail) -cne [string]$output.thumbnail_sha256 -or
            (Get-Item -LiteralPath $outputMedia -Force).Length -ne [int64]$output.size_bytes) {
            throw "Production $($range.name) published output identity does not match export-validation.json."
        }

        $proofRoot = Join-Path $benchmarkRoot 'x'
        [System.IO.Directory]::CreateDirectory($proofRoot) | Out-Null
        $grid = Invoke-ProbeAndGrid -Name "production-$($range.name)" -Media $outputMedia -Origin 'zero' -Directory $proofRoot
        if ([int]$grid.frame_count -ne 300 -or [int64]$grid.first_pts -ne 0) {
            throw "Production $($range.name) output does not contain the exact zero-based 300-frame grid."
        }
        $mediaProof = Invoke-DecodeAndVerifyMedia -Name "production-$($range.name)" `
            -Media $outputMedia -Manifest $manifest -ExpectedStart $range.start_frame `
            -ExpectedCount 300 -Directory $proofRoot
        if (
            [int]$mediaProof.first_source_frame -ne [int]$range.start_frame -or
            [int]$mediaProof.last_source_frame -ne ([int]$range.end_frame - 1) -or
            [int]$mediaProof.marker_count -ne [int]$range.marker_count
        ) {
            throw "Production $($range.name) output does not match its exact half-open source-frame interval."
        }
        $sourceHashAfter = Get-Sha256 $stagedSourceMedia
        if ($sourceHashAfter -cne $sourceHashBefore) {
            throw "Production $($range.name) export mutated its prepared source media."
        }
        [void]$productionExportRecords.Add([ordered]@{
            name = $range.name
            source_interval = "[$($range.start_frame),$($range.end_frame))"
            source_media = Get-RelativeEvidencePath $stagedSourceMedia
            source_sha256_before = $sourceHashBefore
            source_sha256_after = $sourceHashAfter
            output_media = Get-RelativeEvidencePath $outputMedia
            output_thumbnail = Get-RelativeEvidencePath $outputThumbnail
            output_sha256 = [string]$output.sha256
            runner_result = Get-RelativeEvidencePath $runnerResultPath
            export_validation = Get-RelativeEvidencePath $exportValidationPath
            exact_grid = $grid
            marker_av_proof = $mediaProof
        })
    }
    [void]$script:Scenarios.Add([ordered]@{
        name = 'production-exports-boundary-middle-end'
        status = 'passed'
        provenance = 'packaged_tauri_webview_replay_benchmark_production_export'
        marker_corpus_receipt = Get-RelativeEvidencePath $productionCorpusReceiptPath
        intervals = @($productionExportRecords)
    })
    Add-Gate -Name 'production-export-boundary-middle-end' -Passed $true -Detail 'Three isolated packaged Tauri/WebView exports proved exact 300-frame half-open source intervals, zero-based output grids, source preservation, full decode, and bounded A/V marker alignment.'
    $cargoTarget = Join-Path $script:SentinelRoot 'cargo-target'
    if (-not (Test-Path -LiteralPath $cargoTarget)) {
        [System.IO.Directory]::CreateDirectory($cargoTarget) | Out-Null
    }
    $recorderTests = @(
        @('finalizer-probe-rejection','finalizer::tests::probe_result_rejects_timeout_oversize_failure_and_malformed_json'),
        @('finalizer-publication-identity','finalizer::tests::publication_requires_identity_and_exposes_video_then_metadata'),
        @('finalizer-stale-identity','finalizer::tests::publication_rejects_stale_media_identity_without_renaming_candidate')
    )
    foreach ($test in $recorderTests) {
        Invoke-ExactCargoTest -Name $test[0] -ManifestPath (Join-Path $script:Repository 'recorder\Cargo.toml') -TestName $test[1] -CargoTarget $cargoTarget | Out-Null
    }
    foreach ($test in @(
        'library::tests::rejects_schema_v1_unknown_missing_and_mismatched_bundles',
        'library::tests::save_requires_and_preserves_strict_schema_v2_metadata',
        'clip_export::tests::ffmpeg_arguments_use_exact_seek_and_decoded_frame_trim',
        'clip_export::tests::rejects_short_or_out_of_range_requests',
        'clip_export::tests::publication_failure_removes_every_staged_and_final_output'
    )) {
        Invoke-ExactCargoTest -Name ('app-' + ($test -replace '::','-')) -ManifestPath (Join-Path $script:Repository 'app\src-tauri\Cargo.toml') -TestName $test -CargoTarget $cargoTarget | Out-Null
    }
    [void]$script:Scenarios.Add([ordered]@{
        name = 'production-contract-test-hooks'
        status = 'passed'
        provenance = 'production_rust_contract_tests'
        coverage = @('bounded/malformed finalizer probe','publication ordering','stale identity rejection','strict schema-v2 library rejection/preservation','exact export seek/decoded trim arguments','range rejection','failed clip publication removes every staged and canonical output')
    })
    Add-Gate -Name 'production-schema-identity-publication-hooks' -Passed $true -Detail 'Narrow production Rust tests proved strict schema/identity handling and that a mid-publication export failure leaves no canonical clip or thumbnail.'

    Assert-ReparseFreeExistingChain -Path $buildPerf -Label 'dedicated build/perf recorder root'
    $nativeOutput = Join-Path $buildPerf 'qb-replay-012-native'
    if (Test-Path -LiteralPath $nativeOutput) {
        Assert-TreeReparseFree -Root $nativeOutput -Label 'dedicated recorder output root'
    }
    $nativePowerShellArguments = New-Utf8PowerShellArguments -ScriptPath $nativeRunner -Arguments @(
        '-Scenario','steady','-DurationSeconds','6','-ReplayTimeMarkers',
        '-RuntimeRoot',$script:RuntimeRoot,'-OutputRoot',$nativeOutput
    )
    $nativeInvocation = Invoke-Bounded -Name 'native-recorder-fixture' -Executable $script:PowerShell `
        -Arguments $nativePowerShellArguments -TimeoutSeconds 1800 -OutputLimitBytes 16MB `
        -PathDirectories @($script:ToolchainPathDirectories + (Split-Path -Parent $script:Ffmpeg) + (Split-Path -Parent $script:Python)) -AllowFailure

    $recorderSources = [System.Collections.ArrayList]::new()
    foreach ($capture in @(
        [ordered]@{ kind='native'; invocation=$nativeInvocation; allowed=$nativeOutput; relative_video='video.mp4'; sentinel='.queueback-native-fixture-root'; expected_sentinel="QueueBack dedicated native fixture outputs only.`n" }
    )) {
        if (Test-Path -LiteralPath $capture.allowed -PathType Container) {
            Assert-TreeReparseFree -Root $capture.allowed -Label "$($capture.kind) dedicated recorder output root"
        }
        if (-not $capture.invocation.Record.passed) {
            $failureText = $capture.invocation.StandardOutput + "`n" + $capture.invocation.StandardError
            $environmentBlocked = $failureText -match '(?i)(no (?:compatible|capable|supported|suitable).*(?:adapter|device|gpu)|(?:nvenc|wgc).*(?:unavailable|not available|not supported|unsupported)|NO_ENCODE_DEVICE|0x887A0004|failed to create input view|access.*denied|0x80070005|interactive desktop)'
            $failureStatus = if ($environmentBlocked) { 'environment-blocked' } else { 'failed' }
            [void]$script:Scenarios.Add([ordered]@{
                name = "$($capture.kind)-recorder"
                status = $failureStatus
                provenance = "$($capture.kind)_recorder_runner_failed"
                command = $capture.invocation.Record.name
                detail = 'Dedicated recorder fixture failed; no synthetic media was relabeled. Hardware/environment classification is used only for an explicit runner diagnostic.'
            })
            Add-Gate -Name "$($capture.kind)-recorder-evidence" -Passed $false -Detail "Dedicated recorder runner status: $failureStatus; see bounded command logs."
            continue
        }
        $sourceRoot = Get-RecorderEvidenceRoot -Text $capture.invocation.StandardOutput -AllowedRoot $capture.allowed
        $runnerSentinel = Join-Path $capture.allowed $capture.sentinel
        if (-not (Test-Path -LiteralPath $runnerSentinel -PathType Leaf) -or [System.IO.File]::ReadAllText($runnerSentinel) -cne $capture.expected_sentinel) {
            throw "$($capture.kind) recorder output sentinel is missing or has unexpected content."
        }
        $sourceVideo = Join-Path $sourceRoot $capture.relative_video
        $sourceResult = Join-Path $sourceRoot 'result.json'
        foreach ($source in @($sourceVideo, $sourceResult)) {
            if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "$($capture.kind) recorder evidence is missing: $source" }
            Assert-ReparseFreeExistingChain -Path $source -Label "$($capture.kind) evidence file"
        }
        $sourceHashBefore = Get-Sha256 $sourceVideo
        $copyRoot = Join-Path $script:RunRoot "recorder-$($capture.kind)"
        [System.IO.Directory]::CreateDirectory($copyRoot) | Out-Null
        $copiedVideo = Join-Path $copyRoot 'video.mp4'
        $copiedResult = Join-Path $copyRoot 'runner-result.json'
        Copy-NewReadOnly -Source $sourceVideo -Destination $copiedVideo
        Copy-NewReadOnly -Source $sourceResult -Destination $copiedResult -MaximumBytes 16MB
        if ((Get-Sha256 $copiedVideo) -cne $sourceHashBefore) {
            throw "$($capture.kind) recorder copied media does not match the marked source."
        }
        $runnerReport = Assert-RecorderReport -ReportPath $copiedResult `
            -SourceRoot $sourceRoot -SourceVideo $sourceVideo -CopiedVideo $copiedVideo `
            -RuntimeId ([string]$runtimeManifest.runtime_id) -InvocationOutput $capture.invocation.StandardOutput
        $grid = Invoke-ProbeAndGrid -Name "$($capture.kind)-recorder" -Media $copiedVideo -Origin 'zero' -Directory $copyRoot
        $sourceHashAfter = Get-Sha256 $sourceVideo
        if ($sourceHashAfter -cne $sourceHashBefore) { throw "$($capture.kind) recorder source changed while evidence was copied and probed." }
        $record = [ordered]@{
            kind = $capture.kind
            source_root = $sourceRoot
            source_video = $sourceVideo
            source_sha256_before = $sourceHashBefore
            source_sha256_after_copy_probe = $sourceHashAfter
            source_unchanged = $true
            copied_video = $copiedVideo
            copied_video_relative = Get-RelativeEvidencePath $copiedVideo
            copied_result_relative = Get-RelativeEvidencePath $copiedResult
            grid = $grid
            runner_report_schema = $runnerReport.schema
            runner_report_kind = $runnerReport.scope
        }
        [void]$recorderSources.Add($record)
        [void]$script:Scenarios.Add([ordered]@{
            name = "$($capture.kind)-recorder"
            status = 'passed'
            provenance = "$($capture.kind)_dedicated_recorder_runner"
            evidence = $record
        })
        Add-Gate -Name "$($capture.kind)-recorder-evidence" -Passed $true -Detail 'Dedicated runner marker and strict result report were bound to invocation/runtime, terminal capture facts, exact candidate path, and copied media bytes/SHA-256 before exact-grid probing.'
    }

    if ($recorderSources.Count -eq 1) {
        $corpusId = 'qb-replay-012-' + [Guid]::NewGuid().ToString('N')
        $corpusOutput = Join-Path $script:RunRoot $corpusId
        $specPath = Join-Path $script:RunRoot 'corpus-v2.spec.json'
        $fixtureSpecs = [System.Collections.ArrayList]::new()
        $index = 0
        foreach ($source in $recorderSources) {
            $index++
            [void]$fixtureSpecs.Add([ordered]@{
                id = "$($source.kind)-recorder"
                media_id = [Guid]::NewGuid().ToString('D').ToLowerInvariant()
                alias = "$($source.kind)-recorder"
                game_timestamp = (1900000000 + $index).ToString([Globalization.CultureInfo]::InvariantCulture)
                source_video = $source.copied_video
                transform = 'normalize'
                expected_video_codec = 'h264'
                expected_audio_codec = 'aac'
                expected_frame_rate = [ordered]@{ numerator='60'; denominator='1' }
                minimum_duration_seconds = 5
                target_duration_seconds = [ordered]@{ numerator='6'; denominator='1' }
            })
        }
        $spec = [ordered]@{
            schema_version = 2
            corpus_id = $corpusId
            sentinel_root = $script:SentinelRoot
            output_root = $corpusOutput
            fixtures = @($fixtureSpecs)
        }
        Write-NewJson -Path $specPath -Value $spec
        $builderArgs = @($corpusBuilder,'--spec',$specPath,'--media-runtime-root',$script:RuntimeRoot)
        Invoke-Bounded -Name 'corpus-v2-preflight' -Executable $script:Python -Arguments @($builderArgs + '--preflight-only') -TimeoutSeconds 1800 -PathDirectories @((Split-Path -Parent $script:Ffmpeg)) | Out-Null
        Invoke-Bounded -Name 'corpus-v2-build' -Executable $script:Python -Arguments $builderArgs -TimeoutSeconds 7200 -PathDirectories @((Split-Path -Parent $script:Ffmpeg)) | Out-Null
        $receiptPath = Join-Path $corpusOutput 'corpus.json'
        $receipt = Assert-CorpusReceipt -ReceiptPath $receiptPath -CorpusRoot $corpusOutput `
            -CorpusId $corpusId -RuntimeId ([string]$runtimeManifest.runtime_id) -FixtureSpecs @($fixtureSpecs)
        $bundleProofs = [System.Collections.ArrayList]::new()
        foreach ($fixture in $fixtureSpecs) {
            $bundleRoot = Join-Path $corpusOutput $fixture.id
            $bundleResult = Join-Path $script:RunRoot "$($fixture.id).bundle.json"
            $proof = Invoke-FixtureHelper -Name "bundle-$($fixture.id)" -Arguments @(
                'verify-bundle','--metadata',(Join-Path $bundleRoot 'metadata.json'),'--game-log',(Join-Path $bundleRoot 'game_log.json'),
                '--expected-media-id',$fixture.media_id,'--result',$bundleResult
            ) -ResultPath $bundleResult -ExpectedCommand 'verify-bundle'
            [void]$bundleProofs.Add([ordered]@{ id=$fixture.id; media_id=$proof.media_id; result=Get-RelativeEvidencePath $bundleResult })
        }
        $negativeRoot = Join-Path $script:RunRoot 'negative-fixtures'
        $negativeResult = Join-Path $script:RunRoot 'negative-fixtures.json'
        $firstBundle = Join-Path $corpusOutput $fixtureSpecs[0].id
        $negative = Invoke-FixtureHelper -Name 'make-negative-fixtures' -Arguments @(
            'make-negative','--bundle',$firstBundle,'--root',$negativeRoot,'--result',$negativeResult
        ) -ResultPath $negativeResult -ExpectedCommand 'make-negative'
        if ($negative.case_count -ne 5 -or $negative.publication_failure_canonical_trios -ne 0) {
            throw 'Negative fixture generation did not prove all strict schema-v2 rejection cases.'
        }
        foreach ($source in $recorderSources) {
            $afterCorpus = Get-Sha256 $source.source_video
            if ($afterCorpus -cne $source.source_sha256_before) { throw "$($source.kind) original source changed during corpus construction." }
            $source['source_sha256_after_corpus'] = $afterCorpus
        }
        [void]$script:Scenarios.Add([ordered]@{
            name = 'strict-v2-corpus-and-negative-fixtures'
            status = 'passed'
            provenance = 'dedicated_native_recorder_source'
            spec = Get-RelativeEvidencePath $specPath
            receipt = Get-RelativeEvidencePath $receiptPath
            bundle_proofs = @($bundleProofs)
            negative_result = Get-RelativeEvidencePath $negativeResult
            negative_cases = @($negative.cases | ForEach-Object { $_.name })
            source_hashes_preserved = $true
        })
        Add-Gate -Name 'strict-v2-corpus-bundles-negatives' -Passed $true -Detail 'Stable v2 corpus CLI receipt identities/producers/transforms and recomputed bundle hashes were bound before strict bundle and five negative-case checks passed.'
    } else {
        [void]$script:Scenarios.Add([ordered]@{
            name = 'strict-v2-corpus-and-negative-fixtures'
            status = 'blocked-by-recorder-gate'
            provenance = 'not_run_without_genuine_native_recorder_source'
            detail = 'Corpus construction requires successful dedicated native recorder evidence; synthetic media was not substituted.'
        })
        Add-Gate -Name 'strict-v2-corpus-bundles-negatives' -Passed $false -Detail 'Not run because genuine native recorder evidence was not available.'
    }

    $requiredFailures = @($script:Gates | Where-Object { $_.required -and -not $_.passed })
    if ($requiredFailures.Count -eq 0) {
        Write-FinalResult -Status 'passed' -Failure $null
        $exitCode = 0
    } else {
        Write-FinalResult -Status 'failed' -Failure ("Required gates failed: " + (($requiredFailures | ForEach-Object { $_.name }) -join ', '))
        $exitCode = 1
    }
}
catch {
    $message = [string]::Join(' ', $_.Exception.Message.Split([char[]]@("`r", "`n", "`t"), [StringSplitOptions]::RemoveEmptyEntries))
    if ($message.Length -gt 2000) { $message = $message.Substring(0, 2000) }
    if ($null -ne $script:ResultPath -and -not (Test-Path -LiteralPath $script:ResultPath)) {
        try { Write-FinalResult -Status 'failed' -Failure $message } catch { Write-Error "Verifier failed and bounded result publication also failed: $($_.Exception.Message)" }
    }
    Write-Error $message
    $exitCode = 1
}
finally {
    if ($null -ne $script:RunRoot -and (Test-Path -LiteralPath $script:RunRoot)) {
        Assert-ReparseFreeExistingChain -Path $script:RunRoot -Label 'preserved immutable run root'
    }
}
exit $exitCode
