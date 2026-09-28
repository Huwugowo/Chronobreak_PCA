# QB-REPLAY-010 execution checkpoint

Feature: `QB-REPLAY-010`
ExecPlan: `docs/exec-plans/qb-replay-010-replay-opening-collector-reconciliation-v2.md`
Updated: 2026-09-26

## Current milestone

2026-09-26 user clarification: the replacement stopped because the user manually
closed the terminal. It is an externally interrupted measurement, not an
application or benchmark-infrastructure defect. Do not investigate terminal
publication. The incomplete slot remains excluded from performance evidence.
The user now authorizes at most five new reference/candidate pairs, reusing the
prepared inputs, and prohibits another 100-slot campaign or full decode pass.
Screen use must be brief; native measurement is pending clarification because
the existing protocol requires a visible always-on-top window, which cannot be
embedded in ChatGPT's browser panel.

Collector reconciliation v2 M1 is complete (2026-09-25): actual-code deterministic
and native gates, full replay tooling suite and focused implementation reviews
pass. The correction preserves the strict 100/500 ms deadlines, 256-message
production batches, 32/128-pass caps, wire fields and original campaign invalidity.

M2 replacement collection is closed invalid after the frozen r3 campaign stopped
at launch 8. Seven slots are individually valid; slot 8 (N warm/reference/trial 1)
has preserved artifacts but no terminal.json, so the strict runner rejected it.
The app exited 0, fixture post-hashes matched and final Job accounting was
118/118 with zero active, terminated or unobserved processes. The new campaign
uses no retries or pooling; its remaining 92 slots are forfeited. The original
seven-attempt campaign remains separately closed with 93 slots forfeited. The
lifetime opening ceiling remains 107 attempted slots; no performance comparison
or feature pass is claimed. Candidate remains a840c645e5080ec13a7cb23c540eabc388676a83;
reference remains 3b14df20aa7ab4e567fd0372c1abc72c07bbe18e. Full feature stays
in progress.

### Closed UI reconciliation

UI reconciliation M1-M3 and combined native acceptance completed 2026-09-23.
The repository has since advanced to merge commit a840c645 (parents 3426cfa and
6c7174f), with a clean worktree at this resume. This supersedes the prior
checkpoint's uncommitted-merge statement; the agent did not create that commit.
The accepted captured CSS, original input patches, normal subject, diagnostic
receipts and source audits remain preserved. No UI or native acceptance is
reopened; no subsequent sibling edits are included.

### Closed descriptor unit

Minimal replay descriptor M1-M3 complete, 2026-09-22. The separate immutable
descriptor plan was finalized before implementation from checkpoint
`3b14df20aa7ab4e567fd0372c1abc72c07bbe18e`. Backend ownership and descriptor-first
viewer/navigation pass the required automated suites and normal/benchmark builds.
User-operated native acceptance passed on the freshly rebuilt normal subject and
dedicated strict-v2 A/B fixtures: real A/V, controls, held/failed/late details,
retry, clip preservation, stale navigation and overlapping-ID roots. The final
ungated Ahri check passed; the app and diagnostic listener are closed and all
source hashes remain unchanged. No descriptor defect required a product change.

QB-REPLAY-010 remains in progress for separately planned remaining replay scope
and full-feature performance/manual gates. No descriptor latency/I/O improvement
or full-feature completion is claimed. The closed V2/M4 library unit and its
immutable plan remain unchanged; no campaign or disposition was reopened.

### Closed library unit

Attribution M1-M3 concluded with a material library cause. Reviewed v2 library
implementation design is finalized; v2 M1, M2 and the requested frontend M3 are
implemented, verified and independently reviewed. M4's fresh replacement completed
all 50 valid launches and deterministic verification. N/E Games-usable gates pass
and S has no material regression. The four requested historical/resource/reliability
questions now have explicit evidence-backed dispositions with no production change
warranted. The V2 library implementation/benchmark unit is closed. User-assisted
Windows acceptance passed on disposable fixtures, including mutation, A/B/A root
ordering, export, cleanup, source preservation and optional-work overlap. M4 has
no remaining blocker. QB-REPLAY-010 as a whole remains in progress for its separate
descriptor, timeline and playback scope. The immutable attribution plan remains at
`docs/exec-plans/qb-replay-010-measured-library-and-replay-opening.md`.

## Active unit

Narrow performance feasibility and prepared-input reuse. A 14.017-second hash
audit matched all 409 source/staged file identities and write times, both frozen
subjects/archives/build receipts/runtimes, all 20 matrix seals, 100 existing
manifest seals, configs and frozen tools. The 203 successful decode receipts
remain applicable; zero decodes, builds, preparation passes or app launches ran.
Receipt: build/qb010-replay-comparison-v2/checks/reuse-20260926.json.

Criterion 6 remains the combined replay/resource comparison gate. Its current
design requires five processes per arm in each S/R/L/N/E cold/warm stratum.
Ten launches can establish one stratum only, with unchanged metric/validity
thresholds; it cannot establish the complete gate. E cold is the brief candidate
because it exercises the largest selected log and 50-game/50-clip library.
E warm took about 42 seconds per process before overhead/cooldown, so five pairs
would take roughly eight minutes. No native launch is authorized while the
user's inside-ChatGPT versus brief-visible-window constraint remains unresolved.

### Closed reconciliation implementation and verification

No active reconciliation implementation or verification unit remains. M1-M3 are
complete: controller/token/descriptor ownership, participant cards, strict roster
decoders, display-only optimistic save reconciliation and shared rail cleanup are
verified together. Final frontend suite has 166 passing tests; Rust has 92 passing
tests and one pre-existing opt-in ignored; typecheck/build/fmt/Clippy and both
desktop builds pass. Native controls, delayed/failed/retried details, clip/fullscreen
continuity, normal exporter return, filters, save/unsave and A/B/A passed on the
fresh normal subject. Gate removed with zero pending responses; title-bar closure
verified by absent subject/listener. Final video/game-log hashes are unchanged.

The original sibling CSS patch is captured and applied. The sibling remains at
6c7174f and untouched; on 2026-09-23 it additionally has uncommitted LibraryScreen.tsx
and SettingsScreen.tsx changes. Those later edits are outside the captured input;
do not overwrite or silently include them. The original five CSS edits are retained.

Descriptor implementation and acceptance are complete in commit `3426cfa` under
`docs/exec-plans/qb-replay-010-minimal-replay-descriptor.md`. Its native artifacts
remain under `build/qb010-descriptor-manual-20260922/`; no descriptor/M4 rerun follows
from this handoff. No new semantic backend implementation is active.

### Historical closed M4 execution

M4 closed 2026-09-21. The user completed the disposable normal-build sequence:
Games remained usable through an unavailable optional duration; save and clip
deletion occurred during a verified optional child hold; one unsaved recording was
deleted; A/B/A switching showed B's one Lux game/clip and restored A's intended
two-game/one-saved state; source and exported media played; cleanup retained the
saved game and clips; and export submission occurred during verified optional work.
`hold-export2.json` verifies the direct packaged `ffprobe` child and parent identity
with no watcher errors; the user confirmed **EXPORT 1 MP4** while HOLDING. The
retained `audit-export-overlap-verified.json` validates four generated H.264/AAC
exports, their selected durations and full decodes; it shows only deliberate A
fixture mutations and unchanged B. `audit-final-verified.json` records the final
one saved game, retained clips and restored Never retention. The helper's receipt
does not infer UI correctness; these observations are explicit user confirmation.
No benchmark was rerun and no production change was made for M4 closure.

2026-09-21 manual harness follow-up: user launched normal subject PID 1192 and
reported Clips ready, but `hold-roots.json` ended `not-observed`; folder switching
is not accepted. The old receipt contains no rejection details, so the exact
cause is unresolved. Standalone observation caught 10/10 fast invalid-media
probes (26-39 ms lifetime) plus two delayed controls; this does not reproduce or
explain the specific user attempt. No product defect or cache/retry failure is
established. A scout's active-batch retry hypothesis was not adopted: the UI
publishes batch results after the whole response, then clears the active batch
synchronously, so it did not establish the claimed user-click window.
The manual helper now resolves the expected probe path once, retains bounded
candidate/open/identity/path/exit/suspend diagnostics, propagates receipt-write
errors through its resume guard, and reports a failed hold without a traceback.
Path/parent verification is unchanged; the disposable helper permits a 30-second
manual window. Four focused rejection tests pass; replay Python ran 95 tests successfully (one
existing opt-in skip), py_compile/help and git diff --check passed. A standalone
owned packaged-child smoke proved suspension and successful resume; no UI pass
is inferred. Diagnostic sources/results: `build/qb010-m4/manual_watch_diagnostic.py`,
`manual-watch-diagnostic.json`, `manual_hold_smoke.py`, `manual-hold-smoke.json`.
This dated diagnostic record is superseded by the completed user-assisted sequence
above.

2026-09-21 resume: git HEAD remains `3a750e2`; the accumulated M1-M4 changes
are not yet committed. At that resume audit no app was running; launch/log slots were available
after preserving the closed PID 8328 receipts. There is no `hold-roots` receipt:
folder switching is the next unperformed check. `audit-resume-20260921.json`
reconfirms A has two recordings, one saved and nine clips, with the same
intentional deletions and saved metadata change; B is unchanged. Do not repeat
the benchmark or already accepted manual actions. Launch the normal subject from
the user's visible terminal, continue folder switching, then export/return and
cleanup, followed by final verification/documentation and the requested commit.

User-assisted acceptance update, 2026-09-17: the user launched the frozen normal
subject on their visible desktop. Initial Games, local optional failure/retry and
valid clip playback were reported working. The `games` hold caught a child, but
its navigation timing was not confirmed. The later `save` hold and explicit user
confirmation establish Games navigation/save during owned optional work: three
recordings, one saved, ten clips. `audit-save.json` confirms the saved metadata.
The user subsequently confirmed intentionally toggling SAVED several times;
the intermediate unsaved state is explained, not a reproduced regression.
The first `delete` attempt caught no child and remains failed test setup;
`delete2`, triggered by Retry duration, caught a verified child and the user
reported the instructed deletion sequence successful. Its child exited during
the hold; the audit confirms only source-less clip `1789400108_1789500008.mp4`
was removed. The user then saved the first recording and deleted a different
unsaved recording: two recordings, one saved, nine clips were explicitly confirmed.
`audit-recording-delete-verified.json` confirms only game `1789400101` was removed,
game `1789400102` is saved, retained media/log hashes match and B is unchanged.
Receipts are under the existing `manual-assisted-20260917` disposable workspace.
Those formerly remaining checks subsequently passed; this dated partial record is
superseded by the 2026-09-21 closure entry above.

2026-09-17 resume: collected the explicitly requested fresh 50-launch M4 replacement
under `campaigns/replacement-20260917-r1` in the existing M4 sentinel. Before-launch
disposition: `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/disposition.md`.
Exactly 25 new pairs; no old results pooled, no sequence-16 resume or selective
top-up. Old ceiling stays closed; new ceiling 50, lifetime maximum 65 attempts.
Frozen subjects, fixture/config bytes and measurement semantics stay unchanged.
Replacement collection completed at 2026-09-17T08:49:14.885530Z: 50 attempted,
50 valid, zero invalid; ten required-result matrices passed. Durable terminal:
new campaign `live/complete.json`, with all 50 started/completed receipts.
The former tool session expired after completion; receipts and strict bundle
validation establish completion. Never restart the one-shot runner. All 50
preflights, 10 plan verifications, normal
and benchmark desktop builds, frontend 107 tests, Rust 86 tests (one opt-in
ignored), replay Python 91 tests (one opt-in skipped), fmt/Clippy and reference
parity 2 tests passed. Receipts: `build/qb010-m4/checks/r2-*` and new campaign
`preflight/complete.json`. One reference test invocation used the wrong filename
and found no tests; the corrected exact retained parity test passed. Another
checkout's development app remains untouched, sampled at zero CPU over three
seconds, with its Cargo parents waiting. Proceed with that declared background
condition under the requested campaign authorization; retain whole-system CPU.

First V2 M4 campaign disposition: the fresh frozen campaign attempted 15 of 50 launches on
2026-09-16. Runs 1-14 are individually valid; run 15 is incomplete/invalid, with
no terminal, collector-finalization or post-hash evidence. On 2026-09-17 resume,
no campaign process remained. Do not resume the runner or launch 16 onward.
Preserve all artifacts. Every stratum has fewer than five valid processes per
arm, so the comparison cannot pass. Normal-build startup was observed; subsequent
interaction was environment-blocked at that time and later passed with user assistance.
Canonical evidence is retained at
`docs/performance/evidence/qb-replay-010-m4-20260917/`. M1/M2/M3 remain accepted.

## Completed

- 2026-09-25 collector M1 complete: bounded native dequeue with partial errors,
  exact native creation count and existing wire-record projection; shared
  snapshot/count acquisition; sample/final captured output; exclusive failure
  evidence preserving original and publication errors. Deterministic tests cover
  both race directions, churn/caps/deadlines, true loss, errors and integrations.
  Native ordinary/final tests and owned cleanup pass. Protocol updated; initial
  implementation review and focused changed-cone review have no remaining finding.
- 2026-09-25 M2 preparation and freeze completed after the first supervisor
  interruption. The original partial roots and a distinct r2 partial retry are
  preserved with no receipts and are not evidence. Distinct r3 S/R/L, N and E
  preparations passed source/copy hashes, write times and 203 packaged-runtime
  full decodes with zero errors; 20 matrices and all 100 no-launch preflights
  passed. No app launch occurred before the sealed one-shot driver.

- 2026-09-25 replacement collection stopped invalid at launch 8 after seven
  valid slots. Slot 8 was N warm/reference/trial 1; the app exited 0 and fixture
  post-hashes and final Job accounting 118/118 with zero active/terminated/
  unobserved passed, but terminal.json was never published. The runner preserved
  the full artifact and stopped; no retry/top-up/pooling is allowed. The new 92
  unused slots are forfeited and no comparison or performance result is claimed.

- 2026-09-24 first replay-opening collection stopped invalid at launch 7 after
  six valid slots; no retry/top-up. All 100 preflights passed, source/config/cache
  hashes match, six viewer cycles completed and app exit 0. The strict runner
  rejected the first active 28/29 Job/creation-count pair. All 40 sample timings
  satisfy cadence; final Job is 118/118 with zero active/terminated/unobserved.
  The app, descendants and supervisor are closed. Sanitized raw-derived
  evidence and engineering disposition:
  `docs/performance/evidence/qb-replay-010-replay-opening-20260924/`.
- 2026-09-24 deterministic offline reproduction used actual AST-extracted
  `Sync-JobNotifications`, `New-ObserverSample` and
  `Test-ProcessTelemetryComplete`: stable 28/28 passes, post-snapshot creation
  28/29 rejects with unobserved=1. No native child/app launch; runner hash unchanged.
  Probe source and receipt are retained beside the failed campaign disposition.
- 2026-09-24 collector v2 plan finalized after independent design review and
  a fresh focused resolution review. Native partial-error return, production
  test seams, total pass/message caps and failure-artifact write preservation
  are explicit. Final review found no remaining blocking gap; Windows timing
  and sampled-process-table limitations remain declared. No runner implementation
  or replacement launch occurred in this planning pass.

- 2026-09-24 comparison M1 preparation: both exact-commit optimized builds passed;
  identical packaged r6 runtimes; all archived source files unchanged. Fresh
  S/R/L and replacement N/E preparation passed with 203 full media decodes and
  409 unchanged source/copy file identities, 1,010,895,291 accepted media bytes.
  Twenty local plans and one sealed global plan freeze all 100 unused launches,
  unique profiles/scratch/results, exact scenario/config and normalized identity.
  Global precollection verifier and all 100 preflights passed. M2 collection
  has started; no performance result is yet claimed.

- 2026-09-24 comparison tooling gate: full replay suite ran 122 tests, 121 passed
  and one existing opt-in skipped (exit 0). Independent follow-up review found no
  cycle/order/no-retry defect. Environment evidence limits are explicitly retained:
  exact independently measured CPU-quantum equality can reject a run; prelaunch
  installed WebView hash plus actual descendant version is not live byte identity.
  Existing normalized identity is unchanged. Partial preflight is preserved and
  requires diagnosis/disposition before another preparation attempt, never a live
  retry. These are conservative admission/evidence limits, not permission to relax
  the approved strict fingerprint or change runner semantics.

- Comparison M1 initial tooling (2026-09-23): descriptor-agnostic extraction,
  signed payload ordering, symmetric metric inventory, five-process warm
  aggregation and seeded 100-slot global campaign ownership. Exact-byte seals,
  OS lock and durable receipts preserve immutable attempted slots. No subjects,
  corpus or live campaign have been prepared. Review fixes remain in verification.

- 2026-09-23 next-unit planning: confirmed committed integration a840c645 and
  pre-descriptor reference 3b14df2; scoped a tooling-only, minimal-observer opening
  comparison. Independent review tightened payload correlation limits, symmetric
  metric eligibility, global launch verification and workload coverage limits.
  Historical plans, product code and campaigns are unchanged.

- UI reconciliation M3 (2026-09-23): final full affected automated checks, normal
  and benchmark desktop builds, focused review and native acceptance passed.
  User-operated tests cover real A/V, seek/zoom/pan, clip edits and fullscreen
  mid-drag under held details, local failure/retry, late admission without media
  reload, semantic filters, normal editor/return, immediate/persistent Star state,
  Garen selection and overlapping-ID A/B/A. Gate restored; no pending requests;
  normal subject and scoped listener closed. Only Ahri save metadata reserialized
  (optional capture absent ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¾ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ null, saved remains its original true); all video/log
  hashes, all B files and generated source are unchanged. Work remains uncommitted.

- UI reconciliation M1-M2 (2026-09-23): retained captured UI/CSS work with existing
  controller ownership; added required strict roster parity, coherent development
  mocks matching the physical preview MP4, and optimistic save display overlay.
  Shared rail cleanup handles pointer identity, cancellation and disposal. Focused
  backend/frontend regressions pass, including fullscreen mid-drag and benchmark
  media/details arrival ordering. No extra roster filesystem read was introduced.

- UI integration planning: user confirmed the latest sibling line beyond named
  `ui/viewer-foundations-v1` (`82d6704`): participant summaries (`783e4bb`) and
  redesigned cards (`6c7174f`). Common ancestor is `e030161`. A read-only three-way
  merge dry run found App/API/LibraryScreen conflicts; no merge was applied.
  Root verified that the tentative Viewer merge retains descriptor-first resources
  and that summary roster derivation reuses already parsed logs. Incorrect scout
  assumptions about a no-log summary and lost descriptor were rejected against code.
  Fresh independent plan review requested explicit optimistic overlay lifecycle,
  strict roster boundary coverage and stable late-details media/viewport tests;
  all are specified in the finalized plan. A separate read-only scout confirmed
  the additional five CSS-only sibling edits and their dependency on MatchHistory.

- Descriptor M3: required automated verification and normal/benchmark builds
  passed. User-operated native acceptance passed on the fresh normal subject:
  held/failed/late optional details, explicit retry, A/V and ordinary controls,
  unchanged clip drafts/media, stale navigation and overlapping-ID A/B/A roots.
  The user confirmed the final ungated replay and app closure. Final audit shows
  no added/deleted/changed fixture source files; subject and listener are gone.

- Descriptor M1: four-field media projection from the unchanged strict-v2 bundle
  reader; missing/nonregular/empty video rejection; selected-token command with
  one owned blocking slot shared by the unchanged full probe. Worker ownership
  survives waiter drop; refresh/mutation/root A/B/A/shutdown reject late results.
- Descriptor M2: app-owned one-active/one-latest read scheduling, fresh viewer
  navigation origins, descriptor-first persistent media surface, optional full
  details after a paint opportunity, exact identity/time admission, local failure
  and retry, reactive existing stats/events without resetting playback or clip
  selection. Exporter and benchmark callers use the same tokenized read owner.
  Historical full-payload markers retain their meaning, not descriptor readiness.
- Deferred tests prove useful playback with held/failed details, unchanged primary
  element/source/load count/rate/clip draft after late admission or retry, unmount
  and descriptor retry, stale result rejection/coalescing, held assets/durations,
  Settings/exporter returns and overlapping-ID root changes. No ReplayTimeline,
  persistent ReplayIndex, payload split or delivery redesign was introduced.

- M4 replacement: all 25 adjacent pairs completed, five processes per arm in
  every required stratum. Reference/candidate/source/runtime identities, exact
  configs, all 409 fixture hashes and prior raw artifacts match. Reports,
  same-clock opening/resource details, matrices and provenance are retained in
  `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/`.
- Replacement cold Games medians R/C: S 166.4/168.2 ms (non-regression),
  N 2092.6/197.7 ms (90.55% faster), E 2386.6/493.3 ms (79.33% faster).
  Warm-process startup N 2087.3/209.7, E 2404.8/483.6 ms also passes the
  Games improvement gate. Five-remount per-process replay aggregates remain
  separate; no material replay first-frame regression was observed.
- Disposable assisted roots were prepared and baseline-audited after collection:
  initial A three Ahri games/ten clips; B one Lux game/one clip with overlapping
  IDs. Subsequent user-assisted observations and mutations are recorded above;
  the current A state is two games/one saved/nine clips. Remaining observations
  are in replacement evidence `manual-acceptance.md`.
- Mandatory product/workflow/feature/checkpoint/plan/verification bootstrap,
  forward-engineering and Rust skills; relevant time/security architecture.
- Earlier planning handoff at merged `3a750e2` reconciled optional 015 and replay
  ordering, preserving 009's deferred human gate and 011/012 evidence. Preserve
  those unrelated working-tree documentation/roadmap edits.
- Frozen benchmark app/runtime at source
  `3a750e2c003d6e3605b2f907ff19ad7898d3dbd2`, with zero product behavior diff.
- S/R/L strict-v2 corpus and N/E fixed-media/identity 50-game/50-clip libraries;
  all 203 staged media files passed packaged probing and full single-thread decode.
  Corpus size 2.198 GiB under the 8 GiB bound before result collection.
- All five immutable plans and 30 runner preflights passed. All 30 launches were
  attempted in seeded order; 28 are valid and invalid 14/18 remain preserved.
- Added `opening_attribution.py` and synthetic tests. Extracted 53 frontend cycles
  from 28 validated runs: 23 cold, five initial warm mounts, 25 warm remounts.
- Completed the one narrower diagnostic: aggregate timers on actual `list_clips`
  in one optimized native test process, N/E/E/N/N/E. Six calls each returned 50
  clips with positive durations. Removed timers; restored library.rs byte-for-byte.
- Sanitized measurements, diagnostic patch/binary/log identities, launch/artifact
  hashes and disposition retained at
  `docs/performance/evidence/qb-replay-010-20260915/`.
- V2 independent review required explicit primary event validity and secure probe
  input. Applied both; fresh bounded amendment review passed. Existing capability
  routing preserves opened-handle containment; versioned events preserve old reports.
- V2 M1: `library.rs` builds one snapshot with one summary/video-size read per
  accepted game, one clip enumeration, same-scan enrichment/accounting, storage-only
  invalid-name MP4s, and explicitly unknown durations without probes. Strict v2,
  incomplete cards, ordering and source-less clips retain their semantics.
- `library_coordinator.rs` owns one blocking scan slot and one complete snapshot.
  Busy admission never queues; cancellation/shutdown retain the worker permit;
  stale revision/root results (including A/B/A) cannot publish. Same-root scan and
  worker errors retain the previous view; a successful refresh always rebuilds.
- AppState owns the coordinator and exposes the real async `refresh_library` in
  both builds. MediaRoots clones path/ApprovedRoot under one read guard; capture
  and settings publication use coordinator-then-MediaRoots ordering. Settings
  persist before root/revision publication. Independent M1 diff review found no
  actionable issue; root review reconciled worker join failures into the same
  revision/failure path. Durable media-library architecture updated.

- V2 M2: optional duration command accepts at most eight distinct current clip IDs,
  runs one owned blocking batch/child, caches only current-snapshot success or local
  unavailable state, and supports explicit failure retry. Child stdout is capped at
  4 KiB with a ten-second deadline; cancellation/timeout/error kill and reap it
  before releasing the slot. Stderr is discarded. Blocking metadata checks reuse
  QB-REPLAY-011 validated handles and record volume/file identity, length and mtime.
- Probes use only the existing protected clip HTTP route and packaged ffprobe,
  forced MOV demuxing, external/absolute references disabled and `http,tcp` only.
  Replaced/deleted files and stale tokens cannot publish durations. Refresh and all
  invalidations clear optional results. No new delivery route or authority.
- Save/delete/retention and settings use one owned mutation slot off async workers.
  Selected tokens/root/IDs are checked at admission; stale/missing selections reject.
  Completion guards invalidate even on partial errors, panic or dropped waiters.
  Failed settings persistence retains the published root/revision. Export keeps its
  original destination; current matching-root views invalidate even after A/B/A,
  while original epoch/token gates reject old response admission.
- Independent full M2 diff review found no actionable issue. Root then corrected
  matching-root invalidation after export A/B/A or same-root settings; a fresh
  focused independent review found no actionable issue. Relevant media-library
  architecture is updated. All destructive tests use disposable tempfile fixtures.

- V2 M3: `LibraryController` is the sole frontend source for the core snapshot,
  selections, storage usage, mutations, refresh coalescing and optional duration
  state. Same-root refreshes retain cards but make them display-only until the
  new token is admitted; root changes clear cards/selections immediately. Request,
  root and navigation epochs reject late A/B/A and superseded responses while
  completion paths still invalidate the captured logical root.
- All save/delete/retention/settings/export actions carry the originating token.
  Settings and exporter completion reconcile filesystem effects even after stale
  UI responses; export admission retains the backend mutation permit through
  completion. Clips request at most eight visible IDs with one active and one
  coalesced pending intent; unavailable duration is local and retryable, and
  playback remains independent of duration.
- Games render from the core snapshot without waiting for durations or Data
  Dragon. Benchmark-only duration draining preserves historical `library_useful`
  semantics, while `games_library_usable-v1` records the post-paint Games gate.
- Fresh adversarial review found benchmark useful timing, retained-view error
  presentation, return-to-library refresh, and Settings-to-Clips visibility gaps;
  all four were fixed with focused regression coverage where practical.

- The first M4 campaign froze a minimally instrumented reference and corrected
  candidate after deferred App/reference condition-parity tests. It obtained 14
  valid runs, then an incomplete invalid attempt at sequence 15; its rejected
  matrices and all artifacts remain preserved separately.
- The replacement M4 campaign retained sanitized individual/aggregate reports,
  warm initial/remount observations, exact source/binary/artifact identities,
  passed all ten matrices with 50 valid launches, and confirmed all 409 prepared
  files still match their hashes. These are fresh observations, not reused
  attribution results; primary Games-usable gates pass. The complete engineering
  disposition is `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/engineering-disposition.md`:
  working-set and I/O costs are bounded benchmark-only consequences, handle growth
  is a process-sampler baseline artifact, and cancellations are HTTP stream
  teardown with no child-failure evidence. No production change was warranted.
- A separate frozen normal candidate opened disposable root A and showed its
  three Games cards and expected storage totals despite optional asset errors.
  Root B with overlapping IDs was prepared. The initial autonomous capture/pointer
  attempt failed, but later user-assisted acceptance superseded that dated state;
  its retained receipts/audits are recorded in the current milestone above.

## In flight

No new benchmark is in flight. The r3 preparation and matrices remain valid;
original/r2 partial preparation artifacts remain excluded and preserved.
The prior replacement is closed with seven valid observations and one externally
interrupted slot. Await the screen-use constraint before any new native launch.

### Closed first replay-opening campaign

No benchmark or subject is in flight. Supervisor PID 29160 stopped with exit 2
at 08:52:57.345792 UTC, after starting at 08:48:41.418295 UTC on 2026-09-24.
Durable result/log: workspace `checks/run-result.json` / `checks/run.log`.
The campaign has seven started and six completed receipts; launch 7 is
`o-n-warm-can-001-candidate-trial-1-minimal`. Preserve the failed slot and
all 93 unused slots. Never restart the closed driver or use its remaining slots.
Root inspection confirmed the dedicated app, WebView/media children and Python
driver are closed. No timeout or forced Job termination occurred.

Launch 7 app terminal: six cycles, 231 accepted events, zero drops, exit 0.
Final Job totals: 118 created / 118 creation notifications, zero active,
terminated or unobserved processes. Its 40 active samples include one initial
28/29 mismatch at 1028.8903 ms; the other 39 reconcile. No cadence flag occurred
and adjacent gaps are 1000.2247-1021.4098 ms, below the 2500 ms validity ceiling.
All source/config/cache post-hashes match. Runner TELEMETRY_INCOMPLETE and generic
bundle rejection remain authoritative; the partial global campaign also rejects.

`New-ObserverSample` drains notifications, queries a Job snapshot, then
`Sync-JobNotifications` drains again without refreshing that snapshot.
A new creation during that drain advances the global count past the older Job
total; Abs(total-count) then reports one unobserved process. A similar final
snapshot/drain boundary needs the same producer-level analysis. Do not clamp,
ignore the first sample, relax validity, reinterpret old raw data or retry.

All 100 preflights passed (exit 0), 08:45:07ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã¢â‚¬Å“08:47:43 UTC on 2026-09-24;
workspace `checks/preflight-result.json` and sentinel `preflight/complete.json`
bind the complete log set to the sealed campaign. Zero live receipts existed
before admission.

Campaign SHA-256: 649e49466929f19d575d0deac752487e0a2247dbebab831d4b9f52ecf621ee80.
Plan: `build/perf/qb-replay-010-replay-opening/.chronobreak-replay-benchmark/campaign-plan.json`.
Preparation verifier receipt: sentinel `inputs/preparation-verification.json`.
Fresh accepted N/E roots are `library-n-complete` / `library-e-complete`; both
preparation receipts report exit 0. Preserved initial partials have 47 games each
and were abandoned after the tool session ended, without a final decode receipt.
No live attempt or source mutation followed that interruption.

The first freeze stopped because the preparation helper omitted the matrix-plans
parent directory. Corrected helper verifies existing provisional inputs by exact
equality, then creates the missing parent and remaining plans. Final freeze
passed in `checks/freeze-corrected-20260924.log`; both prior failure logs remain.
No immutable plan was overwritten or runner/schema validation weakened.

Reference executable SHA-256: 34bd87bda70ea723a9c2d874a8c612038be3fb2149073d6753fc24ea48bd41c0.
Candidate executable SHA-256: 46323b37c1bb7e29d2df89f5dca115e7837796c1eacc5d7342b517ebbe854f1f.
All 344 reference and 366 candidate archive files remain unchanged after builds.
Reference initial build lacked the runtime required by tauri.windows.conf.json;
failed log/receipt preserved, staged runtime supplied, corrected build passed.

### Closed reconciliation context

No implementation, test, native subject, held response or diagnostic listener is
in flight. Reconciliation was completed without an agent commit; the subsequent repository
merge commit a840c645 is now the accepted baseline.
Original input patches, exact receipts, frozen normal subject, diagnostic history
and source audits are retained under build/qb010-ui-reconciliation/.
The independent review finding in App benchmark paint admission was repaired and
covered by stable/away/away-back regressions. The native exporter loading report
was caused by the deliberate optional-response hold: release resumed the latest
viewer, and normal ungated exporter/return passed. Its exact ordering is now
covered by a passing App regression. No production change was needed for it.
Closed descriptor/M4 subjects and results remain unchanged. The following dated
V2 records are retained history, not current work.

### Historical V2 verification context

No benchmark, product-code change or M1-M3 implementation is in flight. M4 is
closed: its engineering disposition and user-assisted acceptance are complete.
The retained manual helper/procedure and evidence remain at
`tools/replay_benchmark/qb010_manual.py` and replacement evidence
`manual-acceptance.md`. Syntax, Windows self-process
identity/Toolhelp plumbing and optimized-Python rejection of a nine-second hold
were verified without launching/suspending an app. Focused review's resume-failure
ownership and unconditional-safety-check findings were corrected.

Historical completed build context:
The earlier interrupted desktop build obtained no completion evidence; both fresh
builds now passed with logs/exit receipts under `build/qb010-m2-builds/`. Isolated
pre-M2 source copies remain in ignored `build/qb010-m2-baseline/` for review.

No M3 implementation remains in flight. Preserve pre-existing unrelated edits,
including Cargo.toml's line-ending-only status.

M4 preflight (2026-09-16, before collection): fresh isolated reference
source with diagnostic tap is at `build/qb010-m4/reference-source`; exact patch is
`build/qb010-m4/reference-tap.patch`. Preflight found benchmark-only M3 readiness
defects: reactive duration publications could start a scenario before historical
details, and legacy namespaces skipped draining. App now separates readiness
started/emitted state, drains legacy Games scenarios too and rejects an incomplete
drain. Two deferred App integration cases and two isolated reference parity cases
pass. New analyzer cases enforce the explicit frontend origin, count types and
required primary comparison metric; 91 Python tests ran with one existing opt-in
skipped. These are measurement-path corrections, not a controller/snapshot
redesign. Reference/candidate freezing, builds and fixtures are complete; the
partial live/manual results remain insufficient for acceptance.

M4 deterministic checks in `build/qb010-m4/checks`: initial Rust default suite
failed the PowerShell-script cancellation fixture because effective execution
policy is Restricted. With process-local `PSExecutionPolicyPreference=Bypass`,
all 86 Rust tests pass (one opt-in ignored); production and tests are unchanged.
Fmt and Clippy pass. Final frontend 107 tests and both candidate desktop builds
(including TypeScript check and Vite build) pass with the readiness correction.

M4 preparation is frozen at
`build/perf/qb-replay-010-m4/.chronobreak-replay-benchmark`: 409 prepared files
(1,108,793,064 bytes), ten verified immutable plans, and all 50 runner preflights
passed. S retains the original three-bundle S/R/L library shape but selects S only;
N/E retain their exact 50-game/50-clip identities. Full prior decode evidence is
reused only after source/copy hashes and write times matched. Adjacent matched
pairs alternate leading arms in five seeded rounds (20260915), with five-second
warmup/cooldown. Each arm shares the fresh per-cell immutable library/config to
preserve the existing exact config-SHA comparison gate, with separate appdata,
scratch and result roots. Collection subsequently stopped at launch 15; see the
active-unit disposition above.

Frozen executable SHA-256: reference
`e58d5071afbe4b6c3563c0b33533d577984bf97b75e01ad4483fdf29a27fb230`;
candidate `b9d74f34018a4577bab7175f40932801dc257e1f701268a96bdf57c93a3db367`.
Both are fresh optimized Tauri builds. The normal candidate build is separately
frozen as `subjects/manual` for interactions. Early helper preparation failures
and a premature unusable candidate copy are preserved separately and were never
launched. Initial reference build lacked copied staged runtime; staging the same
locked bytes resolved it. Build command receipts prove success despite a local
logger's subsequent cp1252 console-print error (logger corrected).

Ignored root: `build/perf/qb-replay-010/.chronobreak-replay-benchmark`.

- `subject/current-3a750e2/subject.json`: frozen source/binary/runtime identity.
- `generated/.chronobreak-replay-time/sources/opening-srl-v2/corpus.json`:
  successful corpus; failed opening-srl-v1 remains preserved.
- `manifests/prepared-{srl,n,e}.json`: immutable prepared inputs/receipts.
- `matrix-plans/qb010-{s,r,l,n,e}-v1/matrix-plan.json`: six launches per cell.
- `specs/global-order.json`, `launch-NN.log`, `interruption-launch-14.json`:
  exact order/interruption history. Never restart run_cells.ps1 now.
- `reports/attribution-v3`: final local analysis. V1 failed phase assumptions;
  v2 ordinal mapping was corrected in v3. Canonical evidence uses v3 and checks
  each launch/scenario mapping. Earlier outputs remain preserved.
- `specs/clip-diagnostic.log`, `clip-diagnostic.patch`, original source bytes and
  diagnostic source hash: completed native follow-up, not active application code.
- `specs/checkpoint-before-v2.md`: detailed pre-handoff record preserved locally;
  this checkpoint remains the sole current execution authority.

## Remaining

Collector and preparation work are complete. Obtain and analyze only the
user-authorized bounded comparison if compatible with their screen constraint.
Preserve full-feature criterion 6 coverage gaps: one cold stratum does not cover
the remaining workload classes or five independent warm processes per arm.
Criteria 1-5 retain the accepted library/descriptor/UI and manual evidence for
unchanged product bytes. No extra semantic pipeline requirement is established.

### Closed descriptor boundary and historical remaining scope

No descriptor milestone remains. Full-feature replay performance/manual gates
remain separate and unrun for this unit; the deliberate descriptor/full-probe
duplicate strict JSON reads are not a latency or I/O non-regression claim.
The descriptor plan does not authorize semantic timeline/ReplayTimeline work.
The new UI reconciliation plan separately governs adoption of the existing UI
timeline only; semantic backend staging stays out of scope. The historical V2 tasks
below were completed by the recorded M4 closure; do not repeat them.

### Historical V2 handoff list (superseded by closure)

- V2 M4: collection and engineering disposition are sufficient and closed. Preserve
  both campaigns; no further launches or selective top-ups are authorized by this
  replacement disposition. The historical useful/read-I/O cost is the documented
  benchmark-only compatibility drain; N-cold working-set and E-warm handle signals
  are bounded sampler/process effects; cancellations are server-side stream
  teardown with no child-failure evidence. Retain every individual growth signal.
- Complete the dedicated interaction sequence in an environment with working UI
  controls: Games with delayed optional work; Clips select/away/back; owned-batch
  save/delete/export/cleanup; overlapping A/B roots; optional failure; refreshed
  cards/counts, viewer return and playable exported media with source preservation.
- Full 010 minimal replay descriptor/staged semantic timeline criterion remains
  open. Unmodified playback parsing/off-thread work also needs disposition before
  full completion. V2 covers only the attributed library unit.
- Disposition the remaining full-feature post-payload delay/product gates without
  inferring that library measurements authorize payload splitting. The known
  configured-path/resolved-root identity mismatch remains a follow-up risk; only
  initial absolute-root rendering was observed, not alias/root-switch acceptance.

## Verification

- 2026-09-26 reuse audit passed in 14.017 seconds: 409 source/staged identities,
  sizes and write times; subjects, runtime, archives/build receipts; 20 matrices,
  100 manifest seals, configs and frozen tools. Reused 203 prior full-decode
  receipts; zero new decodes or app launches. Exact receipt is linked above.

- 2026-09-25 final collector tree: python -m unittest discover -s
  tools/replay_benchmark/tests -v ran 124 tests: 123 passed, one existing opt-in
  skipped, exit 0, 160.043 seconds. Receipt/log:
  build/qb010-replay-comparison-v2/checks/python-final-native-count-20260925.*.
  Focused final two harness cases pass; native acquisition 80.2378 ms in ten
  four-message test batches, 18 creations/exits (root, 16 shells, owned conhost),
  final active/terminated/unobserved=0, verified root/Job/port closure. Production
  batch limit remains 256. py_compile, canonical 60-item/eight-pair validator
  and git diff --check pass. No product checks or app launch repeated.
- First full 124-test run failed only the ordinary native deadline; all other
  tests passed except the existing skip. A later focused pass again failed at
  118.4479 ms/pass two. Remaining per-record PowerShell count/projection overhead
  was removed: native batch captures exact NEW count; Receive only appends it.
  No prewarm, deadline relaxation or discarded notifications. The subsequent
  focused and full passes above supersede this failure. A later-query failure
  now retains the last snapshot-associated count separately from current count;
  its deterministic regression passes. Earlier logger console UTF-8 decoding
  failed after preserving exact child exit receipts; logger is corrected.

- 2026-09-25 collector focused tests passed (two Python cases, actual deterministic
  and native harnesses). Initial native fixture assumed 17 processes; retained
  identity diagnostic proved Windows adds its owned conhost, so the fixture now
  reconciles that observed helper explicitly. The ordinary 100 ms acquisition
  initially failed at 122.4452 ms before snapshot. Native record construction now
  emits the existing wire shape directly, removing per-record PowerShell map/date
  projection; the unchanged 100 ms ordinary and 500 ms final paths then passed.
  No prewarm or drain outside acquisition. Failure artifacts were copied into
  build/qb010-replay-comparison-v2/checks/collector-gate/; cost diagnostic is retained at
  build/qb010-replay-comparison-v2/checks/native-cost-1/. Full suite pending.

- 2026-09-24 v2 handoff: canonical validator passed 60 roadmap items / eight
  plan-checkpoint pairs; git diff --check passed. Retained diagnostic source and
  runner hashes match the reproduction receipt, both expected control/race gate
  outcomes match, and the four evidence files pass JSON/whitespace/sanitization
  checks. Runner remains unchanged; no corrected-collector test is claimed.

- 2026-09-24 collector diagnosis:
  `powershell -NoProfile -ExecutionPolicy Bypass -File build/qb010-replay-comparison/collector_race_probe.ps1 -RunnerPath tools/replay_benchmark/run.ps1 -OutputPath build/qb010-replay-comparison/checks/collector-race-reproduction.json`
  exited 0 with `QB010-COLLECTOR-RACE-REPRODUCED`. Both expected gate outcomes
  and the exact drain/snapshot/drain order passed. This verifies the existing
  defect only; it is not a corrected collector/native/performance pass.

- 2026-09-24 final tooling suite: `python -m unittest discover -s
  tools/replay_benchmark/tests -v`, 122 run / 121 passed / one existing skip,
  exit 0; log `build/qb010-replay-comparison/checks/python-final-20260924.log`.
  Focused campaign/environment suite: 13 passed in 70.159s; its PowerShell
  redirection produced a shell error wrapper around successful unittest stderr,
  so the final full suite used direct subprocess logging and retained exit 0.
  Read-only environment capture matched twice exactly: CPU quantum 15.625 ms,
  registered WebView 153.0.4234.48. No QueueBack/WebView app pilot was run.
  After the full suite, only two explanatory limitation strings were added.

- Comparison M1 first focused `python -m unittest discover -s
  tools/replay_benchmark/tests -p test_opening*.py -v`: 28 passed (session 58218).
  Initial synthetic-template identity failure was corrected only in the fixture.
  A missing exact-byte campaign-plan seal was fixed with a passing regression.
  Expanded provenance/reliability/fake-driver tests subsequently passed: 31 focused
  tests (session 81866); superseded for the full tooling gate by the 122-test run above.
  No production application or benchmark launch occurred.

- New comparison planning pass: no product checks, builds or benchmark launches
  were run. Previous combined acceptance remains bound to the integrated product.
  Canonical validator passed: 60 roadmap items/eight linked plan-checkpoint pairs.
  Git diff whitespace checks passed. Independent design review passed after its
  bounded amendments; no product or benchmark result is inferred from planning.

- Reconciliation native acceptance closed 2026-09-23 08:27 UTC. User confirms
  A/B/A with correct Lux then Ahri/Garen labels and A/V; config returned to root-a
  with Never retention. After explicit title-bar X, closure-final.json records
  subject_running=false and diagnostic_listener_open=false. Gate had already
  restored ordinary fetch with zero pending responses. audit-after-close.json
  confirms every video/game-log hash and every B file unchanged; only the expected
  Ahri metadata rewrite differs, validated schema-equivalent with saved=true.
  Frozen normal subject 5651f0a851b222b7bd318605b3eda8064ba7bf747c7388adf31d7349d247cb82 and
  original generated source 131a1a096bed8f3e8042059926a65cccc22733639d2bb63d3a2d705a6d7da5fe still
  match preparation. Detailed user observations and diagnostic disposition are in
  build/qb010-ui-reconciliation/native-acceptance.md; raw receipts/audits are in
  native-20260923/. Final frontend 166 tests and typecheck pass after the test-only
  reported-sequence regression; production binaries remain the verified subjects.

- Native ungated checks: user confirms windowed/fullscreen event and champion
  filters, editor opening and return with clip/playback intact, immediate/persistent
  Star state after two toggles, and correct Garen opening. Gate remains removed,
  zero pending. Post-control audit changes only Ahri metadata bytes: original and
  current saved=true; absent optional capture becomes null, schema-equivalent
  after existing Option-field normalization. Every video/game-log hash and all B
  files are unchanged. Root corrected its mistaken unsaved expectation for the
  initially saved Ahri fixture. User is performing final A/B/A and app closure.

- Native retry: user confirms playback and clip selection correct after exporter
  return/failure. Released retry #3, then additional queued #4. Initial finish
  refused a pending response; after #4 settled, finish restored ordinary fetch
  with zero held requests. Primary #2 retained same element/source/loadstart=1,
  readyState=4 and currentTime=152.560963 across delivery. User is checking normal
  export/return, semantic filters, save/unsave and Garen. Full frontend now passes
  166 tests plus typecheck after a test-only regression for the reported held
  exporter round trip (101323); production subject unchanged.

- Native partial observation 2026-09-23: user confirms moving video/audible audio,
  play/pause, seek, zoom/pan, clip edits and fullscreen including mid-drag all worked
  with details held. User then entered exporter and returned, reporting loading.
  Diagnostic showed completed Ahri probe #1 still intentionally held and no video;
  releasing #1 immediately mounted returned viewer #2 at readyState 4 with its new
  probe #2 held. This is the diagnostic occupying the single active reader, not an
  observed media/backend failure. The pending exporter intent was superseded by
  return. Captured baseline for viewer #2; injected local details failure #2 keeps
  same primary/source/loadstart count and readyState 4. Await user retry/clip
  confirmation, then release and remove gate for ungated exporter verification.

- Final benchmark desktop build passed after normal subject freeze, receipt
  build/qb010-ui-reconciliation/desktop-benchmark.log. Canonical validator passed
  60 items/eight pairs. Native fixture preparation completed 2026-09-23 07:50 UTC:
  normal executable SHA-256 5651f0a851b222b7bd318605b3eda8064ba7bf747c7388adf31d7349d247cb82;
  generated source 131a1a096bed8f3e8042059926a65cccc22733639d2bb63d3a2d705a6d7da5fe unchanged;
  runtime queueback-ffmpeg-8.1.2-windows-x86_64-r6. Three media files fully decoded,
  A/B initial source audit unchanged. Helpers are copied into the deeper isolated
  directory with only repository-relative path adjustment. Diagnostic self-test
  passed. User-operated observations are retained in native-acceptance.md.

- Final combined checks 2026-09-23: frontend 22 files/165 tests passed (094342),
  benchmark paint regression 5 App tests passed (094315), typecheck passed; full
  app Rust 92 passed/one pre-existing packaged-runtime opt-in ignored (tool session
  50529, PSExecutionPolicyPreference=Bypass for the dedicated child fixture).
  Rust fmt and Clippy passed (093955), final standalone frontend and normal
  desktop builds passed. Build logs, retained command receipts and verification.json
  are under build/qb010-ui-reconciliation/. Git diff HEAD --check passed. Normal
  build was repeated after the benchmark guard change; older normal output is not
  the frozen subject. Reviewed source paths add only in-memory roster projection.

- Reconciliation 2026-09-23: focused suites passed after test-seam corrections
  (37 tests: command-20260923-092313; rail 6: 092509; App save/mock 6: 092654;
  Viewer 7: 093539; card 1: 093751). Rust library 15 passed (093327), including
  normalized roster order, missing-team/empty/incomplete parity; an initial enum
  assertion compile failure (093140) was corrected in tests. Viewer readiness
  URL equality failure (093505) was corrected to allow the controller session query.
  Full frontend 22 files/162 tests passed (093830); typecheck and Rust fmt passed;
  production frontend build passed (093854). All command receipts are in
  .codex/logs/command-<date-time>.log, with final receipts to be copied under build.

- UI integration 2026-09-22: typecheck passed twice. Focused initial and follow-up
  failures are retained in .codex/logs/command-20260922-165807.log and
  command-20260922-170123.log. The latter is 38 passed/5 failed as detailed above.
  Packaged ffprobe confirms imported mock MP4: 108000 frames at 60/1, time base
  1/15360, first PTS 0, duration 27648000 ticks, Constrained Baseline, no audio.
  The mock descriptor/probe now share its URL and exact timeline/profile. Binary
  media is unchanged. A prior wrong probe path failed setup only.

- 2026-09-22 UI planning only: bootstrap, applicable architecture and plan contract
  read; independent review reconciled as above. Product verification has not run
  against a combined tree because integration has not started. Earlier passing
  descriptor/M4 results below remain historical and are not a combined-UI pass.
  Handoff canonical validator passed: 60 roadmap items/eight plan-checkpoint pairs;
  git diff --check and the new plan's final-newline/trailing-whitespace checks
  passed. Root reviewed the complete plan and canonical diff. Only the three
  handoff documents differ in the root; no commit or product merge was made.

- 2026-09-22 final native confirmation: user replied "all passed and closed" to
  Ahri/details with no stale Lux, reopen Ahri with the hold removed, video/audio,
  play/pause/seek/fullscreen, then close the test app. This completes the actual
  user-operated descriptor procedure; DOM tests are not native playback proof.
  Agent verified subject PID 23268 and WebView2 debugger PID 26316 absent and
  zero listeners on the isolated debug port 52316. The original fetch had already
  been restored with no pending response in
  `diagnostic-2026-09-22T12-21-54-232Z-finish.json`.
  `python build/qb010-descriptor-manual-20260922/prepare.py audit final-closed`
  passed: both A/B roots unchanged, no added/deleted/changed source files.
  Fixture setup, human observations and raw diagnostics/audits remain in the
  ignored workspace; `manual-observations.md` records the instructed sequence
  and explicit replies. No production code changed during native acceptance.
- Final code review: root reviewed backend command/coordinator/projection changes,
  new Rust tests, frontend changes/tests, architecture and the complete canonical
  diff; a fresh read-only
  frontend scout found no actionable descriptor defect. Complete required
  automated results below remain applicable to the unchanged product tree.

- Native failure/retry: user confirmed the injected error text and working
  preview/pause, then confirmed multiple Retry clicks. Request 3 completed into
  the one latest pending request 4; after both authentic responses were released,
  pending/error cleared. Video 2, one loadstart/emptied, paused 71.929267 s, rate 1,
  unmuted volume 1 and windowed state were preserved through admission. A focused
  scout matched this to LibraryController's one-active/one-latest scheduler and
  ViewerScreen's explicit retry button; no automatic loop or defect established.
  The previous error stays visible during a held retry until success. Source
  `audit-retry-complete.json` passes A/B unchanged. Final visual draft confirmation
  after successful retry was subsequently confirmed by the user.

- Stale native navigation: user confirmed the instructed Ahri pending -> Games
  -> Garen selection sequence. Request 5 (old Ahri, game 1790100000) was verified
  held with no primary video. Release admitted only the queued current Garen
  descriptor and request 6/game 1790100001; video 4 mounted. Its own response
  release retained same element/source, loadstart/emptied 1/1, paused time 0/rate 1,
  and cleared details pending/error. User subsequently confirmed visible Garen
  and no stale Ahri before proceeding to root changes.

- Root A-to-B: user reported only Lux after switching via Settings, with Lux
  waiting on Recording. Config confirmed B. Old Ahri request 7 remained held
  and no primary video existed; release admitted B video 6 and request 8 using
  the same game ID 1790100000. User subsequently confirmed Lux playback and return
  to A. Config A, held B request 8 and absent primary video were verified before
  release. Current A video 7/request 9 then appeared. Release 9 and gate removal
  retained element/source, one loadstart/emptied, paused time 0/rate 1, with no
  pending/error. `audit-root-return.json` passes both roots unchanged. Final visual
  Ahri/no-Lux and ungated playback/close were subsequently confirmed as recorded
  above, followed by the final closed-app source audit.

- Assisted first native sequence: user confirmed moving picture, audible sound,
  play/pause/resume, rapid seeks, fullscreen controls/in-out and adjustment of
  both clip handles to a valid draft while details request 1 was verified held.
  Before/after release diagnostics retain identical element/source, loadstart 1,
  emptied 1, play/pause 61/61, seeked 105, paused 130.106781 s, rate 4, unmuted
  volume 1, windowed. Details became available without error. Frame counters
  were 11,307 total/3,276 dropped after the user's rapid seeking/rate/clip exercises;
  this is not a controlled performance or rate-capability claim. Source audit
  `audit-first-release.json` passes A/B unchanged. User subsequently confirmed
  visible details and unchanged clip draft. User evidence: workspace `manual-observations.md`;
  time-stamped baseline/release/status JSON retains supplementary diagnostics.

- 2026-09-22 assisted setup: `npm run desktop:build --prefix app` passed with
  TypeScript/Vite and optimized normal Tauri build; existing warnings non-fatal.
  Frozen normal subject SHA-256:
  `32fd21f9b41db6f6c4c4197699923f3c574d167b0debffee4ae79470ea0bff43`.
  `preparation.json` records the copied subject, packaged r6 runtime and new A/B
  fixtures. All three new videos passed strict exact-60-FPS validation and full
  single-thread decode (exit 0, empty stderr). Prelaunch audit
  `audit-prelaunch-resume.json` confirms all A/B source files unchanged. The generated
  non-personal input was read-only and unchanged; no M4 campaign or fixture was mutated.
- Setup-only failures were preserved: omitted builder keyword corrected before
  generation; v1 audio-remux rejected for nonexact rate/240.021354-second video.
  New v2 exact-copy recipe preserves the validated grid and source AAC. No parser
  tolerance or benchmark tooling changed. Audible output was subsequently
  confirmed by the user in the held-details native sequence above.
- Ignored `session.mjs` uses a child-only loopback debugger setting and isolated
  config/profile; no CSP/Origin/security bypass or alternate UI driver. The first
  unlaunched invoke-wrapper assumption was invalid because Tauri freezes invoke;
  corrected to the exact optional command's fetch response only. `node --check`
  and `gate.test.mjs` pass against installed Tauri 2.11.5 core/transport scripts,
  including authentic held response, failure without fallback/reissue, retry,
  callback cleanup and restoration. Native transport capture and restoration were
  subsequently verified in the held-response sequence above.
  The diagnostic holds frontend delivery, not backend worker ownership; native
  visuals/audio require explicit user observations. No payload, capability URL,
  arbitrary DOM text or input values are retained in diagnostic receipts.

- 2026-09-21 native-acceptance resume: `npm run desktop:build --prefix app`
  passed, including TypeScript/Vite and the optimized normal non-benchmark Tauri
  executable. No product code changed. Subject:
  `app/src-tauri/target/release/league-replay-app.exe`, 16,268,800 bytes,
  SHA-256 `a495d9c3cae12fbab629dd6d549035f3f0ea5bc1f8a4da9a76f926d2b4b7c58c`;
  last-write UTC `2026-09-21T13:58:10Z`. This supersedes the earlier handoff's
  benchmark-enabled last-build note below. Existing Vite/linker warnings remain
  non-fatal. No subject was launched.
- Supported computer-use preflight: imported `@oai/sky` through `node_repl` as
  instructed, then `sky.list_windows()` returned `Computer Use native pipe is
  unavailable: failed to connect native pipe: Le fichier spÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â©cifiÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â© est introuvable.
  (os error 2)`. Reset the JavaScript session, reinitialized and retried discovery;
  the identical error persisted. **Environment-blocked**, not a product failure.
  A subsequent resume repeated discovery and reset/retry with the same error;
  the normal executable still matches the recorded SHA-256. No app was launched.
  No alternate UI driver, browser/DOM substitute, real recording, old M4 fixture
  or acceptance artifact was used. At that preflight, A/V, controls, held/failed/
  late details, retry, clip state, stale navigation/root behavior and source-hash
  acceptance were unperformed; no commit was made. The 2026-09-22 user-operated
  sequence above subsequently completed those descriptor checks.
- A bounded read-only scout found no existing normal-app full-probe hold/fail/
  release helper. Existing M4 ffprobe-duration holds do not hold replay details.
  Establish a descriptor-specific controlled fixture setup when native access is
  restored; this finding does not authorize production hooks or a redesign.

- 2026-09-22 final canonical validator and `git diff --check` passed after the
  descriptor closure edits: 60 roadmap items and eight linked plan/checkpoint
  pairs. The V2 plan, benchmark tooling, campaigns,
  resource/cancellation dispositions and manual-acceptance artifacts are unchanged.
- 2026-09-21 descriptor final automated baseline: `cargo test --manifest-path
  app/src-tauri/Cargo.toml` passed 90 tests (one existing opt-in packaged-probe
  test ignored); app `cargo fmt -- --check` and `cargo clippy --all-targets --
  -D warnings` passed. The two newly included Rust test files were also formatted
  directly with rustfmt. No recorder/media-runtime checks or M4 campaign ran.
- `npm run test --prefix app` passed 123 tests in 17 files; `npm run check
  --prefix app` passed. The initial new deferred clip test used a one-second draft,
  correctly rejected by the existing five-second minimum; corrected to a valid
  ten-second fixture and rerun. No product minimum or validation was weakened.
- `npm run build --prefix app` passed. Both `npm run desktop:build --prefix app`
  and `npm run desktop:build:benchmark --prefix app` passed with fresh TypeScript/
  Vite builds after the final product-code edits. Their warning about the existing
  static/dynamic API import and Windows linker informational output did not fail
  builds. At that handoff the shared executable was benchmark-enabled; the fresh
  normal rebuild and native launch recorded above supersede that handoff state.
- `python -m unittest discover -s tools/replay_benchmark/tests -v` ran 95 tests
  successfully with one existing opt-in skip (94 passed). Detailed combined
  frontend/build/Python log: `.codex/logs/command-20260921-145022.log`.
- At the 2026-09-21 automated handoff, native descriptor acceptance was **not run**;
  the subsequent 2026-09-22 user-operated results above supersede that blocker.
  Automated DOM and ownership tests do not prove native decode, audible A/V, presented-frame or
  source-hash acceptance. Closed M4 acceptance is neither repeated nor reused as
  proof of these new viewer changes. No new benchmark/performance pass is claimed.

- Descriptor handoff validator passed: 60 roadmap items/eight plan-checkpoint
  pairs; whitespace passed. The initial apply_patch engine failed in sandbox;
  approved native apply_patch now works with Windows argument quoting corrected.
- M1: two Rust descriptor tests and two owned-read tests passed. Fixtures cover
  empty/older v2, strict malformed/v1 rejection, absent media, off-thread progress,
  dropped waiter/Busy ownership, failures, refresh/mutation/A-B-A/shutdown rejection.
  Initial compilation found only a missing Path qualification in the new test;
  corrected and rerun successfully. Frontend API tests 9/9 passed.
- M2 initial TypeScript check passed after threading required read-owner props
  into existing viewer/exporter test harnesses. Optional resource errors are read
  through a guarded accessor so failed details cannot throw through video memos.

- Descriptor bootstrap: PRODUCT, WORKFLOW, complete feature, complete checkpoint,
  V2 immutable plan, PLANS, VERIFICATION, forward-engineering/Rust guidance and
  applicable replay/library/security architecture read. HEAD matches 3b14df2;
  initial worktree clean. Product verification not run for descriptor planning.
- Root plan challenge resolved shared read-slot ownership, exact descriptor
  projection, strict bundle compatibility, all three full-probe callers,
  Settings/exporter/benchmark navigation origins and benchmark readiness ordering.
  Delegated scouts never initialized (pending_init); no independent review is
  claimed. Historical opening_attribution assumes payload-before-mount and is not
  reused as a descriptor measurement. Duplicate strict JSON reads are an explicit
  tradeoff, with no replay latency/I/O non-regression claim.

- 2026-09-17 replacement: all 50 strict bundles valid, all ten result matrices pass;
  required event/request loss zero, all source hashes unchanged. Same-clock
  resource/opening extractions cover every run. Full normal verification repeated:
  frontend 107 tests, reference parity 2, Rust 86 (one opt-in ignored), replay
  Python 91 run (one opt-in skipped), fmt/Clippy, normal and benchmark desktop
  builds including TypeScript/Vite passed. Sanitized `verification.json` preserves
  receipts, including the wrong-filename parity invocation and corrected pass.
- Native UI APIs were disabled for the initial autonomous attempt; the current
  `manual-status.json` records the later user-assisted acceptance, verified optional
  child receipts and final audits. The initial startup observation remains distinct
  from the subsequently completed sequence.
- 2026-09-16/17 M4: app Rust 86 passed/one opt-in ignored (process-local execution
  policy override for the script fixture); fmt and Clippy passed. Final frontend
  107/107 across 16 files and reference parity 2/2 passed. Replay Python suite ran
  91 tests, one existing opt-in skipped. Normal/benchmark candidate and benchmark
  reference desktop builds passed, including final npm check/build. Receipts and
  logs: `build/qb010-m4/checks`; sanitized receipts in M4 evidence `verification.json`.
- M4 measurement readiness corrections preserve legacy event semantics and the
  `games-library-usable-v1` contract. Independent measurement review requested
  reference parity cases; both passed before launch. Current tree source hashes
  and all three frozen subjects were rechecked on resume; identities match.
- All 14 completed runs in the first M4 campaign validate, with zero required
  event/request loss and source mutation. Run 15 is invalid: missing both
  terminals, collection result, post-hashes and runner result. All ten
  required-result matrix checks reject that incomplete campaign. The replacement
  has a separate 50/50 valid result; no first-campaign artifact was repaired.
- Initial manual startup alone passed: three Ahri cards, zero saved, storage 23.9 MB
  games / 2.67 MB clips / 26.6 MB total. The initial autonomous CUA attempt could
  not operate the window. This dated result was superseded by user-assisted A/B/A,
  export and cleanup acceptance; four exports now have retained H.264/AAC/full-decode
  evidence. The disposable roots were intentionally mutated only by those actions.
- Final canonical validator, whitespace and retained-evidence checks are recorded
  in M4 evidence `final-checks.json`. No architecture revision was justified by
  incomplete performance evidence or blocked interaction. M3 invariants remain.

- 2026-09-16 M2 final `cargo test --manifest-path app/src-tauri/Cargo.toml` passed
  86 tests with one explicit packaged-runtime test ignored in the default suite.
  `cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check` and
  `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings`
  passed. Final log: `.codex/logs/command-20260916-104351.log`.
- Focused duration suite with `QUEUEBACK_TEST_MEDIA_RUNTIME` set to the staged r6
  root and `cargo test --manifest-path app/src-tauri/Cargo.toml --lib clip_duration
  -- --include-ignored` passed 7/7 (`command-20260916-103426.log`). This includes
  actual ten-second timeout, output overflow, failure/stderr isolation, signalled
  cancellation, metadata replacement/deletion, a real native HTTP child after a
  junction redirect, and packaged ffprobe over the protected route. A copied MOV
  with an external HTTP data reference never connected to its sentinel listener;
  input bytes remained unchanged. This is focused probe verification, not a replay
  attribution/performance run.
- Coordinator tests cover batch/ID/token bounds, explicit retry/cache reset, dropped
  caller scan/probe/mutation/settings ownership, offloading, supersession/shutdown,
  root A/B/A, save/delete/retention success and real partial clip deletion failure,
  settings failure preservation and captured export completion. Barriers/channels
  and child-ready signals establish races; no sleep establishes race ordering.
- `npm.cmd run test --prefix app` passed 87/87; `npm.cmd run check --prefix app` and
  `npm.cmd run build --prefix app` passed. `python -m unittest discover -s
  tools/replay_benchmark/tests -v` passed 88 tests, one existing opt-in test skipped.
  Log: `.codex/logs/command-20260916-103806.log`. Frontend/tool sources are unchanged.
- `npm.cmd run desktop:build --prefix app` and `npm.cmd run desktop:build:benchmark
  --prefix app` both passed on the final corrected M2 tree (only the existing
  non-fatal linker-output warning). Exact command/exit/hash receipts and full logs:
  `build/qb010-m2-builds/{normal,benchmark}-result.json` and corresponding `.log`
  files. The new snapshot/duration commands are registered in both builds; existing
  benchmark-only handlers remain behind their `replay-benchmark` cfg. No benchmark
  observation semantics or frontend taps changed in M2.
- M2 handoff canonical validator passed: 60 roadmap items / eight plan-checkpoint
  pairs. Final tracked diff, new-file whitespace and explicit included-test rustfmt
  checks passed. Immutable plans, attribution evidence and frontend files remain
  unchanged by this milestone. No performance improvement/manual UI claim is made.
- Initial focused compile failed only because a test moved its fixture path; fixed
  the test ownership. Initial junction test tried renaming a directory with an
  opened descendant, which Windows rejects; changed to replace an internal junction
  after validated admission, proving the intended reparse race. Both corrected tests
  pass. Initial logger sandbox denials were resolved by approved reruns.

- 2026-09-16 M3 verification: `npm.cmd run test --prefix app` passed 105 tests
  across 15 files, including deferred controller races for same-root refresh,
  A/B/A identity, stale-token mutation admission, navigation/disposal, bounded
  duration failure/retry/coalescing/drain, export invalidation, strict decoders,
  and Games rendering with Data Dragon offline. `npm.cmd run check --prefix app`
  and `npm.cmd run build --prefix app` passed. Rust `cargo test --manifest-path
  app/src-tauri/Cargo.toml --lib` passed 86 tests; `cargo fmt --manifest-path
  app/src-tauri/Cargo.toml -- --check` and `cargo clippy --manifest-path
  app/src-tauri/Cargo.toml --all-targets -- -D warnings` passed. Python replay
  benchmark tests passed 88 with one existing opt-in test skipped. No benchmark
  comparison was run. Both `npm.cmd run desktop:build --prefix app` and
  `npm.cmd run desktop:build:benchmark --prefix app` passed on the final tree;
  each emitted only the existing non-fatal linker-output warning.
- Fresh adversarial frontend review was performed after implementation. It found
  four substantive gaps: benchmark `library_useful` timing, retained-view error
  presentation, return refresh, and Settings-to-Clips duration visibility. The
  implementation now drains benchmark durations after `games_library_usable`,
  keeps same-root cards visible with an inline error, refreshes on library return,
  and restores `clipsVisible` on Settings return. The lower-confidence raw-path
  alias concern remains bounded by backend token/root admission and is recorded
  as a follow-up risk rather than changing the security contract.

- 2026-09-16 M1 focused `cargo test --manifest-path app/src-tauri/Cargo.toml --lib
  library`: 18 passed, covering 11 new deterministic scan/coordinator cases and
  existing strict library tests. One initial fixture assertion expected 38 bytes
  instead of its actual 40; corrected the assertion, not production accounting.
  Barrier/channel tests prove offloading, dropped-waiter permit retention and Busy
  behavior across roots, revision-only/A-B-A rejection, persistence failure,
  same-root failure/rebuild, worker panic and shutdown. No timing sleeps or media
  processes are used by these new tests; all filesystem fixtures use tempfile.
- Final M1 tree: `cargo test --manifest-path app/src-tauri/Cargo.toml` passed 72
  tests; `cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check` and
  `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings`
  passed. `cargo check --manifest-path app/src-tauri/Cargo.toml --features
  replay-benchmark`, `npm.cmd run check --prefix app` and
  `npm.cmd run build --prefix app` passed. Full Rust tests include the final
  worker-join failure handling. Logger sandbox denials on initial test/build
  attempts were resolved by approved reruns; they were not product failures.
  Local logs: `.codex/logs/command-20260916-095441.log` (focused),
  `command-20260916-095556.log` (full Rust), `command-20260916-095638.log` (build),
  `command-20260916-095711.log` (Clippy), `command-20260916-095840.log` (benchmark check).
- M1 does not claim UI readiness, optional-probe/mutation ownership, a speedup,
  release packaging or full-feature completion. No attribution/media-runtime/
  recorder campaign was repeated; later milestone manual/performance/build gates
  remain unrun for this candidate.
- M1 handoff canonical validator passed: 60 roadmap items / eight plan-checkpoint
  pairs. Final `git diff --check` and new M1 source/checkpoint whitespace checks
  passed; the linked immutable v2 plan and attribution evidence were not edited.
- Benchmark production build passed via `npm.cmd run desktop:build:benchmark --prefix app`,
  including TypeScript/Vite and optimized Tauri; non-fatal linker-output warning.
  Frozen executable SHA-256:
  `f1e3390cb29b3c9465eee3898a46af5d2c3c69894aeb9f7a5ad5c09a1ff75b49`.
  subject.json binds zero product-code diff, seed 20260915 and packaged r6 hashes.
- S/R/L exact grids: 14,400/108,000/216,000 frames, PTS 0, 256 ticks/sample at
  1/15360. All source hashes matched before/preflight/after generation. Video hashes:
  S `131a1a096bed8f3e8042059926a65cccc22733639d2bb63d3a2d705a6d7da5fe`,
  R `c5c3ecf5802dffb8cbcf1382380c505d9e21d27064338037fb5e490230611e09`,
  L `c00f2577b7365f8db868c8243cd434d3453dd6dc42f7c79143ed0ed887052d95`.
- N/E corresponding media/metadata/IDs match. Sparse logs: 7 events/8 snapshots;
  dense: 259/240 per game. Fifty dedicated ten-second H.264/AAC clip copies each.
  Prepared S/R/L, N, E passed 3/100/100 complete decodes with empty error output.
  Exact recipes/counts are in specs/scaling-receipt.json.
- Required-result matrix checks for the earlier attribution campaign: S/R/E
  passed; L failed missing launch-14 terminal; N failed launch-18 telemetry
  completeness. Its cold counts S/R/L/N/E were 5/5/4/4/5. The replacement's ten
  matrices all passed.
- Final `python -m unittest discover -s tools/replay_benchmark/tests -v`: 88 tests
  OK, one opt-in media test skipped. Separate packaged-runtime corpus suite passed
  all 15 tests, including the real repeat/AAC regression and full decode. Helper's
  five synthetic tests and extraction from all 28 valid real runs passed.
- Native diagnostic: `cargo test --manifest-path app/src-tauri/Cargo.toml --release
  --features replay-benchmark --lib library::qb010_clip_work_attribution -- --exact
  --ignored --nocapture --test-threads=1`: one test passed, six measured calls,
  22.30 seconds. Test-only patch retained, source restored. All 400 prepared N/E
  files then matched preparation SHA-256/size receipts.
- Evidence checks passed: 30 identities, 28 exact valid mappings, 53 frontend
  cycles, current helper hashes and sanitized files. No bearer URLs, raw local
  paths or user identity are retained in shared reports.
- Final v2 handoff validator passed: 60 roadmap items / eight plan-checkpoint
  pairs. Required plan sections, UTF-8/final newline, new-document whitespace and
  `git diff --check` passed. This is not project-wide product-code completion.

## Deviations

- Replay-opening launch 7 materially contradicts the unchanged-collector
  assumption: a newer notification drain is paired with an older Job snapshot.
  The rejected sample is preserved without reinterpretation. The reviewed
  collector-reconciliation-v2 plan supersedes only this measurement unit;
  the predecessor plan and closed product evidence remain immutable.

- M4 sequence 15 (N-warm candidate trial 2) began at
  `2026-09-16T14:11:16.8087295Z` and ended without finalization during cycle six.
  Empty outer receipt/log and all partial raw artifacts remain. No process was
  running on resume; the external interruption's cause is unknown. No application
  failure is inferred from missing terminal evidence. The invalid-run rule and
  ceiling prevented completing this campaign; no further launch was made.
- Required manual interaction was actually attempted with the dedicated normal
  build, but the Windows capture/input environment prevented the sequence. Do not
  substitute deterministic race tests or the initial accessible Games view for it.
- M4 preflight fixed only demonstrated benchmark readiness/validation defects.
  No snapshot redesign, payload split, unrelated campaign or root-alias fix.

- Repeat transform make_zero shifted AAC-priming timestamps and stretched the first
  video packet from 256 to 912 ticks. Controlled four-second A/B isolated it;
  changing only repeat's avoid_negative_ts to disabled restored exact 60 FPS.
  Retained strict validation, stream-copy/GOP/fragment flags and independent audio.
  Failed corpus remains preserved; this was fixture tooling, not production work.
- L cold 3 (launch 14) was interrupted with terminal/collection/post-hash missing.
  N cold 4 (18) finished scenario and source hashes, but sample 2 saw 96 Job processes
  versus 95 creation notifications; next sample reconciled 118/118. Every sample
  must be complete, so neither trial is repaired or included in valid aggregates.
- Real extraction contradicted synthetic assumptions: calibrated native payload
  events can precede frontend request, and viewer-mounted generation precedes
  controller load generation. Helper now partitions frontend clocks only and uses
  loadstart generation. Native/server durations are not joined to cycles. Added
  native clock-offset regression; all validated real runs pass extraction.

## Decisions

- Comparison M1 found matrix expansion shares app-data/scratch roots between
  trials, and the app uses the exact app-data root for WebView storage. Added
  opt-in `isolate_launch_roots` to the tooling matrix specification/compiler;
  default false preserves historical plans. The new campaign requires it and
  prepares unique empty app-data/Data Dragon/scratch directories. No app manifest,
  event schema, scenario driver or product source changes.
- Prelaunch environment freeze uses read-only `opening_environment.ps1`, importing
  only the existing runner identity functions and literal metadata through its AST.
  Registered installed WebView bytes are hashed without launching it. The receipt
  is frozen and rechecked before each slot; actual result normalized identity must
  match exactly, including independently measured quantum and descendant versions.
  No tolerance, substitute frozen CPU sample, live byte-proof claim or runner
  schema change is introduced. Any mismatch stops without replacement.

- New comparison uses existing events with a separate offline contract. Optional
  full-payload timing is boundary-local, not generation-correlated. Five warm
  processes per arm avoid treating remounts as independent trials. A global
  immutable campaign plan/verifier supplements twenty plan-local checks. The five
  cells cover workload axes, with no complete Cartesian-product claim.

- Reconciliation review (2026-09-23): fix the concrete pre-existing App benchmark
  fallback that emitted library_useful after the admitted Games view changed
  before painting. Fail this case with phase library_paint, include navigation
  epoch in both paint/drain admission, and retain successful milestone ordering.
  Tests cover stable/away/away-back. This is focused contract preservation, not a
  campaign rerun or a change to accepted historical M4 measurements.

- UI unit: preserve both original lines and the captured current sibling CSS diff;
  no commits. Keep LibraryController/Coordinator and protected playback authority.
  Optimistic Star is a bounded presentation overlay with mandatory filesystem
  refresh, never a second snapshot owner. Required participants expand the IPC
  GameSummary consistently in Rust, both strict decoders, TS and fixtures, without
  changing persisted v2 or the four-field descriptor. Existing ReplayTimeline is
  integrated, not rewritten; no semantic paging/payload producer or campaign.

- The current user request authorized one fresh bounded replacement campaign;
  its before-launch disposition is linked in Active unit. The former no-further-
  launch decision below remains the disposition of the first matrix. Original M4
  evidence is immutable and is not pooled with replacement trials.

- First-campaign gate disposition: insufficient evidence. Its cold Games medians
  were S 313.7/304.9 (one process/arm), N 2100.8/286.7 (two), E 2298.8/471.9
  (one); those observations cannot pass the n=5 gate. This remains historical
  evidence only.
- Replacement gate disposition: Games-usable medians R/C (ms) are S 166.4/168.2,
  N 2092.6/197.7, E 2386.6/493.3 cold and N 2087.3/209.7, E 2404.8/483.6 warm.
  N/E improvement exceeds both required thresholds and S has no material
  regression. Historical useful is slower by 22ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Â ÃƒÂ¢Ã¢â€šÂ¬Ã¢â€žÂ¢ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Â¦Ãƒâ€šÃ‚Â¡ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™Ãƒâ€ Ã¢â‚¬â„¢ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã‚Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€¦Ã¢â‚¬Å“27% because the compatibility
  drain adds about 2.45 seconds; protected-route requests/delivery and read I/O
  rise as documented in replacement evidence.
- Replacement engineering disposition (full table:
  `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/engineering-disposition.md`):
  N-cold working-set peak is concentrated in the WebView during the benchmark-only
  50-clip drain and has no sustained monotonic signal; E-warm handle growth is a
  lower candidate startup baseline with equal late-half growth and equal peak;
  read/request amplification and historical useful latency are the bounded,
  intentional compatibility drain; and candidate cancellations are incomplete
  HTTP bodies during ffprobe metadata completion or teardown. `run_child` retains
  success-status and kill/reap invariants, but campaign telemetry has no per-child
  exit code. CPU/private peaks pass matched thresholds; no sustained candidate
  private-memory, working-set or handle leak, server error, required event/request
  loss, source mutation, stale result, failed bundle, timeout or playback failure
  occurred. The retained partial-body diagnostic is
  `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/cancellation-probe.json`.
  No production change is warranted; retain the cancellation/status correlation
  limitation as a future telemetry watchpoint.

- M2 requires `snapshot_token` on save/delete/retention/export IPC and an explicit
  `retry_unavailable` flag for durations. M3 now carries the originating token from
  each frontend action; missing or stale tokens still fail closed. Retained
  invalidated snapshots remain display-only until refresh succeeds. No tokenless
  legacy mutation bypass.
- Mutation invalidation is conservative for all admitted attempts, including errors
  before side effects; admission rejections do not invalidate. Export invalidation
  uses matching logical path/ApprovedRoot, while response admission additionally
  requires original root epoch and selected token. This observes late publications
  after A/B/A without delivering stale export results to a newer selection.

- M1 adds a separate lightweight clip wire type so existing `list_clips` duration
  semantics and frontend/benchmark readiness remain unchanged before M3. The new
  snapshot token is process-local opaque identity, never delivery authorization.
  Optional duration state and mutation-slot/invalidation wiring are now implemented
  in M2.
- Failed settings persistence leaves both the published root pair and coordinator
  revision/view untouched. M2 moves settings/mutation filesystem execution into its
  owned slot; no configuration/publication lock is held around that I/O.
- Useful cold medians S/R/L/N/E: 92.4/97.8/100.0/2121.6/2312.6 ms. N/E clip commands
  dominate at 1992.6/2196.4 ms. Direct probes take 1665.4-1817.3 ms for 50 clips;
  enrichment takes 19.0-24.2 ms sparse and 287.7-306.3 ms dense. This selects v2;
  it is not a measured candidate speedup.
- Media-duration delay mainly follows payload/mount; its cause remains unresolved.
  No payload split, container/delivery/backend rewrite or second diagnostic is
  authorized by this library finding.
- Cold means fresh process, not cold OS cache. Warm is five remounts within one
  process, initial mount separate, including the 100 ms remount timer. No p95/p99
  or independent-five-process warm claim.
- Thread/working-set growth flags remain watchpoints. Job I/O is not disk I/O.
  Preserve 011's accepted attribution limit and 009's deferred human A/V gate.
- S is generated native media; R/L are derived, not independent long recorder/League
  proof. Historical schema-v1 008 is not a current comparator. Strict v2 authority
  remains mandatory; optional 015 is not a prerequisite or a backend mandate.
- V2 uses a new Games-usable observation in both fresh reference and candidate,
  keeping old library-useful semantics. Its comparison is separately specified;
  do not top up or reinterpret the attribution trials.

## Blockers

No collector or preparation blocker. The prior replacement was manually
interrupted by the user; this does not establish an application or infrastructure
defect. The current blocker is the native screen requirement: the user asked
whether the benchmark can run inside ChatGPT, and the native executable cannot
be embedded in the browser panel. Brief visible execution has not been accepted.
Default sandbox commands work again on 2026-09-26. The full n=5 per-stratum
criterion 6 gate remains unmet; the proposed ten-launch check has narrower scope.

### Historical environment context

The shell sandbox helper still cannot start; reviewed elevated execution works.
The 2026-09-22 pending patch/test action was not executed because automatic approval
review hit an account usage limit (not a safety finding). On 2026-09-23 approved
read/write/test commands work again. On 2026-09-23 computer-use list_apps reports
"native pipe is unavailable ... os error 2". User-operated combined native
acceptance subsequently passed with diagnostic/source-audit support. No
reconciliation blocker remains; the native pass is actual user evidence, not
an inference from DOM tests.

No unresolved minimal-descriptor acceptance or engineering blocker remains.
The missing Windows native pipe persisted after reset/reinitialization; the
user-operated native sequence supplied the actual UI/audio observations instead.
The agent's scoped response diagnostic supplied supplementary admission/media
and source-hash evidence, not a substitute for those native observations. The
shell sandbox helper was unavailable; approved shell execution and the same
apply_patch engine worked. Planning scouts did not initialize; root reviewed the
plan directly. Final native-resume/read-only frontend review completed.

The full feature stays in progress because its separate replay scope and complete
performance/manual gates are not satisfied here. V2/M4 has no remaining blocker;
its campaigns, dispositions and acceptance are closed and unchanged. The first
campaign's coverage gaps remain explicit and are not repaired by pooling.

## Next action

Resolve whether the user permits a roughly 3-4 minute native visible block or
requires execution entirely inside ChatGPT. In the latter case, stop native
measurement: the browser panel cannot substitute for Tauri/WebView performance.
If the brief block is allowed, use the retained E fixture/subjects with a separate
ten-launch paired cold comparison, unchanged thresholds and fresh isolated output
roots. Stop on excessive runtime or invalid evidence. Do not rerun preparation,
decode media, launch the old 100-slot driver, pool old measurements or commit.
Report this stratum's result separately and retain all wider coverage gaps.
