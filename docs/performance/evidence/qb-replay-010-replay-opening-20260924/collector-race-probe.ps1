param(
    [Parameter(Mandatory = $true)][string]$RunnerPath,
    [Parameter(Mandatory = $true)][string]$OutputPath
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    $RunnerPath, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw "Runner parse error" }
foreach ($name in @("Sync-JobNotifications", "New-ObserverSample", "Test-ProcessTelemetryComplete")) {
    $matches = @($ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name
    }, $true))
    if ($matches.Count -ne 1) { throw "Expected one function $name" }
    . ([scriptblock]::Create($matches[0].Extent.Text))
}
function Receive-JobNotifications {
    param([int]$WaitMilliseconds = 0)
    $script:drains++
    $script:trace.Add("drain:$($script:drains)")
    if ($script:advanceDuringSecondDrain -and $script:drains -eq 2) {
        $script:JobNewProcessNotificationCount = [uint64]29
        return [ordered]@{ message_id = 6; message = "NEW_PROCESS" }
    }
}
function Get-BenchmarkJobSnapshot {
    $script:snapshots++
    $script:trace.Add("snapshot:$($script:snapshots)")
    return [pscustomobject]@{
        TotalProcesses = [uint32]28; ActiveProcesses = [uint32]1
        TotalTerminatedProcesses = [uint32]0; CpuTime100ns = [uint64]100000
        ProcessIds = @(7001)
        ReadBytes = [uint64]1234; WriteBytes = [uint64]0; OtherBytes = [uint64]0
        ReadOperations = [uint64]1; WriteOperations = [uint64]0; OtherOperations = [uint64]0
    }
}
function Update-CreatedProcessTree {
    param($Rows, $RootPid, $JobSnapshot)
    return @{ "7001" = $script:testRow }
}
function Get-AliveCreatedRows {
    param($ByPid)
    return $script:testRow
}
function Get-ProcessCreationIdentity {
    param($Row)
    return "2026-09-24T00:00:00.0000000Z"
}
function Get-GpuObservation {
    param($ProcessIds, $Profile)
    return $null
}
function Get-SystemCpuPercent {
    param($PreviousSystemTimes)
    return 0.0
}
$script:testRow = [pscustomobject]@{
    ProcessId = 7001; ParentProcessId = 0; Name = "offline-fixture"
    KernelModeTime = 0; UserModeTime = 100000
    PrivatePageCount = 4096; WorkingSetSize = 4096
    ReadTransferCount = 1234; WriteTransferCount = 0; OtherTransferCount = 0
    ReadOperationCount = 1; WriteOperationCount = 0; OtherOperationCount = 0
    HandleCount = 1; ThreadCount = 1; ExecutablePath = ""
}
function Invoke-Case {
    param([string]$Name, [bool]$Advance, [bool]$ExpectedAccepted)
    $script:drains = 0
    $script:snapshots = 0
    $script:trace = New-Object 'System.Collections.Generic.List[string]'
    $script:advanceDuringSecondDrain = $Advance
    $script:JobNewProcessNotificationCount = [uint64]28
    $script:PreviousJobMembers = @{}
    $script:AppProcess = [pscustomobject]@{ Id = 7001 }
    $script:WebViewVersionPaths = @{}
    $script:WebViewVersions = @{}
    $argsForSample = @{
        Rows = @($script:testRow); MonotonicMs = 1028.8903; IntervalMs = 1000
        PreviousCpu = @{}; PreviousJobAccounting = @{ cpu_time_100ns = [uint64]0 }
        PreviousSystemTimes = @{}; Profile = "minimal"; LogicalProcessors = 1
    }
    $sample = New-ObserverSample @argsForSample
    $records = @(foreach ($time in @(1028.8903, 2028.8903)) {
        [ordered]@{
            schema_version = 1; run_id = "offline"; scenario_id = "race"; trial_id = "1"
            monotonic_ms = $time; process_count = $sample.aggregate.process_count
            process_tree_cpu_percent = $sample.aggregate.cpu_percent_normalized
            process_tree_private_bytes = $sample.aggregate.private_bytes
            job_unobserved_process_count = $sample.aggregate.job_unobserved_process_count
        } | ConvertTo-Json -Compress
    })
    $samplesPath = Join-Path (Split-Path -Parent $OutputPath) "$Name-synthetic-samples.jsonl"
    [System.IO.File]::WriteAllLines($samplesPath, $records, (New-Object System.Text.UTF8Encoding($false)))
    $accepted = Test-ProcessTelemetryComplete -Path $samplesPath -RunId "offline" -ScenarioId "race" -TrialId "1"
    if ($accepted -ne $ExpectedAccepted) { throw "Unexpected strict gate result for $Name" }
    if ($script:snapshots -ne 1 -or $script:drains -ne 2) { throw "Unexpected acquisition order" }
    $expectedDifference = if ($Advance) { 1 } else { 0 }
    if ($sample.aggregate.job_unobserved_process_count -ne $expectedDifference) { throw "Mismatch not reproduced" }
    return [ordered]@{
        case = $Name; calls = $script:trace.ToArray()
        snapshot_total = $sample.aggregate.job_total_processes
        captured_notification_count = $sample.aggregate.job_new_process_notification_count
        unobserved_count = $sample.aggregate.job_unobserved_process_count
        strict_telemetry_accepted = $accepted; native_processes_launched = 0
    }
}
$before = (Get-FileHash -LiteralPath $RunnerPath -Algorithm SHA256).Hash.ToLowerInvariant()
$cases = @(
    Invoke-Case -Name "stable-control" -Advance $false -ExpectedAccepted $true
    Invoke-Case -Name "creation-during-post-snapshot-drain" -Advance $true -ExpectedAccepted $false
)
$after = (Get-FileHash -LiteralPath $RunnerPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($after -ne $before) { throw "Runner changed during diagnostic" }
$result = [ordered]@{
    schema_version = 1; diagnostic = "actual-runner-offline-ordering-reproduction"
    runner_sha256 = $before
    probe_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    runner_unchanged = $true
    extracted_functions = @("Sync-JobNotifications", "New-ObserverSample", "Test-ProcessTelemetryComplete")
    native_boundaries_mocked = $true; cases = $cases
    limitation = "Deterministic producer-order reproduction, not a native Windows or performance pass; the failed live result remains invalid."
}
if (Test-Path -LiteralPath $OutputPath) { throw "Diagnostic output already exists" }
[System.IO.File]::WriteAllText($OutputPath, ($result | ConvertTo-Json -Depth 10) + [Environment]::NewLine,
    (New-Object System.Text.UTF8Encoding($false)))
Write-Output "QB010-COLLECTOR-RACE-REPRODUCED"
