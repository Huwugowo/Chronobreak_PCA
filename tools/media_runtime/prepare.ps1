[CmdletBinding()]
param(
    [string]$BuildBin,
    [string]$RuntimeRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$lockPath = Join-Path $repository "media-runtime\runtime-lock.json"
$lock = Get-Content -Raw -LiteralPath $lockPath | ConvertFrom-Json

if ([int]$lock.contract_version -ne 3) {
    throw "Unsupported media-runtime lock version $($lock.contract_version)."
}

if ([string]::IsNullOrWhiteSpace($BuildBin)) {
    $BuildBin = Join-Path $repository "build\media-runtime\install\ffmpeg-8.1.2-queueback\bin"
}
$BuildBin = [System.IO.Path]::GetFullPath($BuildBin)
if (-not (Test-Path -LiteralPath $BuildBin -PathType Container)) {
    throw "Pinned QueueBack FFmpeg build output is missing at $BuildBin. Run build_ffmpeg.ps1 explicitly."
}

if ([string]::IsNullOrWhiteSpace($RuntimeRoot)) {
    $RuntimeRoot = Join-Path $repository "build\media-runtime\windows-x86_64"
}
$RuntimeRoot = [System.IO.Path]::GetFullPath($RuntimeRoot)

function Assert-Sha256 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Expected,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($actual -ne $Expected.ToLowerInvariant()) {
        throw "$Label SHA-256 mismatch: expected $Expected, got $actual."
    }
}

$runtimeParent = Split-Path -Parent $RuntimeRoot
New-Item -ItemType Directory -Force -Path $runtimeParent | Out-Null
$workRoot = Join-Path $runtimeParent (".prepare-" + [Guid]::NewGuid().ToString("N"))
$candidateRoot = Join-Path $workRoot "runtime"

try {
    New-Item -ItemType Directory -Force -Path (Join-Path $candidateRoot "bin") | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $candidateRoot "licenses") | Out-Null
    Copy-Item -LiteralPath (Join-Path $BuildBin "ffmpeg.exe") -Destination (Join-Path $candidateRoot "bin\ffmpeg.exe")
    Copy-Item -LiteralPath (Join-Path $BuildBin "ffprobe.exe") -Destination (Join-Path $candidateRoot "bin\ffprobe.exe")
    Copy-Item -LiteralPath (Join-Path $repository "build\media-runtime\source\ffmpeg-8.1.2\COPYING.GPLv3") -Destination (Join-Path $candidateRoot "licenses\COPYING.GPLv3.txt")
    foreach ($notice in @("NVCODEC_LICENSE.txt", "AMF_LICENSE.txt", "LIBVPL_LICENSE.txt", "THIRD_PARTY_NOTICES.md", "SOURCE_AND_BUILD.md")) {
        Copy-Item -LiteralPath (Join-Path $repository "media-runtime\notices\$notice") -Destination (Join-Path $candidateRoot "licenses\$notice")
    }
    Copy-Item -LiteralPath $lockPath -Destination (Join-Path $candidateRoot "runtime-manifest.json")

    foreach ($file in $lock.files) {
        $relative = [string]$file.path
        if ([string]::IsNullOrWhiteSpace($relative) -or $relative.Contains("\") -or $relative.StartsWith("/") -or $relative.Split('/') -contains "..") {
            throw "Unsafe locked path $relative."
        }
        $candidate = Join-Path $candidateRoot ($relative.Replace('/', '\'))
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            throw "Staged runtime is missing $relative."
        }
        $item = Get-Item -LiteralPath $candidate
        if ([int64]$item.Length -ne [int64]$file.size) {
            throw "$relative size mismatch: expected $($file.size), got $($item.Length)."
        }
        Assert-Sha256 -Path $candidate -Expected $file.sha256 -Label $relative
    }
    if (Test-Path -LiteralPath (Join-Path $candidateRoot "bin\ffplay.exe")) {
        throw "ffplay.exe must not be packaged."
    }

    if (Test-Path -LiteralPath $RuntimeRoot) {
        $resolvedRuntime = (Resolve-Path -LiteralPath $RuntimeRoot).Path
        $resolvedParent = (Resolve-Path -LiteralPath $runtimeParent).Path
        if (-not $resolvedRuntime.StartsWith($resolvedParent + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to replace runtime outside $resolvedParent."
        }
        $existingManifestPath = Join-Path $resolvedRuntime "runtime-manifest.json"
        if (-not (Test-Path -LiteralPath $existingManifestPath -PathType Leaf)) {
            throw "Refusing to replace an unrecognized directory at $resolvedRuntime."
        }
        $existingManifest = Get-Content -Raw -LiteralPath $existingManifestPath | ConvertFrom-Json
        $recognizedIds = @(
            "gyan-ffmpeg-8.1.2-essentials-windows-x86_64",
            "queueback-ffmpeg-8.1.2-windows-x86_64-r1",
            "queueback-ffmpeg-8.1.2-windows-x86_64-r2",
            "queueback-ffmpeg-8.1.2-windows-x86_64-r3",
            "queueback-ffmpeg-8.1.2-windows-x86_64-r4",
            "queueback-ffmpeg-8.1.2-windows-x86_64-r5"
        )
        if ($recognizedIds -notcontains [string]$existingManifest.runtime_id) {
            throw "Refusing to replace unrecognized runtime $($existingManifest.runtime_id) at $resolvedRuntime."
        }
        Remove-Item -Recurse -Force -LiteralPath $resolvedRuntime
    }
    Move-Item -LiteralPath $candidateRoot -Destination $RuntimeRoot
}
finally {
    if (Test-Path -LiteralPath $workRoot) {
        Remove-Item -Recurse -Force -LiteralPath $workRoot
    }
}

Write-Output "Prepared $($lock.runtime_id) at $RuntimeRoot"
