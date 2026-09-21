# QB-REPLAY-010 V2 M4 replacement evidence

The fresh replacement campaign is complete: 50 launches in 25 adjacent matched
pairs, five independent processes per arm for each required cold cell and five
warm processes per arm for Normal and event-heavy. All 50 bundles validate and
all ten required-result matrix checks pass. The first campaign remains preserved
separately (14 valid runs and one incomplete invalid run); none of its results are
pooled here, and no old sequence was resumed or selectively topped up.

The before-launch disposition is [`disposition.md`](disposition.md). Frozen
reference and candidate executables, runtime, fixture bytes, preparation hashes,
exact config bytes and measurement semantics are unchanged. The global order is
seeded `20260915`; adjacent pairs alternate the leading arm. Cold means a fresh
process, not an empty operating-system cache. Warm initial mounts and five
remounts are retained separately. No percentile claim is made.

Frozen executable SHA-256 identities are reference
`e58d5071afbe4b6c3563c0b33533d577984bf97b75e01ad4483fdf29a27fb230`, candidate
`b9d74f34018a4577bab7175f40932801dc257e1f701268a96bdf57c93a3db367`, and the
separate normal/manual build `253a7eec067a846a4efe94b81b1ee50351d1246aab13fa34780db3985656f305`.

## Primary Games-usable result

`games-library-usable-v1` is measured from the frontend library request to the
post-paint Games event. Lower is better. Improvement must exceed both 5% and the
matched-reference repeatability band.

| Cell | Reference ms | Candidate ms | Candidate change | Gate |
| --- | ---: | ---: | ---: | --- |
| S cold | 166.4 | 168.2 | 1.1% slower | Pass: no material regression |
| N cold | 2,092.6 | 197.7 | 90.6% faster | Pass |
| E cold | 2,386.6 | 493.3 | 79.3% faster | Pass |
| N warm | 2,087.3 | 209.7 | 90.0% faster | Pass |
| E warm | 2,404.8 | 483.6 | 79.9% faster | Pass |

The dramatic improvement reproduces with complete matched evidence. It is the
core Games snapshot becoming usable before optional clip details; it is not a
claim that the historical full-details milestone became faster.

## Historical details and deferred work

The legacy `library_useful` event keeps its original full clip-detail semantics.
Candidate medians are S 168.4 ms, N cold 2,642.5 ms, E cold 2,966.7 ms, N warm
2,648.0 ms and E warm 2,935.2 ms, versus reference medians of S 166.4, N cold
2,092.6, E cold 2,386.6, N warm 2,087.3 and E warm 2,404.8 ms. The candidate is
therefore about 22–27% slower for that event. The same-clock extraction shows a
uniform roughly 2.45-second Games-to-historical gap in N/E, consistent with the
benchmark-only compatibility drain of all 50 optional clip durations.

That drain uses the existing bounded eight-ID API and protected clip HTTP route;
production requests only the visible first eight clips and active item. Across
the 20 candidate N/E processes, the compatibility drain adds exactly 1,000 clip
requests (50 per process) and about 14.7–14.8 MB delivered per process. This is a
bounded, understood source of deferred-work cost, but it remains a measured
request/I/O regression under the unchanged gates and is not waived or tuned away.

## Resource and reliability disposition

Process-tree CPU mean and private-memory peak do not have a disposition-requiring
median regression in any cell. Candidate private peaks are lower in N/E warm and
within the matched bands in N/E cold. N-cold working-set peak is 578.8 MiB versus
548.0 MiB reference (+29.4 MiB, 5.6%); the same-clock process-class evidence
concentrates the difference in the WebView while the benchmark-only compatibility
drain is active, with no sustained monotonic working-set or private-memory signal.
Working-set monotonic flags occur in two of five runs in each arm, so there is no
systematic candidate-only growth signal.
E-warm first-to-last handle growth is 833 versus 663 (+170, 25.6%), but candidate
starts lower (median 2,937 versus 3,105), peak handles are effectively equal
(3,796 versus 3,799), late-half growth medians are equal at 56, and every per-run
monotonic flag is false. These are bounded benchmark/process-sampler effects, not
evidence of a production leak. The complete table and action record are in
[`engineering-disposition.md`](engineering-disposition.md); individual traces
remain in [`gate-summary.json`](gate-summary.json) and
[`resource-details.json`](resource-details.json).

Candidate N/E read-byte medians increase by about 51–58% in cold runs and 11% in
warm runs. Protected server delivery rises from about 7.97 MB to 22.7 MB cold and
47.8 MB to 62.6 MB warm, with 51 versus 1 requests cold and 56 versus 6 warm.
These are mandatory I/O/request dispositions even though the additional work is
the expected optional compatibility drain. Write bytes, CPU means and private
peaks remain within their gates. Job I/O is aggregate process accounting, not a
disk-I/O claim.

There are no server errors and no required event/request loss in either arm. The
candidate has 11 cancelled requests across the campaign versus two reference
cancellations. Seven candidate clip reads are partial HTTP 206 bodies (167,936–
200,704 of 296,175 bytes, ending about 10 ms after start), consistent with ffprobe
closing a protected stream after obtaining metadata. The remaining cancellations
are near-complete game-video range reads and also occur in the reference. The
server telemetry does not retain a child PID or exit status; the implementation
does retain the stronger local invariant that probe bytes are returned only after
`try_wait()` observes a successful child exit, while cancellation/deadline paths
kill and reap the child. The focused [`cancellation-probe.json`](cancellation-probe.json)
reproduces the shape with packaged ffprobe returning code 0 after a partial 206
body and the server observing the client close/reset. Focused child-lifecycle and
packaged protected-route tests passed. This is therefore an expected
stream-cancellation watchpoint, not a claimed child failure. No stale result, source
mutation, failed bundle, timeout or playback failure was observed.

All raw per-run telemetry, sanitized reports, opening attribution, matrix checks,
source preservation, helper-source snapshots and verification receipts are in
this directory. The original evidence remains at
[`../qb-replay-010-m4-20260917/`](../qb-replay-010-m4-20260917/).
The final command/check disposition is [`final-checks.md`](final-checks.md).

## Manual acceptance

Native UI controls were initially unavailable (`cua.getState()` returned no apps
or browsers), then the user completed the prepared normal-build sequence. Manual
acceptance passed on 2026-09-21. Fresh disposable roots were baseline-audited without any
mutation: A has three Ahri games and ten clips (including one invalid duration),
and B has one Lux game and one clip with overlapping IDs. The status and unchanged
hashes and completed user observations are in [`manual-status.json`](manual-status.json).

The reproducible historical procedure is retained in
[`manual-acceptance.md`](manual-acceptance.md). From the repository PowerShell
window, it started with:

```powershell
python tools/replay_benchmark/qb010_manual.py launch
```

It used labelled `hold` and `audit` commands for Games
while optional work is owned, local failure/retry, save/delete, A/B/A switching,
export/return, cleanup and source-preserving playback. The helper never clicks or
types and cannot infer a manual pass; the actual observations are retained in the
audits. The completed export overlap used the later 25-second helper window.
retain “overlap not demonstrated” rather than claiming it.

## Closure

The primary performance improvement and all four requested engineering
dispositions are complete. The working-set, handle, request/I/O and cancellation
signals have bounded evidence-backed explanations and do not warrant a production
change. The V2 library optimization implementation/benchmark unit and its manual
acceptance are closed; M4 has no remaining blocker. Full QB-REPLAY-010 remains
in progress for the minimal replay descriptor/staged semantic timeline, playback
blocking-work disposition and its remaining full-feature gates. No payload split,
ReplayIndex, descriptor timeline or unrelated viewer optimization was started from
this M4 evidence.
