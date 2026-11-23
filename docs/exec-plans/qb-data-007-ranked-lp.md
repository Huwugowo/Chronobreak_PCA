# QB-DATA-007 ranked LP design

## Purpose

Show the League client's confirmed signed LP adjustment beside the recorded match's Victory or Defeat in Match History.

## Relevant planning-time architecture

The recorder owns the LCU session. Its Windows discovery checks the client's process and adjacent lockfile. `league_client::transport::Transport` uses bounded authenticated loopback HTTPS. `ContextSession` produces an exact recording binding from two agreeing LCU and Live Client observations. `service::stop_recording_inner` passes that binding and a guarded directory to `post_game::Handle` before publication and reports publication or cancellation separately. EOG and history results already have independent durable lifecycles. Neither acquires ranked notifications.

`DirectoryGuard` can recover a bounded directory locator across restarts and already verifies published metadata, journal manifest and provisional match identity before installing final facts. The existing app preparer coalesces native notifications, maintains one SQLite writer per library and refreshes checked history cards separately from replay import. `HistoryController` keeps displayed pages during background refresh. Solid match cards are keyed by match group and participant lists already preserve equal values.

## Scope and non-goals

Collect League ranked solo/duo and flex LP notifications for newly admitted bindings. Match an explicit queue type to the supported ranked queue IDs. Store only the signed delta needed by History and the identities required to validate it. Legacy recordings retain their normal history and replay behavior. No historical endpoint pass, rank snapshots, rating estimation, or change to the existing Live, EOG or match-history retry cadence.

## Exploration findings

The recorder has no existing WebSocket. Reuse `Transport` and its authenticated HTTPS client for the loopback upgrade and add a bounded WebSocket frame library. The existing per-recording final-result source carries outcome and stats and can be late or missing. Updating it for LP would tie two independent facts together. Adding LP to the journal fingerprint would run card and replay preparation unnecessarily.

## Chosen design and rationale

Under the existing service-owned post-game coordinator, admit only ranked bindings and process their existing submit, published and cancel signals off the finalization path. Reuse the recorder's bounded, atomically replaced ledger and directory primitives for a small separate LP state file. Retain at most eight recent jobs and observe unresolved jobs for up to five minutes from admission. On restart check saved deadlines and recover only the recorded allocation, including publication inferred from the existing core identity check. No credentials are stored. Drop cancelled, expired and invalid allocations.

While there are unresolved eligible jobs, open at most one loopback WebSocket and subscribe to `OnJsonApiEvent_lol-ranked_v1_current-lp-change-notification`. Then take one snapshot with `GET /lol-ranked/v1/current-lp-change-notification` to cover notification arrival before subscription and the restart gap. A failed or disconnected socket allows one delayed reconnection per unresolved batch, with a snapshot on the replacement connection. A new unresolved admission can take one snapshot on an existing connection for its own pre-subscription gap. Consume exact topic Create/Update messages rather than fetching for every event. Check a bounded payload, positive numeric `gameId`, recognized queue type and an explicit signed integer `leaguePointsDelta` before matching. Ignore deletes, mismatches, missing values and duplicates. Keep the first confirmed receipt for a binding. Use the same client and discovery checks as other LCU work and release the HTTP permit after upgrade. The socket and timers are service-owned, bounded and cancelled on shutdown.

Durably save a matching receipt before install and write `league_ranked_lp.json` once per published recording with the bound media ID, game ID, queue ID and exact signed delta. Publication and receipt can arrive in either order. Read an existing matching sidecar for idempotent recovery after a completed rename. Preserve a preexisting file and respect directory generation checks. Split recordings for the same game and local account can reuse the one accepted delta. LP availability has no effect on capture publication or result selection.

Use an optional SQLite column projected by the existing card writer. An LP-only filesystem hint prepares the named bundle through the same root and writer ownership, validates the sidecar against its exact match identity, then updates only the nullable scalar when different. Keep the LP sidecar out of the replay source stamp. The existing discovery and recovery pass also checks the sidecar for ranked bundles, including a rebuilt cache. The existing paged history query includes the scalar from the chosen recording row. A read-only connection to a cache that predates the additive change returns unknown until the writer migrates it.

Use the existing prepared-data notification and page lifecycle. Keep reactive dependencies scalar for LP and stable for unchanged row stats, artwork and participants. Use a small blue plus for positive LP, red minus for negative LP and neutral zero. Render beside the existing outcome for Victory and Defeat with accessible LP text. Suppress the indicator when the optional field is absent.

## Rejected alternatives and planning decisions

Periodic GET would compete with required LCU requests and repeat work for games with no notification. Persisting only to SQLite would leave the app cache as the sole authority and require capture to know the app index. Extending a final-result sidecar would force rewrites and couple LP to result availability. Selectively reading LP files from History queries would add source I/O on navigation. The signed delta owns presentation because promotions, demotions and special adjustments can differ from a subtraction of rank values or the expected outcome sign.

## Milestones

1. Add the shared bounded LP receipt, recorder transport and post-game lifecycle. Exercise realistic event and snapshot payloads, guarded publication and restart recovery.
2. Add additive index projection, targeted preparation, History API projection and stable row presentation. Exercise old caches, source changes and mounted-card updates.
3. Run the applicable project verification and targeted short scenarios, document durable facts and record evidence.

## Verification design

Use `cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, and `npm run test --prefix app` with focused fixtures first. Verify stale event versus current job, early and late events, duplicate and conflicting receipt, promotion/demotion and exceptional sign or zero, missing notification, restart before and after installation, and a removed or replaced allocation in a dedicated library. Verify native hint classification and targeted cache projection while ready replay state is retained. Verify API decoding, small LP labels and stable card, participant and image nodes during a prepared update. Use the applicable routine checks and canonical validation in `docs/development/VERIFICATION.md`.

## Performance and reliability gates

Verify zero ranked requests with no eligible binding, no LP polling while a socket is healthy, at most one reconnect per unresolved batch, bounded event and ledger memory, and no write for an unchanged receipt. An LP-only update must not start catalog discovery, journal import or overlay preparation. The existing query has the same query and source-file access pattern. Exercise the budgets with short focused fixtures and inspect the query shape. Any live-client or interactive observations are recorded separately from generated evidence.
