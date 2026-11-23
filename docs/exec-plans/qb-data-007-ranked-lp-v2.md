# QB-DATA-007 ranked LP design, revision 2

## Purpose and scope

Show the League client's confirmed signed LP adjustment alongside Victory or Defeat for recorded solo/duo and flex matches. Preserve zero and exceptional signs. Existing recordings receive no historical LCU requests. The preceding design is retained in qb-data-007-ranked-lp.md.

## Architecture decision

The recorder already has an authenticated, bounded loopback HTTPS client, one shared request permit and a bounded, atomically replaced history-job ledger. A WebSocket would add a transport dependency and authentication, frame bounds, reconnect and cancellation logic. It would keep a connection active for unresolved matches and still need a snapshot to cover events before subscription and during restarts. Its advantage is catching short-lived notifications between samples. Reuse HTTPS and the existing history coordinator. The incremental work is at most four small notification GETs and one account check after a valid candidate per match. There is no new connection lifecycle, background worker or idle request stream. A notification that disappears between checks can remain unknown.

## Recorder

Enroll ranked bindings at recording close through the existing post-game admission path. Budget notification checks at approximately 0, 5, 20 and 60 seconds, expiring at 90 seconds. Share the slot reservations and first accepted result across recording parts for the same account, game and queue, including after a restart. Persist each reservation before the request and skip elapsed slots to avoid catch-up bursts. Reuse the existing process discovery and short request timeouts. Other queues and legacy ledger rows do not enroll.

Decode only a numeric game ID, supported queue string, signed integer delta and bounded reason strings. Require exact game ID, queue and current summoner PUUID, with unchanged LCU session identity. Accept explicit in-game reasons and the client's postgame promotion, demotion, placement and series notices. Reject decay, inactivity, administrative and unknown explicit causes. A neutral cause with an ordinary notification needs an exact completed ranked result from the existing result sources. The client-supplied delta is authoritative even when the displayed rank reset or the sign differs from the outcome. Accept once, stop acquisition and never replace the receipt from a later observation. Ignore stale and incomplete responses. If persisted recording parts disagree, suppress the displayed delta for that match.

Persist the accepted scalar in the current ledger before installing an immutable per-recording receipt. Resolve the allocation through its existing directory-generation pin and validate the published metadata, journal manifest and provisional match identity. Preserve any existing file and treat an equal receipt as installed. Receipt arrival and publication can occur in either order. Installation retries are bounded and use the same worker. Recording finalization and existing history/result acquisition do not wait for LP.

## App and UI

Use an additive nullable index column with a partial index on match group and LP. Validate the receipt against source identities in the existing preparer. Handle LP file notifications as targeted updates to the named recording with the existing writer and prepared notification. Reuse the existing background recovery pass for an empty cache or missed notifications. Exclude LP from replay fingerprints. A group-level lookup within the existing history SELECT returns the common confirmed delta from any indexed part, independently of the card's representative, and returns unknown on disagreement. Use a legacy projection before the writer migrates a cache. No History request reads a receipt or adds a separate database query.

Present a compact signed LP label adjacent to the outcome. Blue represents a gain, red a loss, and neutral zero is visible. Preserve the row, participants, artwork and focus on late refresh through stable row keys and reactive values.

## Milestones and verification

1. Add shared receipt validation and the bounded recorder acquisition/install path. Exercise stale payloads, exact account and queue, ordinary and exceptional adjustments, absence, duplicate parts, request budget, early and late publication and restart recovery.
2. Add index migration, source and targeted projection, group lookup and row presentation. Exercise representative changes, conflicting/removed sources, cache compatibility, no replay work and stable UI nodes.
3. Run the applicable verification commands, validate lifecycle artifacts and record automated and any live evidence. Use dedicated fixtures and short representative scenarios. Inspect the query plan and the bounded request/write paths.
