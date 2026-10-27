# QB-DEV-006 — First-use benchmark export validation

## Purpose

Accept a successful first production backend export in an isolated benchmark library without rejecting the persistent idle export lease. Continue rejecting unexpected outputs and unsafe staging; preserve production publication and recovery behavior.

## Relevant planning-time architecture

`tools/replay_benchmark/run.ps1:Get-ClipDirectorySnapshot` snapshots direct clip entries before launch and after the owned process tree exits. `Invoke-ExportValidation` rejects every new directory, validates the unique `export_completed` payload, exact requested presets and fixture-derived filenames, requires new MP4/JPEG pairs, probes H.264/AAC/duration/size, fully decodes each MP4 and hashes both members. The analyzer requires `newly_created_entries` to be exactly the reported output files. Source integrity and finite process shutdown are separate unchanged runner gates.

`app/src-tauri/src/export_staging.rs:Lease::acquire` creates `clips/.chronobreak-export/.lease`, a regular zero-byte file with an OS-held lock. `Batch::drop` removes only its validated flat batch, leaving that parent and lease. Successful idle staging has no persistent parent ownership manifest. The per-batch ownership record has already been removed; the post-process validator cannot prove an earlier OS lock. Production directory guards, no-replace MP4-first publication, bounded recovery and benchmark read-only recovery policy are described in `docs/architecture/media-library.md` and remain authoritative.

## Scope and non-goals

Change only runner classification of newly created idle staging, regression coverage and explanatory documentation. No Rust/export/recovery changes, fabricated preparation directory, cleanup, broad directory whitelist, analyzer relaxation, measurement threshold change or frontend refactor. Preexisting staging remains governed by existing behavior: explicit benchmark export preserves earlier staging, including retained batches. This feature does not certify arbitrary preexisting staging contents or add a recursive inventory contract.

## Exploration findings

The preserved QB-DEV-004 first-use failure contains a valid fully decoded 5-second H.264/AAC MP4 and JPEG. Both errors concern the new staging parent. Subsequent genuine exports pass because that parent is then preexisting. Recognizing a name alone would hide foreign output. Conversely, removing the production parent/lease would compromise its established safety contract. There is no durable parent provenance marker to validate after shutdown; recognize the exact idle shape only within the already validated sentinel, coupled with the existing successful export evidence.

## Chosen design and rationale

Add a focused read-only helper in `run.ps1`. Only a newly created direct directory whose name is case-exact `.chronobreak-export` is a candidate. Assert containment below the current clips root and reparse-free paths. Enumerate direct children with `-Force` and require exactly one: case-exact `.lease`, a regular non-directory, non-reparse, zero-byte file. Use ordinal name comparisons. Open the existing lease read-only with `FileShare.None` and check its length while held: this nonblocking exclusive-share check rejects an active production lease or another open writer without creating or changing a file. Missing lease, differently cased names, extra/hidden files, nested batches/ownership records, nonempty/held lease and reparse paths fail closed. Enumeration/read errors remain validation failures. No recursive traversal, deletion, write or recovery occurs.

Only after complete successful classification, exclude this residue from `newly_created_entries` and the unexpected-entry checks. Preserve separate runner-only `staging_residue` diagnostics with canonical relative directory/lease paths, zero-byte size and exclusive-access result. These diagnostics are not an analyzer-validated ownership claim. The output-entry array retains its existing analyzer meaning and exact-set rules; analyzer code and thresholds remain unchanged. All existing output/event/media/source/shutdown checks are still mandatory; residue alone cannot make a run pass. Every other new directory or file retains its previous rejection.

## Rejected alternatives and planning decisions

- Reject name-only or recursive staging exemptions: foreign files and failed batches must remain visible on first use.
- Reject changing production cleanup or preparing an empty staging directory: these bypass the mismatch and alter or hide actual first-use behavior.
- Reject treating the idle lease shape as a durable provenance or cryptographic ownership proof. The receipt documents precisely what was checked.
- Preserve preexisting staging semantics rather than expanding this bounded correction into recovery/inventory redesign.

## Milestones

1. Add executable PowerShell validator regressions before changing the runner and retain the failing first-use result. Freeze assertions for the exact output-entry contract, subsequent use, strict negative shapes, output failures and preservation of preexisting staging.
2. Implement the narrow read-only classification and residue receipt; run focused tests. Review the changed boundary against production staging without changing it.
3. Run the complete replay analyzer/tool suite and fresh native first/subsequent production backend exports in a new sentinel library. Inspect decode/codec/duration, exact outputs, source/preexisting preservation and shutdown. Update durable documentation and canonical completion evidence only when every required check passes.

## Verification design

Run `python -m unittest discover -s tools/replay_benchmark/tests -p test_export_staging.py -v` for focused tests, then `python -m unittest discover -s tools/replay_benchmark/tests -v`. Tests invoke the actual runner functions in PowerShell with temporary directories; media-tool stubs isolate directory classification while existing analyzer negatives and native packaged media establish the unchanged media contract. Cover first use, no residue, subsequent use with retained earlier staging, foreign/case-mismatched directory, extra/hidden child, batch/owner record, absent/nonzero/directory/case-mismatched/held lease, staging/lease reparse points, and unexpected top-level file. Assert expected codec/decode/output failures still reject alongside valid residue. Link tests use Windows directory junctions or file symbolic links when supported and report any unavailable case explicitly. Compare recursive bytes/hashes for preexisting valid and foreign/malformed staging fixtures before/after validation; validation must not recover or mutate them. Production explicit-export preservation is also covered by the existing `export_staging` backend tests; run that focused suite unchanged.

Use a fresh `build/perf/qbd6/.chronobreak-replay-benchmark` root, existing explicitly identified generated preproduction bundles, the locked packaged runtime, copied offline cache and an immutable production benchmark executable with its hash recorded. Use `prepare.ps1`, `matrix.py plan/verify`, and `run.ps1` preflight/run, retaining unique result roots for first and subsequent exports. Do not precreate staging or remove earlier results. A previously built preserved executable is suitable here only because executable inputs do not change: this is a runner correction, not a new application build claim. Record exact preparation/run commands and fixture/binary identities in the execution checkpoint. Native evidence must show the first run began without staging, the next began with the genuine residue, both analyzer-valid, source hashes unchanged, complete decode and finite shutdown. Hash the first output pair before/after the second run.

Run canonical schema/invariant/linked-artifact validation from `VERIFICATION.md` and `git diff --check`. Rust/frontend/recorder rebuilds are outside this runner-only dependency cone; no new app behavior or performance claim is made. Preserve all failed evidence.

## Performance and reliability gates

The helper runs only after process shutdown, reads one directory level and validates the sole allowed child. It does not change measured actions, timing, observer events, output publication or recovery. New unsafe or foreign artifacts fail validation. Source media and preexisting outputs/staging remain untouched. No statistical performance campaign is required for this classification correction; a media/source/shutdown failure blocks completion and cannot be excused by a valid residue.
