# Replay-opening campaign disposition — 2026-09-24

The campaign is **insufficient and closed**. It attempted seven of its 100
slots: six results are individually valid; N-warm candidate launch 7 is invalid.
The remaining 93 slots are unused and forfeited. There was no retry, top-up,
selective sample removal or eligible before/after comparison.

[failed-campaign.json](failed-campaign.json) retains the sanitized frozen
subjects/tools/order, six diagnostic extractions and failure/closure evidence.
The original seal is
`649e49466929f19d575d0deac752487e0a2247dbebab831d4b9f52ecf621ee80`.
Raw artifacts remain in
`build/perf/qb-replay-010-replay-opening/.chronobreak-replay-benchmark/`.
The failed result is `o-n-warm-can-001-candidate-trial-1-minimal`.

Both archived optimized builds and fixture preparation passed before collection:
203 media decodes, 409 matching source/copy hashes, then all 100 no-launch
preflights. Live collection ran 08:48:41.418295–08:52:57.345792 UTC.
The driver stopped with exit 2. The dedicated app, descendants and driver were
confirmed closed.

## Failure and root cause

Launch 7 completed all six viewer cycles, accepted 231 app events without drops,
and exited 0 without timeout or forced termination. Source/config/cache hashes
all match. Final Job accounting is 118 created / 118 creation notifications,
zero active, terminated or unobserved processes.

The first active process sample at 1028.8903 ms pairs Job total 28 with 29
creation notifications and reports one unobserved process. The remaining 39
active samples reconcile. All adjacent gaps are 1000.2247–1021.4098 ms; none
trips the cadence flag or 2500 ms maximum. Thus the strict telemetry rejection
comes from the inconsistent first accounting pair, not missing sample count or
an excessive interval. Final equality does not repair that earlier sample.

The unchanged runner `New-ObserverSample` drains notifications, queries a Job
snapshot, then calls `Sync-JobNotifications`, which drains again. The second
drain can consume a creation that happened after the snapshot. Its count advances
while the snapshot stays old; `Abs(snapshot.TotalProcesses - count)` then treats
this acquisition-order difference as unobserved telemetry. Terminal accounting
has the same structural query/drain boundary, though the observed final counts
in this run reconcile.

[collector-race-reproduction.json](collector-race-reproduction.json) records a
deterministic reproduction using AST-extracted **actual** runner functions and
mocked native boundaries. A stable 28/28 control passes the actual strict
telemetry function. A creation during the second drain reproduces 28/29,
unobserved=1 and rejection. Both paths perform drain → snapshot → drain.
The runner hash remains
`3312dfbec9852e5f5d4f74ded174152bab41265f124a988b773d18babc733811`.
No native child, app or benchmark was launched by the diagnostic. This proves
the producer-order defect, not that a proposed correction passes Windows or
performance checks.

The reproducible source is
[collector-race-probe.ps1](collector-race-probe.ps1). It accepts the frozen old
runner path and a fresh output path in an existing dedicated directory. The
retained invocation used `build/qb010-replay-comparison/collector_race_probe.ps1`
with the current unchanged runner and output
`build/qb010-replay-comparison/checks/collector-race-reproduction.json`;
exit 0 emitted `QB010-COLLECTOR-RACE-REPRODUCED`.

## Engineering decision

Correct acquisition in a reviewed superseding design. Retain exact notification
accounting and existing validity gates; do not clamp, skip the first sample,
retroactively rewrite raw evidence or retry this campaign. The proposed design
uses one bounded reconciliation deadline and fresh snapshots after drains,
including terminal accounting. Bounded native batches and explicit failure
evidence prevent continuous churn or expiry from silently losing observation.

The six valid results remain diagnostic only; they cannot meet five matched
processes per arm in every stratum. No latency/resource improvement, lack of
regression or feature completion is claimed. Existing accepted M4, descriptor
and combined native UI evidence is unaffected by this external collector fault.
Canonical progress and the reviewed implementation cursor live in
[the execution checkpoint](../../../execution/qb-replay-010.md).
