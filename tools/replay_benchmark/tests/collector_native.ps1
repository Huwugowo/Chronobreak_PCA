param([Parameter(Mandatory=$true)][string]$RunnerPath, [Parameter(Mandatory=$true)][string]$OutputRoot)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
. "$PSScriptRoot/collector_import.ps1" -RunnerPath $RunnerPath
$script:BenchmarkJob=[IntPtr]::Zero
$script:BenchmarkCompletionPort=[IntPtr]::Zero
$script:ResultRootCreated=$OutputRoot
$script:JobNotifications=New-Object 'System.Collections.Generic.List[object]'
$script:JobNewProcessNotificationCount=[uint64]0
$process=$null
$passed=$false
$caseTimer=[Diagnostics.Stopwatch]::StartNew()
$result=[ordered]@{closed=$false;app_launches=0}
try {
    $script:BenchmarkJob=[QueueBackReplayJob]::CreateKillOnClose()
    $script:BenchmarkCompletionPort=[QueueBackReplayJob]::CreateCompletionPort()
    [QueueBackReplayJob]::AssociateCompletionPort($script:BenchmarkJob,$script:BenchmarkCompletionPort)
    # Root plus sixteen shells; Windows may add one owned console host.
    $shell=Join-Path $env:SystemRoot 'System32/cmd.exe'
    $process=[QueueBackReplayJob]::StartSuspended($script:BenchmarkJob,$shell,
        '/d /q /c for /l %i in (1,1,16) do @cmd.exe /d /q /c exit 0',
        (Join-Path $OutputRoot 'child.stdout.log'),(Join-Path $OutputRoot 'child.stderr.log'))
    $identities=@{}
    while (-not $process.HasExited) {
        if ($caseTimer.Elapsed.TotalMilliseconds -ge 8000) { throw 'Native work exceeded 8 seconds (2 seconds reserved for cleanup)' }
        foreach ($row in [QueueBackReplayJob]::SnapshotProcesses($script:BenchmarkJob)) {
            $identities[[string]$row.ProcessId]=[ordered]@{pid=$row.ProcessId;name=$row.Name;parent=$row.ParentProcessId}
        }
        $null=$process.WaitForExit(10)
    }
    $result.sampled_identities=@($identities.Values)
    if($process.ExitCode -ne 0){throw "Native root exit $($process.ExitCode)"}
    $observation=Get-ReconciledJobObservation -MaximumMessages 4
    $final=Get-FinalJobAccounting
    $new=@($script:JobNotifications | Where-Object {$_.message_id -eq 6})
    $exits=@($script:JobNotifications | Where-Object {$_.message_id -in @(7,8)})
    $result.notifications=$script:JobNotifications.ToArray()
    $result.final=$final
    $consoleHosts=@($identities.Values | Where-Object {$_.name -eq 'conhost.exe' -and $_.parent -eq $process.Id})
    if ($consoleHosts.Count -gt 1) { throw 'Unexpected console-host membership' }
    $expectedProcesses=17+$consoleHosts.Count
    $result.expected_processes=$expectedProcesses
    if($new.Count -ne $expectedProcesses -or $exits.Count -ne $expectedProcesses){throw "Native counts: $($new.Count)/$($exits.Count)"}
    if(@($new.process_id | Sort-Object -Unique).Count -ne $expectedProcesses){throw 'Duplicate native creation identity'}
    if(@(Compare-Object ($new.process_id | Sort-Object) ($exits.process_id | Sort-Object)).Count -ne 0){throw 'Native creation/exit membership mismatch'}
    if($final.total_processes -ne $expectedProcesses -or $final.active_processes -ne 0 -or
        $final.terminated_processes -ne 0 -or $final.unobserved_process_count -ne 0 -or
        $final.new_process_notification_count -ne $expectedProcesses){throw 'Invalid native final accounting'}
    if($observation.Passes -le 1){throw 'Native multi-batch case was not exercised'}
    if($caseTimer.Elapsed.TotalMilliseconds -ge 10000){throw 'Native case exceeded 10 seconds'}
    $result.new_processes=$new.Count
    $result.exit_processes=$exits.Count
    $result.final=$final
    $result.batches=$observation.Passes
    $result.acquisition_ms=$observation.ElapsedMilliseconds
    $result.case_ms=$caseTimer.Elapsed.TotalMilliseconds
    $result.notifications=$script:JobNotifications.ToArray()
    $passed=$true
}
catch {$result.error=$_.Exception.Message;throw}
finally {
    $cleanupFailure=$null
    $activeAfterCleanup=$null
    $rootExited=($null -eq $process)
    try {
        if($script:BenchmarkJob -ne [IntPtr]::Zero){
            if(-not $passed){[QueueBackReplayJob]::Terminate($script:BenchmarkJob,2)}
            do {
                $activeAfterCleanup=[QueueBackReplayJob]::Snapshot($script:BenchmarkJob).ActiveProcesses
                if($activeAfterCleanup -eq 0){break}
                if($caseTimer.Elapsed.TotalMilliseconds -ge 10000){throw 'Owned Job did not become inactive within case deadline'}
                Start-Sleep -Milliseconds 10
            } while($true)
        }
    }
    catch {$cleanupFailure=$_.Exception.Message}
    finally {
        try {
            if($script:BenchmarkJob -ne [IntPtr]::Zero){
                [QueueBackReplayJob]::Close($script:BenchmarkJob)
                $script:BenchmarkJob=[IntPtr]::Zero
            }
        }
        catch {$cleanupFailure="$cleanupFailure; Job close: $($_.Exception.Message)"}
        try {
            if($script:BenchmarkCompletionPort -ne [IntPtr]::Zero){
                [QueueBackReplayJob]::Close($script:BenchmarkCompletionPort)
                $script:BenchmarkCompletionPort=[IntPtr]::Zero
            }
        }
        catch {$cleanupFailure="$cleanupFailure; Port close: $($_.Exception.Message)"}
        if($null -ne $process){
            try {
                $remaining=[Math]::Max(0,10000-[int]$caseTimer.Elapsed.TotalMilliseconds)
                $rootExited=$process.WaitForExit($remaining)
                if(-not $rootExited){throw 'Owned root did not exit within case deadline'}
            }
            catch {$cleanupFailure="$cleanupFailure; Root exit: $($_.Exception.Message)"}
            finally {$process.Dispose()}
        }
    }
    $result.closed=($null -eq $cleanupFailure -and $rootExited -and $activeAfterCleanup -eq 0)
    $result.cleanup_active_processes=$activeAfterCleanup
    $result.root_exited=$rootExited
    $result.cleanup_error=$cleanupFailure
    $result.notifications=$script:JobNotifications.ToArray()
    [IO.File]::WriteAllText((Join-Path $OutputRoot 'native-result.json'),($result|ConvertTo-Json -Depth 20))
    if($null -ne $cleanupFailure){throw "Native cleanup failed: $cleanupFailure"}
}

$result|ConvertTo-Json -Depth 20
