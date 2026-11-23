# QB-DATA-007 execution checkpoint

Feature: `QB-DATA-007`
ExecPlan: `docs/exec-plans/qb-data-007-ranked-lp-v2.md`
Updated: 2026-11-23

## Current milestone

Live League notification verification.

## Active unit

Observe when the accepted notification becomes available during an ordinary recorded ranked match.

## Completed

Audited LCU capture, exact recording identity, post-game enrichment, background preparation and History rendering. Superseded the initial plan after comparing transport cost and reliability, reviewing notification reasons and tracing grouped History records. Implemented bounded acquisition, durable immutable receipts, group projection, targeted preparation and stable History rendering.

Corrected notification validation against two captured LCU responses and verified that repeated LP file hints no longer request another History refresh.

## In flight

Automated verification has passed. Live verification is pending.

## Remaining

Measure notification availability within the existing four-attempt window during a live ranked session. Verify that it produces the receipt and History row.

## Verification

The full recorder suite passed 305 tests with 18 opt-in tests ignored across all targets and features. It covers both captured notifications, mismatched or incomplete responses, exceptional signs and zero, durable shared scheduling, late publication and restart, and immutable conflicting receipts. The Windows fixture tests ran with normal directory-handle access.

The desktop Rust suite passed 231 tests with 13 opt-in tests ignored. Focused checks prove that a receipt on a nonrepresentative part is visible, changing representatives preserves the delta, conflicting parts suppress it, and LP hints leave already prepared replay data ready. The shared-writer regression checks an LP gain, a confirmed zero and removal, with one refresh signal for each change and zero signals for two repeated hints per value. The production SQL plan searches the partial LP index by match group. The legacy snapshot continues to query while the additive migration runs.

The frontend suite passed all 643 tests, including a mounted row with unchanged focus and image nodes and no image source mutations when LP arrives. Type checking and the production build passed. Recorder check, strict Clippy for both Rust applications, formatting and canonical validation passed.

The checked-in notification bodies match the original LeagueStats blobs byte for byte. Both use `GAME_END` with `LEAGUE_POINTS_UPDATE`, one with -20 LP and the other with +19 LP. [Captured loss](https://github.com/wApMorty/LeagueStats/blob/080d02452c38a29b725dcc677fa8c560c143cc96/tests/fixtures/lcu_endgame/lp_change_notification.json) [Captured win](https://github.com/wApMorty/LeagueStats/blob/080d02452c38a29b725dcc677fa8c560c143cc96/tests/fixtures/spike_gameplay/current-lp-change-notification.json)

## Deviations

The account check shares the reserved notification slot. If it fails or is interrupted, a later slot can try again. The normal successful case still makes one identity request, and the failure case makes at most one per slot. No separate account reservation write is needed.

The captures establish `GAME_END` as the explicit match cause. The validator no longer accepts guessed cause or ordinary notification strings or falls back to a completed match result for a missing cause. Riot's shipped post-game and leagues plugins recognize the retained promotion, demotion, placement and series notices. Each still requires `GAME_END`, the exact game and queue, and the current account check. This also excludes inactivity demotions, even when the demotion notice itself is recognized. [Riot post-game plugin](https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-postgame/global/default/rcp-fe-lol-postgame.js) [Riot leagues plugin](https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-leagues/global/default/rcp-fe-lol-leagues.js)

## Decisions

Reuse bounded post-game HTTPS checks and the existing history ledger. Require an exact match and genuine notification cause. Keep immutable part receipts and project their confirmed value by match group.

Deduplicate unchanged LP notifications in the existing background preparation state. Desktop can still announce a new value after the headless worker writes the shared index first. LP hints retain the existing targeted projection and require no History query or replay preparation when the observation repeats.

## Blockers

No League client process is currently available for live observation.

## Next action

Use a fresh ranked recording to measure notification timing and verify the receipt and History row.
