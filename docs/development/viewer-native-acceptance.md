# Ordinary Viewer acceptance

## Reported normal user result

On 2026-10-28 the user reported: "I ran all normal user tests, all good."
Record ordinary interaction acceptance as passed by operator report. The categories
below describe those normal checks; they are not a request to repeat them.
No builds, environment configuration, copied-file manipulation, console probing or
fault injection are requested from the user.

## Current library and corrected setup

Use the ordinary app's current preprod library, `build/preprod/library`. The normal
local desktop config, release launcher and dev launcher now select this same
library. It contains the restored original Master Yi video, seven newer retrieved
recording bundles (including two partial captures), the Akali Practice fixture and
existing clips. Akali remains synthetic; the recovered complete recordings contain
actual captured media.

The earlier manual command incorrectly selected `build/perf/qbd7-ui-retry`, an
old two-game synthetic control copy. It must not be used for ordinary current
preprod acceptance. The subsequent claim that the normal preprod still used only
those placeholders was wrong: the restored recordings had been installed under
`C:/Users/Hugo/LeagueReplays`, separate from the old preprod working copy. That split
has now been consolidated without deleting the recovered source files.

No setup commands or repetition of the user's passing normal checks are requested.
[Current preprod workflow and historical seed provenance](../../fixtures/preprod-library/README.md).

## Normal interaction categories

1. Open a recording, wait for video and League details, use **Back to Match
   History**, then reopen it. Switch to another recording and back. Check
   that the selected recording, duration and details belong to that recording.
2. Click the video or Play/Pause, then use Space with focus outside an input.
   Confirm pause stops the replay clock and Play advances it. Select each **Replay
   speed** value (0.25x, 0.5x, 1x, 2x, 4x, 8x), allowing time to settle. Record
   any explicit unsupported/limited state. Restore 1x. Check mute and volume.
3. Select a timeline event named **Seek to … at m:ss**. A **Zoom into N events**
   button expands a cluster first. Confirm the event's position and
   recorded state follow the jump. On the focused **Replay position** slider,
   rapidly alternate Left/Right, Ctrl+Left/Right, Home/End and Page Up/Down; also
   drag the playhead rapidly in both directions. Release at a known position and
   check the final replay clock/state settles there without a later stale jump.
4. Start playback, choose **Open fullscreen replay**, then press Escape. Repeat
   while paused, at 2x, and after a seek. Confirm time/rate/paused state and the
   selected recording persist; playback must not restart at zero. Restore 1x.
5. Choose **Clip**. Drag **Clip starts at …** and **Clip ends at …** independently.
   Focus each handle and use Left/Right for frame changes; hold briefly to check
   accelerated edits. Confirm valid ordered endpoints and matching preview.
   Space previews the selection. Enter/exit fullscreen during an endpoint drag
   and check that releasing the pointer ends the gesture. Use **Cancel** (or
   Escape after leaving fullscreen) and confirm ordinary seeking works again.
6. From the Viewer, open **Settings**, then **Back**. Confirm the same replay is
   selected and the intended playback state returns. Record the observed time
   and whether this navigation remounts playback; do not infer DOM persistence
   across navigation that unmounts the Viewer. Repeat after a clip edit.

## Remaining agent-owned evidence

Native primary-element/load continuity and controlled unavailable-media recovery
have not been demonstrated in the current native session. Earlier native
capture/input attempts failed; those failures remain in the execution checkpoint.
Technical fault injection and internal inspection are agent responsibilities, not
normal user tests. The user declined those steps; they are not recorded as passed.

Read-only investigation confirmed that all 14 copied source files, including both
MP4s, still match the retained ownership receipt. That proves file preservation,
not a native error/retry or DOM-continuity check.

The existing "Retry recording" button is ordinary Viewer descriptor-error UI.
It retries opening the selected existing recording; it never starts capture.
It predates QB-DEV-007. Its wording/presentation was raised as a product UI concern;
no product UI change is made by this validation update.

On 2026-10-28 the user explicitly accepted QB-DEV-007 implementation completion
with these native limits and remaining performance uncertainty documented, and
stopped further benchmark campaigns as completion prerequisites. The CPU control
remains failed, affected-path statistics were not run, and the internal native
checks above remain unverified. No further operator steps are requested. Detailed
results and final disposition belong to
[the execution checkpoint](../execution/qb-dev-007.md).
