# QB-CAP-003 execution checkpoint

Feature: `QB-CAP-003`
ExecPlan: `docs/exec-plans/qb-cap-003-interrupted-recording-recovery-v3.md`
Updated: 2026-10-30

## Current milestone

Focused engineering correction of e5bbc92 implemented and automatically verified.

## Active unit

None. No unvalidated product edits remain.

## Completed

Original receipt-bound no-replace publication/reconciliation and strict app receipt
compatibility remain intact. Directory/media IDs, volume, length, last-write, UUID,
strict existing media validation and exact timeline equality still precede promotion.
Canonical/conflicting/ambiguous bytes are retained without duplicate library entries.

Review established three defects: recovery preceded watcher construction; every pass
restarted the same enumeration/probe prefix; canonical entries pinned full ancestry.
V3 removes recovery from startup and the watcher loop. One joined maintenance pass
runs only after startup, active capture and post-game owners join, before
ShutdownComplete, with the production owner lease retained. Fresh process checks
before regions, candidate pinning and probes latch the first League observation and
end admission. An admitted probe remains joined; no task/thread/timer/service added.

Canonical metadata now declines recovery before ancestry pins or per-entry process
queries. Actual candidates still pin/recheck metadata, validate identities and use
unchanged handle-relative no-replace renames. A fixed-size directory-bound advisory
prefix persists bounded depth-first discovery across exits; only saturated regions
split, exhausted/empty regions prune. Exact-prefix bundles precede descendants.
Entries <=512, prefix searches <=512, probes <=4, ten-second admission; no growing
skip/full sorted scan. Search handles close before awaits. Unknown/corrupt/copied
cursor bytes stay intact and permit a bounded root pass without persistent fairness.
Unchanged prefixes incur no rewrite/sync.

## In flight

None.

## Remaining

Interactive open/seek and real League compatibility/FPS/GPU evidence remain pending.
These observations do not block this focused correction/commit. Feature stays
in-progress because its canonical manual verification is not established.

## Verification

All generated/temporary libraries; no real recordings modified. Correction logs:
`build/verification/qb-cap-003-review/`.

- Final locked recorder all-target/all-feature tests: 241 library + four binary +
  three fixture passed, ten opt-in tests ignored (`tests-final.log`). Formatting
  and strict all-target/all-feature Clippy passed (`clippy-final.log`).
- Packaged crash fixture explicitly ran and passed (`packaged-final.log`). Processes
  killed/reaped at pending sync, video rename and metadata rename; surviving source
  bytes unchanged, repeated recovery used zero probes, full decode and three seeks
  passed. Valid footage behind six invalid candidates recovered through production
  discovery in four reopened passes (15 total probes, at most four per pass).
- 600 canonical entries plus eight invalid receipt-bound candidates: every candidate
  reached across 17 reopened cursor passes, all per-pass caps held, all source trees
  unchanged. Final first pass with actual minimal ProcessWatcher queries: 512 entries,
  six process queries, 65.14 ms. Entire mixed-policy deterministic fixture: 438.61 ms.
  Later passes use a deterministic idle callback; this is not a League/FPS benchmark.
- Actual cost review rejected per-entry process-list refresh: initial measured
  512-entry pass cost 4.476 s / 513 queries. Moving fresh checks to discovery regions
  and actual candidates preserved game admission while reducing measured work above.
- Focused regressions cover game present before any library access, appearance before
  discovery and after an exact-prefix probe (no resume/search/cursor save), refused
  candidate pinning, canonical junction fast skip with no process query, preserved
  incomplete junction rejection, copied/foreign/malformed progress hints and all
  preexisting receipt/conflict/integrity cases. No new ignored tests added.
- Independent focused code review found an admission check missing before discovery
  and candidate inspection; corrected and regressions passed. No further concrete
  ownership, media-mutation or work-bound defect found in that review. Root checked
  post-probe discovery admission and removed unchanged-hint writes.
- Canonical validation and whitespace checks run before commit. App/frontend code
  unchanged by the correction; prior app/frontend results retained below, not rerun.

Original e5bbc92 verification (retained provenance): recorder 238+4+3 passed, ten
ignored; packaged recovery passed; app library 17 including actual recovered output,
full app rerun 163 passed/ten ignored. One unchanged export-staging assertion failed
initially, passed isolated and on full rerun. Recorder/app fmt/Clippy, recorder check,
frontend check/build and canonical validation passed. Logs `.codex/logs/command-20261030-124134.log`,
`124211`, `124435`, `124725`, `124746`, `124909`, `125016`, `125110`, `125244`,
`125543` (same command/date prefix); copied under `build/verification/qb-cap-003/`.
Earlier fixture corrections reused production mux flags and enabled OpenOptions
write=true; validation was never weakened. Interactive Media Player Open could not
be observed because native capture timed out; no interactive success was inferred.
Retained generated fixture: `C:/Users/Hugo/AppData/Local/Temp/.tmpW60ILB`, sentinel
marked; recovered sample under `build/verification/qb-cap-003/recovered`.

## Deviations

User explicitly authorized review, correction and commit without a separate planning
turn. V2 scheduling was materially invalidated by capture-first priority; immutable
v3 was reviewed/linked before product edits, predecessors preserved. Hint read/write
failure cannot weaken media proof: unreadable hints are preserved and only bounded
root discovery remains available. Broader app/frontend checks were not replayed for
this recorder-only correction.

## Decisions

Shutdown maintenance is the only simple non-concurrent placement that removes all
recovery filesystem work from an available service's capture admission path. An
idle-loop pass or timeout cannot preempt an already blocking OS operation. Graceful
exits with no League are required for progress; capture wins over completeness.
Prefix completion relies on exhausting a search, never on result ordering. Saturated
regions descend deterministically through the canonical name grammar (depth <=24),
so finite stable libraries with valid writable progress and eligible exits are covered.
New entries behind the frontier wait for wrap. Hints authorize scheduling only;
recognizable version/directory-bound private hints may be updated, other bytes stay.
Receipts still detect ordinary generations, not hostile timestamp-restoring edits.

## Blockers

No implementation/commit blocker. Real League measurements and interactive playback
remain pending. No orderly exit/continuous League presence can defer recovery
indefinitely. Corrupt/unwritable hints lose cross-exit fairness but not safe root-pass
recovery. Bare partials and receipt-less older pairs are not promoted. An admitted
probe or OS filesystem operation can extend orderly exit and hold the owner lease;
there is no hard shutdown deadline or guarantee against every launch/OS-call race.

## Next action

Observe open/seek on retained generated recovered footage and measure real League
compatibility/FPS/GPU when access is available; record those results before any
feature-done claim. No further implementation or validation campaign is indicated.
