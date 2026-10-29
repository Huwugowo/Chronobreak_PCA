# QB-LIB-023 execution checkpoint

Feature: `QB-LIB-023`
ExecPlan: `docs/exec-plans/qb-lib-023-controlled-league-data-initialization.md`
Updated: 2026-10-29

## Current milestone

Implementation complete and accepted for delivery by explicit user decision on
2026-10-29. Deliberate fixture-recovery observation is optional follow-up.
Recorded automated/build evidence and positive release feedback remain valid;
unobserved native scenarios are not claimed passed.

## Active unit

None. The verified typed-history correction is already committed as `9e5ca04`.
This documentation handoff records the revised acceptance and confirms delivery.

## Completed

Read mandatory product/workflow material, relevant canonical features and current
verification/architecture; applied forward-engineering and read PLANS. Added a
single ready planned feature for the user-authorized coordinated correction.
Read-only scouts investigated actual preprod coverage and startup ownership.
Root inspected critical admission/import/UI paths and current SQLite using
URI mode=ro. No app, recording, cache, config or source journal was mutated.

The current configured preprod root has nine game directories: seven finalized
video.mp4/metadata bundles and two partial-only directories with no metadata.
The matching cache contains seven rows, all telemetry ready, facts current and
tail_bytes=0. Master Yi, Graves and Syndra each have history_flags=25 and indexed
final result JSON. Their event endpoint source coverage reports respectively
5/5/2 gap attempts, 2/9/1 unresolved actors and 18/14/2 unmapped observations.
These are observation coverage diagnostics, not proven missing event/video counts.
No complete-media decode or full-match video-integrity claim was made.

Catalog attempts accepted_game on recognized incomplete summaries, returns
recording-media-identity-unavailable as an item error and makes History set global
indexError. Unknown count includes the catalog/index deficit plus all relevant
coverage flags, explaining nine even after seven imports settle. Library line
also concatenates fast-changing intermediate query fragments. The indexError
itself is sticky within a stable context; no captured runtime trace proves
that flag repeatedly toggled in the reported launch.

Independent History and selected-replay writer calls share the native single
try-acquired permit. Context/navigation invalidation does not end a running native
worker. Therefore an overlap can produce Busy, which the other consumer treats
as terminal until retry/new context. Exact exception from the reported replay
was not retained, so historical attribution to this race remains unproven.

Candidate plan review required four clarifications: distinct native reset/admin
ownership, explicit generation/final-query initialization barrier, total replay
pending/partial/unavailable reducer, and exact incomplete catalog wire behavior.
Root incorporated them; fresh focused final review approved with no consequential
contradiction. Immutable plan linked only after that approval.

## In flight

None. The following is historical launch/verification context, not current
process state or an instruction to resume acceptance:

Implementation integrated; fresh read-only focused review found no consequential
contract bug. User-operated launch used process 35688 and the isolated profile
build/qb-lib-023/native/manual-20261028T132923800Z. User reports the app behaves
very well; only requests suppressing the brief replay loading label on normal
opens. Prior instances are closed for the release update. The first rebuild
found lingering fixture process 31532 locking the executable; verified its exact
path/start time and terminated that test process before retrying the build.
The refined release is open as process 33968 (13:45:37 UTC), session
manual-20261028T134536775Z reusing manual-20261028T132923800Z for its warm cache
and WebView profile. No normal user config/cache/recording data is a write target.
The later typed-history follow-up is verified in source/frontend output; no new
desktop executable build or native interaction is claimed for that correction.

## Remaining

No required implementation or acceptance work. Deliberate native fixture
recovery remains unobserved and optional; individual manual scenario results
were not supplied. The last typed-history follow-up has focused frontend/build
verification, without a separately claimed desktop rebuild/native interaction.

## Verification

2026-10-29 scope/state handoff: canonical validator passed (102 items / 22
plan/checkpoint pairs); whitespace check passed. Documentation/state changes
only; no product suite, native scenario or benchmark rerun is claimed.

GitHub refs/heads/main confirmed at `cc4938238c48bf9f3b538a42be5dc998f5d78897`,
which already contains the verified implementation commit `9e5ca04`.

Read-only current-cache queries completed: three named games ready/zero-tail/
flags25/final-result-present; seven total indexed rows current/ready. Source
inspection substantiates the partial-catalog error and writer conflict paths.
Pre-fix focused frontend: 6 expected failures, 71 passes; writer conflict in both
directions, deferred viewport unreported, concatenated library fragments, replay
status outside header. Pre-fix native catalog: 1 expected failure on partial-only
bundle (`recording media identity unavailable`). Logs: build/qb-lib-023/pre-fix-*.log.
At the pre-fix boundary no release build/native acceptance had run; subsequent
post-fix verification is recorded below.
Post-fix focused frontend: 117 passed across six files, including App wiring,
queue lifecycle/priority, final-query success/failure/foreground barrier and replay
partial failure/pending settlement. Existing 14 benchmark trace snapshots passed
unchanged after fixing a transient premature readiness publication caught by them.
TypeScript check passed. Native catalog tests: 2 passed, including ten invalid
metadata/media/telemetry cases and unfinished membership/pagination. Logs under
build/qb-lib-023/focused-frontend-3.log, check-2.log, focused-catalog-2.log.
Full frontend: 545 passed (40 files), TypeScript/Vite build passed. Rust fmt and
Clippy all-targets/-D warnings passed. Initial full native run: 149 passed, 1 child
fixture failed, 9 ignored. Diagnosis: machine has no execution policy configured,
so PowerShell refuses the test-created .ps1 before writing its ready marker;
repeat reproduces, and process-only PSExecutionPolicyPreference=Bypass permits
the unchanged fixture. Full native rerun with that process-only setting passed;
release build passed. No system policy or product/test validation changed.
Production executable SHA-256:
ed227490270514f1fc49e808a7db404ae77a82dab7e8d1f37177aeb9a07f2806.
Canonical validation passed (102 items / 21 linked pairs), diff whitespace passed.
Prepared build/qb-lib-023/native as a fresh sentinel-owned acceptance profile,
copying only historical seed bundles with labelled synthetic MP4s, plus independently
identified malformed optional-source and partial-only variants. Source-before
hash manifest retained. Original normal app was idle; closed it to rebuild release.
Computer Use could read the old minimized app's accessibility tree after restore
(including the reported nine/incomplete/unavailable notice), but its screenshot
capture timed out. A hidden isolated acceptance launch (21576) was not targetable
and was stopped; a visible isolated launch (31532) followed. The user stopped
Computer Use with physical Escape during window selection. Native interaction,
fresh-release visual verification and after-source-hash comparison remain not run.
On the user's two-minute resume (13:14:46 UTC), no existing acceptance app window
was available. A normal isolated cold-cache launch (36240) ran in the execution
environment without a targetable desktop window and was stopped. Interactive
desktop launch (12196, app-data-3, same isolated library/profile) exposed one
window, but screenshot capture failed with `FrameArrived timed out`; accessibility
showed only generic WebView panes and window chrome, no usable app controls.
No play/seek/navigation or visual success is claimed from that attempt.
Source comparison passed for both historical seed and isolated fixture files:
build/qb-lib-023/native/source-after.json records seed_unchanged=true and
fixture_unchanged=true against source-before.json. The interactive fixture app may
remain open. The user time limit ended further native automation.
Planning canonical/linked-artifact validation and whitespace checks are recorded
by the final handoff validation; they do not establish a product fix.
User-operated launch preparation: release SHA-256 matches the completed build;
launch-manual.ps1 parses and uses a new cache/WebView profile on every invocation,
with the sentinel-owned library and retention disabled. Its launch.json and
games-before.json retain process/profile/source evidence. Existing Master Yi
journal supplies productive multi-page work without delays or product hooks.
Manual scenarios include normal Master Yi/Akali, deliberate malformed optional
Ashe source and unfinished partial-only bundle. Await user observations, then
prepare deliberate Ashe source recovery and final hash comparison.
User reports overall good behaviour in the prepared app; no individual scenario
breakdown was supplied, so none is invented. Post-user source comparison passed:
source-after-user-check.json records both historical seed and fixture unchanged.
The comparison first used a games-relative base against a library-relative
manifest; corrected that path-only harness error and reran the exact hash check.
Replay notice refinement: focused Viewer suite 48 passed; full frontend suite
549 passed across 40 files; typecheck and production frontend build passed.
Logs: loading-notice-tests-3.log, loading-notice-full.log,
loading-notice-check.log under build/qb-lib-023. Timer regressions cover fast
success with no notice, 499/500 ms boundary, immediate clearing when the final
owned lane settles, immediate unavailable feedback, and disposal before reveal.
The first regression setup released indexing while a real viewport read was
still pending; corrected it to hold/release the final viewport lane. Existing
recovery assertion now waits for actual event/benchmark readiness instead of
inferring readiness from absence of the deliberately delayed notice. All frozen
benchmark traces still pass.
Refined production desktop build passed (loading-notice-release-2.log) after
closing the identified earlier fixture process that locked the executable.
Release SHA-256: 349bd27936ff7cacdad3a2d5756ad6b3936d9a31c1d3f2da07d6a20d8642b3c5.
Canonical validation and whitespace checks passed. Native Rust source is
unchanged by this presentation refinement; its previously passed checks remain
valid. Warm isolated relaunch succeeded; the user accepted the refinement and
requested GitHub delivery. This does not invent an explicit fixture-recovery
observation, which remains outstanding.
Follow-up to 1750337: added eight focused regressions; before correction they
failed as expected (24 existing cases passed), including the hidden unfiltered
warning. Replaced the conflated History string with a typed limited/failed issue,
retaining caught diagnostics privately and clearing it on fresh accepted context.
Catalog item errors and returned unavailable progress are nonterminal limited
results; rejected/invalid catalog or recording operations stop the session and
are failed, irrespective of their underlying source/cache cause. The UI retains
loading/final-query precedence, shows a safe operational failure warning without
semantic filters and keeps source/coverage warnings filter-relevant. Benchmark
consumers received only the field rename, preserving their strict issue guards.
Final affected frontend tests: 147 passed across six files (history, LibraryScreen,
shared writer, App replay wiring and existing search/application observer tests).
Tests cover incomplete bundles, legitimate unknown coverage, catalog item errors,
returned unavailable results, terminal catalog/recording failures, final-query
settlement, no idle retry, fresh-refresh recovery and an enabled replay-open action
while the warning is visible. TypeScript and production Vite build passed.
Logs: build/qb-lib-023/history-issue-pre-fix.log, history-issue-tests-final.log,
history-issue-check.log and history-issue-build.log. No native/recorder or unrelated
suite rerun was needed for this bounded frontend correction.

## Deviations

2026-10-29 explicit user-directed requirement revision: deliberate native fixture
recovery is optional follow-up, not a completion/delivery gate. This does not
convert unobserved scenarios into passes. The plan carries this narrow amendment.

Originally no design deviation. The root-cause investigation expanded the
user-facing label request into
one coordinated initialization/ownership feature rather than masking failures.
User refinement after native observation: delay only the replay loading notice
by 500 ms to avoid a flash during ordinary opens. The underlying loading/work and
benchmark readiness remain immediate; settled notices are never delayed and no
minimum display duration prolongs finished work. This is presentation timing,
not a changed ownership/classification design.

## Decisions

2026-10-29: accept the verified implementation for delivery and close this feature.
Do not prolong implementation for optional deliberate fixture recovery. Existing
code/verification history is retained; no product code changed in this handoff.

The user explicitly answered Yes, fix the behaviour too. Implementation is
authorized; do not ask the same scope question again in the execution pass.
Keep one frontend writer owner under LibraryController across lifetimes, with
latest queued history/replay/maintenance purposes. Native writer/read/admin
permit contracts remain unchanged. Supersession never claims to cancel running
IPC. Loading represents active/scheduled current work and ends on failure;
source gaps never become an endless loading condition. No automatic generic
retry loop, relaxed query coverage, partial-bundle deletion or media remount.

Pre-implementation contract clarification (2026-10-28): only absent metadata is
recognized unfinished metadata. The existing broad GameSummary.incomplete flag
also covers read/parse/validation errors and cannot itself authorize skipping
indexing. Present malformed, unsupported, mismatched or unreadable metadata stays
an error. Valid metadata with missing/empty/non-file finalized video also remains
a strict import error; duplicate identity and invalid telemetry remain errors.

Every queued writer promise settles on replacement, navigation, refresh, root
change or disposal. Active calls retain ownership until actual success/failure,
then release in finally even for synchronous throws. Reset is queued maintenance
inside the existing mutation lifecycle, admitted by captured root/request/token;
navigation does not cancel admitted maintenance. It must not deadlock on busy.

Optional loading represents current scheduled/active work, including deferred
viewport loads. It never gates/remounts media or erases useful same-source facts,
windows or overlays. Pending work takes precedence over partial failure; when
idle, useful survivors yield limited details and no survivors yield unavailable.
Source changes still invalidate source-specific data. Benchmark readiness retains
its stricter complete-data meaning independently of user presentation.
The typed history distinction describes continuation versus session termination,
not guessed exception text. It preserves all existing source validation, query
coverage, writer scheduling and loading/final-query settlement paths.

## Blockers

None for delivery or completion under the user-revised scope. Earlier Computer
Use failures and lack of individual manual observations remain recorded
limitations, not passing results.

## Next action

No active implementation or required acceptance work. The verified implementation
is already on origin/main; this scope/closure handoff is authorized for delivery.
Optional fixture recovery may be revisited only when useful to a product question.
