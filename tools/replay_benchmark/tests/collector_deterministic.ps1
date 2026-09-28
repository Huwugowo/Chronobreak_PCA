param([Parameter(Mandatory=$true)][string]$RunnerPath, [Parameter(Mandatory=$true)][string]$OutputRoot)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/collector_import.ps1" -RunnerPath $RunnerPath
$results = New-Object 'System.Collections.Generic.List[object]'
function Assert($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Record([string]$Name) { $results.Add($Name) }

# The production C# loop, including partial native errors and unexpected exceptions.
foreach ($ending in @('empty', 'error', 'throw', 'cap')) {
    $script:nativeCalls=0
    $script:nativeWaits=New-Object 'System.Collections.Generic.List[uint32]'
    $script:ending=$ending
    $callback=[Func[uint32,QueueBackReplayJobDequeueResult]] {
        param([uint32]$Wait)
        $script:nativeCalls++; $script:nativeWaits.Add($Wait)
        if ($script:nativeCalls -le 2 -or $script:ending -eq 'cap') {
            return [QueueBackReplayJobDequeueResult]@{Success=$true; MessageId=6; ProcessId=$script:nativeCalls}
        }
        if ($script:ending -eq 'throw') { throw 'injected dequeue exception' }
        return [QueueBackReplayJobDequeueResult]@{Success=$false; ErrorCode=$(if ($script:ending -eq 'empty') {258} else {6})}
    }
    $batch=[QueueBackReplayJob]::DrainNotificationBatch($callback,7,4)
    $expected=if ($ending -eq 'cap') {4} else {2}
    Assert ($batch.NewProcessCount -eq $expected) 'Native creation count'
    Assert ($batch.Notifications.Count -eq $expected) "Native partial records lost: $ending"
    Assert ($script:nativeWaits[0] -eq 7) 'Native initial wait'
    Assert (@($script:nativeWaits | Select-Object -Skip 1 | Where-Object {$_ -ne 0}).Count -eq 0) 'Native repeated wait'
    Assert ($batch.QueueEmpty -eq ($ending -eq 'empty')) 'Native empty flag'
    Assert ($batch.HitBatchLimit -eq ($ending -eq 'cap')) 'Native cap flag'
    Assert (($batch.NativeErrorCode -ne 0) -eq ($ending -in @('error','throw'))) 'Native error flag'
    Record "native-$ending"
}
foreach ($cap in @(0,257)) {
    $rejected=$false
    try { $null=[QueueBackReplayJob]::DrainNotificationBatch($callback,0,$cap) } catch {$rejected=$true}
    Assert $rejected 'Invalid native cap accepted'
}
Record 'native-invalid-caps'
function New-Batch([int]$New=0,[bool]$Empty=$true,[bool]$Full=$false,[int]$ErrorCode=0,[double]$Delay=0) {
    $records=@(for($i=0;$i -lt $New;$i++) {
        [pscustomobject]@{message_id=[uint32]6;message='new_process';process_id=[int64]($i+1);observed_utc=[DateTime]::UtcNow.ToString('o')}
    })
    return [pscustomobject]@{Notifications=$records;QueueEmpty=$Empty;HitBatchLimit=$Full;NativeErrorCode=$ErrorCode;NativeErrorMessage='injected';Delay=$Delay;NewProcessCount=[uint32]$New}
}
function Reset-Case([string]$Name,[uint64]$Initial,[object[]]$Batches,[int[]]$Totals) {
    $script:ResultRootCreated=Join-Path $OutputRoot $Name
    $null=New-Item -ItemType Directory -Path $script:ResultRootCreated
    $script:JobNotifications=New-Object 'System.Collections.Generic.List[object]'
    $script:JobNewProcessNotificationCount=$Initial
    $script:batches=$Batches; $script:totals=$Totals
    $script:drains=0; $script:queries=0; $script:clock=0.0
    $script:snapshotDelay=0.0; $script:clockCall=0; $script:preDrainExpiry=$false
    $script:trace=New-Object 'System.Collections.Generic.List[string]'
    $script:waits=New-Object 'System.Collections.Generic.List[int]'
    $script:starts=New-Object 'System.Collections.Generic.HashSet[long]'
    $script:throwSnapshot=$false; $script:throwDrain=$false; $script:throwOnQuery=0
}
function Get-JobObservationElapsedMs {
    param([long]$StartedTimestamp)
    $null=$script:starts.Add($StartedTimestamp); $script:clockCall++
    if($script:preDrainExpiry -and $script:clockCall -eq 1){$script:clock=100.0}
    return $script:clock
}
function Get-JobNotificationBatch {
    param([int]$WaitMilliseconds=0,[int]$MaximumMessages=256)
    Assert ($MaximumMessages -eq 256) 'Production batch cap changed'
    Assert ($WaitMilliseconds -ge 0 -and $WaitMilliseconds -le 10) 'Wait out of bounds'
    if($script:drains -eq 0){Assert ($WaitMilliseconds -eq 0) 'First drain blocked'}
    $script:waits.Add($WaitMilliseconds); $script:trace.Add('drain')
    $batch=$script:batches[[Math]::Min($script:drains,$script:batches.Count-1)]
    $script:drains++
    if($script:throwDrain){throw 'unexpected wrapper failure'}
    $script:clock+=$batch.Delay
    return $batch
}
function Get-BenchmarkJobSnapshot {
    $script:trace.Add('snapshot')
    $total=$script:totals[[Math]::Min($script:queries,$script:totals.Count-1)]
    $script:queries++; $script:clock+=$script:snapshotDelay
    if($script:throwSnapshot -or $script:queries -eq $script:throwOnQuery){throw 'snapshot API failure'}
    return [pscustomobject]@{
        TotalProcesses=[uint32]$total;ActiveProcesses=[uint32]0;TotalTerminatedProcesses=[uint32]0
        CpuTime100ns=[uint64](100000*$script:queries);ProcessIds=@()
        ReadBytes=[uint64](1234*$script:queries);WriteBytes=[uint64]2;OtherBytes=[uint64]3
        ReadOperations=[uint64]4;WriteOperations=[uint64]5;OtherOperations=[uint64]6
    }
}
function Check-Success([int]$Passes,[int]$Count,[int]$Records,[switch]$Final) {
    $observation=Get-ReconciledJobObservation -Final:$Final
    Assert ($observation.Passes -eq $Passes -and $script:queries -eq $Passes) 'Wrong pass/query count'
    Assert ($observation.NewProcessNotificationCount -eq $Count -and $observation.Snapshot.TotalProcesses -eq $Count) 'Inconsistent captured observation'
    Assert ($observation.Notifications.Count -eq $Records -and $script:JobNotifications.Count -eq $Records) 'Consumed records lost'
    Assert ($script:starts.Count -eq 1) 'Deadline reset'
    Assert (($script:trace -join ',') -eq ((1..$Passes | ForEach-Object {'drain,snapshot'}) -join ',')) 'Stale snapshot ordering'
    Assert (-not(Test-Path (Join-Path $script:ResultRootCreated 'job-reconciliation-error.json'))) 'Success wrote failure'
    Record (Split-Path -Leaf $script:ResultRootCreated)
    return $observation
}
function Check-Failure([string]$Reason,[int]$Passes,[int]$Records,[switch]$Final) {
    $failure=$null
    try{$null=Get-ReconciledJobObservation -Final:$Final}catch{$failure=$_.Exception.Message}
    Assert ($null -ne $failure -and $failure.Contains('REPLAY-BENCHMARK-PROCESS_JOB') -and $failure.Contains($Reason)) "Wrong failure: $failure"
    $evidence=Get-Content -Raw (Join-Path $script:ResultRootCreated 'job-reconciliation-error.json') | ConvertFrom-Json
    Assert ($evidence.reason.Contains($Reason) -and $evidence.passes -eq $Passes) 'Wrong failure evidence'
    Assert ($evidence.notifications.Count -eq $Records -and $script:JobNotifications.Count -eq $Records) 'Partial evidence lost'
    Assert ($script:starts.Count -eq 1) 'Failure reset deadline'
    Record (Split-Path -Leaf $script:ResultRootCreated)
    return $evidence
}
Reset-Case stable 28 @((New-Batch)) @(28)
$null=Check-Success 1 28 0
Reset-Case notification-ahead 28 @((New-Batch 1),(New-Batch)) @(28,29)
$null=Check-Success 2 29 1
Reset-Case snapshot-ahead 28 @((New-Batch),(New-Batch 1)) @(29,29)
$null=Check-Success 2 29 1
Reset-Case multiple-advances 28 @((New-Batch 1),(New-Batch 2),(New-Batch 1)) @(30,32,32)
$null=Check-Success 3 32 4
Reset-Case full-batch-needs-empty 0 @((New-Batch 256 $false $true),(New-Batch)) @(256,256)
$null=Check-Success 2 256 256
Reset-Case missing 28 @((New-Batch)) @(29)
$null=Check-Failure pass-cap-exhausted 32 0
Reset-Case duplicate 28 @((New-Batch 1),(New-Batch)) @(28)
$null=Check-Failure pass-cap-exhausted 32 1
Reset-Case continuous-full 0 @((New-Batch 256 $false $true)) @(256)
$null=Check-Failure pass-cap-exhausted 32 8192
Reset-Case continuous-full-final 0 @((New-Batch 256 $false $true)) @(256)
$e=Check-Failure pass-cap-exhausted 128 32768 -Final
Assert ($e.budget_ms -eq 500) 'Wrong final budget'
Reset-Case partial-native-error 28 @((New-Batch 1 $false $false 6)) @(29)
$e=Check-Failure native-dequeue-error 1 1
Assert ($e.new_process_notification_count -eq 29 -and $script:queries -eq 0) 'Native error not preserved before query'
Reset-Case unexpected-drain 28 @((New-Batch)) @(28)
$script:throwDrain=$true
$null=Check-Failure 'unexpected wrapper failure' 1 0
Reset-Case unexpected-query 28 @((New-Batch 1)) @(29)
$script:throwSnapshot=$true
$null=Check-Failure 'snapshot API failure' 1 1
Reset-Case query-after-partial 28 @((New-Batch 1),(New-Batch 1)) @(28,30)
$script:throwOnQuery=2
$e=Check-Failure 'snapshot API failure' 2 2
Assert ($e.last_snapshot.TotalProcesses -eq 28 -and $e.last_snapshot_notification_count -eq 29 -and $e.new_process_notification_count -eq 30) 'Diagnostic mixed the older snapshot and newer count'
foreach($kind in @('both-flags','short-full','silent-empty','error-empty','over-cap')) {
    $b=New-Batch
    switch($kind){
        'both-flags' {$b.HitBatchLimit=$true}
        'short-full' {$b.HitBatchLimit=$true;$b.QueueEmpty=$false}
        'silent-empty' {$b.QueueEmpty=$false}
        'error-empty' {$b.NativeErrorCode=6}
        'over-cap' {$b=New-Batch 257 $false $true}
    }
    Reset-Case $kind 0 @($b) @(0)
    $null=Check-Failure invalid-batch-metadata 1 $b.Notifications.Count
}
Reset-Case deadline-before-drain 28 @((New-Batch)) @(28)
$script:preDrainExpiry=$true
$null=Check-Failure deadline-exhausted-before-drain 0 0
Assert ($script:drains -eq 0 -and $script:queries -eq 0) 'Calls after expiry'
Reset-Case deadline-before-snapshot 28 @((New-Batch 1 $true $false 0 100)) @(29)
$null=Check-Failure deadline-exhausted-before-snapshot 1 1
Assert ($script:queries -eq 0) 'Query after expiry'
Reset-Case deadline-after-snapshot 28 @((New-Batch)) @(28)
$script:snapshotDelay=100
$null=Check-Failure deadline-exhausted-after-snapshot 1 0
Reset-Case remaining-wait 28 @((New-Batch 0 $true $false 0 97),(New-Batch 1 $true $false 0 3)) @(29)
$null=Check-Failure deadline-exhausted-before-snapshot 2 1
Assert ($script:waits[1] -eq 3) 'Wait ignored remaining milliseconds'
Reset-Case final-remaining-wait 28 @((New-Batch 0 $true $false 0 498),(New-Batch 1 $true $false 0 2)) @(29)
$null=Check-Failure deadline-exhausted-before-snapshot 2 1 -Final
Assert ($script:waits[1] -eq 2) 'Final wait ignored deadline'

# Actual exclusive publication preserves the partial and both failure causes.
Reset-Case publication-failure 28 @((New-Batch 1 $false $false 6)) @(29)
$destination=Join-Path $script:ResultRootCreated 'job-reconciliation-error.json'
[IO.File]::WriteAllText($destination,'original')
$failure=$null
try{$null=Get-ReconciledJobObservation}catch{$failure=$_.Exception.Message}
Assert ($failure.Contains('native-dequeue-error') -and $failure.Contains('write/publication also failed')) 'Publication replaced original error'
Assert ([IO.File]::ReadAllText($destination) -eq 'original') 'Overwrote failure artifact'
$partials=@(Get-ChildItem -LiteralPath $script:ResultRootCreated -Filter '*.partial')
Assert ($partials.Count -eq 1) 'Publication discarded partial'
$partial=Get-Content -Raw $partials[0].FullName | ConvertFrom-Json
Assert ($partial.notifications.Count -eq 1) 'Partial lost native record'
Record publication-failure
Reset-Case write-failure 28 @((New-Batch 1 $false $false 6)) @(29)
$script:ResultRootCreated=Join-Path $script:ResultRootCreated 'absent-parent'
$failure=$null
try{$null=Get-ReconciledJobObservation}catch{$failure=$_.Exception.Message}
Assert ($failure.Contains('native-dequeue-error') -and $failure.Contains('write/publication also failed')) 'Write replaced original error'
Record write-failure

# Later mutable global changes must not change sample/final projections.
$script:realReconcile=(Get-Item Function:Get-ReconciledJobObservation).ScriptBlock
function Get-ReconciledJobObservation {
    param([switch]$Final)
    $script:usedFinal=[bool]$Final
    $value=& $script:realReconcile -Final:$Final
    $script:JobNewProcessNotificationCount=999
    return $value
}
function Update-CreatedProcessTree {param($Rows,$RootPid,$JobSnapshot) return @{}}
function Get-AliveCreatedRows {param($ByPid) return @()}
function Get-GpuObservation {param($ProcessIds,$Profile) return $null}
function Get-SystemCpuPercent {param($PreviousSystemTimes) return 0.0}
Reset-Case captured-sample 28 @((New-Batch 1),(New-Batch)) @(28,29)
$script:PreviousJobMembers=@{};$script:AppProcess=[pscustomobject]@{Id=7001}
$sample=New-ObserverSample -Rows @() -MonotonicMs 1000 -IntervalMs 1000 -PreviousCpu @{} -PreviousJobAccounting @{cpu_time_100ns=0} -PreviousSystemTimes @{} -Profile minimal -LogicalProcessors 1
Assert (-not $script:usedFinal) 'Sample used final budget'
Assert ($sample.job_membership.new_process_notification_count -eq 29 -and $sample.aggregate.job_new_process_notification_count -eq 29) 'Sample read later global count'
Assert ($sample.aggregate.job_unobserved_process_count -eq 0 -and $sample.aggregate.cpu_time_100ns -eq 200000 -and $sample.aggregate.io_read_bytes -eq 2468) 'Sample used stale snapshot'
Assert ($sample.job_membership.completion_notifications.Count -eq 1) 'Sample lost notifications'
Record captured-sample
Reset-Case captured-final 28 @((New-Batch 1),(New-Batch)) @(28,29)
$final=Get-FinalJobAccounting
Assert $script:usedFinal 'Final did not use final budget'
Assert ($final.new_process_notification_count -eq 29 -and $final.total_processes -eq 29 -and $final.unobserved_process_count -eq 0) 'Final read later global count'
Assert ($final.cpu_time_100ns -eq 200000 -and $final.io_read_bytes -eq 2468 -and $final.io_other_operations -eq 6) 'Final used stale snapshot'
Record captured-final
foreach($case in @('valid','unobserved','missing','gap','backwards')) {
    $times=switch($case){'missing'{@(1000)} 'gap'{@(1000,3501)} 'backwards'{@(1000,999)} default{@(1000,2000)}}
    $records=@(foreach($time in $times){
        [ordered]@{schema_version=1;run_id='test';scenario_id='s';trial_id='1';monotonic_ms=$time;process_count=1;process_tree_cpu_percent=0;process_tree_private_bytes=4096;job_unobserved_process_count=$(if($case -eq 'unobserved'){1}else{0})}|ConvertTo-Json -Compress
    })
    $path=Join-Path $OutputRoot "$case.jsonl"
    [IO.File]::WriteAllLines($path,$records)
    $accepted=Test-ProcessTelemetryComplete -Path $path -RunId test -ScenarioId s -TrialId 1
    Assert ($accepted -eq ($case -eq 'valid')) "Strict sample validator changed: $case"
    Record "strict-$case"
}
[ordered]@{cases=$results.ToArray();passed=$results.Count;app_launches=0}|ConvertTo-Json -Depth 10
