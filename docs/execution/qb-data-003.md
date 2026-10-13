# QB-DATA-003 execution checkpoint

Feature: `QB-DATA-003`
ExecPlan: `docs/exec-plans/qb-data-003-recording-match-context-v3.md`
Updated: 2026-10-08

## Current milestone

Milestone 6: QB-DATA-003 accepted.
The provisional reducer, recording-owned collector, poller/service seams, playback
projection and real app-closed publication check are complete. Exact post-game EOG
confirmation and final facts remain the separate QB-DATA-002 unit.

## Active unit

No active QB-DATA-003 implementation unit. The accepted real recorder integration
used the isolated configuration/library at `build/qb-data-003/live-integration-1790859507089`;
the recorder PID was verified as `recorder/target/release/recorder.exe` before the run.
Configuration did not alter user settings. No loopback device was found, so this run
used silent AAC fallback. QB-DATA-002 now has a finalized user-reviewed v2 plan;
its implementation is the next unit. No 002 product code is part of this feature.

## Completed

- Read current product/workflow, complete feature, checkpoint before immutable v3
  plan, verification, planning contract, forward-engineering skill and applicable
  recorder lifecycle/time architecture. Dependencies QB-CAP-001, QB-DATA-001 and
  QB-REPLAY-012 are canonically done.
- Reconciled canonical requirements with the user-requested v2 before implementation:
  required game/queue IDs and static composition corroboration replaced launch-ID/
  platform/listener-owner proof. Preserve v1/v2 immutable history.
- Historical source inventory remains in `docs/product/league-data-capabilities.md`.
  Earlier log/WMI/OS-proof research is provenance only, not active implementation proof.
- First real SWIFTPLAY bot session: LCU reached InProgress with positive game ID,
  explicit queue ID, local PUUID/full Riot ID, compatible map and queue-specific mode.
  The corrected v2 diagnostic reported three stable rounds, including exact local
  identity/champion agreement and five selections against five mapped human Live
  entries. Each had 10 Live entries, 5 bots, 9 full IDs and 10 mapped champions.
  These are field observations, not an authoritative association.
- That session exposed only 4 teamOne entries, no teamTwo entries and 5 selections.
  This invalidated roster composition as a general in-game binding contract.
- Accepted user-directed v3 split: 003 captures provisional identity; 002 confirms
  exact EOG gameId and owns final outcome/statistics; 004 owns richer timed telemetry.
- Revised `league_client/{mod,probe,transport,tests}.rs`, poller identity tap,
  standalone example CLI and README. Removed champion-manifest loading and every
  roster/champion association field; malformed irrelevant LCU roster shapes are ignored.
- Live observation requires successful active-player and gamestats responses and uses
  their minimum request start/maximum finish. A failed roster request cannot suppress
  identity; the normal snapshot request set and error behavior are preserved.
- V3 coherent round requires InProgress, positive game ID, explicit queue ID (including
  zero), bounded local PUUID, exact full Riot ID, a present compatible map and exact
  comparable queue/live mode. Missing queue mode cannot fall back to broad map mode.
- Diagnostic history stores only one coherent in-memory observation. Failed/incoherent
  rounds clear it; ordered distinct fresh rounds must agree on required evidence and
  opaque credential epoch. All emitted fields are sanitized aggregate values.
- Read-only scout identified cancellation during an owned credential recheck: root
  added a pending Ctrl+C check after joining the worker and before evidence admission.
  Cancelled/deadline-expired rounds cannot report coherent evidence.
- Reports are schema version 2 with `contract: provisional_v3`,
  `scenario_requested`, `provisional_round_coherent` and
  `two_rounds_coherent`. Feasibility and authoritative-confirmation flags remain false.
- Added a diagnostic lifecycle counter retaining one last-coherent game ID and an
  aggregate count. It survives unavailable rounds only for coverage review, never
  pair admission. A synthetic regression verifies new-game detection across a gap
  while pair history remains cleared.
- Observed v3 startup, stable mid-match and standalone diagnostic restart with the
  viewer/recorder closed. These are provisional field/lifecycle observations only.

## In flight

No QB-DATA-003 work remains in flight. Reducer/collector, sidecar, provisional
playback projection and the real app-closed publication check are implemented and
verified. The accepted recorder returned to idle after publishing its provisional
sidecar. Existing changes are preserved.

## Remaining

1. Implement the separate QB-DATA-002 feature from its finalized user-reviewed v2
   plan for exact EOG confirmation and final facts. Do not add EOG requests or final
   facts to QB-DATA-003.

## Verification

- Playback unit: full app Rust tests 96 passed, 1 existing environment test ignored;
  all 24 frontend test files/216 tests passed; TypeScript check, production frontend
  build and app Clippy -D warnings passed. Logs: build/qb-data-003-playback-{tests,clippy}.log.
  Focused tests preserve descriptor/scan identity, exact large decimal game ID, queue
  zero, save protection, save/unsave sidecar preservation and deletion in dedicated
  fixtures. Missing/corrupt/oversized/wrong-media/unsupported/unproved-confirmed files
  preserve schema-v2 playback/events. Viewer test displays provisional marker and
  recorded K/D/A without remounting media. No final facts are supplied by 003.
  Vite initially failed sandbox child creation with spawn EPERM; outside-sandbox
  frontend tests/build passed. Existing build chunk warning is unchanged.
- Recorder sidecar all-target/all-feature check and Clippy -D warnings passed;
  both recorder/app fmt --check and git diff --check passed after formatting correction.
  Read-only writer and producer/consumer compatibility review found no concrete defect.
- Dedicated service capture harness builds in release and passes Clippy. Two short
  sandbox attempts failed native startup with empty WGC content size; waiting for
  the fixture's first correctly sized draw did not resolve the sandbox failure.
  Same harness outside the sandbox passed a ten-second rehearsal at
  build/qb-data-003/capture-fixtures/b60047fa-5bdc-435b-b53a-62936014ec8f.
  The 600-second run passed at
  build/qb-data-003/capture-fixtures/6c07d434-bef6-4151-a224-ddfa74708341.
  Only generated window/silent audio/test library are used; real recordings untouched.
- Generated capture published matching schema-v2 media UUIDs, H.264/AAC at 60 FPS,
  36,284 frames/116,426,135 video bytes after about 604.7 seconds including readiness
  and shutdown. Packaged FFmpeg strict single-thread full decode exited zero with
  no errors (build/qb-data-003-capture-decode.log). 61 resident-memory samples:
  70,045,696 initial, 72,630,272 peak, 72,593,408 final bytes. These are resident
  memory samples, not GPU/private-memory or League performance evidence.
  Calibration never became available: zero snapshots/events and zero LCU requests/
  bytes/candidate, no context sidecar. Normal service shutdown joined all context
  ownership and published healthy media. No delayed LCU acquisition was exercised.
- Final recorder suite after adding the capture harness: 159 library/3 binary/3
  example tests passed, 6 existing checks ignored; release recorder build passed.
  Log: build/qb-data-003-final-recorder-tests.log.
- Real recorder integration, 2026-10-01, isolated output
  `build/qb-data-003/live-integration-1790859507089`: process-driven native capture
  started after exact HWND discovery, Live clock calibration succeeded, and normal
  game closure joined the collector with 4 LCU requests/18,900 response bytes and
  one immutable provisional candidate. The 612.053-second published bundle contains
  `video.mp4` (930,874,018 bytes), `metadata.json`, `game_log.json` (61 snapshots,
  70 events) and an 859-byte `league_match.json` with schema version 1/status
  `provisional`, decimal game ID, queue ID and two-round provenance. The sidecar media
  ID matched both metadata and game log; credentials/PUUID/Riot ID values were not
  logged. Packaged ffprobe exited zero: H.264 video, AAC audio, 60 FPS, 612.053333
  seconds (`build/qb-data-003-real-ffprobe.json`). This proves integrated provisional
  admission/publication and healthy media isolation. The user accepted this run as
  sufficient QB-DATA-003 evidence; recording duration is not an acceptance criterion.
  Exact EOG confirmation was not attempted because it belongs to QB-DATA-002.
- Sidecar unit: seven focused Windows tests passed; all-target/all-feature recorder
  tests 159 library passed, 6 existing manual/environment tests ignored, 3 binary and
  3 example tests passed. Log: build/qb-data-003-sidecar-tests.log. Fixtures cover
  64-KiB serialization cap, provisional/large decimal ID, media identity, no-clobber,
  concurrent single winner, removed/replaced/junction directories, sibling preservation,
  normal completion and abandoned owner/cancelled finish joining before return.
  All filesystem fixtures are dedicated temporary directories. The first Win32
  relative-root rename failed with invalid parameter; native NtSetInformationFile
  FileRenameInformation corrected the operation and all filesystem tests passed.
  Exclusive temporary creation and rename use the pinned handle, never mutable paths.
  Allocation handle permits deletion; final pin rejects replacement and blocks rename.
  Service awaits this optional worker after media validation and before publication.
  No power-loss durability or protection of sibling-media deletion is claimed.
- Reducer/collector unit: 23 focused league_client tests passed; all-target/all-feature
  recorder tests 152 library passed, 6 existing manual/environment tests ignored,
  3 binary and 3 example tests passed. Check and Clippy -D warnings passed.
  Logs: build/qb-data-003-association-{tests,check,clippy}.log.
  Shared checks preserve exact identity/IDs/context and enforce consecutive Live
  sequence continuity. Binding is immutable and survives closure.
- Virtual-hour fixtures: failed rounds limited to 360 rounds/720 modeled GETs/23,040
  modeled bytes; submissions stop after close. After binding, requests stay fixed at
  six GETs even as another virtual hour of producer updates arrives. This proves
  algorithmic work bounds, not real memory/performance or media finalization.
- Owned-worker tests passed for normal stop, direct abandoned owner and cancellation
  of an in-flight stop future. Production context requires the recorder's existing
  multi-thread runtime; exceptional Drop joins via block_in_place, normal stop awaits.
  OS worker waits can extend shutdown; no abort/detach or false OS deadline claim.
- Fast failed/recovered Live updates cannot bridge a sequence gap. Actual LCU HTTP
  round starts are throttled after discovery; slow then fast discovery regression passed.
- Poller tap now feeds the recording collector without a second Live request owner.
  Diagnostic streamed body cap remains separate; production body handling is unchanged.

- Canonical validation passed after this unit's edits: 62 items/9 plan-checkpoint pairs.
  This validates canonical structure and links, not product implementation.
- Cross-PC handoff, 2026-10-08: canonical validation passed for 62 items/10
  plan-checkpoint pairs. Source, tests, plans, checkpoint and sanitized evidence
  summaries are versioned. Referenced raw captures, test recordings, logs and other
  ignored build outputs remain local; they are not transferred by Git. The receiving
  PC should use the recorded acceptance and regenerate build dependencies as needed.
- Historical v2 recorder check/tests/fmt/Clippy passed. Its test log
  `build/qb-data-003-tests.log` records **141 library tests passed, 6 ignored**
  (147 total), plus 3 binary and 3 example tests. The earlier checkpoint incorrectly
  called all 147 passed; this count is corrected.
- V3 unit, 2026-10-01:
  - Initial focused league_client tests: 12 passed, before the lifecycle counter.
  - All-target/all-feature recorder tests: 142 library passed, 6 existing manual/
    environment tests ignored; 3 binary and 3 example tests passed. Rerun after the
    cancellation fix and counter addition passed. Log: `build/qb-data-003-v3-tests.log`.
  - All-target/all-feature cargo check and Clippy with `-D warnings`: passed.
    Logs: `build/qb-data-003-v3-check.log`, `build/qb-data-003-v3-clippy.log`.
  - Recorder fmt --check and git diff --check: passed.
  - Standalone example build: passed.
  - Synthetic regressions cover exact tags, large u64 game IDs, explicit zero/missing/
    malformed queue IDs, missing/conflicting map or queue mode, bounded strings,
    absent/partial/malformed LCU rosters, fresh/order/sequence/epoch changes and
    candidate changes. Network fixture proves failed Live roster parsing preserves
    active/game identity for initial and steady snapshots without adding requests.
    Counter regression covers a new coherent game after an unavailable gap.
- V3 preflight: `build/qb-data-003/v3-preflight-20261001-124052.jsonl` (ignored).
  Six seconds, one round, two LCU GETs, 9,131 response bytes, unchanged credentials.
  LCU exposed retained game/queue values but was not InProgress; Live identity absent.
  Coherent round/pair false; both feasibility/authoritative flags false. This proves
  bounded ordinary-user client access and rejection of idle prior-session values.
- Historical v2 preflight: two requests/472 bytes, no game or Live identity; client
  access only, not an in-game pass.
- Historical bot mid-match report:
  `build/qb-data-003/mid-match-fixed2-20261001-092556.jsonl` (ignored).
  Three reported stable rounds support field availability and the roster-boundary
  decision. They do not replace v3 lifecycle validation.
- Historical startup:
  `build/qb-data-003/startup-20261001-091944.jsonl` (ignored) records pre-game
  to InProgress while Live initially unavailable. Earlier pre-fix/mode-projection
  reports are not used as completion evidence.
- V3 startup/mid-match: `build/qb-data-003/v3-lifecycle-20261001-124135.jsonl`.
  38 rounds total, first coherent round sequence 26 at 301,170 ms with evidence age
  39 ms, first coherent pair sequence 27 at 311,211 ms with evidence age 36 ms.
  13 coherent rounds/12 coherent pairs before intentional probe termination.
  Idle retained-session values never produced a coherent round; all in-game required
  comparison flags agreed. Maximum reported LCU round span 17 ms.
- Diagnostic restart: old diagnostic process stopped deliberately after validating
  its exact workspace executable path (PID 25288); updated example rebuilt and one
  replacement started. This was forced probe termination, not graceful cancellation
  or game closure evidence. Report:
  `build/qb-data-003/v3-restart-lifecycle-20261001-124853.jsonl`.
  Sequence 1 at 50 ms coherent/one observed game/pair false, sequence 2 at 10,089 ms
  pair true, fresh age 34 ms, unchanged same-epoch/static evidence. No old pending
  evidence was restored. Requested label recorder_restart is diagnostic-only.
- Lifecycle report reviewed through sequence 92 at 942,427 ms: first game 1-72
  coherent (71 pairs); gap 73-76 not InProgress/no Live identity; second game 77-82
  coherent (5 pairs); after closure 83-92 unavailable/no pairs. First closure bracket
  713,373-725,581 ms; second startup 761,700-771,744 ms; second closure 821,981-834,018 ms.
  Sequence 77 changed game/count 1 to 2 without a pair; sequence 78 at 781,797 ms
  formed its own pair. All 78 coherent rounds agreed on required fields, evidence ages
  30-57 ms, all available epoch comparisons matched. User identifies ordinary bot then
  Practice Tool: cross-mode consecutive sessions, not two ordinary bot matches.
  Root reviewed transition rows; initial field/lifecycle feasibility passed.
- Probe force-stopped after exact workspace executable identity check. This is not
  graceful cancellation or media-finalization evidence.
- Exact EOG confirmation and final facts have not run because they belong to the
  separate QB-DATA-002 feature; the real QB-DATA-003 recording produced and published
  the provisional sidecar described above.
- V3 plan self-review passed for ownership split, provisional persistence, EOG
  handoff, compatibility, bounded failure and verification boundaries.
- User-directed decision on 2026-10-01: recording duration has no bearing on
  QB-DATA-003 acceptance, so the 612.053-second integrated run is accepted.

## Deviations

Real bot-game evidence and the user-directed provisional/confirmed split invalidated
v2. V2 remains immutable; v3 is authoritative. Old roster output is historical evidence
only and all roster dependencies have been removed from the active diagnostic path.
The user explicitly removed recording duration as an acceptance condition and accepted
the real 612.053-second integrated run for QB-DATA-003. This does not move EOG or final
facts into 003.

## Decisions

- Required positive gameId and explicit queueId; missing queue never defaults to zero.
- Provisional gate is exact local full Riot ID, InProgress, required IDs and compatible
  map/mode. No roster, champion mapping, bot semantics or composition fingerprint.
- Nonempty PUUID remains required source provenance from current-summoner.
- At least one LCU map source and the Live map must be present and compatible; if both
  LCU map sources exist, either contradiction rejects the round.
- Queue gameMode is the observed comparable mode. Broad LCU map.gameMode is retained
  as source provenance and may be CLASSIC when queue/Live are SWIFTPLAY. Missing
  comparable queue/live mode leaves the round unavailable; no broad-mode fallback.
- Two distinct increasing rounds: existing 10-second-or-slower cadence, 15-second
  freshness, 2–30-second separation. Required evidence and credential epoch must agree.
- Diagnostic pair agreement is not a production candidate, lifecycle pass or final fact.
- QB-DATA-002 must confirm exact provisional gameId through
  `/lol-end-of-game/v1/eog-stats-block` before attaching final facts.
- Reuse existing Live requests/timing; no added endpoints, raw archive, polling burst,
  replay-clock change or capture lifecycle dependency.
- Fixtures are synthetic and dedicated; no destructive use of real recordings.
- Old v2 composition requirements are historical and must not be restored.

## Blockers

No blocker remains for QB-DATA-003. Exact EOG confirmation and final facts are the
next feature's work and must not be attached to this provisional unit.

## Next action

Resume QB-DATA-002 from `docs/execution/qb-data-002.md` and its user-reviewed
`docs/exec-plans/qb-data-002-post-game-confirmation-v2.md`. Its Milestone 1 defines
result/coverage types and EOG parsing fixtures. The accepted QB-DATA-003 run ended
with the recorder idle; silent AAC fallback does not establish loopback-audio behavior.
