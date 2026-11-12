# Match History and clip replay navigation

## Purpose

Players can find exported moments from a Match History card and jump back to the exact spot in the source replay while it remains available.

## Relevant planning-time architecture

QB-LIB-030 groups History cards by validated game and local account. Each recording part has its own media ID and clock. QB-LIB-002 supplies the durable clip identity, game group and source frame interval plus the paged clip projection. The app owns playback navigation and the existing replay controller owns seeking.

## Scope and non-goals

Add the Match History clip count and inline strip, replay navigation from the clip player, and return navigation to the originating view.

## Exploration findings

The Match History card already has a separate action rail and a multipart recording chooser. Clip actions belong outside the main replay button. A clip draft enters editing, so the viewer needs a distinct initial seek intent. The clip can outlive its exact source even while another part of the game remains.

## Chosen design and rationale

- Join clip counts by validated match group onto the existing History page after page selection. Distinguish an unresolved clip projection from a confirmed zero. Display an accessible `Clips · N` control beside the card actions for a positive count. Expand a bounded inline thumbnail strip with explicit continuation and reuse the existing clip player. Count clips from all recording parts and preserve the replay and multipart actions.
- Resolve the selected clip's stored media ID against the accepted source inventory and revalidate its descriptor and frame interval. Dispatch a distinct initial frame seek through the existing viewer authority. Return to the originating Clips group or expanded History card, with focus restored. If the exact part is missing, keep the clip playable and show that its source replay is unavailable. Clarify in the recording deletion prompt that exported clips stay.
- Keep root, revision, snapshot and request ownership in the existing coordinator and controller lanes. Cancel or reject superseded strip and navigation results.

## Rejected alternatives and planning decisions

An inline strip preserves the user's place in History. A different part cannot supply the original frame coordinate. The clip player uses the clip's first included frame rather than an event position for its replay action.

## Milestones

1. Add bounded counts and group-scoped clip access to History and connect the inline strip.
2. Add exact-source navigation, unavailable state and Back restoration.
3. Validate the complete navigation flow and History performance.

## Verification design

Use generated multipart games and two accounts to verify count, scope and ordering. Verify History-to-clip playback, clip-to-exact-part/frame seek, unavailable-source playback and Back restoration. Exercise root switches, refresh, deletion and focus/keyboard behavior. Run applicable app, frontend and production checks from VERIFICATION.md and record evidence in the execution checkpoint.

## Performance and reliability gates

Use the prepared startup and large History fixtures. Preserve page timing within repeat-run variation. Counts use one existing page query without per-card filesystem or IPC requests. Clip lists load only when expanded. Replay opening still uses the approved media authority, and failure or deletion cannot redirect playback to a different part.
