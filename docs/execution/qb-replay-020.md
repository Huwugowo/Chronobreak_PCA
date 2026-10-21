# QB-REPLAY-020 execution checkpoint

Feature: `QB-REPLAY-020`
ExecPlan: `docs/exec-plans/qb-replay-020-recorded-overlay-timeline-v2.md`
Updated: 2026-10-21

## Current milestone

M3, opening-loadout and presentation follow-ups complete.

## Active unit

None.

## Completed

Compact production SQLite page/IPC/strict decoder, committed revision guard, independent source timeline and binary lookup. Event queries now depend on viewport/source, not playhead. Static identities belong to overlay source, and recorded spell variants mount/decode once; seeks only change visibility. Removed superseded single-roster projection helpers.

Initial details readiness includes complete overlay loading; optional-data failure keeps video usable. Existing development preview now adapts its fixed observations to the new page contract. Shared read scheduling admits independent details/overlay lanes with one read active overall. Durable architecture and canonical evidence are updated. Current production frontend and Tauri release built and exercised; normal isolated release launcher is running.

Name-label follow-up: ChampionFilter renders the existing source identity's name before the first # above each overlay portrait. Full Riot IDs remain unchanged for identity/filtering. The stylesheet reserves vertical label space, aligns names inward from the video edges and ellipsizes unusually long names within 144 pixels. No read model, timing, image lifetime or recorder changes.

Opening-loadout follow-up: complete source loading derives a distinct spell-only context from the first mapped sample. Before it (including before the first video observation), the rails show those recorded slots with first-recorded tooltips. Normal timed slots replace them including nulls. No level/death/timer/score fallback; no later missing-sample fallback. Context clears on source change/error/dispose. Fullscreen team headings are omitted, retaining accessible grouping and panel/rail headings. Backend, data format and retained image lifetime are unchanged.

Flash-marker follow-up: suppress `.local::after` only in fullscreen champion rails. The legacy four-pixel white local-player indicator was positioned against the expanded button and overlapped Flash. Other filter layouts retain their existing presentation; no spell asset or state logic changed.

## In flight

None. Existing paused-seek fixes and unrelated dirty-tree work are preserved.

## Remaining

None for this feature. Evolved Smite asset coverage remains QB-LIB-013.

## Verification

Flash-marker follow-up (2026-10-21): all 262 frontend tests / 30 files pass; TypeScript, Vite and fresh production Tauri build pass. Actual isolated release `node build/overlay-flash-marker-check.cjs` passes at 1440x900 and 1000x650: the local button's pseudo-element has `content: none`, both spells are loaded/visible, all twenty normal spell icons are visible and participant selection toggles/restores. Both cropped screenshots were visually inspected; ignored evidence is `build/overlay-flash-no-marker.png`, `build/overlay-flash-no-marker-compact.png` and `build/overlay-flash-marker-result.json`. The initial verification helper expected the library, but the app restored the replay; the helper was corrected to accept either existing route. The sandbox could not terminate the earlier test app and the launcher correctly refused its held session lock; restarting that exact preprod executable outside the sandbox allowed the fresh build/run. No recording data was touched. Earlier unchanged timeline/backend evidence remains applicable. Updated isolated release is open.

Opening follow-up focused controller/projection + viewer checks: 21 tests pass. They cover complete-load-only context, source/error/disposal clearing, before-first spell-only display, normal evolved/null temporal replacement, absent fullscreen headings, and initial visible icons without video/timeline input. TypeScript passes. Initial test-harness assumptions that the component was already mounted and that the unknown HUD had no container were corrected to await mounted rails and assert the existing four unknown placeholders; no product behavior was changed for those assumptions.

Opening follow-up final checks (2026-10-21): `node node_modules/vitest/vitest.mjs run --environment node` from app passes all 262 frontend tests / 30 files. Running this suite initially from the repository root made one existing relative preview-asset read fail; running from its documented app working directory passes, with no asset/product change. The shell's npm-test logging hook could not write its protected log directory, so the same installed Vitest CLI was invoked directly. TypeScript, Vite and fresh Tauri production release builds pass through the normal isolated launcher. The sandbox-started native window did not expose a working WebView debugging endpoint; reopening the same isolated app outside the sandbox provided the normal WebView session. Intentional process termination to replace the executable is not a build failure. No recorder/backend source or stored-format change in this follow-up; prior backend checks remain applicable.

Actual release `node build/overlay-opening-check.cjs` passes: normal library → Swiftplay open, paused time 0, zero timeline inputs, all twenty selected spell icons loaded and visible, ten names above portraits, zero headings, levels or death timers. Each selected slot exactly matches the first mapped sample at 24.573533 seconds and has a first-recorded tooltip. At 1440x900 and 1000x650, names stay inside the video and both spell slots remain visible. End/Home restores opening spells and hides future temporal state with all 24 recorded-variant image nodes retained. Screenshots build/overlay-opening.png and build/overlay-opening-compact.png visually inspected; measurements build/overlay-opening-result.json.

Repeated release `node build/large-seek-overlay-check.cjs --assert` passes ten distant paused jumps: 255 accepted-target frame samples, zero missing/stale levels or Yi spells, zero image mutations/loads, original nodes retained and zero seek-triggered reads. Initial four pages / 537,670 bytes completed 327 ms after reload/opening on the prepared cache (not a cold-install budget). The existing status/control check passes dead level 3 / timer 10, playback 7, backward 10, 2x 6, stable pause, alive color, fixed order, filtering and fullscreen. Sanitized results build/overlay-status-opening-result.json. Practice source switch passes with two opening spell icons, no before-first temporal status, 32 mapped samples, original last clock 1:28 / level 6 and unchanged zoom behavior; later out-of-media facts remain excluded. Final canonical and diff checks pass. The updated isolated production release remains open.

Baseline `build/large-seek-overlay-check.cjs` on actual isolated Swiftplay: distant paused backward seeks have 13–15 blank frames, about 216–284 ms. Forward seeks begin with level 3 before correcting to 11–17. Earlier short-seek evidence did not establish large-seek correctness.

Historical earlier repair (2026-10-20): fixed rails joined recorded spells/level/death by participant identity; retained accepted RVFC state through ordinary seek queues, shared duplicate in-flight seek ownership/results, and required actual presentation for idle deduplication. The 256-test frontend suite and short paused pointer/keyboard/drag run passed, including timer values 10 / 5 / 10 and backward zero. Artifacts build/paused-overlay-check.cjs and build/paused-overlay-fixed.png retain that narrower evidence. The subsequent large-seek reproduction invalidated its completion claim and motivated this plan.

M1 real filesystem fixture import/query/rebuild test passed with 488 Swiftplay samples in four pages and 32 mapped Practice samples in one page, each below 2 MiB and each source below 1 MiB. Continuation revision mismatch and oversized limits reject.

2026-10-21 automated checks against integrated changes: frontend suite 260 tests / 30 files passed; TypeScript passed. App Rust suite 136 passed / 3 existing ignored, including the real fixture import/query/rebuild; Clippy all-targets with warnings denied passed. The final SQLite projection extracts only compact fields, avoiding parsing raw descriptions/items. Its first compile caught unsupported direct u64 SQLite conversions; using the existing checked optional_unsigned helper fixed these, and the complete Rust suite passed afterward.

Actual dev desktop `node build/large-seek-overlay-check.cjs --assert`: ten paused jumps 940, 52, 554, 105, 800, 70, 600, 110, 950, 60 seconds. Every presented target frame had the correct level and two loaded Yi spells: zero missing/stale frames; zero image additions/removals or loads; original nodes retained; zero overlay/event requests during these seeks. Initial source loaded four pages / 537,670 JSON bytes, 488 samples; last page arrived 1,319 ms after page reload/library opening. This is fixture/environment characterization, not a universal startup budget. Evidence: ignored build/large-seek-overlay-check.cjs and build/large-seek-overlay-result.json.

Actual dev desktop `node build/overlay-status-check.cjs`: dead level 3 / timer 10 at 105 seconds, grayscale portrait; paused timer stable; normal playback timer 7, backward restore 10, 2x playback timer 6; alive sample removes timer/grayscale; backward zero hides future state. Rail order, original Yi node, participant filtering and entering/exiting fullscreen remain correct. Hidden recorded spell variants are preloaded; unsupported evolved Smite retains its existing placeholder (QB-LIB-013).

Fresh production Tauri build through `powershell.exe -NoProfile -ExecutionPolicy Bypass -File build/preprod-verify-launch.ps1` passed (TypeScript, Vite and release executable). Repeating the enhanced distant-seek check against that actual release again passed: 293 target-frame samples, zero blank/stale frames, zero image mutations/loads, original nodes retained, zero seek reads; four initial pages / 537,670 bytes completed 438 ms after page reload/opening. Data/cache already prepared; this is not a cold-install or general latency guarantee. The release status/control procedure passed with the same 10 → 7 / backward 10 / 2x 6 timers, stable pause, before-first unavailable, dead/alive, filter and fullscreen behavior. Sanitized summary: build/overlay-status-release-result.json (raw local asset URLs remain only in ignored evidence).

`node build/practice-overlay-check.cjs` against release passed: one identity, 32 mapped samples, before-first state unavailable; at the video end (131.916667 seconds), level 6 and original last valid mapped clock 1:28. Later accelerated observations remain outside media coverage; no fitted timing or future state is fabricated. Zoom/full-match restore retains the same champion node and requests no new overlay page. An initial test assumption that the final mapped clock must be accelerated was false; the test was corrected to assert the actual original mapped 1:28 observation rather than changing product timing. Evidence: build/practice-overlay-result.json.

Final app fmt check, canonical validator (71 items / 13 plan-checkpoint pairs) and git diff check pass. The instrumented verification app was stopped after the procedure; its launcher reports exit -1 from intentional process termination, not a build/runtime check failure. The normal `tools/preprod_launch.ps1` then rebuilt the same production frontend/release and started the isolated app without the debugging override. No personal recording was modified. Export encoding/A/V and acquisition were not rerun: their implementation is outside this repair; existing clip/viewer regression tests pass.

Name-label follow-up verification (2026-10-21): TypeScript and 15 existing ViewerScreen tests pass. Fresh production frontend and Tauri release build pass through the isolated launcher. Actual release `node build/overlay-player-names-check.cjs` passes: all ten real Swiftplay identity names equal the part before #, sit above their own portrait and remain within the video at 1440x900 and 1000x650. End/Home seeks preserve the same displayed names. Screenshots visually inspected for spacing and legibility: build/overlay-player-names.png and build/overlay-player-names-compact.png; measurements in build/overlay-player-names-result.json. No new permanent tests or framework. Backend/timeline checks above remain valid; this follow-up changes only presentation. Canonical/diff validation repeated. The updated isolated production app is open with the verification browser connection disconnected.

## Deviations

The latest user request requires spell icons before the first mapped roster. Actual fresh replay open at paused zero shows ten portraits, twenty loaded but hidden spell images and no levels. The prior design intentionally made all before-first state unavailable; v2 preserves it as history and makes the narrowly typed first-recorded spell context explicit. No image loading race was reproduced. This user-authorized implementation follow-up supersedes the workflow's planning-only stop. The bounded design was challenged against evolving spells, missing slots, source changes, errors and temporal frontier semantics before linking.

Workflow planning-stop instruction is superseded by the user's explicit request to implement this repair in this turn. Runtime exposed the existing library read scheduler's single latest-purpose generation cancelling independent event/overlay reads. The existing scheduler now has two explicit lanes, one active read overall and one latest pending per lane; invalidation still clears both. This is required integration of the chosen design, not a new service. Dedicated regression added.

Cold-open measurement exposed an incomplete timeline being reported ready; readiness now includes the selected overlay's completion, while video remains usable and unproven positions remain unavailable. The existing browser preview adapter now supplies the same fixed mock observations through the new read contract, preserving preview compatibility without changing production/preprod consumers. Focused integration review found that preview omission and no other production-path defect.

After completion, the user explicitly added player names above portraits without Riot tags. This small presentation addition is recorded here and in the feature criteria; it does not invalidate the immutable recorded-timeline design.

## Decisions

Compact complete snapshots with 128-sample/2 MiB pages; one source and one request at a time. Strict partial frontier and `(tick, observation)` cursor; same committed revision across pages. Keep existing explicit approximate clock only where RVFC is unavailable. No recorder or persisted schema changes. Opening spell context comes only from the first complete mapped sample after source loading succeeds; it supplies no temporal status, and timed null/evolved slots fully replace it.

## Blockers

None.

## Next action

None; resume only for new user work or contrary evidence.
