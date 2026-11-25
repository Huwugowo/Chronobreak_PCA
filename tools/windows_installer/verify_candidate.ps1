[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$CandidateRoot,
    [Parameter(Mandatory)][string]$Installer
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = (Resolve-Path -LiteralPath $CandidateRoot).Path
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$sevenZip = Get-Command 7z.exe -ErrorAction SilentlyContinue
$extractor = if ($null -eq $sevenZip) { Join-Path $env:ProgramFiles "7-Zip\7z.exe" } else { $sevenZip.Source }
if (-not (Test-Path -LiteralPath $extractor -PathType Leaf)) { throw "7-Zip is required to inspect the Windows installer." }

$extracted = Join-Path $root ("extracted-" + [Guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Path $extracted | Out-Null
& $extractor x -y "-o$extracted" $installerPath
if ($LASTEXITCODE -ne 0) { throw "Could not extract the setup payload." }

$payloads = @(Get-ChildItem -LiteralPath $extracted -Recurse -File -Filter "payload.json")
if ($payloads.Count -ne 1 -or $payloads[0].Directory.Name -cne "payload") { throw "Unexpected setup payload layout." }
$payload = $payloads[0].Directory.FullName
$container = Split-Path -Parent $payload
$coordinator = Join-Path $container "chronobreak-setup.exe"
$envelopePath = Join-Path $container "envelope.json"
function Get-Sha256([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}
foreach ($file in @("envelope.json", "chronobreak-setup.exe", "uninstall.exe")) {
    if ((Get-Sha256 (Join-Path $container $file)) -cne (Get-Sha256 (Join-Path $root $file))) {
        throw "Extracted $file differs from staging."
    }
}
$envelope = Get-Content -LiteralPath $envelopePath -Raw | ConvertFrom-Json
if ($envelope.schema -ne 1 -or @("unsigned-local", "signed") -cnotcontains $envelope.signing -or
    (Get-Sha256 $coordinator) -cne $envelope.coordinator_sha256 -or
    (Get-Sha256 (Join-Path $container "uninstall.exe")) -cne $envelope.uninstaller_sha256 -or
    (Get-Sha256 $payloads[0].FullName) -cne $envelope.payload_sha256 -or
    (Get-Sha256 $payloads[0].FullName) -cne (Get-Sha256 (Join-Path $root "payload\payload.json"))) {
    throw "Setup envelope or payload manifest did not verify."
}
if ($envelope.signing -ceq "signed") {
    $signed = @($installerPath, $coordinator, (Join-Path $container "uninstall.exe"),
        (Join-Path $payload "league-replay-app.exe"), (Join-Path $payload "recorder.exe"))
    foreach ($file in $signed) {
        if ((Get-AuthenticodeSignature -LiteralPath $file).Status -ne "Valid") {
            throw "Authenticode did not verify for $(Split-Path -Leaf $file)."
        }
    }
}

$manifest = Get-Content -LiteralPath $payloads[0].FullName -Raw | ConvertFrom-Json
$fileCount = @($manifest.files.PSObject.Properties).Count
$archiveFiles = @(Get-ChildItem -LiteralPath $extracted -Recurse -File -Force)
$pluginNames = @('nsDialogs.dll', 'nsExec.dll', 'System.dll')
$toolkit = Join-Path $env:LOCALAPPDATA "tauri\NSIS"
$pluginDirectory = Join-Path $toolkit "Plugins\x86-unicode"
$pluginArchive = Join-Path $extracted '$PLUGINSDIR'
$payloadPaths = @($manifest.files.PSObject.Properties.Name | ForEach-Object { Join-Path $payload $_ })
$allowed = @($coordinator, $envelopePath, $payloads[0].FullName, (Join-Path $container "uninstall.exe")) + $payloadPaths
foreach ($entry in $archiveFiles) {
    if ($allowed -contains $entry.FullName) { continue }
    $expectedPath = if ($entry.Name -ceq 'modern-wizard.bmp') {
        Join-Path $toolkit "Contrib\Graphics\Wizard\nsis.bmp"
    } elseif ($pluginNames -ccontains $entry.Name) {
        Join-Path $pluginDirectory $entry.Name
    }
    if ($entry.DirectoryName -cne $pluginArchive -or -not $expectedPath -or
        (Get-Sha256 $entry.FullName) -cne (Get-Sha256 $expectedPath)) {
        throw "Setup archive contains unexpected file $($entry.Name)."
    }
}
if (@($archiveFiles | Where-Object { $allowed -contains $_.FullName }).Count -ne $fileCount + 4) {
    throw "Setup archive is missing payload files."
}
& $coordinator verify-payload $payload
if ($LASTEXITCODE -ne 0) { throw "Extracted payload verification failed." }
Write-Output "Verified $fileCount payload files and setup envelope in $extracted"
