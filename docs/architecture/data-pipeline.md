# League data acquisition and preparation

The recorder collects facts; the app prepares local read models ahead of the
screens that use them. Opening a view or seeking never requests League data.
Precise media/game clock calibration remains independent of event precision.

## Acquisition

Live `gamestats`, `playerlist`, and `eventdata` polling is deliberately retained.
Clock calibration, match/account association, timestamped events (including
multikills), and player snapshots with dead/alive and respawn state still come
from the existing Live Client collector. This change does not replace those
facts with minute-spaced match-history timeline frames. Replay APIs are unused.

After recording closes, the service submits the confirmed match/account binding
to two independent acquisition paths:

- The existing finite EOG collector and its ledger remain in place. Its result is
  `league_result.json`.
- The history collector requests `/lol-match-history/v1/games/{gameId}`. It joins
  participants to identities by participant ID, requires the exact game ID and
  local PUUID, and normalizes accepted final facts into
  `league_history_result.json`. Boolean player wins determine the outcome, with a
  recognized team result as fallback. Conflicting local player/team results are rejected. Team wins,
  objective totals/first-objective flags, and available surrender indicators are
  retained. Missing values stay unknown; early-ending flags do not prove a remake.

History retries at 2, 5, 15, 30, 60, 120, then 300 second intervals. The bounded
24-job ledger `league_history_jobs.json` persists admission, the next attempt,
accepted results, and installation completion. It is written on transitions,
not every idle tick. Completed jobs make room for later games. If all 24 jobs are
still pending, the oldest is evicted so permanently unavailable matches cannot block new games.
The retained admission order and pending work survive a service restart; fetched
facts can be shared across recording splits of the same match/account. Credentials are never persisted. Requests are bounded
and use the existing single LCU HTTP permit.

Facts install atomically into the original directory allocation only after its
canonical recording/match identity is available. Removed directories are never
recreated. History is preferred when valid; EOG remains a fallback and its bytes
are preserved for comparison. Final match duration and acquisition receipts do
not replace the recorded video/game clock mapping. No end timestamp is invented
from game creation plus duration.

LCU timeline acquisition is intentionally absent from this implementation: no
current feature needs its additional minute-spaced data, and it cannot replace
the useful live events/player states. Spell usage/cooldowns are not synthesized.

## Preparation and persistence

The app has a `--prepare-library <absolute library>` mode that starts before
Tauri initialization. It runs without a window, WebView, playback server, media
probe, or decoder. A packaged recorder launches its sibling app in this mode,
hidden and below normal process priority, and owns its lifetime. Without that
sibling executable, preparation runs when the app is opened.

An app-owned preparation task also observes the configured library. Both paths
share a local, disposable SQLite cache and a cross-process writer lock. WAL
readers remain independent. Preparation uses the captured `ApprovedRoot` final
path for cache identity, writer ownership and source reads, matching accepted UI
commands regardless of the configured Windows path spelling. It separately
revalidates the logical path against that captured approval before each unit,
so alias retargeting and directory replacement still invalidate work.
The cached catalog is bound to its captured approval; a different approval
requires full discovery even when only an incremental notification arrives.
Preparation publishes the entire pending catalog's small metadata/final-result
card facts before importing any full journal. Card freshness has its own source
stamp and `history_ready` status; it does not imply replay readiness. Journal
imports retain validated card facts between bounded batches. A real result or
source change must revalidate those facts rather than preserving old enrichment.
Each preparation unit streams at most 32 committed journal batches into one
atomic SQL transaction, or reconstructs one bounded overlay page, then releases
the writer and connection and yields without a fixed inter-unit delay. Every
journal commit still passes checksum, sequence, scope and checkpoint-state
validation, with cancellation/root checks before and after its SQL projection.
The unit publishes its final checkpoint and committed-prefix rows together;
interruption rolls back that unit, leaving the previous prefix resumable.
Prepared SQL statements are reused within the unit. No database connection is
retained across ownership boundaries, so cache reset cannot leave a worker
writing an obsolete database. The
preparer uses native filesystem notifications, coalesced over one second, to
refresh and prepare only affected published recordings. Idle ticks do not scan
the library, hash sources, or open SQLite. Unpublished live journal/manifest
writes require no preparation. Clip inventory is never scanned by this worker.
Startup, root/cache replacement and notification errors/overflow request an
authoritative games scan. A five-minute recovery scan catches missed events or
filesystems without notifications. Transient writer contention retains the dirty
work with a five-second backoff; each bounded page checks ownership independently
without repeating catalog reconciliation. Unchanged summaries keep their search
facts current when other recordings enter/leave the catalog. A bad recording
settles as unavailable for its source version without blocking later recordings.

Catalog identity depends on actual library membership/basic metadata, rather
than a window-specific admission token. Source fingerprints include both final
result files, metadata, the observation manifest and journal generation. UI
admission still checks the accepted filesystem snapshot and selected root.
Reset holds the same writer lock through quarantine/replacement; subsequent
preparation rebuilds from the recording sources. Source files remain authoritative.

Overlay and scoreboard samples are materialized in SQLite before a recording is
reported ready. Revision-bound continuation state makes preparation resumable;
a source change invalidates stale prepared rows. Persistent optional-data errors
settle as unavailable for that source version rather than endless UI polling.

## UI reads

Ordinary recording/catalog status commands are read-only. Explicit maintenance
remains a separate write operation. History queries, scopes, event windows and
overlay pages read local SQLite. The viewer mounts media independently while
optional details finish. Prepared notifications refresh mounted history and
matching replay details without remounting the video.
History status polling refreshes queries when card readiness or full preparation
changes, not after unchanged pending responses. Background refreshes retain the
foreground loading state, and equal roster contents retain their mounted nodes.
Known stale card facts are withheld until their source stamp is validated again.

At desktop startup, Data Dragon publishes the newest complete local manifest
cache before awaiting its online version check. Cached card and replay artwork
can therefore render immediately. The background check can refresh the cache;
failure retains the existing offline-cache/placeholder behavior.

Scoreboard enrichment begins during idle time after the compact replay overlay,
before the first TAB press. Its panel stays mounted while hidden so its existing
bounded asset preloader can warm icons; seek/playback work pauses preloading.
This does not promise that every remote icon is cached before opening a replay.
Missing assets continue to use the existing cached/placeholder behavior.

## Evidence and limits

Automated fixtures cover history identity/outcomes and restart-safe installation
without overwriting EOG, history precedence/fallback, background preparation
without a window, cached overlay equivalence, cache reset/restart, corrupt-source
isolation, and removed-bundle pruning. The existing recorder, backend, frontend
and shared-data suites exercise the surrounding contracts.

Actual League history availability, payload completeness, and in-game resource
impact still need observation in real games. No live-game performance or instant
availability claim follows from these fixture checks. Cold/new libraries can
still need initial preparation; UI reads do not cause that preparation.

Verification after the targeted review on 2026-11-03: all 573 frontend tests
passed; the app backend suite passed (172 tests, 10 explicitly ignored), as did
the recorder all-target/all-feature suite (270 library tests plus 13 binary/example
tests, 12 explicitly ignored) and 33 shared League-data tests. Fixtures include
native sidecar notifications, unchanged/unpublished idle behavior, targeted
preparation, writer contention/backoff and bounded history overflow after restart.
Type checking, frontend production build, Rust formatting and Clippy with warnings
denied passed. Hardware/packaged-runtime checks marked ignored were not run.
