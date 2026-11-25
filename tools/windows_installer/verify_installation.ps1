[CmdletBinding()]
param([string]$ArchivedBeta, [string]$ProductionPayload)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$build = Join-Path $repo 'build\windows-installer'
$working = Join-Path $build ('verify-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $working | Out-Null
$env:CARGO_TARGET_DIR = Join-Path $repo 'build\windows-installer\fixture-target'
$env:CHRONOBREAK_SETUP_BUILD_FIXTURE = '1'
$compiler = Join-Path $env:LOCALAPPDATA 'tauri\NSIS\Bin\makensis.exe'
$cargo = (Get-Command cargo.exe).Source
$template = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'candidate.nsi') -Raw
$legacy = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'coordinator\src\legacy-0.1.0.json') -Raw | ConvertFrom-Json

function Hash([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}
function Write-Text([string]$Path, [string]$Value) {
    [IO.File]::WriteAllText($Path, $Value)
}
function Assert([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function New-Setup([string]$Version, [string]$SourcePayload = '') {
    $stage = Join-Path $working ('package-' + $Version)
    $payload = Join-Path $stage 'payload'
    $manifest = Join-Path $payload 'payload.json'
    if ($SourcePayload) {
        [IO.Directory]::CreateDirectory($stage) | Out-Null
        Copy-Item -LiteralPath $SourcePayload -Destination $payload -Recurse
    } else {
        $members = [ordered]@{}
        foreach ($name in (@($legacy.PSObject.Properties.Name | Where-Object { $_ -cne 'uninstall.exe' }) + 'recorder.exe')) {
            $path = Join-Path $payload $name
            [IO.Directory]::CreateDirectory((Split-Path -Parent $path)) | Out-Null
            Write-Text $path "Disposable fixture $Version $name"
            $members[$name] = @{ bytes = (Get-Item -LiteralPath $path).Length; sha256 = Hash $path }
        }
        Write-Text $manifest (ConvertTo-Json @{ schema = 1; revision = 'fixture'; version = $Version; runtime = 'fixture'; files = $members } -Depth 5)
    }
    $env:CHRONOBREAK_PAYLOAD_SHA256 = Hash $manifest
    & $cargo build --locked --offline --manifest-path (Join-Path $PSScriptRoot 'coordinator\Cargo.toml') --features fixture | Out-Host
    Assert ($LASTEXITCODE -eq 0) 'Fixture helper did not build.'
    $helper = Join-Path $stage 'chronobreak-setup.exe'
    Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'debug\chronobreak-setup.exe') -Destination $helper
    & (Join-Path $PSScriptRoot 'prepare_launcher.ps1') -Stage $stage -Version $Version
    Write-Text (Join-Path $stage 'envelope.json') (ConvertTo-Json @{
        schema = 1; signing = 'unsigned-local'; payload_sha256 = $env:CHRONOBREAK_PAYLOAD_SHA256
        coordinator_sha256 = Hash $helper; uninstaller_sha256 = Hash (Join-Path $stage 'uninstall.exe')
    })
    $env:CHRONOBREAK_RELEASE_STAGE = $stage
    $setup = Join-Path $stage 'Chronobreak-fixture-setup.exe'
    $script = Join-Path $stage 'setup.nsi'
    Write-Text $script $template.Replace('{{product_name}}', 'Chronobreak').Replace('{{version_with_build}}', "$Version.0").Replace('{{version}}', $Version).Replace('{{out_file}}', $setup)
    & $compiler /V2 /WX /DCHRONOBREAK_SETUP_FIXTURE $script | Out-Host
    Assert ($LASTEXITCODE -eq 0) 'Fixture installer did not build.'
    & (Join-Path $PSScriptRoot 'verify_candidate.ps1') -CandidateRoot $stage -Installer $setup | Out-Host
    return @{ Setup = $setup; Helper = $helper; Manifest = Hash $manifest; Stage = $stage; Script = $script }
}
function New-Fixture {
    $name = 'chronobreak-fixture-' + [guid]::NewGuid().ToString('N')
    $root = [IO.Directory]::CreateDirectory((Join-Path $build $name)).FullName
    Add-Content -LiteralPath (Join-Path $working 'fixture-paths.txt') -Value $root
    Write-Text (Join-Path $root '.setup-fixture') 'Chronobreak disposable test fixture'
    Write-Text (Join-Path $root 'config.toml') 'Config sentinel'
    Write-Text (Join-Path $root 'outside.clip.mp4') 'Outside clip sentinel'
    return @{
        Root = $root; Stable = Join-Path $root 'League Replay'; State = Join-Path $root '.Chronobreak-setup'
        Key = "Registry::HKEY_CURRENT_USER\Software\ChronobreakSetupFixture\$name"
    }
}
function New-ArchivedBetaFixture([string]$Source) {
    $beta = New-Fixture
    foreach ($entry in $legacy.PSObject.Properties) {
        $from = Join-Path $Source $entry.Name
        Assert ((Hash $from) -ceq $entry.Value.sha256) "Incorrect archived beta member $($entry.Name)."
        $to = Join-Path $beta.Stable $entry.Name
        [IO.Directory]::CreateDirectory((Split-Path -Parent $to)) | Out-Null
        Copy-Item -LiteralPath $from -Destination $to
    }
    New-Item -Path ($beta.Key + '\Legacy') -Force | Out-Null
    Set-Item -LiteralPath ($beta.Key + '\Legacy') -Value $beta.Stable
    $arp = $beta.Key + '\Uninstall'
    New-Item -Path $arp -Force | Out-Null
    foreach ($item in @{
        MainBinaryName = 'league-replay-app.exe'; DisplayName = 'League Replay'; DisplayVersion = '0.1.0'; Publisher = 'leaguereplay'
        DisplayIcon = '"' + (Join-Path $beta.Stable 'league-replay-app.exe') + '"'
        InstallLocation = '"' + $beta.Stable + '"'; UninstallString = '"' + (Join-Path $beta.Stable 'uninstall.exe') + '"'
    }.GetEnumerator()) {
        New-ItemProperty -LiteralPath $arp -Name $item.Key -Value $item.Value -PropertyType String | Out-Null
    }
    foreach ($name in @('NoModify', 'NoRepair', 'EstimatedSize')) {
        New-ItemProperty -LiteralPath $arp -Name $name -Value 1 -PropertyType DWord | Out-Null
    }
    $beta
}
function Invoke-Setup($Fixture, [string]$Executable, [int]$Expected, [string]$Fault = '', [string]$WebView = '', [string]$Arguments = '/S') {
    $env:CHRONOBREAK_SETUP_FIXTURE_ROOT = $Fixture.Root
    $env:CHRONOBREAK_SETUP_TEST_FAULT = $Fault
    $env:CHRONOBREAK_SETUP_TEST_WEBVIEW = $WebView
    $result = Start-Process -FilePath $Executable -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
    if ($result.ExitCode -ne $Expected) {
        $log = Join-Path $Fixture.Root 'setup-error.log'
        $detail = if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log -Raw }
        throw "Expected setup exit $Expected. Got $($result.ExitCode). $detail"
    }
}
function Assert-Installed($Fixture, $Package) {
    Assert ((Hash (Join-Path $Fixture.Stable 'payload.json')) -ceq $Package.Manifest) 'Unexpected installed generation.'
    $shortcut = Join-Path $Fixture.Root 'Programs\Chronobreak.lnk'
    Assert (Test-Path -LiteralPath $shortcut) 'Start Menu shortcut is missing.'
    $link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
    Assert ($link.TargetPath -ieq (Join-Path $Fixture.Stable 'league-replay-app.exe') -and !$link.Arguments) 'Start Menu shortcut points away from the installed executable.'
    Assert (Test-Path -LiteralPath ($Fixture.Key + '\Uninstall')) 'Uninstall registration is missing.'
    Assert (-not (Test-Path -LiteralPath (Join-Path $Fixture.State 'transaction.json'))) 'Installation journal is still unsettled.'
}
function Assert-RejectedPackage($Package, [string]$Relative, [string]$Mutation, [int]$ExitCode) {
    $member = Join-Path $Package.Stage $Relative
    $backup = if (Test-Path -LiteralPath $member) { [IO.File]::ReadAllBytes($member) }
    $variant = Join-Path $Package.Stage 'Chronobreak-invalid-setup.exe'
    $variantScript = Join-Path $Package.Stage 'invalid.nsi'
    try {
        switch ($Mutation) {
            'alter' {
                $changed = [byte[]]$backup.Clone()
                $changed[-1] = $changed[-1] -bxor 1
                [IO.File]::WriteAllBytes($member, $changed)
            }
            'missing' { [IO.File]::Delete($member) }
            'extra' { Write-Text $member 'Unexpected content' }
        }
        Write-Text $variantScript ([IO.File]::ReadAllText($Package.Script).Replace($Package.Setup, $variant))
        $env:CHRONOBREAK_RELEASE_STAGE = $Package.Stage
        & $compiler /V2 /WX /DCHRONOBREAK_SETUP_FIXTURE $variantScript | Out-Host
        Assert ($LASTEXITCODE -eq 0) "Could not build $Mutation $Relative rejection package."
        $fixture = New-Fixture
        Invoke-Setup $fixture $variant $ExitCode
        Assert (-not (Test-Path -LiteralPath $fixture.Stable) -and -not (Test-Path -LiteralPath $fixture.Key)) "$Mutation $Relative reached installation."
    } finally {
        if ($null -eq $backup) { [IO.File]::Delete($member) }
        else { [IO.File]::WriteAllBytes($member, [byte[]]$backup) }
    }
}

if ($ProductionPayload) {
    $source = (Resolve-Path -LiteralPath $ProductionPayload).Path
    $manifest = Get-Content -LiteralPath (Join-Path $source 'payload.json') -Raw | ConvertFrom-Json
    Assert ($manifest.revision -cmatch '^[0-9a-f]{40}$') 'Expected a production candidate payload.'
    $package = New-Setup $manifest.version $source
    $fixture = if ($ArchivedBeta) { New-ArchivedBetaFixture (Resolve-Path -LiteralPath $ArchivedBeta).Path } else { New-Fixture }
    $preserved = @('config.toml', 'outside.clip.mp4') | ForEach-Object { Join-Path $fixture.Root $_ }
    $before = @($preserved | ForEach-Object { Hash $_ })
    Invoke-Setup $fixture $package.Setup 0
    Assert-Installed $fixture $package
    if ($ArchivedBeta) {
        Assert (-not (Test-Path -LiteralPath ($fixture.Key + '\Legacy'))) 'Production package left legacy registration.'
    }
    Invoke-Setup $fixture $package.Setup 78 'exit:after-previous'
    Assert (-not (Test-Path -LiteralPath $fixture.Stable)) 'Injected interruption did not leave the stable name absent.'
    Invoke-Setup $fixture $package.Setup 0
    Assert-Installed $fixture $package
    foreach ($member in $manifest.files.PSObject.Properties) {
        Assert ((Hash (Join-Path $fixture.Stable $member.Name)) -ceq $member.Value.sha256) "Installed production member changed: $($member.Name)."
    }
    foreach ($executable in @('league-replay-app.exe', 'recorder.exe')) {
        $receipt = Join-Path $working "$executable.identity"
        Invoke-Setup $fixture (Join-Path $fixture.Stable $executable) 0 '' '' ('--release-identity "' + $receipt + '"')
        $identity = @(Get-Content -LiteralPath $receipt)
        Assert ($identity.Count -eq 2 -and $identity[0] -ceq $manifest.revision -and $identity[1] -ceq $manifest.version) "Incorrect process identity: $executable."
    }
    $smoke = Join-Path $repo 'tools\media_runtime\smoke.ps1'
    & $smoke -RuntimeRoot (Join-Path $fixture.Stable 'resources\media-runtime') -ResultRoot (Join-Path $working 'media-smoke')
    Invoke-Setup $fixture (Join-Path $fixture.Stable 'uninstall.exe') 0
    Assert (-not (Test-Path -LiteralPath $fixture.Stable)) 'Production payload remained after uninstall.'
    Assert (-not (Test-Path -LiteralPath ($fixture.Key + '\Uninstall'))) 'Production uninstall registration remained.'
    Assert (@(Compare-Object $before @($preserved | ForEach-Object { Hash $_ })).Count -eq 0) 'Production setup or uninstall changed user data.'
    $entry = if ($ArchivedBeta) { 'exact beta upgrade' } else { 'fresh installation' }
    Write-Output "Passed isolated $entry, interrupted repair, release identities, media processing and uninstall of the production payload. Evidence: $working"
    return
}

$first = New-Setup '0.1.1'
$second = New-Setup '0.1.2'
Assert-RejectedPackage $first 'chronobreak-setup.exe' 'alter' 11
Assert-RejectedPackage $first 'uninstall.exe' 'alter' 1
Assert-RejectedPackage $first 'payload\payload.json' 'alter' 1
Assert-RejectedPackage $first 'payload\recorder.exe' 'alter' 1
Assert-RejectedPackage $first 'payload\recorder.exe' 'missing' 1
Assert-RejectedPackage $first 'payload\extra.txt' 'extra' 1
$bootstrapper = Join-Path $working 'MicrosoftEdgeWebView2Setup.exe'
Copy-Item -LiteralPath $first.Helper -Destination $bootstrapper
$sigOutput = & $first.Helper install-webview $working 2>&1 | Out-String
Assert ($LASTEXITCODE -eq 1 -and $sigOutput -match 'bootstrapper signature did not verify') 'The WebView2 path accepted an unsigned executable.'
Write-Output 'Passed altered helper, uninstaller, manifest and payload, missing and extra members, and unsigned WebView2 bootstrapper refusals.'
$fixture = New-Fixture
Invoke-Setup $fixture $first.Setup 12 '' 'failed'
Assert (-not (Test-Path -LiteralPath $fixture.Stable)) 'WebView2 failure wrote an installation.'
Assert (-not (Test-Path -LiteralPath $fixture.Key)) 'WebView2 failure wrote registration.'
Invoke-Setup $fixture $first.Setup 0
Assert-Installed $fixture $first
Assert (-not (Test-Path -LiteralPath ($fixture.Key + '\Run'))) 'Fresh setup created startup registration.'
Invoke-Setup $fixture $first.Setup 0
Assert-Installed $fixture $first
Assert (-not (Test-Path -LiteralPath ($fixture.Key + '\Run'))) 'Repair created startup registration.'
Write-Output 'Passed prerequisite failure, fresh installation and same-version repair.'

$runKey = $fixture.Key + '\Run'
New-Item -Path $runKey -Force | Out-Null
$ownedRun = '"' + (Join-Path $fixture.Stable 'recorder.exe') + '"'
New-ItemProperty -LiteralPath $runKey -Name 'Chronobreak Recorder' -Value $ownedRun -PropertyType String | Out-Null
New-ItemProperty -LiteralPath $runKey -Name 'Unrelated Fixture' -Value 'keep' -PropertyType String | Out-Null
foreach ($lease in @('maintenance.lock', 'fixture-recorder.lock')) {
    $lock = [IO.File]::Open((Join-Path $fixture.State $lease), 'Open', 'ReadWrite', 'None')
    try { Invoke-Setup $fixture $second.Setup 1 } finally { $lock.Dispose() }
    Assert-Installed $fixture $first
}
$lock = [IO.File]::Open((Join-Path $fixture.Stable 'league-replay-app.exe'), 'Open', 'Read', 'None')
try { Invoke-Setup $fixture $second.Setup 1 } finally { $lock.Dispose() }
Assert-Installed $fixture $first
foreach ($point in @('before-stage', 'after-stage', 'before-previous', 'after-previous', 'before-publish', 'after-publish', 'before-metadata', 'before-shortcut:1', 'shortcut:1', 'before-registry-value:1', 'registry-value:1', 'after-metadata')) {
    Invoke-Setup $fixture $second.Setup 78 "exit:$point"
    $env:CHRONOBREAK_SETUP_TEST_FAULT = ''
    & $first.Helper repair $fixture.Root | Out-Host
    Assert ($LASTEXITCODE -eq 0) "Repair failed at $point."
    & $first.Helper repair $fixture.Root | Out-Host
    Assert ($LASTEXITCODE -eq 0) "Repair is not idempotent at $point."
    Assert-Installed $fixture $first
    Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name 'Chronobreak Recorder') -ceq $ownedRun) "Upgrade altered the Run value at $point."
}
Invoke-Setup $fixture $second.Setup 1 'error:after-publish'
Assert-Installed $fixture $first
Invoke-Setup $fixture $second.Setup 78 'exit:after-commit'
Invoke-Setup $fixture $second.Setup 0
Assert-Installed $fixture $second
Invoke-Setup $fixture $first.Setup 1
Assert-Installed $fixture $second
Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name 'Chronobreak Recorder') -ceq $ownedRun) 'Committed upgrade altered the owned Run value.'
Write-Output 'Passed setup lease contention, transactional upgrade, rollback and idempotent interruption repair.'

$preserved = @((Join-Path $fixture.Root 'config.toml'), (Join-Path $fixture.Root 'outside.clip.mp4'),
    (Join-Path $fixture.Stable 'recordings\match.mp4'), (Join-Path $fixture.Stable 'resources\media-runtime\extra-clip.mp4'))
foreach ($path in $preserved | Select-Object -Skip 2) {
    [IO.Directory]::CreateDirectory((Split-Path -Parent $path)) | Out-Null
    Write-Text $path 'Inside media sentinel'
}
$before = @($preserved | ForEach-Object { Hash $_ })
Invoke-Setup $fixture $second.Setup 1
Assert-Installed $fixture $second
Assert (@(Compare-Object $before @($preserved | ForEach-Object { Hash $_ })).Count -eq 0) 'Upgrade refusal changed user data.'
$uninstaller = Join-Path $fixture.Stable 'uninstall.exe'
$uninstallCopy = Join-Path $working 'uninstall-test.exe'
Copy-Item -LiteralPath $uninstaller -Destination $uninstallCopy
$lock = [IO.File]::Open((Join-Path $fixture.State 'maintenance.lock'), 'Open', 'ReadWrite', 'None')
try {
    Invoke-Setup $fixture $uninstallCopy 1 '' '' ("/S _?=" + $fixture.Stable)
} finally {
    $lock.Dispose()
}
Invoke-Setup $fixture $uninstallCopy 78 'exit:delete-file' '' ("/S _?=" + $fixture.Stable)
Assert (Test-Path -LiteralPath ($fixture.Key + '\Uninstall')) 'Interrupted uninstall removed its registration.'
Assert (Test-Path -LiteralPath $uninstaller) 'Interrupted uninstall removed its retry launcher.'
Invoke-Setup $fixture $uninstaller 0
Assert (-not (Test-Path -LiteralPath $uninstaller)) 'Uninstaller remained after successful removal.'
Assert (-not (Test-Path -LiteralPath ($fixture.Key + '\Uninstall'))) 'Uninstaller registration remained.'
Assert (-not (Test-Path -LiteralPath (Join-Path $fixture.Root 'Programs\Chronobreak.lnk'))) 'Start Menu shortcut remained.'
Assert (-not ((Get-ItemProperty -LiteralPath $runKey).PSObject.Properties.Name -contains 'Chronobreak Recorder')) 'Owned Run value remained.'
Assert ((Get-ItemPropertyValue -LiteralPath $runKey -Name 'Unrelated Fixture') -ceq 'keep') 'Unrelated Run value changed.'
Assert (@(Compare-Object $before @($preserved | ForEach-Object { Hash $_ })).Count -eq 0) 'Uninstall changed user data.'

$foreign = New-Fixture
Invoke-Setup $foreign $first.Setup 0
New-Item -Path ($foreign.Key + '\Run') -Force | Out-Null
New-ItemProperty -LiteralPath ($foreign.Key + '\Run') -Name 'Chronobreak Recorder' -Value 7 -PropertyType DWord | Out-Null
Invoke-Setup $foreign $second.Setup 0
Assert-Installed $foreign $second
Assert ((Get-ItemPropertyValue -LiteralPath ($foreign.Key + '\Run') -Name 'Chronobreak Recorder') -eq 7) 'Upgrade altered a foreign typed Run value.'
Invoke-Setup $foreign (Join-Path $foreign.Stable 'uninstall.exe') 0
Assert ((Get-ItemPropertyValue -LiteralPath ($foreign.Key + '\Run') -Name 'Chronobreak Recorder') -eq 7) 'Foreign typed Run value was altered.'
Write-Output 'Passed interrupted and completed uninstall, exact Run ownership and media/configuration preservation.'

$unsupported = New-Fixture
New-Item -Path ($unsupported.Key + '\Msi') -Force | Out-Null
Invoke-Setup $unsupported $first.Setup 1
Assert (-not (Test-Path -LiteralPath $unsupported.Stable)) 'Installer admitted an MSI installation.'
$unsupported = New-Fixture
New-Item -Path ($unsupported.Key + '\Legacy') -Force | Out-Null
Set-Item -LiteralPath ($unsupported.Key + '\Legacy') -Value (Join-Path $unsupported.Root 'custom')
Invoke-Setup $unsupported $first.Setup 1
Assert (-not (Test-Path -LiteralPath $unsupported.Stable)) 'Installer admitted a legacy custom path.'
$unsupported = New-Fixture
[IO.Directory]::CreateDirectory($unsupported.Stable) | Out-Null
$foreignFile = Join-Path $unsupported.Stable 'foreign.txt'
Write-Text $foreignFile 'Foreign content'
$before = Hash $foreignFile
Invoke-Setup $unsupported $first.Setup 1
Assert ((Hash $foreignFile) -ceq $before) 'Installer altered a foreign product folder.'
Write-Output 'Passed MSI, custom path and foreign folder refusals.'

$redirected = New-Fixture
$target = Join-Path $redirected.Root 'safe-target'
[IO.Directory]::CreateDirectory($target) | Out-Null
$sentinel = Join-Path $target 'keep.txt'
Write-Text $sentinel 'Do not follow product junctions'
$before = Hash $sentinel
New-Item -ItemType Junction -Path $redirected.Stable -Target $target | Out-Null
Invoke-Setup $redirected $first.Setup 1
Assert ((Hash $sentinel) -ceq $before) 'Installer followed a product junction.'

if ($ArchivedBeta) {
    $beta = New-ArchivedBetaFixture (Resolve-Path -LiteralPath $ArchivedBeta).Path
    Invoke-Setup $beta $first.Setup 78 'exit:after-previous'
    $env:CHRONOBREAK_SETUP_TEST_FAULT = ''
    & $first.Helper repair $beta.Root | Out-Host
    Assert ($LASTEXITCODE -eq 0) 'Legacy repair did not restore the exact previous installation.'
    foreach ($entry in $legacy.PSObject.Properties) {
        Assert ((Hash (Join-Path $beta.Stable $entry.Name)) -ceq $entry.Value.sha256) "Legacy rollback changed $($entry.Name)."
    }
    Invoke-Setup $beta $first.Setup 0
    Assert-Installed $beta $first
    Assert (-not (Test-Path -LiteralPath ($beta.Key + '\Legacy'))) 'Legacy registration was not retired.'
    Write-Output 'Passed exact historical beta rollback and upgrade.'
}
Write-Output "Fixture evidence: $working"
