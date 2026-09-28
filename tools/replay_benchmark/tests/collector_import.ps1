param([string]$RunnerPath)
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    $RunnerPath, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw ($parseErrors | Out-String) }
$native = @($ast.FindAll({ param($node)
    $node -is [System.Management.Automation.Language.CommandAst] -and
    $node.GetCommandName() -eq "Add-Type"
}, $true))
if ($native.Count -ne 1) { throw "Expected one native definition" }
. ([scriptblock]::Create($native[0].Extent.Text))
foreach ($name in @(
    "Stop-Benchmark", "Get-BenchmarkJobSnapshot", "Get-JobNotificationBatch",
    "Receive-JobNotifications", "Get-JobObservationElapsedMs", "Write-JobReconciliationFailure",
    "Get-ReconciledJobObservation", "Get-FinalJobAccounting", "New-ObserverSample",
    "Test-ProcessTelemetryComplete", "Close-BenchmarkJob"
)) {
    $matches = @($ast.FindAll({ param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name
    }, $true))
    if ($matches.Count -ne 1) { throw "Expected one function $name" }
    . ([scriptblock]::Create($matches[0].Extent.Text))
}
