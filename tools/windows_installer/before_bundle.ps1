[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$stage = $env:CHRONOBREAK_RELEASE_STAGE
$revision = $env:CHRONOBREAK_RELEASE_REVISION
$version = $env:CHRONOBREAK_RELEASE_VERSION
$target = "x86_64-pc-windows-msvc"
if ($revision -cnotmatch '^[0-9a-f]{40}$' -or $version -cnotmatch '^\d+\.\d+\.\d+$' -or
    $env:CHRONOBREAK_SETUP_BUILD_FIXTURE -ceq "1" -or
    [string]::IsNullOrWhiteSpace($stage) -or -not [IO.Path]::IsPathRooted($stage) -or
    -not (Test-Path -LiteralPath (Join-Path $stage ".chronobreak-candidate") -PathType Leaf)) {
    throw "Start the Windows candidate build with tools/windows_installer/build_candidate.ps1."
}
$buildRoot = (Resolve-Path -LiteralPath (Join-Path $repository "build")).Path
$stage = (Resolve-Path -LiteralPath $stage).Path
$candidateParent = Join-Path $buildRoot "windows-installer"
if ((Split-Path -Parent $stage) -ne $candidateParent -or
    (Get-Item -LiteralPath $stage -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw "The candidate directory must be an ordinary directory under the workspace build folder."
}
$cargo = (Get-Command cargo.exe -ErrorAction Stop).Source
$dumpbin = $env:CHRONOBREAK_RELEASE_DUMPBIN
$app = Join-Path $repository "app\src-tauri\target\$target\release\league-replay-app.exe"
if (-not (Test-Path -LiteralPath $app -PathType Leaf)) { throw "Tauri did not produce the expected Windows x64 desktop." }

function Assert-Revision {
    $actual = & git -C $repository rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $actual -cne $revision) { throw "Source revision changed during the build." }
    $changes = & git -C $repository status --porcelain --untracked-files=normal
    if ($LASTEXITCODE -ne 0 -or $changes) { throw "The candidate requires the same clean source revision throughout the build." }
}

function Assert-NativeImports([string]$Path) {
    $headers = & $dumpbin /NOLOGO /HEADERS $Path | Out-String
    if ($LASTEXITCODE -ne 0 -or $headers -notmatch '(?im)^\s*8664 machine \(x64\)') {
        throw "Expected a Windows x64 executable: $Path"
    }
    $imports = & $dumpbin /NOLOGO /IMPORTS $Path | Out-String
    if ($LASTEXITCODE -ne 0) { throw "Could not inspect imports: $Path" }
    $imports | Set-Content -LiteralPath (Join-Path $stage ("imports-" + (Split-Path -Leaf $Path) + ".txt"))
    $dlls = [regex]::Matches($imports, '(?im)^\s+([\w.-]+\.dll)\s*$')
    if ($dlls.Count -eq 0) { throw "Import inspection found no DLLs: $Path" }
    foreach ($dll in $dlls) {
        $name = $dll.Groups[1].Value
        if ($name -match '^(api-ms-win-|ext-ms-win-)') { continue }
        if ($name -match '^(vcruntime|msvcp|msvcr1|concrt|vcomp)' -or
            -not (Test-Path -LiteralPath (Join-Path "$env:SystemRoot\System32" $name) -PathType Leaf)) {
            throw "Unstaged dependency $name in $Path"
        }
    }
}

function Assert-Identity([string]$Path) {
    $receipt = Join-Path $stage ((Split-Path -Leaf $Path) + ".identity")
    $process = Start-Process -FilePath $Path -ArgumentList @("--release-identity", ('"' + $receipt + '"')) -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Executable identity command failed: $Path" }
    $lines = @(Get-Content -LiteralPath $receipt)
    if ($lines.Count -ne 2 -or $lines[0] -cne $revision -or $lines[1] -cne $version) {
        throw "Executable source revision or version mismatch: $Path"
    }
}

function Write-Json([string]$Path, $Value) {
    [IO.File]::WriteAllText($Path, (ConvertTo-Json -InputObject $Value -Depth 8), [Text.UTF8Encoding]::new($false))
}

function Get-Sha256([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

Assert-Revision
$env:CARGO_ENCODED_RUSTFLAGS = "-Ctarget-feature=+crt-static"
$env:CARGO_TARGET_DIR = Join-Path $repository "build\windows-installer\native-target"
$products = Join-Path $env:CARGO_TARGET_DIR "$target\release"
& $cargo build --release --locked --target $target --manifest-path (Join-Path $repository "recorder\Cargo.toml")
if ($LASTEXITCODE -ne 0) { throw "Windows recorder build failed." }
$recorder = Join-Path $products "recorder.exe"
Assert-NativeImports $app
Assert-NativeImports $recorder
Assert-Identity $app
Assert-Identity $recorder

$payload = Join-Path $stage "payload"
$runtime = Join-Path $repository "build\media-runtime\windows-x86_64"
if (Test-Path -LiteralPath $payload) { throw "The candidate payload directory must be new." }
# The signed release will sign leaf executables before this staging and hashing step.
$shortStage = Join-Path $buildRoot ("payload-" + [Guid]::NewGuid().ToString("N").Substring(0, 12))
& (Join-Path $repository "tools\media_runtime\stage_release.ps1") -RuntimeRoot $runtime -ReleaseRoot $shortStage -RecorderPath $recorder -AppPath $app
$resolvedShortStage = (Resolve-Path -LiteralPath $shortStage).Path
if ((Split-Path -Parent $resolvedShortStage) -ne $buildRoot -or
    (Get-Item -LiteralPath $resolvedShortStage -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw "Unexpected payload staging directory."
}
Move-Item -LiteralPath $resolvedShortStage -Destination $payload
Remove-Item -LiteralPath (Join-Path $payload ".queueback-portable-release")
$files = [ordered]@{}
foreach ($entry in (Get-ChildItem -LiteralPath $payload -Recurse -Force -File | Sort-Object FullName)) {
    if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Redirected payload member." }
    $key = $entry.FullName.Substring($payload.Length + 1).Replace('\', '/')
    $files[$key] = [ordered]@{ bytes = $entry.Length; sha256 = Get-Sha256 $entry.FullName }
}
$runtimeId = (Get-Content -LiteralPath (Join-Path $payload "resources\media-runtime\runtime-manifest.json") -Raw | ConvertFrom-Json).runtime_id
$manifest = Join-Path $payload "payload.json"
Write-Json $manifest ([ordered]@{ schema = 1; revision = $revision; version = $version; runtime = $runtimeId; files = $files })

$env:CHRONOBREAK_PAYLOAD_SHA256 = Get-Sha256 $manifest
$coordinatorManifest = Join-Path $PSScriptRoot "coordinator\Cargo.toml"
& $cargo build --release --locked --target $target --manifest-path $coordinatorManifest
if ($LASTEXITCODE -ne 0) { throw "Setup coordinator build failed." }
$coordinator = Join-Path $stage "chronobreak-setup.exe"
Copy-Item -LiteralPath (Join-Path $products "chronobreak-setup.exe") -Destination $coordinator
Assert-NativeImports $coordinator
& $coordinator verify-payload $payload
if ($LASTEXITCODE -ne 0) { throw "Setup coordinator rejected its payload." }
# The signed release will sign the bound coordinator before recording this envelope.
# Generate the uninstaller from that final coordinator and sign it before hashing
# the envelope. The outer installer is signed only after it has been compiled.
& (Join-Path $PSScriptRoot "prepare_launcher.ps1") -Stage $stage -Version $version
Write-Json (Join-Path $stage "envelope.json") ([ordered]@{
    schema = 1; signing = "unsigned-local"; payload_sha256 = $env:CHRONOBREAK_PAYLOAD_SHA256
    coordinator_sha256 = Get-Sha256 $coordinator
    uninstaller_sha256 = Get-Sha256 (Join-Path $stage "uninstall.exe")
})
Assert-Revision
