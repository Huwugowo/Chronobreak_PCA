[CmdletBinding()]
param(
    [string]$PresentMonPath = (Join-Path $env:USERPROFILE "Downloads\PresentMon-2.5.1-x64.exe"),
    [string]$LeagueConfigPath = (Join-Path $env:SystemDrive "Riot Games\League of Legends\Config\game.cfg"),
    [string]$RecorderSourceConfig = (Join-Path $env:APPDATA "LeagueReplay\config\config.toml"),
    [string]$RecorderPath,
    [ValidateSet("nvenc", "amf", "qsv")]
    [string]$TargetEncoder = "nvenc",
    [string]$MediaRuntimeRoot,
    [string]$FfmpegPath,
    [string]$FfprobePath,
    [string]$ResultRoot,
    [switch]$PublishResults
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$runner = Join-Path $PSScriptRoot "run_qb_perf_001.ps1"
$forward = @{}
foreach ($key in $PSBoundParameters.Keys) {
    $forward[$key] = $PSBoundParameters[$key]
}
$forward["ManifestSchemaVersion"] = "2"
$forward["TargetEncoder"] = $TargetEncoder

& $runner @forward
exit $LASTEXITCODE
