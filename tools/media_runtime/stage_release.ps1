[CmdletBinding()]
param(
    [string]$RuntimeRoot,
    [string]$ReleaseRoot,
    [string]$RecorderPath,
    [string]$AppPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
if ([string]::IsNullOrWhiteSpace($RuntimeRoot)) { $RuntimeRoot = Join-Path $repository "build\media-runtime\windows-x86_64" }
if ([string]::IsNullOrWhiteSpace($ReleaseRoot)) { $ReleaseRoot = Join-Path $repository "build\release\queueback" }
if ([string]::IsNullOrWhiteSpace($RecorderPath)) { $RecorderPath = Join-Path $repository "recorder\target\release\recorder.exe" }
if ([string]::IsNullOrWhiteSpace($AppPath)) { $AppPath = Join-Path $repository "app\src-tauri\target\release\league-replay-app.exe" }

$RuntimeRoot = [System.IO.Path]::GetFullPath($RuntimeRoot)
$ReleaseRoot = [System.IO.Path]::GetFullPath($ReleaseRoot)
foreach ($required in @($RuntimeRoot, $RecorderPath, $AppPath)) {
    if (-not (Test-Path -LiteralPath $required)) { throw "Required release input is missing: $required" }
}

$releaseParent = Split-Path -Parent $ReleaseRoot
New-Item -ItemType Directory -Force -Path $releaseParent | Out-Null
$candidate = Join-Path $releaseParent (".queueback-release-" + [Guid]::NewGuid().ToString("N"))
try {
    New-Item -ItemType Directory -Force -Path (Join-Path $candidate "resources") | Out-Null
    Copy-Item -LiteralPath $RecorderPath -Destination (Join-Path $candidate "recorder.exe")
    Copy-Item -LiteralPath $AppPath -Destination (Join-Path $candidate "league-replay-app.exe")
    Copy-Item -Recurse -LiteralPath $RuntimeRoot -Destination (Join-Path $candidate "resources\media-runtime")
    [System.IO.File]::WriteAllText((Join-Path $candidate ".queueback-portable-release"), "QueueBack portable release layout.`n", [System.Text.UTF8Encoding]::new($false))
    & (Join-Path $PSScriptRoot "verify.ps1") -RuntimeRoot $RuntimeRoot -ReleaseRoot $candidate
    if ($LASTEXITCODE -ne 0) { throw "Portable release verification failed." }
    if (Test-Path -LiteralPath $ReleaseRoot) {
        $resolvedRelease = (Resolve-Path -LiteralPath $ReleaseRoot).Path
        $resolvedParent = (Resolve-Path -LiteralPath $releaseParent).Path
        if (-not $resolvedRelease.StartsWith($resolvedParent + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to replace release outside $resolvedParent."
        }
        if (-not (Test-Path -LiteralPath (Join-Path $resolvedRelease ".queueback-portable-release") -PathType Leaf)) {
            throw "Refusing to replace an unrecognized directory at $resolvedRelease."
        }
        Remove-Item -Recurse -Force -LiteralPath $resolvedRelease
    }
    Move-Item -LiteralPath $candidate -Destination $ReleaseRoot
}
finally {
    if (Test-Path -LiteralPath $candidate) { Remove-Item -Recurse -Force -LiteralPath $candidate }
}
Write-Output "Staged portable QueueBack release at $ReleaseRoot"

