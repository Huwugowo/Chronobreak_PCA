# Grouped Match History

## Purpose

Show one League match with individually playable recordings after reconnects. Keep usable footage without a match identity available on disk.

## Relevant architecture and findings

`league_match::read` validates a recording-bound candidate before result facts are accepted. The candidate already includes the exact decimal game ID and local PUUID. `library` snapshots retain per-recording replay authority, and League preparation populates a disposable SQLite cache with separate card-fact and replay readiness. Exported clips are separate files. `LibraryMutation` serializes storage effects and pins approved directory generations.

## Design and scope

- Derive an opaque group key from the validated account and game ID. Carry it in the snapshot and index without exposing the account ID. Keep per-media catalog rows and identifiers for the replay viewer, export, Save and Delete. Restrict History to keyed playable recordings. Group before cursoring and counting. Use stable newest-part recency for ordering and the newest checked final-result part for card facts, with the newest checked recorded facts as fallback. Filter card facts on the representative, search events across member sources, and treat any saved part as a saved match.
- Open single recordings directly. Multipart cards show the part count and open a chooser with start time, duration, Play, Save and Delete for each part in chronological order. Do not merge media or timestamps. Existing clip files remain independently playable.
- During normal app library maintenance, move strictly published unmatched bundles as whole directories to the approved root's `unmatched-recordings` collection. Revalidate source generation and eligibility with the existing per-library mutation lease. Use a same-volume no-replace rename on the retained source handle relative to the retained destination. Keep the original on rejection and report failures in Settings. Include archived bundles in storage and existing age and saved-protection rules. Leave partials in recorder recovery and use the ordinary maintenance path for files encountered on disk. Settings opens the approved archive folder in Explorer.

## Decisions

Use existing match metadata and account scope without new endpoints. Recorder publication and loading-screen diagnosis are independent. The app's grouped index is derived and rebuildable. Duration and wall-clock proximity cannot establish identity.

## Milestones

1. Wire the validated grouping key through snapshots and the index. Implement grouped History, counts and cursoring.
2. Wire the chooser and Settings UI. Integrate guarded archive maintenance and retention.
3. Verify group semantics, independent replay and clip access, storage safety and project checks. Update durable architecture and completion evidence.

## Verification and reliability

Use dedicated fixtures covering three parts, one part, identical numeric IDs on different accounts, absent or invalid context, older event hits, saved parts, preferred final facts and stable pagination. Use temporary libraries for rename success, collision, guard rejection, exact byte preservation and age/saved/incomplete retention. Check per-part playback descriptors and existing clip access. Run the app Rust tests, fmt, strict Clippy, frontend tests, typecheck, production build and canonical validation in `docs/development/VERIFICATION.md`. Keep grouped pages bounded, preserve the fast card-facts pass and avoid additional capture work or copying large recordings.
