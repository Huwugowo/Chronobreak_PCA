param([string]$QueueRoot)
$ErrorActionPreference = 'Stop'
$queue = (Resolve-Path -LiteralPath $QueueRoot).Path
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$expected = Join-Path $repository 'build/perf/qb-clip-010'
if (-not $queue.StartsWith($expected, [StringComparison]::OrdinalIgnoreCase)) { throw 'Queue must be in the owned measurement directory' }
Set-Content -LiteralPath (Join-Path $queue 'worker.ready') -Value 'ready'
$deadline = [DateTime]::UtcNow.AddMinutes(30)
while ([DateTime]::UtcNow -lt $deadline -and -not (Test-Path -LiteralPath (Join-Path $queue 'worker.stop'))) {
    foreach ($request in @(Get-ChildItem -LiteralPath $queue -Filter '*.request.json')) {
        $job = Get-Content -LiteralPath $request.FullName -Raw | ConvertFrom-Json
        $trial = (Resolve-Path -LiteralPath $job.TrialRoot).Path
        if (-not $trial.StartsWith((Join-Path $expected 'trials/'), [StringComparison]::OrdinalIgnoreCase)) { throw 'Trial outside owned directory' }
        Move-Item -LiteralPath $request.FullName -Destination ($request.FullName + '.running')
        & (Join-Path $PSScriptRoot 'cpu.ps1') -AppProcessId $job.AppProcessId -TrialRoot $trial
    }
    Start-Sleep -Milliseconds 100
}
