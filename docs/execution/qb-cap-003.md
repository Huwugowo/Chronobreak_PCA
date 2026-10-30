# QB-CAP-003 execution checkpoint

Feature: `QB-CAP-003`
ExecPlan: `docs/exec-plans/qb-cap-003-interrupted-recording-recovery-v2.md`
Updated: 2026-10-30

## Current milestone

Implementation and available automated verification complete; manual gate pending.

## Active unit

None. No unvalidated implementation edits remain.

## Completed

Bootstrap, original review and two read-only scouts; immutable reviewed plan handoff.
Receipt binds UUID to directory/media IDs, volume, length and last-write. Pinned
ordinary paths and handle-relative no-replace publication/rollback preserve conflicts.
One owner-only startup pass before capture, bounded to 512 entries/four probes/ten
seconds of admission. Admitted probes join before return; shutdown checked between
candidates. Existing strict validator/producer reconciliation reused. Both staged
publication states recover in place, without new game directories. Ambiguous,
invalid, inaccessible, missing-evidence and deferred bytes are retained and logged.
Strict app reader admits/preserves optional receipt and rejects other unknown fields.
Small lifecycle/replay-time architecture updates. Focused review reporting issue
resolved; no other concrete safety defect found.

## In flight

None.

## Remaining

Required interactive open/seek observation. Keep feature non-done until established.
No League campaign; League compatibility/FPS/GPU remain separately pending.

## Verification

All on 2026-10-30, generated temporary libraries only. Logs under `.codex/logs/`
and copied to `build/verification/qb-cap-003/`:

- Recorder check/all-target/all-feature tests/fmt/Clippy passed. Baseline log
  `command-20261030-124211.log`; final tests, packaged crash fixture, fmt/Clippy
  `command-20261030-125244.log`: 238 library, four binary, three fixture passed;
  ten opt-in tests ignored. Packaged recovery explicitly ran and passed separately.
- Focused finalizer suite including packaged fixture: 14 passed, logs
  `command-20261030-124134.log` and post-review `command-20261030-124435.log`.
  Child killed/reaped after pending sync, video rename and metadata rename.
  Incomplete pairs each recovered with one probe; repeats used zero probes with
  exact video bytes and unchanged two-file tree. Completed pair used zero probes.
  Strict full decode and 0.1/1.0/1.8-second seeks passed. Recovered cases took
  276/269 ms including decode/seeks; this is not a League or FPS/GPU measurement.
  Killed unfinalized mux output retained as MissingEvidence with zero probes.
  Identity/copy/reparse/collision/oversize/torn/foreign metadata and probe/shutdown
  budget cases passed. Final foreign-field guard included in final recorder pass.
- App library tests: 17 passed (`command-20261030-124725.log`), including actual
  recovered fixture -> one playable entry on repeated refresh, descriptor admission,
  receipt save roundtrip and unrelated unknown-field rejection.
- Full app final rerun: 163 passed, ten ignored (`command-20261030-125110.log`).
  Initial attempt: 162 passed, one unchanged export-staging assertion failed at
  report.retained >= 2 (`command-20261030-124909.log`). It passed isolated
  (`command-20261030-125016.log`) and on full rerun. Export cursor policy unchanged;
  original intermittent failure retained as evidence.
- App fmt/strict Clippy and recorder fmt passed (`command-20261030-125016.log`).
- Frontend check/build passed (`command-20261030-124746.log`); existing dynamic-import
  and plugin timing notices only. No frontend code changed.
- Canonical schema/graph/artifact validation passed: 104 items, 24 linked pairs.
  Diff whitespace validation passed.
- Interactive playback/seek environment-blocked: browser connector disabled locally;
  Windows Media Player launched but Open yielded unavailable accessibility then
  FrameArrived capture timeout. No interactive success inferred from decode/seeks.
- Restricted Cargo attempt never ran because required logger could not write its
  log; successful tests used normal local access. First fixture omitted production
  delay_moov/negative-timestamp flags and correctly failed cadence validation;
  reused existing production flags. Next run exposed missing OpenOptions write=true
  for create_new despite Windows access flags; fixed before passing publication
  tests. No validation weakened.
- Retained generated-only fixture `C:/Users/Hugo/AppData/Local/Temp/.tmpW60ILB`,
  sentinel marked. Recovered sample copied to `build/verification/qb-cap-003/recovered`.
  App tests mutated only fresh temporary copies. No real recordings used.

## Deviations

User authorized planning followed by implementation in one session; validated
handoff preceded edits. Original plan assumed unknown metadata fields were tolerated.
Strict app inspection contradicted this: preserved first plan, finalized/reviewed v2
and linked it before the narrow reader change. Manual playback could not be observed.

## Decisions

Removed QB-CAP-002 dependency during planning: existing clean terminal producer
facts suffice. Receipt-less pairs/bare partials retain bytes without invented proof.
Receipts detect ordinary generation changes, not hostile timestamp-restoring edits.
App treats receipt as bounded opaque data; canonical timeline remains media authority.
Foreign pending metadata fields are rejected to match the strict canonical reader.

## Blockers

Interactive desktop capture timeout leaves manual gate open and QB-CAP-003 in-progress.
No League access: compatibility/FPS/GPU measurements pending. Startup caps may leave
entries unexamined; complete large-library salvage/fairness and bare-fragment remux
are not implemented.

## Next action

When interactive desktop access works, open and seek retained generated recovered
output, record the result, then reassess completion. No additional implementation
or automated campaign is required by current evidence.
