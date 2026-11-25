[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Stage,
    [Parameter(Mandatory)][string]$Version
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$compiler = Join-Path $env:LOCALAPPDATA "tauri\NSIS\Bin\makensis.exe"
if (-not (Test-Path -LiteralPath $compiler -PathType Leaf)) {
    throw "Tauri's NSIS compiler is required before the setup launchers can be bundled."
}
$coordinator = Get-Item -LiteralPath (Join-Path $Stage "chronobreak-setup.exe")
if ($coordinator.Length -gt 16MB -or $Version -cnotmatch '^\d+\.\d+\.\d+$' -or
    $Stage -match '[\$\r\n"%!]' -or $PSScriptRoot -match '[\$\r\n"%!]') {
    throw "Unsupported setup staging inputs."
}
$coordinatorHash = (Get-FileHash -LiteralPath $coordinator.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$definitions = [Collections.Generic.List[string]]::new()
$definitions.Add('!define CHRONOBREAK_PAYLOAD_STAGE "' + $Stage + '"')
$definitions.Add('!define CHRONOBREAK_INSTALLER_SCRIPTS "' + $PSScriptRoot + '"')
$definitions.Add('!define CHRONOBREAK_WINDOWS_VERSION "' + $Version + '"')
$definitions.Add('!define CHRONOBREAK_COORDINATOR_BYTES ' + $coordinator.Length)
$definitions.Add('!define CHRONOBREAK_COORDINATOR_SHA256 "' + $coordinatorHash + '"')
[IO.File]::WriteAllLines((Join-Path $Stage "payload.nsh"), $definitions, [Text.UTF8Encoding]::new($true))
$oldStage = $env:CHRONOBREAK_RELEASE_STAGE
try {
    $env:CHRONOBREAK_RELEASE_STAGE = $Stage
    $arguments = @("/V3", '/WX')
    if ($env:CHRONOBREAK_SETUP_BUILD_FIXTURE -ceq "1") { $arguments += '/DCHRONOBREAK_SETUP_FIXTURE' }
    & $compiler @arguments (Join-Path $PSScriptRoot "uninstall.nsi") | Out-Host
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $Stage "uninstall.exe") -PathType Leaf)) {
        throw "The NSIS uninstaller could not be generated."
    }
} finally {
    $env:CHRONOBREAK_RELEASE_STAGE = $oldStage
}
