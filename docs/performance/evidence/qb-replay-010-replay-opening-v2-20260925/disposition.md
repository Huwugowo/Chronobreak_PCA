# Replay-opening v2 replacement disposition

The reviewed collector-reconciliation-v2 ExecPlan permits one fresh fixed
comparison after its collector gate. That gate passed on 2026-09-25: 124 replay
tooling tests run, 123 passed and one existing opt-in skipped; deterministic
actual-code and native ordinary/final accounting checks passed; focused reviews
found no unresolved collector issue. Exact logs, prior failures, native identities
and source hashes remain under build/qb010-replay-comparison-v2/checks/.

The first opening campaign remains closed and insufficient. Its plan SHA-256 is
649e49466929f19d575d0deac752487e0a2247dbebab831d4b9f52ecf621ee80:
seven attempts, six individually valid, one invalid, zero retries. Its 93 unused
slots are forfeited. None of its results are reused, promoted or pooled.

The new sentinel is
build/perf/qb-replay-010-replay-opening-v2/.chronobreak-replay-benchmark.
The replacement ceiling is exactly 100 launches (50 adjacent pairs), giving a
lifetime opening-comparison ceiling of 107. No pilot, live smoke, selective
replacement or automatic retry is permitted. An invalid slot stops collection.
An interrupted prefix may continue only under the immutable plan's exact valid
completion and identity rules; an incomplete started slot is invalid.

Both subjects retain their original optimized build provenance and unchanged
product bytes. Reference commit 3b14df20aa7ab4e567fd0372c1abc72c07bbe18e has
binary SHA-256 34bd87bda70ea723a9c2d874a8c612038be3fb2149073d6753fc24ea48bd41c0.
Candidate commit a840c645e5080ec13a7cb23c540eabc388676a83 has binary SHA-256
46323b37c1bb7e29d2df89f5dca115e7837796c1eacc5d7342b517ebbe854f1f.
Both carry the identical locked r6 runtime. No rebuild is needed for the external
collector correction. New frozen tools distinguish this acquisition from the
closed campaign; app/scenario/wire semantics are unchanged.

Preparation completed before launch. The original and r2 partial attempts remain
preserved without receipts and are excluded. Distinct r3 S/R/L, N and E roots
passed source/copy SHA-256 and write-time checks, with 203 full packaged-runtime
decodes and zero decode errors; archived source provenance is unchanged. The
frozen campaign plan has SHA-256
cc681fb7fb648bc17c7fdd5dee96b52f58141fd62bb10640ebfb8a8e0e86e681, contains 20
matrix plans, and all 100 no-launch preflights passed.

The one-shot collection stopped at launch 8. Seven slots are individually valid;
slot 8 is N warm/reference/trial 1 and is invalid because the runner did not
find terminal.json. Its app exited 0, fixture post-hashes matched and final Job
accounting was 118/118 with zero active, terminated or unobserved processes.
The preserved runner-error.json, collection-result.json and full raw-derived
artifacts remain under results/o-n-warm-ref-001-reference-trial-1-minimal/. Sanitized summary: failed-campaign.json.
The campaign stopped without retry; its remaining 92 planned slots are forfeited.
With fewer than five valid paired processes per arm, no comparison or performance
pass is claimed. The strict evidence is complete and the feature remains in progress.
