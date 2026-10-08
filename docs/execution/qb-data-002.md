# QB-DATA-002 execution checkpoint

Feature: `QB-DATA-002`
ExecPlan: `docs/exec-plans/qb-data-002-post-game-confirmation-v2.md`
Updated: 2026-10-08

## Current milestone

Milestone 4: real app-closed validation. Implementation, automated checks,
production builds and generated missing-data media validation pass. The feature
remains in-progress. The first current-build app-closed WIN recording passes;
LOSS, aborted/remake, delayed/consecutive-game and full resource evidence remain.

## Active unit

The first real Co-op vs AI recording passed exact association, final-stat,
production-consumer and full-media inspection. The recorder remains ready for the
next match; the user agreed to run a LOSS case. Inspect the next bundle and collect
the primary-local LOSS encoding before enabling LOSS. No minimum duration applies.

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
- Validated the first actual current-build app-closed WIN recording on 2026-10-08:
  exact candidate/result game identity, consistent core media identity, ten-player
  final facts, unchanged sidecars during production-consumer inspection and clean
  full H.264/AAC decode. Detailed evidence is below.
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
WIN result. PID 14560 was verified alive/idle after publication. Keep it running
for consecutive-game validation; the user agreed to run a LOSS case next.

## Remaining

1. Record an actual app-closed local LOSS session, plus aborted/remake, delayed and
   consecutive games as available. The first WIN case passes. Verify exact
   candidate/EOG game ID, source/result stats, media/events and saved-state compatibility.
2. Establish the real primary-local LOSS encoding (aggregate confirmation logs
   distinguish binary 1/0 without emitting identifiers), then enable LOSS and
   rerun the affected outcome checks.
3. Retain actual request/latency/pending/result-write resource aggregates and
   coordinated documentation, then apply the complete feature completion gate.

## Verification

- 2026-10-08 real app-closed Co-op vs AI WIN, current release build at `03a50a9`:
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
  joined coordinator shutdown; this hidden headless instance has no external
  shutdown command. They are not yet collected, and the process remains alive
  for consecutive-game coverage. Do not force-kill it to claim those metrics.
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

Real local LOSS and remake/early-surrender encoding remain unobserved. The current
integrated WIN case and three older successful EOG captures prove local WIN=1;
older opposing-roster WIN=0 does not enable LOSS. Full real EOG resource totals
remain uncollected until an owned joined coordinator shutdown can be exercised.

## Next action

The user agreed to play a LOSS case and has been told the recorder remains ready.
When they report completion, verify PID/path and inspect the next isolated bundle
non-destructively: exact provisional/result IDs, primary-local binary observation,
corroborating team outcome, final stats and healthy media. Establish the actual
local loss encoding before changing LOSS projection; then validate only the affected
outcome dependency cone. Preserve the prior WIN bundle for consecutive identity
isolation. No recording duration condition applies.
