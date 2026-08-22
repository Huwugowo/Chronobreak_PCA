[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$lock = Get-Content -Raw -LiteralPath (Join-Path $repository "media-runtime\runtime-lock.json") | ConvertFrom-Json
$goodBuild = Join-Path $repository "build\media-runtime\install\ffmpeg-8.1.2-queueback\bin"
if (-not (Test-Path -LiteralPath (Join-Path $goodBuild "ffmpeg.exe") -PathType Leaf)) {
    throw "Build the pinned QueueBack FFmpeg runtime before running this test."
}

$testRoot = Join-Path $repository ("build\media-runtime\prepare-test-" + [Guid]::NewGuid().ToString("N"))
$badBuild = Join-Path $testRoot "bad-build"
$target = Join-Path $testRoot "runtime"
New-Item -ItemType Directory -Force -Path $testRoot | Out-Null

try {
    Copy-Item -LiteralPath $goodBuild -Destination $badBuild -Recurse
    $badFfmpeg = Join-Path $badBuild "ffmpeg.exe"
    $stream = [System.IO.File]::Open($badFfmpeg, [System.IO.FileMode]::Append, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
    try { $stream.WriteByte(0) } finally { $stream.Dispose() }

    New-Item -ItemType Directory -Force -Path $target | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $target "sentinel.txt"), "must survive bad archive`n", [System.Text.UTF8Encoding]::new($false))
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "prepare.ps1") -BuildBin $badBuild -RuntimeRoot $target 2>&1 | Out-File -Encoding utf8 (Join-Path $testRoot "bad-build.log")
        $badExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorAction
    }
    if ($badExitCode -eq 0) { throw "A corrupt build unexpectedly passed preparation." }
    if (-not (Test-Path -LiteralPath (Join-Path $target "sentinel.txt") -PathType Leaf)) {
        throw "Bad build validation modified the existing target."
    }

    Remove-Item -Recurse -Force -LiteralPath $target
    & (Join-Path $PSScriptRoot "prepare.ps1") -BuildBin $goodBuild -RuntimeRoot $target
    & (Join-Path $PSScriptRoot "verify.ps1") -RuntimeRoot $target
}
finally {
    if (Test-Path -LiteralPath $testRoot) {
        $resolved = (Resolve-Path -LiteralPath $testRoot).Path
        $allowed = [System.IO.Path]::GetFullPath((Join-Path $repository "build\media-runtime"))
        if ($resolved.StartsWith($allowed + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -Recurse -Force -LiteralPath $resolved
        }
    }
}

Write-Output "Media-runtime preparation failure/atomicity tests passed."
