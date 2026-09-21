# QB-REPLAY-010 V2 M4: interrupted matched comparison

The library optimization unit is **not complete**. The new comparison attempted
15 of its 50 allowed launches: 14 individually valid runs, one incomplete invalid
run, and 35 unlaunched manifests. No stratum has five independent processes per
arm. Neither performance nor non-regression acceptance is established. Full
QB-REPLAY-010 also retains its separate descriptor/staged-timeline gates.

The authoritative execution cursor remains
[`docs/execution/qb-replay-010.md`](../../../execution/qb-replay-010.md).
This directory retains sanitized evidence, not a second execution checkpoint.

## Frozen inputs and collection

The reference is base `3a750e2c003d6e3605b2f907ff19ad7898d3dbd2` with only the
new diagnostic tap and benchmark startup-order correction. Candidate is the
M1-M3 implementation plus the M4 benchmark-readiness correction. Source hashes,
executable hashes and every attempted run's artifact hashes are in
[`provenance.json`](provenance.json); the exact reference patch is retained here.

| Subject | Executable SHA-256 |
| --- | --- |
| Reference benchmark | `e58d5071afbe4b6c3563c0b33533d577984bf97b75e01ad4483fdf29a27fb230` |
| Candidate benchmark | `b9d74f34018a4577bab7175f40932801dc257e1f701268a96bdf57c93a3db367` |
| Candidate normal/manual | `253a7eec067a846a4efe94b81b1ee50351d1246aab13fa34780db3985656f305` |

Ten immutable plans and all 50 launch preflights passed before collection. The
fresh sentinel contains byte-identical prepared S/N/E inputs: 409 files,
1,108,793,064 bytes. S preserves its three-bundle prepared shape but selects S
only; N/E each contain 50 games/50 clips. Identity-bound prior media decodes were
reused; historical attribution timings were not used as the comparator.

Both arms use the same per-cell library and exact config bytes, separate appdata,
scratch and result roots, identical locked r6 runtime, full observer, seed
20260915, adjacent pairs with alternating leading arms, and five-second active
warmup/cooldown. Warm manifests declare one initial mount plus five remounts.
The `qb010-library-v2-` namespace requires `games-library-usable-v1`, one frontend
request/admission/usable chain, identical tokens/counts and two paint frames.
Historical `library_useful` still waits for complete clip details.

Local raw root:
`build/perf/qb-replay-010-m4/.chronobreak-replay-benchmark`.
Frozen subjects, preparation receipts, manifests, preflights, global order,
per-launch logs and raw telemetry remain there, including incomplete run 15 and
the empty `live/15.json`/`15.log`. No artifact was repaired to make that run valid.

## Invalid-run and paired-repeat disposition

Launch 15 is N warm candidate trial 2. It started at
`2026-09-16T14:11:16.8087295Z`; telemetry ends during the sixth opening cycle.
It lacks app/final terminal, collection result, post-hashes and runner result.
On 2026-09-17 resume no campaign Python/app process remained. Available events
do not establish an application error or timeout; the cause of the external
interruption is unknown. Missing collector-finalization evidence is sufficient
to reject the run. Launch 16, its reference mate, never started.

Stop is the disposition. No additional benchmark launch, selective top-up or
replacement was made. A fresh complete paired repeat would need new manifests
and result roots, while retaining this failure. Completing all five processes
per arm would require at least 51 attempts including the invalid run, beyond
the approved ceiling. Further collection therefore needs an explicitly approved
bounded collection disposition and resolution of the interruption. The current
50-launch campaign cannot be declared successful. All ten `--require-results`
matrix checks fail with the expected missing result/terminal; see
[`matrix-verification.json`](matrix-verification.json).

## Measured results and gates

All values below are medians of the available independently launched processes.
These are diagnostic observations, **not passing comparisons**. Each report's
`status: valid` means its included runs validated; it does not make the incomplete
campaign sufficient. All entries in [`comparison.json`](comparison.json) are
ineligible (`insufficient balanced evidence`). No percentile claim is made.

| Cell | Processes per arm | Games usable reference / candidate ms | Historical useful reference / candidate ms |
| --- | ---: | ---: | ---: |
| S cold | 1 | 313.7 / 304.9 | 313.7 / 305.2 |
| N cold | 2 | 2100.8 / 286.7 | 2100.9 / 2730.6 |
| E cold | 1 | 2298.8 / 471.9 | 2298.8 / 2780.4 |
| N warm startup | 1 | 2003.0 / 193.9 | 2003.1 / 2408.5 |
| E warm startup | 2 | 2257.9 / 452.6 | 2258.0 / 2788.5 |

N/E cold Games medians are lower by 86.4%/79.5%. The observed absolute reductions
1814.1/1826.9 ms exceed the numerical five-percent and current reference
three-MAD/floor bands (N 369.0 ms; E 5.0 ms), but n=2/n=1 cannot establish the
required improvement gate. S's single pair cannot establish non-regression.
Historical useful is slower in N/E by 20-30%; this remains an open disposition,
not an accepted limitation or a reason to redefine the historical event.

Warm replay keeps initial mounts separate. For each process, take the median of
its five remounts, then the median across processes. Request-to-first-authoritative
frame is N 172.8 -> 180.8 ms (one process/arm), E 202.7 -> 207.3 ms (two/arm),
including the existing 100 ms remount timer. Initial mounts are N 174.8 -> 151.1
ms, E 184.5 -> 180.3 ms. [`opening.json`](opening.json) retains each cycle and
per-process aggregate, using frontend clocks only; no server/native timing join.
Warm regression characterization is also insufficient.

Resource medians below retain the whole-process observation window, including
benchmark-only historical detail draining. Memory is MiB; Job read/write amounts
are decimal MB and are aggregate accounting, **not disk-I/O attribution**.

| Cell | CPU mean % R/C | Private peak MiB R/C | Working peak MiB R/C | Job reads MB R/C | Job writes MB R/C | Server requests R/C |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| S cold | 7.92 / 7.30 | 405.0 / 395.7 | 511.1 / 507.8 | 5.04 / 5.02 | 5.17 / 5.13 | 2 / 2 |
| N cold | 7.81 / 7.04 | 426.9 / 452.2 | 535.6 / 561.7 | 17.62 / 27.25 | 6.22 / 4.95 | 1.5 / 51.5 |
| E cold | 8.09 / 6.77 | 421.0 / 454.4 | 542.8 / 567.3 | 19.42 / 28.75 | 6.83 / 4.70 | 2 / 52 |
| N warm | 5.42 / 5.93 | 464.4 / 474.8 | 600.8 / 601.1 | 80.89 / 90.16 | 29.01 / 28.00 | 6 / 56 |
| E warm | 6.11 / 6.14 | 482.7 / 485.7 | 612.5 / 624.0 | 91.76 / 103.39 | 28.55 / 28.60 | 6 / 56 |

Candidate detail probes use the protected HTTP clip route, explaining the added
50 clip requests and about 14.8 MB delivery per N/E process; reference probes
read local paths. Optional work is bounded and starts after Games paint. That
mechanism explains accounting differences, but does not establish acceptable
cost or waive the historical timing/resource gates. More matched evidence is
required, especially for cold private peaks, N-warm CPU, reads and useful timing.

E-cold candidate has one sustained-working-set flag; N-cold reference has one
sustained-thread flag. Retain both as watchpoints. The candidate signal requires
disposition regardless of the insufficient sample count and is **not resolved**.
All 14 valid runs have zero server errors, zero required event/request loss and
matching source hashes. Candidate S cold and each N cold run have one ordinary
cancelled server request. No stale-result/error/timeout evidence is present in
the valid runs, which is narrower than a complete reliability acceptance claim.
Optional GPU counters remain unavailable, not zero. The final 409-file source
audit passes but cannot supply run 15's missing completion evidence.

## Dedicated-library interaction results

The frozen normal candidate launched with a dedicated config and isolated
WebView profile. `LEAGUE_REPLAY_OUTPUT_PATH` and benchmark environment were absent.
Two disposable roots were created: A three games/ten clips (one intentionally
invalid optional-duration clip), B one game/one clip with an overlapping numeric
ID and a distinct Lux label. Prepared benchmark fixtures were never mutated.

Accessibility observation confirmed ordinary Games rendering: three Ahri cards,
zero saved, recordings 23.9 MB, clips 2.67 MB, total 26.6 MB. Data Dragon asset
errors did not prevent that view. The normal build returns no benchmark session;
event-writer/terminal handlers remain compile-feature gated and automated scenario
navigation did not run. All 25 disposable-root files still match their hashes.

The required interaction sequence is **environment-blocked**, not passed.
Windows capture failed with `SetIsBorderRequired ... 0x80004002`; after fresh
window selection accessibility worked, but clicking Clips failed with
`coordinate input geometry is unavailable`. A keyboard attempt was rejected for
input interference; after re-observation, the Tab retry produced no observable
focus transition. No Clips entry/selection, delayed-owned-batch mutation,
save/delete/cleanup/export, A/B root switch, optional-failure presentation,
viewer-return or exported-media playback result is claimed. No export exists to
probe/decode. The test process alone was then stopped. Raw accessibility evidence
and app logs remain under local `manual/`; sanitized status is in
[`manual.json`](manual.json).

The configured-path/resolved-root identity mismatch remains a follow-up risk.
The initial absolute root worked; unavailable A/B interaction evidence neither
reproduces nor clears the alias risk. No scope expansion or architecture change
was justified.

## Defects corrected and verification

Preflight contradicted benchmark-only readiness assumptions: reactive duration
publication could start the scenario before historical useful; legacy namespaces
skipped the compatibility drain; a false drain result could be treated as ready.
Separate started/emitted state, explicit drain-success handling and legacy Games
draining fix those paths. Production snapshot/controller/token/race ownership is
unchanged. Analyzer validation now pins the frontend request origin, marker/event
count types and required primary metric; historical clock semantics are unchanged.

- Frontend: 107 tests passed across 16 files, including two deferred App readiness
  cases. Two isolated reference parity cases passed.
- Python replay suite: 91 tests run, one existing opt-in skipped. New versioned
  event/origin/primary-metric cases passed.
- Rust app: 86 passed, one packaged-runtime opt-in ignored; fmt and Clippy with
  warnings denied passed. The initial cancellation fixture was blocked by
  effective PowerShell Restricted policy; the successful full rerun used only
  process-local `PSExecutionPolicyPreference=Bypass`, without changing tests or
  system policy. Earlier M2 packaged-probe results remain accepted.
- Normal candidate, benchmark candidate and diagnostic reference desktop builds
  passed, including final TypeScript check and Vite build. Initial reference build
  lacked staged runtime; staging the identical locked bytes resolved it before
  freezing. Helper console-encoding/preparation failures were preserved and never
  counted as benchmark launches.
- Final canonical validation, evidence integrity and whitespace results are
  retained in `final-checks.json`.

Exact command/exit receipts are in [`verification.json`](verification.json);
full logs remain under `build/qb010-m4/checks`. No recorder/media-runtime or
unrelated seek/scrub/decoder/capture campaign was repeated. No payload split was
started, and durable architecture needed no new claim from these results.
