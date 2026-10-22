param([string]$Toolchain = '1.97.1')
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path "$PSScriptRoot/../..").Path
$previousTauriConfig = $env:TAURI_CONFIG
# Static checks do not package the separately verified media runtime.
$env:TAURI_CONFIG = '{"bundle":{"resources":[]}}'
Push-Location $repoRoot
try {
    foreach ($crate in @('replay-time', 'league-data', 'media-runtime', 'recorder', 'app/src-tauri')) {
        $manifest = "$crate/Cargo.toml"
        & cargo "+$Toolchain" fmt --manifest-path $manifest -- --check
        if ($LASTEXITCODE -ne 0) { throw "Formatting failed: $crate" }
        & cargo "+$Toolchain" clippy --locked --manifest-path $manifest --all-targets --all-features -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw "Clippy failed: $crate" }
        # Deliberately omit --ignored: those suites need staged media/hardware.
        & cargo "+$Toolchain" test --locked --manifest-path $manifest --all-targets --all-features
        if ($LASTEXITCODE -ne 0) { throw "Tests failed: $crate" }
    }
} finally {
    Pop-Location
    $env:TAURI_CONFIG = $previousTauriConfig
}
