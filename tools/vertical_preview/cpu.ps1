param([int]$AppProcessId, [string]$TrialRoot)
$ErrorActionPreference = 'Stop'
$startId = 'clip-start-' + [guid]::NewGuid().ToString('N')
$stopId = 'clip-stop-' + [guid]::NewGuid().ToString('N')
$reasons = [Collections.Generic.List[string]]::new()
$samples = [Collections.Generic.List[object]]::new()
$events = [Collections.Generic.List[object]]::new()
function Save-Result($value) {
    $pending = Join-Path $TrialRoot 'cpu.json.pending'
    $value | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $pending
    Move-Item -LiteralPath $pending -Destination (Join-Path $TrialRoot 'cpu.json')
}
try {
    Register-CimIndicationEvent -Query 'SELECT * FROM Win32_ProcessStartTrace' -SourceIdentifier $startId | Out-Null
    Register-CimIndicationEvent -Query 'SELECT * FROM Win32_ProcessStopTrace' -SourceIdentifier $stopId | Out-Null
    function Take-Sample {
        $rows = @()
        foreach ($ownedId in $owned) {
            $proc = [Diagnostics.Process]::GetProcessById($ownedId)
            $proc.Refresh()
            $rows += [pscustomobject]@{ id=$ownedId; created=$proc.StartTime.ToUniversalTime().Ticks; cpu=$proc.TotalProcessorTime.TotalSeconds }
            $proc.Dispose()
        }
        [pscustomobject]@{ ms=$clock.Elapsed.TotalMilliseconds; processes=$rows }
    }
    Set-Content -LiteralPath (Join-Path $TrialRoot 'cpu.ready') -Value 'ready'
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    while (-not (Test-Path -LiteralPath (Join-Path $TrialRoot 'start'))) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Start signal timeout' }
        Start-Sleep -Milliseconds 20
    }
    # Snapshot the complete warmed-up tree, including children created during warmup.
    $all = @(Get-CimInstance Win32_Process)
    $owned = [Collections.Generic.HashSet[int]]::new()
    [void]$owned.Add($AppProcessId)
    do {
        $added = $false
        foreach ($item in $all) {
            if ($owned.Contains([int]$item.ParentProcessId) -and $owned.Add([int]$item.ProcessId)) { $added = $true }
        }
    } while ($added)
    # Warmup events are not part of the timed window.
    Get-Event -SourceIdentifier $startId -ErrorAction SilentlyContinue | Remove-Event
    Get-Event -SourceIdentifier $stopId -ErrorAction SilentlyContinue | Remove-Event
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $samples.Add((Take-Sample))
    Set-Content -LiteralPath (Join-Path $TrialRoot 'cpu.started') -Value 'started'
    $nextSampleAt = $clock.Elapsed.TotalMilliseconds + 250
    while (-not (Test-Path -LiteralPath (Join-Path $TrialRoot 'stop'))) {
        if ($clock.Elapsed.TotalSeconds -gt 20) { throw 'Stop signal timeout' }
        $delay = [Math]::Max(1, [Math]::Ceiling($nextSampleAt - $clock.Elapsed.TotalMilliseconds))
        Start-Sleep -Milliseconds $delay
        $samples.Add((Take-Sample))
        $nextSampleAt += 250
    }
    $samples.Add((Take-Sample))
    # Allow WMI delivery of trace events generated during the window.
    Start-Sleep -Milliseconds 750
    foreach ($identifier in @($startId,$stopId)) {
        foreach ($eventItem in @(Get-Event -SourceIdentifier $identifier -ErrorAction SilentlyContinue)) {
            $eventData = $eventItem.SourceEventArgs.NewEvent
            if ($owned.Contains([int]$eventData.ProcessID) -or ($identifier -eq $startId -and $owned.Contains([int]$eventData.ParentProcessID))) {
                $events.Add([pscustomobject]@{ type=$identifier; id=$eventData.ProcessID; time=$eventItem.TimeGenerated.ToUniversalTime().ToString('o') })
                $reasons.Add('Owned process started/exited during or adjacent to measurement')
            }
        }
    }
    $first = $samples[0]; $last = $samples[$samples.Count-1]
    for ($i=1; $i -lt $samples.Count; $i++) {
        if ($samples[$i].ms - $samples[$i-1].ms -gt 500) { $reasons.Add('CPU sample gap exceeded 500ms') }
        foreach ($row in $samples[$i].processes) {
            $previous = @($samples[$i-1].processes | Where-Object id -eq $row.id)
            if ($previous.Count -ne 1 -or $row.created -ne $previous[0].created -or $row.cpu -lt $previous[0].cpu) { $reasons.Add('Process identity/counter mismatch') }
        }
    }
    $cpuSeconds = ($last.processes | Measure-Object cpu -Sum).Sum - ($first.processes | Measure-Object cpu -Sum).Sum
    $wallSeconds = ($last.ms - $first.ms) / 1000
    Save-Result ([pscustomobject]@{ valid=($reasons.Count -eq 0); reasons=@($reasons); logical_cores=$cpuSeconds/$wallSeconds; cpu_seconds=$cpuSeconds; wall_seconds=$wallSeconds; events=@($events); samples=@($samples) })
} catch {
    Save-Result @{valid=$false;reasons=@($_.Exception.Message)}
    Set-Content -LiteralPath (Join-Path $TrialRoot 'cpu.ready') -Value 'unavailable'
    exit 1
} finally {
    foreach ($identifier in @($startId,$stopId)) {
        Unregister-Event -SourceIdentifier $identifier -ErrorAction SilentlyContinue
        Get-Event -SourceIdentifier $identifier -ErrorAction SilentlyContinue | Remove-Event
    }
}
