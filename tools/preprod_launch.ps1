[CmdletBinding()]
param([switch]$Dev, [switch]$Reset, [switch]$PrepareOnly)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$owned = [IO.Path]::GetFullPath((Join-Path $repo 'build/preprod'))
$source = Join-Path $repo 'fixtures/preprod-library/library'
$sentinel = Join-Path $owned '.chronobreak-preprod.json'

function Assert-NoLinks([string]$Path) {
    $current = $Path
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse path refused: $current" }
        }
        $current = [IO.Path]::GetDirectoryName($current)
    }
}
function Assert-Tree([string]$Path) {
    Assert-NoLinks $Path
    if (Test-Path -LiteralPath $Path -PathType Container) {
        foreach ($item in Get-ChildItem -LiteralPath $Path -Force) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse entry refused: $($item.FullName)" }
            if ($item.PSIsContainer) { Assert-Tree $item.FullName }
        }
    }
}

Assert-NoLinks $owned
Assert-Tree $source
if (!(Test-Path -LiteralPath (Join-Path $source 'games/1792431584/video.mp4'))) { throw 'Prepared Swiftplay fixture is missing.' }
if (Test-Path -LiteralPath $owned) {
    Assert-Tree $owned
    if (!(Test-Path -LiteralPath $sentinel -PathType Leaf)) { throw 'Existing preprod directory is unowned; refusing to modify it.' }
    $owner = Get-Content -LiteralPath $sentinel -Raw | ConvertFrom-Json
    if ($owner.format_version -ne 1 -or $owner.root -cne $owned) { throw 'Preprod ownership does not match.' }
} else {
    New-Item -ItemType Directory -Path $owned -Force | Out-Null
    @{format_version=1;root=$owned} | ConvertTo-Json | Set-Content -LiteralPath $sentinel -Encoding UTF8
    $Reset = $true
}

# Keep one launcher alive for the entire app session. FileShare.None also blocks reset.
$session = $null
$oldEnvironment = @{}
try {
    $session = [IO.File]::Open((Join-Path $owned '.session.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    if (Get-Process -Name 'league-replay-app' -ErrorAction SilentlyContinue) { throw 'Close the desktop app before launching or resetting preprod.' }
    if ($Reset) {
        # Conservatively refuse reset even if a previous launcher was interrupted.
        if (Get-Process -Name 'league-replay-app' -ErrorAction SilentlyContinue) { throw 'Close the desktop app before resetting preprod.' }
        foreach ($name in @('library','app-data','webview2')) {
            $target = [IO.Path]::GetFullPath((Join-Path $owned $name))
            if (![string]::Equals([IO.Path]::GetDirectoryName($target), $owned, [StringComparison]::OrdinalIgnoreCase)) { throw 'Reset target escaped owned root.' }
            Assert-Tree $target
            if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $owned 'library') -Recurse
        $seed = Join-Path $repo 'fixtures/preprod-library/app-data'
        if (Test-Path -LiteralPath $seed) { Assert-Tree $seed; Copy-Item -LiteralPath $seed -Destination (Join-Path $owned 'app-data') -Recurse }
        $output = (Join-Path $owned 'library').Replace('\','/')
        $configText = @"
[recording]
profile = "auto"
codec = "auto"
[storage]
output_path = "$output"
auto_delete_days = 0
[app]
autostart = false
hevc_playback_supported = false
"@
        [IO.File]::WriteAllText((Join-Path $owned 'config.toml'), $configText, (New-Object Text.UTF8Encoding($false)))
    }
    $configPath = Join-Path $owned 'config.toml'
    $config = Get-Content -LiteralPath $configPath -Raw
    $expectedOutput = (Join-Path $owned 'library').Replace('\','/')
    if ($config -notmatch ('(?m)^output_path = "' + [regex]::Escape($expectedOutput) + '"\r?$') -or $config -notmatch '(?m)^auto_delete_days = 0\r?$') {
        throw 'Preprod config changed its output/retention; use -Reset to restore the owned configuration.'
    }
    if ($PrepareOnly) { Write-Host "Prepared preprod: $owned"; return }
    Push-Location $repo
    try {
        if ((& git branch --show-current) -ne 'main') { throw 'Launch preprod from the integrated main checkout.' }
        $values = @{
            LEAGUE_REPLAY_CONFIG=$configPath
            LEAGUE_REPLAY_OUTPUT_PATH=(Join-Path $owned 'library')
            LEAGUE_REPLAY_APP_DATA_DIR=(Join-Path $owned 'app-data')
            WEBVIEW2_USER_DATA_FOLDER=(Join-Path $owned 'webview2')
            QUEUEBACK_MEDIA_RUNTIME_DIR=(Join-Path $repo 'build/media-runtime/windows-x86_64')
            VITE_UI_MOCKS=$null
        }
        foreach ($key in $values.Keys) {
            $oldEnvironment[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
            [Environment]::SetEnvironmentVariable($key, $values[$key], 'Process')
        }
        if ($Dev) {
            & npm.cmd run tauri --prefix app -- dev
            if ($LASTEXITCODE -ne 0) { throw 'Desktop dev launch failed.' }
        } else {
            & npm.cmd run desktop:build --prefix app
            if ($LASTEXITCODE -ne 0) { throw 'Fresh desktop build failed; old executable was not launched.' }
            $app = Start-Process -FilePath (Join-Path $repo 'app/src-tauri/target/release/league-replay-app.exe') -WindowStyle Normal -PassThru
            $app.WaitForExit()
            if ($app.ExitCode -ne 0) { throw "Desktop app exited with $($app.ExitCode)." }
        }
    } finally { Pop-Location }
} finally {
    foreach ($key in $oldEnvironment.Keys) { [Environment]::SetEnvironmentVariable($key, $oldEnvironment[$key], 'Process') }
    if ($session) { $session.Dispose() }
}
