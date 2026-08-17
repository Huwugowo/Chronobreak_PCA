[CmdletBinding()]
param(
    [ValidateRange(10, 3600)]
    [int]$DurationSeconds = 240,

    [ValidateSet('ffmpeg-native', 'native-ffmpeg')]
    [string]$Order = 'ffmpeg-native',

    [ValidateRange(0, 300)]
    [int]$CooldownSeconds = 15,

    [string]$MediaRuntimeRoot,
    [string]$ResultRoot,
    [string]$CargoPath,
    [switch]$PreflightOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$script:Invariant = [System.Globalization.CultureInfo]::InvariantCulture
$script:Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$manifest = Get-Content -Raw -LiteralPath (Join-Path $repo 'media-runtime\runtime-lock.json') | ConvertFrom-Json
$allowedResultsRoot = Join-Path $repo 'evidence\preliminary-backend-ab'

function Stop-WithCode {
    param([string]$Code, [string]$Message)
    throw "CHRONOBREAK-AB-${Code}: $Message"
}

function Write-Json {
    param([string]$Path, [object]$Value, [int]$Depth = 8)
    $json = $Value | ConvertTo-Json -Depth $Depth
    [System.IO.File]::WriteAllText($Path, $json, $script:Utf8NoBom)
}

function Get-AbsolutePath {
    param([string]$Path)
    return [System.IO.Path]::GetFullPath($Path)
}

function Assert-LockedRuntime {
    param([string]$Root)

    if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
        Stop-WithCode 'RUNTIME-MISSING' "The pinned r5 runtime directory is missing at $Root. Supply the exact staged queueback-ffmpeg-8.1.2-windows-x86_64-r5 pair."
    }
    $runtimeManifest = Join-Path $Root 'runtime-manifest.json'
    if (-not (Test-Path -LiteralPath $runtimeManifest -PathType Leaf)) {
        Stop-WithCode 'RUNTIME-MANIFEST' "runtime-manifest.json is missing at $Root."
    }
    $lockPath = Join-Path $repo 'media-runtime\runtime-lock.json'
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $runtimeManifest).Hash -ne
        (Get-FileHash -Algorithm SHA256 -LiteralPath $lockPath).Hash) {
        Stop-WithCode 'RUNTIME-LOCK' 'The staged runtime manifest is not byte-identical to the embedded r5 lock.'
    }
    foreach ($file in $manifest.files) {
        $relative = [string]$file.path
        if ([string]::IsNullOrWhiteSpace($relative) -or $relative.Contains('\') -or
            $relative.StartsWith('/') -or $relative.Split('/') -contains '..') {
            Stop-WithCode 'RUNTIME-PATH' "The lock contains an unsafe path: $relative"
        }
        $candidate = Join-Path $Root ($relative.Replace('/', '\'))
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            Stop-WithCode 'RUNTIME-FILE' "The staged runtime is missing $relative."
        }
        $item = Get-Item -LiteralPath $candidate
        if ([int64]$item.Length -ne [int64]$file.size) {
            Stop-WithCode 'RUNTIME-SIZE' "$relative is $($item.Length) bytes, expected $($file.size)."
        }
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $candidate).Hash.ToLowerInvariant()
        if ($actual -ne ([string]$file.sha256).ToLowerInvariant()) {
            Stop-WithCode 'RUNTIME-HASH' "$relative does not match the locked SHA-256."
        }
    }

    $ffmpeg = Join-Path $Root 'bin\ffmpeg.exe'
    $version = (& $ffmpeg -hide_banner -version 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0 -or $version -notlike "*ffmpeg version $($manifest.ffmpeg.version_banner)*") {
        Stop-WithCode 'RUNTIME-IDENTITY' 'ffmpeg.exe does not report the locked r5 version banner.'
    }
    $filters = (& $ffmpeg -hide_banner -filters 2>&1 | Out-String)
    $encoders = (& $ffmpeg -hide_banner -encoders 2>&1 | Out-String)
    foreach ($required in @('gfxcapture', 'scale_d3d11')) {
        if ($filters -notmatch "(?m)^\s*[TSC\.]{3}\s+$([Regex]::Escape($required))\s") {
            Stop-WithCode 'RUNTIME-CAPABILITY' "ffmpeg.exe does not advertise required filter $required."
        }
    }
    if ($encoders -notmatch '(?m)^\s*V.....\s+h264_nvenc\s') {
        Stop-WithCode 'RUNTIME-CAPABILITY' 'ffmpeg.exe does not advertise h264_nvenc.'
    }
}

function Resolve-Cargo {
    if (-not [string]::IsNullOrWhiteSpace($CargoPath)) {
        $candidate = Get-AbsolutePath $CargoPath
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            Stop-WithCode 'CARGO-MISSING' "CargoPath is not a file: $candidate"
        }
        return $candidate
    }
    $command = Get-Command cargo.exe -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        Stop-WithCode 'CARGO-MISSING' 'cargo.exe is not on PATH; pass -CargoPath explicitly.'
    }
    return $command.Source
}

function Build-TestBinary {
    param([string]$Cargo, [string]$LogPath)

    $artifact = $null
    $output = & $Cargo test --manifest-path (Join-Path $repo 'recorder\Cargo.toml') `
        --release --lib --all-features --no-run --message-format=json 2>&1
    $exitCode = $LASTEXITCODE
    [System.IO.File]::WriteAllLines($LogPath, [string[]]$output, $script:Utf8NoBom)
    foreach ($line in $output) {
        try {
            $message = [string]$line | ConvertFrom-Json -ErrorAction Stop
            if ($message.reason -eq 'compiler-artifact' -and $message.profile.test -and
                $null -ne $message.executable -and $message.target.kind -contains 'lib') {
                $artifact = [string]$message.executable
            }
        } catch {
            # Cargo diagnostics that are not JSON remain preserved in the build log.
        }
    }
    if ($exitCode -ne 0) {
        Stop-WithCode 'BUILD' "Release test build failed with exit $exitCode; see $LogPath."
    }
    if ([string]::IsNullOrWhiteSpace($artifact) -or -not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
        Stop-WithCode 'BUILD-ARTIFACT' 'Cargo did not report the release library test executable.'
    }
    return $artifact
}

function Parse-InvariantNumber {
    param([string]$Value)
    return [double]::Parse($Value, $script:Invariant)
}

function Get-CpuDelta {
    param([object[]]$Rows)
    $ordered = @($Rows | Sort-Object { [DateTime]$_.utc })
    if ($ordered.Count -lt 2) { return 0.0 }
    return (Parse-InvariantNumber $ordered[-1].cpu_seconds) -
        (Parse-InvariantNumber $ordered[0].cpu_seconds)
}

function Test-DirectChildProcess {
    param([int]$ProcessId, [int]$ParentProcessId)

    try {
        $process = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId = $ProcessId" `
            -ErrorAction Stop
        return $null -ne $process -and [int]$process.ParentProcessId -eq $ParentProcessId
    } catch {
        Stop-WithCode 'PROCESS-OWNERSHIP' "Could not prove that ffmpeg PID $ProcessId belongs to recorder test PID ${ParentProcessId}: $($_.Exception.Message)"
    }
}

function Summarize-Arm {
    param([string]$ArmRoot, [string]$Backend)

    $rows = @(Import-Csv -LiteralPath (Join-Path $ArmRoot 'process-resources.csv'))
    $rust = @($rows | Where-Object role -eq 'rust-test' | Sort-Object { [DateTime]$_.utc })
    $ffmpegGroups = @(
        $rows | Where-Object role -eq 'ffmpeg-child' | Group-Object pid |
            Sort-Object Count -Descending
    )
    if ($rust.Count -lt 2 -or $ffmpegGroups.Count -lt 1) {
        Stop-WithCode 'SAMPLES' "$Backend did not retain enough Rust/FFmpeg resource samples."
    }
    $recordingFfmpeg = @($ffmpegGroups[0].Group | Sort-Object { [DateTime]$_.utc })
    $start = [DateTime]$recordingFfmpeg[0].utc
    $end = [DateTime]$recordingFfmpeg[-1].utc
    $elapsed = ($end - $start).TotalSeconds
    if ($elapsed -le 0) {
        Stop-WithCode 'SAMPLES' "$Backend has a non-positive sampled recording interval."
    }
    $rustWindow = @($rust | Where-Object {
        $time = [DateTime]$_.utc
        $time -ge $start -and $time -le $end
    })
    if ($rustWindow.Count -lt 2) {
        Stop-WithCode 'SAMPLES' "$Backend has fewer than two Rust samples inside its recording window."
    }
    $rustCpu = Get-CpuDelta $rustWindow
    $ffmpegCpu = Get-CpuDelta $recordingFfmpeg
    $maxCombinedPrivate = 0.0
    foreach ($childRow in $recordingFfmpeg) {
        $childTime = [DateTime]$childRow.utc
        $nearest = $rustWindow | Sort-Object {
            [Math]::Abs((([DateTime]$_.utc) - $childTime).TotalMilliseconds)
        } | Select-Object -First 1
        if ($null -ne $nearest) {
            $combined = (Parse-InvariantNumber $childRow.private_bytes) +
                (Parse-InvariantNumber $nearest.private_bytes)
            $maxCombinedPrivate = [Math]::Max($maxCombinedPrivate, $combined)
        }
    }
    $probe = Get-Content -Raw -LiteralPath (Join-Path $ArmRoot 'ffprobe.json') | ConvertFrom-Json
    $videoStream = $probe.streams | Where-Object codec_type -eq 'video' | Select-Object -First 1
    $audioStream = $probe.streams | Where-Object codec_type -eq 'audio' | Select-Object -First 1
    $video = Join-Path $ArmRoot 'recording\video.mp4'
    return [ordered]@{
        backend = $Backend
        recording_ffmpeg_pid = [int]$recordingFfmpeg[0].pid
        validation_ffmpeg_pids = @($ffmpegGroups | Select-Object -Skip 1 | ForEach-Object { [int]$_.Name })
        sampled_recording_seconds = $elapsed
        rust_cpu_seconds = $rustCpu
        recording_ffmpeg_cpu_seconds = $ffmpegCpu
        total_cpu_seconds = $rustCpu + $ffmpegCpu
        total_machine_cpu_percent = (($rustCpu + $ffmpegCpu) / $elapsed * 100.0 / [Environment]::ProcessorCount)
        max_combined_private_mib = $maxCombinedPrivate / 1MB
        video_bytes = (Get-Item -LiteralPath $video).Length
        video_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $video).Hash.ToLowerInvariant()
        video_codec = [string]$videoStream.codec_name
        video_width = [int]$videoStream.width
        video_height = [int]$videoStream.height
        video_frame_rate = [string]$videoStream.avg_frame_rate
        video_duration_seconds = [double]::Parse([string]$videoStream.duration, $script:Invariant)
        decoded_video_frames = [uint64]$videoStream.nb_read_frames
        audio_codec = [string]$audioStream.codec_name
        audio_duration_seconds = [double]::Parse([string]$audioStream.duration, $script:Invariant)
        terminal_evidence = 'recording/recording-evidence.txt'
    }
}

function Assert-ComparableArm {
    param([object]$Summary)

    if ($Summary.video_codec -ne 'h264' -or $Summary.audio_codec -ne 'aac') {
        Stop-WithCode 'MEDIA-CONTRACT' "$($Summary.backend) did not produce H.264/AAC media."
    }
    if ($Summary.video_width -ne 1920 -or $Summary.video_height -ne 1080 -or
        $Summary.video_frame_rate -ne '60/1') {
        Stop-WithCode 'MEDIA-CONTRACT' "$($Summary.backend) did not produce 1920x1080 at 60/1 FPS."
    }
    if ($Summary.decoded_video_frames -lt ([uint64]$DurationSeconds * 55) -or
        $Summary.video_duration_seconds -lt ($DurationSeconds * 0.95) -or
        $Summary.video_duration_seconds -gt ($DurationSeconds + 15) -or
        $Summary.audio_duration_seconds -lt ($DurationSeconds * 0.95)) {
        Stop-WithCode 'MEDIA-CONTRACT' "$($Summary.backend) media duration/frame evidence is outside the bounded comparison contract."
    }
}

function Run-Arm {
    param(
        [string]$Backend,
        [string]$TestBinary,
        [uint32]$FixturePid,
        [string]$RuntimeRoot,
        [string]$Root
    )

    $existingFfmpeg = @(Get-Process -Name ffmpeg -ErrorAction SilentlyContinue)
    if ($existingFfmpeg.Count -gt 0) {
        Stop-WithCode 'FFMPEG-BUSY' 'An unrelated ffmpeg.exe is already running; preserve it and retry when the process list is unambiguous.'
    }
    New-Item -ItemType Directory -Path $Root | Out-Null
    $recording = Join-Path $Root 'recording'
    $stdout = Join-Path $Root 'test.stdout.log'
    $stderr = Join-Path $Root 'test.stderr.log'
    $filter = if ($Backend -eq 'ffmpeg') {
        'encoder::tests::real_ffmpeg_lifecycle_records_a_non_league_window'
    } else {
        'native::lifecycle::tests::real_native_lifecycle_records_a_non_league_window'
    }

    $env:QUEUEBACK_MEDIA_RUNTIME_DIR = $RuntimeRoot
    $env:QUEUEBACK_FFMPEG_LIFECYCLE_TEST_PID = [string]$FixturePid
    $env:QUEUEBACK_FFMPEG_LIFECYCLE_TEST_SECONDS = [string]$DurationSeconds
    $env:QUEUEBACK_FFMPEG_LIFECYCLE_TEST_OUTPUT = $recording
    $env:QUEUEBACK_NATIVE_LIFECYCLE_TEST_PID = [string]$FixturePid
    $env:QUEUEBACK_NATIVE_LIFECYCLE_TEST_FFMPEG = Join-Path $RuntimeRoot 'bin\ffmpeg.exe'
    $env:QUEUEBACK_NATIVE_LIFECYCLE_TEST_SECONDS = [string]$DurationSeconds
    $env:QUEUEBACK_NATIVE_LIFECYCLE_TEST_OUTPUT = $recording

    $started = [DateTime]::UtcNow
    $lockedFfmpegPath = Get-AbsolutePath (Join-Path $RuntimeRoot 'bin\ffmpeg.exe')
    $ownedFfmpegPids = @{}
    $process = Start-Process -FilePath $TestBinary -ArgumentList @(
        $filter, '--ignored', '--exact', '--nocapture', '--test-threads=1'
    ) -RedirectStandardOutput $stdout -RedirectStandardError $stderr `
        -WindowStyle Hidden -PassThru
    $samples = New-Object System.Collections.Generic.List[object]
    $sampleIndex = 0
    $deadline = [DateTime]::UtcNow.AddSeconds($DurationSeconds + 120)
    try {
        while (-not $process.HasExited) {
            if ([DateTime]::UtcNow -ge $deadline) {
                Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
                Stop-WithCode 'TIMEOUT' "$Backend exceeded its bounded test deadline."
            }
            foreach ($observed in @(Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) {
                $samples.Add([pscustomobject]@{
                    sample = $sampleIndex
                    utc = [DateTime]::UtcNow.ToString('o')
                    role = 'rust-test'
                    pid = $observed.Id
                    cpu_seconds = ([double]$observed.CPU).ToString('R', $script:Invariant)
                    working_set_bytes = ([double]$observed.WorkingSet64).ToString('R', $script:Invariant)
                    private_bytes = ([double]$observed.PrivateMemorySize64).ToString('R', $script:Invariant)
                    handles = $observed.HandleCount
                    threads = $observed.Threads.Count
                })
            }
            foreach ($observed in @(Get-Process -Name ffmpeg -ErrorAction SilentlyContinue)) {
                try {
                    $observedPath = Get-AbsolutePath $observed.Path
                    $startedInArm = $observed.StartTime.ToUniversalTime() -ge $started.AddSeconds(-1)
                } catch {
                    # A child may exit between enumeration and property access.
                    continue
                }
                $isLockedBinary = [string]::Equals(
                    $observedPath,
                    $lockedFfmpegPath,
                    [StringComparison]::OrdinalIgnoreCase
                )
                if ($startedInArm -and $isLockedBinary -and
                    (-not $ownedFfmpegPids.ContainsKey($observed.Id))) {
                    if (Test-DirectChildProcess -ProcessId $observed.Id -ParentProcessId $process.Id) {
                        $ownedFfmpegPids[$observed.Id] = $true
                    }
                }
                if ($ownedFfmpegPids.ContainsKey($observed.Id)) {
                    $samples.Add([pscustomobject]@{
                        sample = $sampleIndex
                        utc = [DateTime]::UtcNow.ToString('o')
                        role = 'ffmpeg-child'
                        pid = $observed.Id
                        cpu_seconds = ([double]$observed.CPU).ToString('R', $script:Invariant)
                        working_set_bytes = ([double]$observed.WorkingSet64).ToString('R', $script:Invariant)
                        private_bytes = ([double]$observed.PrivateMemorySize64).ToString('R', $script:Invariant)
                        handles = $observed.HandleCount
                        threads = $observed.Threads.Count
                    })
                }
            }
            $sampleIndex++
            Start-Sleep -Seconds 1
            $process.Refresh()
        }
    } finally {
        if (-not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            [void]$process.WaitForExit(5000)
        }
        foreach ($ownedPid in @($ownedFfmpegPids.Keys)) {
            Stop-Process -Id $ownedPid -Force -ErrorAction SilentlyContinue
        }
    }
    $samples | Export-Csv -LiteralPath (Join-Path $Root 'process-resources.csv') `
        -NoTypeInformation -Encoding UTF8
    if ($process.ExitCode -ne 0) {
        Stop-WithCode 'ARM-FAILED' "$Backend exited $($process.ExitCode); see $stdout and $stderr."
    }
    $video = Join-Path $recording 'video.mp4'
    if (-not (Test-Path -LiteralPath $video -PathType Leaf)) {
        Stop-WithCode 'MEDIA-MISSING' "$Backend did not publish recording/video.mp4."
    }
    $ffprobe = Join-Path $RuntimeRoot 'bin\ffprobe.exe'
    $probeOutput = & $ffprobe -v error -count_frames -show_entries `
        'format=duration,size,bit_rate:stream=index,codec_name,codec_type,width,height,avg_frame_rate,r_frame_rate,start_time,duration,nb_read_frames' `
        -of json $video 2>&1
    if ($LASTEXITCODE -ne 0) {
        Stop-WithCode 'FFPROBE' "$Backend output failed locked ffprobe inspection."
    }
    [System.IO.File]::WriteAllLines(
        (Join-Path $Root 'ffprobe.json'),
        [string[]]$probeOutput,
        $script:Utf8NoBom
    )
}

if ([string]::IsNullOrWhiteSpace($MediaRuntimeRoot)) {
    $MediaRuntimeRoot = Join-Path $repo 'build\media-runtime\windows-x86_64'
}
$MediaRuntimeRoot = Get-AbsolutePath $MediaRuntimeRoot
Assert-LockedRuntime $MediaRuntimeRoot

if ($PreflightOnly) {
    Write-Host "CHRONOBREAK_PRELIMINARY_AB_PREFLIGHT=PASS"
    Write-Host "RUNTIME=$($manifest.runtime_id)"
    Write-Host 'SCOPE=non-League preliminary evidence only; not QB-PERF-002 or M8 acceptance'
    exit 0
}
$cargo = Resolve-Cargo

if ([string]::IsNullOrWhiteSpace($ResultRoot)) {
    $ResultRoot = Join-Path $allowedResultsRoot ([DateTime]::Now.ToString('yyyyMMdd-HHmmss'))
}
$ResultRoot = Get-AbsolutePath $ResultRoot
$allowedResultsRoot = Get-AbsolutePath $allowedResultsRoot
if (-not $ResultRoot.StartsWith(
    $allowedResultsRoot + [System.IO.Path]::DirectorySeparatorChar,
    [StringComparison]::OrdinalIgnoreCase
)) {
    Stop-WithCode 'RESULT-PATH' "ResultRoot must be a new child of $allowedResultsRoot."
}
if (Test-Path -LiteralPath $ResultRoot) {
    Stop-WithCode 'RESULT-EXISTS' "ResultRoot already exists; evidence roots are immutable: $ResultRoot"
}
New-Item -ItemType Directory -Force -Path $ResultRoot | Out-Null

$controlledEnvironment = @(
    'QUEUEBACK_MEDIA_RUNTIME_DIR',
    'QUEUEBACK_FFMPEG_LIFECYCLE_TEST_PID',
    'QUEUEBACK_FFMPEG_LIFECYCLE_TEST_SECONDS',
    'QUEUEBACK_FFMPEG_LIFECYCLE_TEST_OUTPUT',
    'QUEUEBACK_NATIVE_LIFECYCLE_TEST_PID',
    'QUEUEBACK_NATIVE_LIFECYCLE_TEST_FFMPEG',
    'QUEUEBACK_NATIVE_LIFECYCLE_TEST_SECONDS',
    'QUEUEBACK_NATIVE_LIFECYCLE_TEST_OUTPUT'
)
$previousEnvironment = @{}
foreach ($name in $controlledEnvironment) {
    $item = Get-Item "Env:\$name" -ErrorAction SilentlyContinue
    $previousEnvironment[$name] = if ($null -eq $item) { $null } else { $item.Value }
}
$fixtureProcess = $null
try {
    $testBinary = Build-TestBinary -Cargo $cargo -LogPath (Join-Path $ResultRoot 'cargo-build.log')
    $ready = Join-Path $ResultRoot 'fixture.ready'
    $fixtureStdout = Join-Path $ResultRoot 'fixture.stdout.log'
    $fixtureStderr = Join-Path $ResultRoot 'fixture.stderr.log'
    $fixtureDuration = ($DurationSeconds * 2) + ($CooldownSeconds * 2) + 180
    $fixtureProcess = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
        (Join-Path $repo 'tools\native_backend\animated_capture_fixture.ps1'),
        '-DurationSeconds', [string]$fixtureDuration,
        '-ReadyFile', $ready,
        '-Width', '1920', '-Height', '1080', '-Borderless'
    ) -RedirectStandardOutput $fixtureStdout -RedirectStandardError $fixtureStderr `
        -WindowStyle Hidden -PassThru
    $readyDeadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not (Test-Path -LiteralPath $ready -PathType Leaf)) {
        if ($fixtureProcess.HasExited) {
            Stop-WithCode 'FIXTURE-EXIT' 'The animated fixture exited before readiness.'
        }
        if ([DateTime]::UtcNow -ge $readyDeadline) {
            Stop-WithCode 'FIXTURE-TIMEOUT' 'The animated fixture did not become ready within 15 seconds.'
        }
        Start-Sleep -Milliseconds 100
    }
    $readyParts = (Get-Content -Raw -LiteralPath $ready).Trim().Split('|')
    if ($readyParts.Count -ne 2) {
        Stop-WithCode 'FIXTURE-READY' 'The animated fixture readiness file is malformed.'
    }
    $fixturePid = [uint32]$readyParts[0]

    $backends = $Order.Split('-')
    for ($index = 0; $index -lt $backends.Count; $index++) {
        $backend = $backends[$index]
        Write-Host "Running $backend for $DurationSeconds seconds against fixture PID $fixturePid..."
        Run-Arm -Backend $backend -TestBinary $testBinary -FixturePid $fixturePid `
            -RuntimeRoot $MediaRuntimeRoot -Root (Join-Path $ResultRoot $backend)
        if ($index -lt ($backends.Count - 1) -and $CooldownSeconds -gt 0) {
            Write-Host "Cooling down for $CooldownSeconds seconds..."
            Start-Sleep -Seconds $CooldownSeconds
        }
    }

    $ffmpegSummary = Summarize-Arm -ArmRoot (Join-Path $ResultRoot 'ffmpeg') -Backend 'ffmpeg'
    $nativeSummary = Summarize-Arm -ArmRoot (Join-Path $ResultRoot 'native') -Backend 'native'
    Assert-ComparableArm $ffmpegSummary
    Assert-ComparableArm $nativeSummary
    $revision = (& git -C $repo rev-parse HEAD).Trim()
    $dirty = @(& git -C $repo status --short).Count -gt 0
    $summary = [ordered]@{
        schema = 1
        scope = 'preliminary non-League backend A/B; not QB-PERF-002 or M8 acceptance'
        target = 'animated 1920x1080 borderless exact-HWND fixture'
        duration_seconds_per_arm = $DurationSeconds
        order = $Order
        logical_processors = [Environment]::ProcessorCount
        runtime_id = [string]$manifest.runtime_id
        recorder_revision = $revision
        recorder_dirty = $dirty
        ffmpeg = $ffmpegSummary
        native = $nativeSummary
        native_minus_ffmpeg_machine_cpu_percentage_points =
            $nativeSummary.total_machine_cpu_percent - $ffmpegSummary.total_machine_cpu_percent
        decision = 'none; preserve both arms and use League M8 for the migration decision'
    }
    Write-Json -Path (Join-Path $ResultRoot 'comparison-summary.json') -Value $summary
    Write-Host 'CHRONOBREAK_PRELIMINARY_BACKEND_AB=PASS'
    Write-Host "EVIDENCE=$ResultRoot"
    Write-Host ($summary | ConvertTo-Json -Depth 8)
} finally {
    foreach ($name in $controlledEnvironment) {
        if ($null -eq $previousEnvironment[$name]) {
            Remove-Item "Env:\$name" -ErrorAction SilentlyContinue
        } else {
            Set-Item "Env:\$name" $previousEnvironment[$name]
        }
    }
    if ($null -ne $fixtureProcess -and -not $fixtureProcess.HasExited) {
        Stop-Process -Id $fixtureProcess.Id -Force -ErrorAction SilentlyContinue
    }
}
