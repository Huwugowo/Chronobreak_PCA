[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if (-not (Get-Command pwsh.exe -ErrorAction SilentlyContinue)) {
    throw "PowerShell 7 is required to build the Windows candidate."
}

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$revision = & git -C $repository rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $revision -cnotmatch '^[0-9a-f]{40}$') { throw "Could not find the current source revision." }
$changes = & git -C $repository status --porcelain --untracked-files=normal
if ($LASTEXITCODE -ne 0 -or $changes) { throw "Build the unsigned candidate from a clean checkout." }
$version = (Get-Content -LiteralPath (Join-Path $repository "app\src-tauri\tauri.windows.conf.json") -Raw | ConvertFrom-Json).version
if ($version -cnotmatch '^\d+\.\d+\.\d+$' -or [version]$version -le [version]"0.1.0") {
    throw "The Windows candidate version must be higher than the historical 0.1.0 installer."
}

$dumpbin = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
if ($null -eq $dumpbin) {
    $vswhere = Join-Path "${env:ProgramFiles(x86)}" "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { throw "MSVC dumpbin.exe was not found." }
    $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $vs) { throw "MSVC x64 build tools were not found." }
    $dumpbinPath = Get-ChildItem -LiteralPath (Join-Path $vs "VC\Tools\MSVC") -Directory |
        Sort-Object Name -Descending | ForEach-Object { Join-Path $_.FullName "bin\Hostx64\x64\dumpbin.exe" } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
} else {
    $dumpbinPath = $dumpbin.Source
}
if (-not $dumpbinPath) { throw "MSVC x64 dumpbin.exe was not found." }

$stage = Join-Path $repository ("build\windows-installer\candidate-" + $revision.Substring(0, 12) + "-" + [Guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Path $stage | Out-Null
New-Item -ItemType File -Path (Join-Path $stage ".chronobreak-candidate") | Out-Null
$variables = @{
    CHRONOBREAK_RELEASE_STAGE = $stage
    CHRONOBREAK_RELEASE_REVISION = $revision
    CHRONOBREAK_RELEASE_VERSION = $version
    CHRONOBREAK_RELEASE_DUMPBIN = $dumpbinPath
}
$previous = @{}
foreach ($name in $variables.Keys) {
    $previous[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
    [Environment]::SetEnvironmentVariable($name, $variables[$name], "Process")
}

try {
    $target = "x86_64-pc-windows-msvc"
    if ($env:CARGO_TARGET_DIR) { throw "Unset CARGO_TARGET_DIR for a candidate build." }
    Push-Location (Join-Path $repository "app")
    try {
        $tauri = Join-Path (Get-Location).Path "node_modules\.bin\tauri.cmd"
        & $tauri build --ci --no-sign --target $target --bundles nsis -- --locked
        if ($LASTEXITCODE -ne 0) { throw "Unsigned Windows candidate build failed." }
    } finally {
        Pop-Location
    }

    $bundle = Join-Path $repository "app\src-tauri\target\$target\release\bundle\nsis\Chronobreak_${version}_x64-setup.exe"
    if (-not (Test-Path -LiteralPath $bundle -PathType Leaf)) { throw "NSIS candidate output was not found." }
    if ((Get-AuthenticodeSignature -LiteralPath $bundle).Status -ne "NotSigned") { throw "Expected an unsigned local installer." }
    $setup = Join-Path $stage "Chronobreak-$version-x64-local-setup.exe"
    Copy-Item -LiteralPath $bundle -Destination $setup
    & (Join-Path $PSScriptRoot "verify_candidate.ps1") -CandidateRoot $stage -Installer $setup
    $hash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$setup.sha256", "$hash  $(Split-Path -Leaf $setup)`n")
    Write-Output $setup
} finally {
    foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name, $previous[$name], "Process") }
}
