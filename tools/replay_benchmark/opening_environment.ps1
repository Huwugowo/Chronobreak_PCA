# Read-only prelaunch identity. This never launches QueueBack or WebView.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Runner,
    [string]$MediaRuntimeId = "queueback-ffmpeg-8.1.2-windows-x86_64-r6"
)
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Import only the two existing identity functions; never dot-source the runner.
$parseTokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    [IO.Path]::GetFullPath($Runner), [ref]$parseTokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw "Cannot parse frozen runner." }
foreach ($functionName in @("Get-EnvironmentIdentity", "Measure-CpuAccountingQuantum")) {
    $definitions = @($ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $functionName
    }, $true))
    if ($definitions.Count -ne 1) { throw "Ambiguous runner identity function: $functionName" }
    . ([scriptblock]::Create($definitions[0].Extent.Text))
}
foreach ($variable in @("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", "WEBVIEW2_RELEASE_CHANNEL_PREFERENCE",
                        "WEBVIEW2_CHANNEL_SEARCH_KIND", "WEBVIEW2_RELEASE_CHANNELS",
                        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "WEBVIEW2_USER_DATA_FOLDER")) {
    if (-not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($variable))) {
        throw "Undeclared WebView environment override: $variable"
    }
}
$registrations = @()
foreach ($clientRoot in @(
    "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients",
    "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients",
    "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients"
)) {
    if (Test-Path -LiteralPath $clientRoot) {
        foreach ($client in Get-ChildItem -LiteralPath $clientRoot) {
            $value = Get-ItemProperty -LiteralPath $client.PSPath
            if ($value.PSObject.Properties["name"] -and $value.name -eq "Microsoft Edge WebView2 Runtime") {
                if (-not $value.PSObject.Properties["pv"] -or -not $value.PSObject.Properties["location"]) {
                    throw "Incomplete installed WebView registration."
                }
                $registrations += [pscustomobject]@{ version = [string]$value.pv; location = [string]$value.location }
            }
        }
    }
}
$registrations = @($registrations | Sort-Object version,location -Unique)
if ($registrations.Count -ne 1) { throw "Expected one unambiguous installed WebView runtime." }
$webviewPath = Join-Path (Join-Path $registrations[0].location $registrations[0].version) "msedgewebview2.exe"
$webviewVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($webviewPath).FileVersion
if ($webviewVersion -ne $registrations[0].version) { throw "WebView registered/file version mismatch." }
$webviewHash = (Get-FileHash -LiteralPath $webviewPath -Algorithm SHA256).Hash.ToLowerInvariant()

# Copy the runner's literal identity fields from its AST, so wording remains
# identical to the strict existing analyzer fingerprint, not a parallel formula.
$assignments = @($ast.FindAll({
    param($node)
    $node -is [System.Management.Automation.Language.AssignmentStatementAst] -and
    $node.Left.Extent.Text -eq '$runnerMetadata'
}, $true))
if ($assignments.Count -ne 1) { throw "Ambiguous runner metadata declaration." }
$tables = @($assignments[0].Right.FindAll({
    param($node)
    $node -is [System.Management.Automation.Language.HashtableAst]
}, $true))
if ($tables.Count -ne 1) { throw "Ambiguous runner metadata fields." }
function Get-RunnerLiteral {
    param([string]$Name)
    $pairs = @($tables[0].KeyValuePairs | Where-Object { $_.Item1.Value -eq $Name })
    if ($pairs.Count -ne 1) { throw "Missing runner literal $Name" }
    $expression = $pairs[0].Item2.PipelineElements[0].Expression
    if ($expression -isnot [System.Management.Automation.Language.StringConstantExpressionAst] -and
        $expression -isnot [System.Management.Automation.Language.ConstantExpressionAst]) {
        throw "Runner field is not a literal: $Name"
    }
    return $expression.Value
}
$quantum = Measure-CpuAccountingQuantum
$environment = Get-EnvironmentIdentity
if ($environment.limitations.Count -ne 0) { throw "Static machine/display identity is incomplete." }
[ordered]@{
    schema_version = 1
    installed_webview_sha256 = $webviewHash
    runner_environment = [ordered]@{
        config_sha256 = $null
        observer_cadence_ms = Get-RunnerLiteral "observer_cadence_ms"
        logical_processors = [Environment]::ProcessorCount
        cpu_accounting_quantum_ms = [double]$quantum.quantum_ms
        cpu_accounting_quantum_method = [string]$quantum.method
        cpu_accounting_reported_counter_unit_ms = [double]$quantum.reported_counter_unit_ms
        cpu_accounting_limitation = [string]$quantum.limitation
        gpu_collection = Get-RunnerLiteral "gpu_collection"
        process_tree_collection_method = Get-RunnerLiteral "process_tree_collection_method"
        process_tree_assignment_limitation = Get-RunnerLiteral "process_tree_assignment_limitation"
        environment = $environment
        media_runtime_id = $MediaRuntimeId
        webview2_runtime_versions = @($webviewVersion)
        webview2_runtime_limitation = $null
    }
    version_evidence = "Installed registered executable is the prelaunch expectation; every live descendant version must still match exactly."
} | ConvertTo-Json -Depth 20
