[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$RuntimeRoot,
    [string]$ReleaseRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$lock = Get-Content -Raw -LiteralPath (Join-Path $repository "media-runtime\runtime-lock.json") | ConvertFrom-Json

function Assert-Runtime {
    param([Parameter(Mandatory = $true)][string]$Root)

    $Root = [System.IO.Path]::GetFullPath($Root)
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
        throw "Media runtime is missing at $Root."
    }
    $manifestPath = Join-Path $Root "runtime-manifest.json"
    $actualManifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
    if (($actualManifest | ConvertTo-Json -Depth 20 -Compress) -ne ($lock | ConvertTo-Json -Depth 20 -Compress)) {
        throw "$manifestPath does not match the checked-in lock."
    }

    foreach ($file in $lock.files) {
        $path = Join-Path $Root ([string]$file.path).Replace('/', '\')
        $item = Get-Item -LiteralPath $path
        if ([int64]$item.Length -ne [int64]$file.size) {
            throw "$($file.path) size mismatch."
        }
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        if ($hash -ne ([string]$file.sha256).ToLowerInvariant()) {
            throw "$($file.path) SHA-256 mismatch."
        }
    }
    if (Test-Path -LiteralPath (Join-Path $Root "bin\ffplay.exe")) {
        throw "ffplay.exe is not part of the QueueBack runtime contract."
    }

    $ffmpeg = Join-Path $Root "bin\ffmpeg.exe"
    $ffprobe = Join-Path $Root "bin\ffprobe.exe"
    $version = (& $ffmpeg -hide_banner -version 2>&1 | Out-String)
    $probeVersion = (& $ffprobe -hide_banner -version 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0 -or -not $version.Contains("ffmpeg version $($lock.ffmpeg.version_banner)") -or -not $probeVersion.Contains("ffprobe version $($lock.ffmpeg.version_banner)")) {
        throw "Packaged ffmpeg/ffprobe identity check failed."
    }
    foreach ($flag in $lock.ffmpeg.required_configure_flags) {
        if (-not $version.Contains([string]$flag) -or -not $probeVersion.Contains([string]$flag)) {
            throw "Packaged tool configuration is missing $flag."
        }
    }
    $filters = (& $ffmpeg -hide_banner -filters 2>&1 | Out-String)
    $hwaccels = (& $ffmpeg -hide_banner -hwaccels 2>&1 | Out-String)
    $encoders = (& $ffmpeg -hide_banner -encoders 2>&1 | Out-String)
    foreach ($name in $lock.capabilities.filters) {
        if ($filters -notmatch "(?m)(^|\s)$([regex]::Escape([string]$name))(\s|$)") { throw "Missing filter $name." }
    }
    foreach ($name in $lock.capabilities.hwaccels) {
        if ($hwaccels -notmatch "(?m)^$([regex]::Escape([string]$name))\s*$") { throw "Missing hardware acceleration $name." }
    }
    foreach ($name in $lock.capabilities.encoders) {
        if ($encoders -notmatch "(?m)(^|\s)$([regex]::Escape([string]$name))(\s|$)") { throw "Missing encoder $name." }
    }
    Write-Output "Verified $($lock.runtime_id) at $Root"
}

Assert-Runtime -Root $RuntimeRoot
if (-not [string]::IsNullOrWhiteSpace($ReleaseRoot)) {
    $resolvedRelease = [System.IO.Path]::GetFullPath($ReleaseRoot)
    foreach ($executable in @("recorder.exe", "league-replay-app.exe")) {
        $path = Join-Path $resolvedRelease $executable
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Portable release is missing $path." }
    }
    $releaseRuntime = Join-Path $resolvedRelease "resources\media-runtime"
    Assert-Runtime -Root $releaseRuntime
}
