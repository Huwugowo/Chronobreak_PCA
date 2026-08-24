[CmdletBinding()]
param(
    [switch]$Acquire,
    [ValidateRange(1, 64)][int]$Jobs = 8,
    [string]$BuildRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$lock = Get-Content -Raw -LiteralPath (Join-Path $repository "media-runtime\runtime-lock.json") | ConvertFrom-Json
if ([int]$lock.contract_version -ne 3) {
    throw "Unsupported media-runtime lock version $($lock.contract_version)."
}
if ([string]::IsNullOrWhiteSpace($BuildRoot)) {
    $BuildRoot = Join-Path $repository "build\media-runtime"
}
$BuildRoot = [System.IO.Path]::GetFullPath($BuildRoot)
$allowedRoot = [System.IO.Path]::GetFullPath((Join-Path $repository "build\media-runtime"))
if (-not ($BuildRoot -eq $allowedRoot -or $BuildRoot.StartsWith($allowedRoot + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase))) {
    throw "BuildRoot must stay inside $allowedRoot."
}

$sourceRoot = Join-Path $BuildRoot "source"
$prefix = Join-Path $BuildRoot "prefix"
$ffmpegBuild = Join-Path $BuildRoot "build\ffmpeg-8.1.2-queueback"
$installRoot = Join-Path $BuildRoot "install\ffmpeg-8.1.2-queueback"
New-Item -ItemType Directory -Force -Path $sourceRoot, $prefix, $ffmpegBuild, $installRoot | Out-Null

$sourceDirectories = [ordered]@{
    "ffmpeg" = "ffmpeg-8.1.2"
    "nv-codec-headers" = "nv-codec-headers-12.2.72.0"
    "amf" = "amf-1.4.36"
}
if (@($lock.sources).Count -ne $sourceDirectories.Count) {
    throw "The runtime lock and build script source directory map disagree."
}
$sources = foreach ($source in $lock.sources) {
    $directory = $sourceDirectories[[string]$source.name]
    if ([string]::IsNullOrWhiteSpace($directory)) {
        throw "No build directory is defined for locked source $($source.name)."
    }
    [PSCustomObject]@{
        Name = $directory
        Url = [string]$source.url
        Tag = [string]$source.tag
        Commit = ([string]$source.commit).ToLowerInvariant()
    }
}

foreach ($source in $sources) {
    $path = Join-Path $sourceRoot $source.Name
    if (-not (Test-Path -LiteralPath $path -PathType Container)) {
        if (-not $Acquire) {
            throw "Pinned source $($source.Name) is missing. Re-run explicitly with -Acquire."
        }
        & git clone --depth 1 --branch $source.Tag $source.Url $path
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to acquire $($source.Name)."
        }
    }
    $actual = (& git -C $path rev-parse HEAD).Trim().ToLowerInvariant()
    if ($LASTEXITCODE -ne 0 -or $actual -ne $source.Commit) {
        throw "$($source.Name) is not the pinned commit $($source.Commit); found $actual."
    }
}

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

$ffmpegSource = Join-Path $sourceRoot $sourceDirectories["ffmpeg"]
foreach ($patch in $lock.patches) {
    $relative = [string]$patch.path
    if ([string]$patch.applies_to -ne "ffmpeg") {
        throw "Unsupported patch target $($patch.applies_to) for $relative."
    }
    if ([string]::IsNullOrWhiteSpace($relative) -or $relative.Contains("\") -or $relative.StartsWith("/") -or $relative.Split('/') -contains "..") {
        throw "Unsafe locked patch path $relative."
    }
    $patchPath = Join-Path $repository ($relative.Replace('/', '\'))
    if (-not (Test-Path -LiteralPath $patchPath -PathType Leaf)) {
        throw "Locked source patch is missing at $patchPath."
    }
    Assert-Sha256 -Path $patchPath -Expected ([string]$patch.sha256) -Label $relative

    $sourceStatus = @(& git -C $ffmpegSource status --short --untracked-files=all)
    if ($LASTEXITCODE -ne 0) {
        throw "Could not inspect the pinned FFmpeg source tree."
    }
    if ($sourceStatus.Count -gt 0) {
        & git -C $ffmpegSource apply --reverse --check $patchPath
        if ($LASTEXITCODE -ne 0) {
            throw "Pinned FFmpeg source contains changes other than the exact locked patch $relative."
        }
        & git -C $ffmpegSource apply --reverse $patchPath
        if ($LASTEXITCODE -ne 0) {
            throw "Could not temporarily reverse locked patch $relative."
        }
        $remainingStatus = @(& git -C $ffmpegSource status --short --untracked-files=all)
        if ($LASTEXITCODE -ne 0 -or $remainingStatus.Count -gt 0) {
            & git -C $ffmpegSource apply $patchPath
            throw "Pinned FFmpeg source is dirty beyond the exact locked patch $relative."
        }
    }

    & git -C $ffmpegSource apply --check $patchPath
    if ($LASTEXITCODE -ne 0) {
        throw "Locked patch $relative does not apply to the pinned FFmpeg source."
    }
    & git -C $ffmpegSource apply $patchPath
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to apply locked patch $relative."
    }
}

$msysRoot = "C:\msys64"
$bash = Join-Path $msysRoot "usr\bin\bash.exe"
$pacman = Join-Path $msysRoot "usr\bin\pacman.exe"
if (-not (Test-Path -LiteralPath $bash -PathType Leaf) -or -not (Test-Path -LiteralPath $pacman -PathType Leaf)) {
    throw "MSYS2 UCRT64 is required at $msysRoot."
}

$requiredPackages = [ordered]@{}
foreach ($package in $lock.toolchain) {
    $name = [string]$package.package
    if ($requiredPackages.Contains($name)) {
        throw "Duplicate toolchain package in runtime lock: $name."
    }
    $requiredPackages.Add($name, [string]$package.version)
}
foreach ($entry in $requiredPackages.GetEnumerator()) {
    $line = (& $pacman -Q $entry.Key 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $line -ne "$($entry.Key) $($entry.Value)") {
        throw "Build package mismatch for $($entry.Key): expected $($entry.Value), found $line."
    }
}

function ConvertTo-MsysPath([string]$Path) {
    $full = [System.IO.Path]::GetFullPath($Path).Replace('\', '/')
    if ($full -notmatch '^([A-Za-z]):/(.*)$') {
        throw "Cannot convert path to MSYS form: $Path"
    }
    return "/$($Matches[1].ToLowerInvariant())/$($Matches[2])"
}

$nvSource = Join-Path $sourceRoot "nv-codec-headers-12.2.72.0"
$amfInclude = Join-Path $sourceRoot "amf-1.4.36\amf\public\include"
$amfTarget = Join-Path $prefix "include\AMF"
New-Item -ItemType Directory -Force -Path $amfTarget | Out-Null
Copy-Item -LiteralPath (Join-Path $amfInclude "core") -Destination $amfTarget -Recurse -Force
Copy-Item -LiteralPath (Join-Path $amfInclude "components") -Destination $amfTarget -Recurse -Force

$ffmpegSourceMsys = ConvertTo-MsysPath (Join-Path $sourceRoot "ffmpeg-8.1.2")
$nvSourceMsys = ConvertTo-MsysPath $nvSource
$prefixMsys = ConvertTo-MsysPath $prefix
$buildMsys = ConvertTo-MsysPath $ffmpegBuild
$installMsys = ConvertTo-MsysPath $installRoot

$configureArguments = @(
    "--prefix=$installMsys",
    "--extra-version=queueback-6-captureabi1-nvcodec12.2-amf1.4.36",
    "--pkg-config-flags=--static",
    "--extra-cflags=-I$prefixMsys/include",
    "--extra-ldflags=-static",
    "--extra-libs=-lstdc++",
    "--enable-gpl", "--enable-version3", "--enable-static", "--disable-shared",
    "--disable-debug", "--disable-doc", "--disable-ffplay", "--disable-autodetect",
    "--enable-libx264", "--enable-libvpl", "--enable-amf",
    "--enable-d3d11va", "--enable-dxva2", "--enable-mediafoundation",
    "--enable-ffnvcodec", "--enable-nvenc",
    "--enable-filter=gfxcapture", "--enable-filter=scale_d3d11",
    "--disable-filter=amf_capture", "--enable-indev=dshow"
) -join " "

$bashScript = @"
set -e
export PATH=/ucrt64/bin:/usr/bin
cd '$nvSourceMsys'
make PREFIX='$prefixMsys' install
export PKG_CONFIG_PATH='$prefixMsys/lib/pkgconfig:/ucrt64/lib/pkgconfig'
cd '$buildMsys'
'$ffmpegSourceMsys/configure' $configureArguments
make -j$Jobs
make install
"@

$env:MSYSTEM = "UCRT64"
$env:CHERE_INVOKING = "1"
& $bash -lc $bashScript
if ($LASTEXITCODE -ne 0) {
    throw "FFmpeg build failed with exit code $LASTEXITCODE."
}

$ffmpeg = Join-Path $installRoot "bin\ffmpeg.exe"
$ffprobe = Join-Path $installRoot "bin\ffprobe.exe"
if (-not (Test-Path -LiteralPath $ffmpeg -PathType Leaf) -or -not (Test-Path -LiteralPath $ffprobe -PathType Leaf)) {
    throw "Build completed without ffmpeg.exe and ffprobe.exe."
}

Write-Output "Built QueueBack FFmpeg runtime at $installRoot"
