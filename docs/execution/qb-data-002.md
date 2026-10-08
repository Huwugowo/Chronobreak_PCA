# QB-DATA-002 execution checkpoint

Feature: `QB-DATA-002`
ExecPlan: `docs/exec-plans/qb-data-002-post-game-confirmation-v2.md`
Updated: 2026-10-08

## Current milestone

Milestone 4: real app-closed validation. Implementation, automated checks,
production builds and generated missing-data media validation pass. The feature
remains in-progress. App-closed WIN, unavailable-result and consecutive-candidate
isolation cases pass. Local LOSS encoding, explicit delayed-EOG confirmation and
full real EOG resource evidence remain.

## Active unit

Await the user-deferred ordinary LOSS match next session. The rebuilt recorder is
idle and ready, with terminal aggregate diagnostics validated. Two user-reported
quit/surrender sessions were PRACTICETOOL in both APIs: healthy recordings, distinct
provisional candidates and no result; the current EOG diagnostic returned 404.
These establish unavailable-result behavior, not local LOSS. No minimum duration applies.

## Completed

- Added shared `chronobreak-league-data` schema-v1 result types, nullable numeric
  stats, fixed field aliases, family coverage and bounded participant/loadout facts.
  The recorder purpose-built EOG DTO skips unlisted/account/reward fields.
- Added exact top-level game-ID confirmation; association contradictions reject
  the block, while result contradictions retain stats with unknown outcome.
  Synthetic tests include large IDs, stale/invalid IDs, duplicate stats, alias
  conflicts, malformed roster rows, local flags and nullable loadouts.
- Added one service-owned in-memory coordinator: four pending jobs, oldest overflow,
  at most 18 attempts, ten-second-or-slower spacing and 180-second closure expiry.
  Shared permits serialize LCU requests and joined discovery. Acquisition remains
  owned while failure, expiry, deletion and shutdown are processed.
- Capture closure snapshots 003 before poller/video joins. EOG acquisition starts
  concurrently, confirmed facts wait for healthy canonical publication, and
  failed publication cancels enrichment. Optional setup failure preserves recording.
- Added generation-bound late result installation with share-delete handles,
  bounded core/provisional revalidation, exclusive temporary creation and no-replace
  rename. Dedicated Windows tests prove deletion before/during installation, no
  recreation, no-clobber, media/game mismatch and replaced-directory rejection.
- Added bounded optional app reading in the existing replay slot, a compact result
  summary in library scans, typed frontend decoding and EOG-labelled viewer totals.
  Descriptor/media mounting and recorded totals remain independent of result facts.
- Validated the first actual integrated app-closed WIN recording on 2026-10-08:
  exact candidate/result game identity, consistent core media identity, ten-player
  final facts, unchanged sidecars during production-consumer inspection and clean
  full H.264/AAC decode. Detailed evidence is below.
- Validated two consecutive PRACTICETOOL quit/surrender recordings: healthy media
  and production-reader acceptance with no final facts. All three candidate IDs
  are distinct and the prior WIN never attaches to either later recording.
- Added aggregate diagnostics on terminal job transitions as well as joined
  shutdown. Corrected omitted in-flight expiry accounting and separated deletion
  and cancellation. Tests assert terminal classifications and virtual-hour totals.
- Read-only implementation audit identified expiry/cancellation, nested parsing,
  false-local flags, conflicting team winner flags, coverage and optional-startup
  issues. Corrected these paths and added focused tests.
- Read the complete QB-DATA-002 feature entry, product/workflow/verification
  authorities, PLANS contract, recorder lifecycle and media-library architecture.
- Reviewed the sanitized 2026-09-29 EOG evidence: exact game identity agreement,
  result evidence, final-stat families and unavailable aborted Practice Tool
  behavior. History enrichment is intentionally deferred in v2. LOSS still
  requires explicit coverage.
- Accepted the user-directed boundary: QB-DATA-003 remains provisional; this unit
  alone may request EOG, transition to confirmed, and attach final facts. Recording
  duration is not an acceptance condition.
- Challenged retry bounds, in-memory job overflow, deletion races, alias/conflict,
  identity and consumer-compatibility risks. Accepted v2's deliberate deferrals:
  no persisted journal or recorder-restart recovery, no history endpoint and no
  redesign of app deletion. Rejected history-position matching, Live GameEnd
  inference, raw archives and a competing match-fact database.
- Adopted the user-reviewed immutable design at
  `docs/exec-plans/qb-data-002-post-game-confirmation-v2.md`; v1 remains
  superseded history.

## In flight

A headless release recorder was started on 2026-10-08 at 09:08 UTC, PID 14560,
using ignored `build/qb-data-002/live-b9526871-6d9f-4695-8fab-9cff146cb20a`.
Its isolated library uses high/H.264 and retention/autostart disabled. INFO
logging is explicit after correcting an inherited RUST_LOG=warn. An initial
medium-profile setup was rejected by the existing native High/Auto-only contract;
the test config was corrected to High. The
viewer is closed. `build/qb-data-002/active-validation.json` records the local
PID, executable and paths; verify these before relying on the process on resume.
This local process/config/evidence is not transferred to another PC by Git.
The first bundle is `library/games/1791450934`; it finalized and received a confirmed
WIN result. Two subsequent PRACTICETOOL bundles are `1791452159` (quit) and
`1791452250` (surrender), both healthy without a result after the retry windows.
PID 14560 was stopped only after both retry windows ended and after verifying its
executable, idle state and no active League game. Its joined resource totals were
not collected; stopping it is not evidence of graceful coordinator shutdown.
The rebuilt release started at 09:57:44 UTC, PID 17588, with explicit initialized/
idle evidence and the same isolated library/config. `active-validation.json`
records this current PID. The user deferred the ordinary LOSS check to next session.

## Remaining

1. Record an actual app-closed local LOSS session and explicit delayed-EOG
   confirmation as available. WIN, unavailable quit/surrender and consecutive
   candidate isolation pass. Verify exact candidate/EOG game ID, source/result
   stats, media/events and saved-state compatibility.
2. Establish the real primary-local LOSS encoding (aggregate confirmation logs
   distinguish binary 1/0 without emitting identifiers), then enable LOSS and
   rerun the affected outcome checks.
3. Retain actual request/latency/pending/result-write resource aggregates and
   coordinated documentation, then apply the complete feature completion gate.

## Verification

- 2026-10-08 app-closed user-reported custom quit/surrender, identified as
  PRACTICETOOL by both LCU and Live, queue 3140. Bundles `1791452159` (quit,
  closed 09:36:44 UTC) and `1791452250` (surrender, closed 09:40:24 UTC) were
  canonically published in 92/77 ms respectively. Both retain valid provisional
  context and matching core media identities; neither has a result after its
  180-second window. All three candidates have different exact game IDs; the
  first unavailable job overlapped the second game's startup without carrying
  over the earlier WIN. This is unavailable-result and consecutive isolation
  evidence, not primary-local LOSS or observed remake/early-surrender encoding.
- Both actual unavailable bundles pass existing production descriptor, replay
  and library readers. Each projects two events inside media and two player
  timeline points, null final facts/result summary, saved=false and byte-identical
  JSON before/after inspection. Logs retain four/17 snapshots with available
  calibration. Packaged r6 ffprobe verifies 1080p60 H.264/stereo 48-kHz AAC:
  43.626667 seconds/2,617 frames/64,024,568 bytes and
  173.568 seconds/10,414 frames/260,324,924 bytes. Strict single-thread full
  video/audio decode exited 0 with no error output for each.
- After both windows, one bounded read-only diagnostic requested only EOG:
  HTTP 404, 113 response bytes, 31 ms. Sanitized output is ignored at
  `build/qb-data-002/live-b9526871-6d9f-4695-8fab-9cff146cb20a/current-eog-diagnostic.json`.
  This observes the current endpoint; old product logs do not expose each retry's
  response and cannot prove that every prior attempt returned 404.
- Terminal diagnostics unit: recorder all-target/all-feature check/test, fmt and
  Clippy pass; release rebuild passes. Tests: 172 library + three binary + three
  example, six existing ignored checks. The first sandbox test run failed the
  pre-existing child timeout fixture before its wait loop; normal Windows retry
  passed. In-flight expiry/deletion/cancellation classification passes; the
  virtual hour asserts peak four, one oldest drop, four expiries, 72 attempts and
  72 not-ready responses. No real terminal aggregate is claimed for the new build
  yet; it started idle at 09:57:44 UTC for the next session.
- 2026-10-08 real app-closed Co-op vs AI WIN, capture release build at `03a50a9`:
  ignored root `build/qb-data-002/live-b9526871-6d9f-4695-8fab-9cff146cb20a`,
  bundle `library/games/1791450934`. Target closure at 09:28:06.053 UTC, healthy
  canonical publication at 09:28:06.395, EOG confirmation at 09:28:16.120.
  The exact decimal candidate/result game IDs agree; metadata, log, provisional
  and result media UUIDs agree. Local Riot ID and champion agree; Live ORDER maps
  to EOG team 100. Candidate queue 890 and EOG SWIFTPLAY labels are compatible.
  Source is `lcu_eog`, confirmed WIN, not ended early, no label mismatch.
  Aggregate primary WIN counters are one binary 1 and zero binary 0 observations.
  This confirms WIN only; it does not establish primary-local LOSS encoding.
- Final result: 21,785 bytes, two teams/five players each; local 21/4/2 KDA,
  76 minions, 17,722 gold earned, 31,910 champion damage, seven retained item slots,
  six rune/perk counters. Combat/economy/damage/support/vision/objectives/runes are
  present and loadout is explicitly partial. No PUUID is stored in the result.
- Existing production app functions were exercised read-only through the ignored
  source-import harness `build/qb-data-002/consumer-verifier`: `replay_descriptor`,
  `playback_probe` and `list_games` all accepted the real bundle. Descriptor/full
  probe timelines agree; both result summaries agree and are 52 bytes. All 85
  events map inside media; 18 player and 27 KDA timeline points project. Recorded
  and final KDA both equal 21/4/2. Saved remains false and the four JSON sidecars
  are byte-identical before/after inspection. One combined read/projection took
  49,424 us; this is diagnostic timing, not a performance-budget claim.
- Packaged r6 ffprobe verifies 1920x1080 H.264 at exact 60 FPS and stereo 48-kHz
  AAC, duration 750.464 seconds, 1,138,203,185 bytes. Metadata records 45,027
  encoded frames with terminal progress; game log has available five-sample
  calibration, 73 snapshots, 85 chronological events and 343 derived changes.
  Packaged FFmpeg strict single-thread full video/audio decode
  (`-nostdin -v error -xerror -threads 1`, both streams, null output) exited 0
  without errors. The existing no-loopback-device silent-track fallback was used;
  this validates the media stream, not actual game-audio capture.
- Current real-run resource evidence is partial: 003 collector logged four
  requests/18,868 response bytes; post-game confirmation logged primary counters
  and result size is known. EOG request/latency/pending/write totals emit only at
  joined coordinator shutdown in the original build. That process has no external
  shutdown command and its aggregates were not collected. The follow-up terminal
  logging unit removes this observation limitation for future runs; it does not
  recover counters from the old process.
- 2026-10-08: production recorder release and Tauri desktop build passed. The
  current release recorder is the executable used by the isolated live run.
- Generated 10-second missing-Live service fixture passed with healthy canonical
  H.264/AAC publication and no match/result sidecar. Root:
  `build/qb-data-003/capture-fixtures/a2d655bb-3737-40ba-add2-10b81e515b89`.
  Resident memory samples: 75,452,416 -> 75,251,712 bytes. The first attempt
  lacked the helper executable; after building it, sandbox WGC reported empty
  content size. The normal Windows capture retry passed. Delayed EOG was not
  exercised by this fixture; it cannot obtain a candidate with Live absent.
- Standalone scan-resource characterization (24 synthetic ten-player bundles,
  ten scans per condition) passed: 87,579 us without vs 258,056 us with results,
  19,133-byte result and 52-byte summary. Eight process samples observed a
  9,801,728-byte peak working set across the test; this is local diagnostic
  characterization, not a real-game resource or product performance claim.
- Canonical validation passed: 62 roadmap items and ten plan/checkpoint pairs.
  The immutable reviewed v2 ExecPlan remains unchanged.
- 2026-10-08: recorder all-target/all-feature test passed: 172 library + 3 binary
  + 3 example tests, with six pre-existing hardware/runtime checks ignored.
  Recorder all-target/all-feature check and Clippy passed; fmt passed.
- App Rust tests passed: 99 tests, one pre-existing staged-runtime check ignored.
  App all-target Clippy and fmt passed. Frontend: 220 tests/25 files passed;
  TypeScript check and production Vite build passed. Vite retains its existing
  ineffective-dynamic-import warning. Sandbox process spawn was rejected on the
  first frontend test attempt; the normal elevated retry passed.
- Virtual hour: four retained jobs x 18 attempts = 72 requests maximum; a fifth
  drops the oldest. Cancellation/deletion/failure/shutdown tests end work, including
  an in-flight owned acquisition. Confirmed jobs wait without further requests
  until publication. Windows result-write and app optional-compatibility tests pass.
- Synthetic scan characterization: 24 bundles x 10 scans, 169,741 us without results
  versus 299,760 us with 19,133-byte ten-player result fixtures; compact summary
  is 52 bytes. This is diagnostic local timing, not a product performance budget.
- Planning review covered exact `gameId` equality, local identity, participant
  conflicts, result precedence, missingness/provenance, bounded in-memory retry
  budgets, writer/deletion races and preservation of healthy media.
- The v2 plan defines remaining real-game and resource gates. Automated evidence
  below is current-tree implementation evidence, not completion evidence.
- Cross-PC handoff, 2026-10-08: canonical validation passed for 62 items/10
  plan-checkpoint pairs. The reviewed v2 plan and this checkpoint are included in
  the source handoff; product implementation still starts at Milestone 1.

## Deviations

Setup also exposed an existing headless lifecycle issue: service initialization
errors leave the process waiting for Ctrl+C without an idle state. It is recorded
in QB-DIST-002 for supervision work. The isolated run uses the supported High
profile and has explicit initialized/idle evidence; no unrelated fix was made.

The original roadmap item was expanded from result-only acquisition to exact
provisional-candidate confirmation plus useful allowlisted final statistics, per
the user-approved data split. The user-reviewed v2 deliberately narrows the first
implementation to an in-memory post-game window: no history enrichment, persisted
job journal, recorder-restart recovery or deletion-protocol redesign. Recording
duration remains outside acceptance.

## Decisions

- Shared result types live in the focused `league-data` crate to prevent producer/
  consumer schema divergence. This is file placement within the approved design.
- Existing scans may show empty/invalid timestamp directories as incomplete rows.
  The v2 deletion race is accepted without redesign; stray results cannot supply
  playable media or confirmed facts without valid core/provisional context.
- Local WIN:1 is supported; WIN:0 remains unknown until primary-local LOSS is
  captured. Team.stats.WIN is aggregate and is never used for outcome proof.
- EOG top-level `gameId` equality is mandatory; local-player/participant identities
  are corroboration. No history endpoint is used in v2.
- Only confirmed results may project WIN/LOSS or authoritative final totals.
- Missing, delayed, conflicting and aborted data remain explicitly partial or
  unknown; no unavailable result sidecar is created and values are never fabricated
  as zero.
- Attempts and pending jobs remain bounded in memory; recorder restart may lose the
  optional result. The late writer revalidates generation identity and yields to
  unchanged app deletion semantics.
- Provisional context, metadata.saved, video, game logs, replay clocks and Live
  polling remain owned by their existing features.

## Blockers

Real primary-local LOSS remains unobserved; the user deferred an ordinary match to
next session. Neither PRACTICETOOL quit/surrender returned a confirmed EOG block.
Current/older WIN observations and older opposing-roster WIN=0 do not enable LOSS.
Full real EOG resource totals remain uncollected; terminal diagnostics are now
available without requiring a recorder shutdown. Delayed confirmed EOG still needs
explicit response evidence; the earlier WIN's ten-second confirmation delay alone
does not prove the not-ready/stale response shape.

## Next action

Next session, verify the revised recorder PID/path/idle state from
`active-validation.json` and use an ordinary match that ends with Defeat. After
completion inspect the new bundle non-destructively: exact provisional/result IDs,
primary-local binary observation, corroborating team outcome, final stats, healthy
media and terminal resource aggregates. Establish actual local loss encoding
before enabling LOSS and validate the affected outcome cone. Preserve all prior
bundles; no recording duration condition applies.
