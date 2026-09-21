# User-assisted M4 acceptance procedure and result

Native Windows controls are disabled in the current agent session. The available
CUA inventory returned no apps/browsers; the older screenshot/input failures
remain retained separately. User-assisted interaction has since begun; see the
current results in `docs/execution/qb-replay-010.md` and `manual-status.json`.

Benchmark collection is complete. The user completed this procedure on 2026-09-21:
Games/local optional failure and clip playback, save/delete during optional work,
A/B/A switching, viewer/export/return, export submission while HOLDING, cleanup,
and final playback all passed. `hold-export2.json` and
`audit-export-overlap-verified.json` retain the final overlap/media evidence;
`audit-final-verified.json` retains the final fixture state. This procedure remains
as reproducible history; do not recreate its fixtures or repeat the campaign.
The helper never clicks or types. It creates fresh disposable copies, records
process/fixture evidence and can delay one real optional probe. Original benchmark
and earlier manual roots remain intact. Do not choose a real media folder.

From a PowerShell window in the repository:

```powershell
python tools/replay_benchmark/qb010_manual.py launch
```

Preparation (`python tools/replay_benchmark/qb010_manual.py prepare`) has already
run and refuses an existing destination. Run only `launch`; if already launched,
use that app instead of starting another copy.
The launcher prints the dedicated PID and A/B paths:

```text
<repo>\build\perf\qb-replay-010-m4\.chronobreak-replay-benchmark\manual-assisted-20260917\root-a
<repo>\build\perf\qb-replay-010-m4\.chronobreak-replay-benchmark\manual-assisted-20260917\root-b
```

A starts with three Ahri recordings and ten clips, including one deliberately
invalid optional-duration file. B has one Lux recording/one clip with IDs also
present in A. Initial retention is Never. All recordings are dated August 2026;
seven-day retention is intentionally destructive to unsaved disposable games.

## How to establish a real owned optional batch

Use a second PowerShell window for each labelled hold, for example:

```powershell
python tools/replay_benchmark/qb010_manual.py hold --label games --seconds 8
```

To trigger work explicitly, open **Clips**, wait for **Retry duration** on the
unavailable clip, arm the command, then click **Retry duration**. Simply reopening
Clips did not catch a child in the retained `delete` attempt; `delete2` succeeded
with explicit retry. The helper waits up to 60 seconds for a
direct ffprobe child of the recorded normal app, verifies the parent path/creation
identity and child executable path, records the child creation identity, and
suspends only that child. On **HOLDING**, do the stated UI
action immediately. It resumes within the selected bounded window (up to 30
seconds), or records that invalidation killed the child. The app itself
is never suspended. `finally` resumes the child if the helper exits normally or
is interrupted. The app's own deadline/cancellation still kills and reaps it.

A dash alone is not proof of an owned request. Require the HOLDING receipt and
your observation that the action occurred before the hold ended. If no child was
caught, refresh and retry using a new label, such as `games2`; keep the failed
receipt. If the action is too slow, record **overlap not demonstrated** rather
than claiming the race passed. Do not arm while an export is already running;
that could catch export validation rather than an optional duration probe.

## Minimal ordered sequence

1. **Useful Games and optional failure.** Observe initial three Games cards and
   correct storage. Arm hold `games`, click Clips to start optional work, then
   click Games while HOLDING and confirm cards
   are usable despite pending clip details. Return to Clips after release; play a
   valid clip, close it with Escape, switch Games → Clips. The deliberately invalid
   newest clip must show **Unavailable** / **Retry duration** without blocking
   Games or valid clip playback. Retry it once; failure stays local. If Data Dragon
   is offline, placeholders must leave those actions usable. Record what occurred.

2. **Save during optional work.** Refresh, arm `hold --label save`, enter Clips;
   while HOLDING click Games and **SAVE** on the newest recording. Expect **SAVED**,
   one saved recording, three game cards, unchanged clip count. Old duration
   completion must not restore obsolete state. Capture an audit:

   ```powershell
   python tools/replay_benchmark/qb010_manual.py audit --label save --note "Describe Games, failure/retry, clip playback, and whether SAVE happened while HOLDING."
   ```

3. **Delete during optional work.** Refresh and arm `hold --label delete`; in Clips
   while HOLDING delete one valid source-less clip, confirm **Delete clip**. Expect
   nine clips. Delete one **unsaved** recording separately (or use a second hold
   and Games → DELETE → Delete recording). Expect two games, one saved. Keep the
   saved recording for export/cleanup. The audit identifies exactly which IDs were
   removed; every retained media/log file must be unchanged.

4. **A/B/A ordering.** Refresh A; arm `hold --label roots`, enter Clips, then while
   HOLDING open Settings → **Choose folder** and select the exact B path above.
   Old selection/cards must disappear; final B shows only one Lux game/one clip,
   even after the old request settles. Return to A through Choose folder; A's
   saved/deleted state must be preserved. B's files must remain unchanged. Repeat
   A/B/A rapidly if the first switch completed after the hold. Record overlap
   accurately; folder-selection speed is not assumed.

5. **Viewer/export/return.** In A, open the saved recording and check playback.
   Click **CLIP**, **EXPORT CLIP**, then **EXPORT 1 MP4** in the Clip Workbench.
   Keep defaults Horizontal / No music / game audio 100%. Require **EXPORTED IN**
   and one output, then **VIEW CLIPS**; expect ten clips including the new playable
   export. Return to Games and reopen the source; it must still play. For the
   requested export-during-optional-work check, first rehearse this click path,
   then refresh and use `hold --label export`: Clips → HOLDING → Games → saved
   recording → CLIP → EXPORT CLIP → EXPORT 1 MP4. Only claim overlap if submission
   occurred within the recorded hold. The earlier eight-second path may be too short;
   if so retain **export overlap not demonstrated** as a remaining manual gate.
   A successful separate export does not substitute for that gate. Each extra
   successful export adds one clip; adjust the expected count accordingly.

6. **Cleanup during optional work.** In A Settings choose **7 days** first and
   wait for settings to save. Return to Games and refresh. Arm `hold --label cleanup`,
   enter Clips, then while HOLDING Settings → **Clean up now**. Expect only the
   remaining unsaved old game to be removed: one saved game remains; clips and B
   remain untouched. Restore **Never** before closing the app. Verify the surviving
   saved recording and exported clip still play. Use Back to verify return
   navigation and fresh counts. Record the final audit:

   ```powershell
   python tools/replay_benchmark/qb010_manual.py audit --label final --note "Record actual final counts, A/B/A results, playback/export results, and each overlap passed/failed/not demonstrated."
   ```

The audit lists additions/deletions/changed hashes and saved metadata. It runs
packaged ffprobe plus full single-thread decode on new MP4 exports and retains
exit codes, stream codecs and duration. A pass requires H.264/AAC, expected
selected duration, successful decode and user-observed playback. Source deletion
is expected only for the explicitly selected disposable items; retained media/log
hashes and all B hashes must match. Save may change only the selected metadata's
saved state. No script result alone establishes UI correctness or A/V perception.

Retain screenshots if available, plus `hold-*.json`, `audit-*.json`, app logs and
launch/preparation receipts under the printed manual-assisted root. Send the
observed outcomes or point the agent to those files for canonical disposition.
The helper's syntax/identity plumbing can be checked without launching the app;
actual holding, UI actions and export media validation require this procedure.
