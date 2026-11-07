# QB-REPLAY-030 execution checkpoint

Feature: `QB-REPLAY-030`
ExecPlan: `docs/exec-plans/qb-replay-030-front-fragment-index.md`
Updated: 2026-11-07

## Current milestone

Production virtual index implemented and measured in the Windows release.

## Active unit

Focused release verification for the viewer, protected ranges, and canonical media.

## Completed

- Reviewed fragment relocation, AAC timing and edit handling, bounded range streaming, source replacement, descriptor publication, and existing clip and benchmark consumers.
- Proved the virtual layout on all five original gameplay sources with the standalone Edge control. The probe reads fragment metadata and serves original initialization and sample data around the added indexes. It keeps the source file open read-only and does not remux.
- Built and retained the pre-index desktop release. Recorded a fresh complete-opening baseline on the same five prepared recordings.
- Implemented bounded native MP4 eligibility, SIDX serialization and delivery on the existing protected game route. Preparation is opt-in for the replay viewer's opening. The shared library descriptor and export paths use the canonical recording.
- Cache four verified indexes by file facts and exact timeline, including reuse while leased. Active viewer URLs pin their layouts independently of cache eviction, with eight bounded leases and raw fallback for new opens at capacity. Indexed reads validate the source generation on the streaming handle.

## In flight

No implementation work is in flight.

## Remaining

The video-only and finite-AAC layouts have parser-level coverage. The full generated-media playback comparison and complete project-wide backend suite remain open.

## Verification

- `build/perf/complete-open-20261106/index-only-probe.json` records one request through frame zero for both indexed originals, versus 351–427 requests. First presentation was 95–135 ms indexed versus 2.22–2.77 s original. Browser duration was identical on each pair.
- `build/perf/complete-open-20261106/index-seek-probe.json` covers all five sources. Frame-zero presentation was 73–124 ms indexed versus 2.69–3.67 s original. Duration and all 15 matched early/middle/late RVFC timestamps agree. Total indexed requests were six or seven including the three seeks. The first middle and late seeks commonly took longer indexed, approximately 195–333 ms for those slow cases, so keep seek timing separate from opening. This is standalone video-element evidence.
- `npm run desktop:build --prefix app` passed. The overlay-optimized pre-index executable is `build/perf/complete-open-20261106/pre-index.exe`, SHA-256 `1D42288738C8D2CA3B3FCDD9F15330236E499D5C9DB4E0493B6AD79AF0D1F56B`. The frontend import warning and linker-output warning were present.
- Fresh production baseline in `build/perf/complete-open-20261106/pre-index-2/summary.json`. Graves (1792962427) reached complete opening in 5,131 ms, first frame in 5,068 ms, metadata in 4,801 ms and TAB in 507 ms. It issued 363 requests before frame zero. The other four recordings reached details and TAB in 688–835 ms, cycled `emptied`/`loadstart` and timed out without metadata or presented video within 12 s. They issued 832–1,084 requests through timeout across load attempts. Keep each failed recording and phase visible. The previous trusted report had two completed pairs and three recoveries.
- The first contained Windows launch failed during app setup with `local data directory unavailable` and could not resolve the packaged runtime from the copied executable location. The successful baseline used `measure-indexed-normal.mjs`, the locked runtime via `QUEUEBACK_MEDIA_RUNTIME_DIR`, Windows launch outside that restriction, the existing disposable config/app-data and a new isolated profile with normal WebView sandboxing. The script is a copy of `measure-local.mjs` with the game identity extracted in memory instead of retaining the capability URL, and the normal WebView flags restored. Run it from the repo root with the Playwright module path, executable, new result directory and new profile, and use matching settings for the indexed release. Raw command output is `pre-index-probe-2.log` under the same artifact root.
- Recorded SHA-256 fingerprints of the five canonical videos and their metadata in `build/perf/complete-open-20261106/pre-index-source-hashes.json` for the candidate source-preservation comparison.
- Production candidate `sidx-1/summary.json` in the same artifact root completed all five cold opens with one protected 206 each through frame zero. Selection through full paint, including index preparation, took 902, 697, 774, 384 and 529 ms respectively. Graves is the matched successful pair at 5,131 → 384 ms (13.4×) and 363 → 1 pre-frame requests. Reopening Graves took 308 ms. Its descriptor IPC was 6.2 ms cold and 2.1 ms on reopening.
- The final release `sidx-final/summary.json` repeated the five openings in 825, 720, 761, 319 and 613 ms, with one to three pre-frame requests. Graves reopened in 320 ms. Its cold open was 16.1× faster than baseline. The release executable SHA-256 is `DFD336E5280026CC754D1A321B8D9D32876A55EA0D9E1183B917517678DBF4F8`.
- `sidx-media-check-2/summary.json` matched Graves' original browser duration and exact early, middle and late presented frames. Locked ffprobe matched 107 packets per window across both tracks, including timing, flags and SHA-256 payloads. Play/pause passed. The first distant seek took 534 ms in this run. All five video and five metadata SHA-256 fingerprints were unchanged.
- The packaged export fixture passed with silent and audible canonical sources across all three presets and music choices, including cancellation and source preservation. The protected-route regression passed with range crossing, ranged HEAD, active cache eviction, release, wrong-game and duplicate tokens, changed file facts and raw compatibility.
- All 35 focused backend playback tests passed with the replay-benchmark feature enabled, including decoder binding and protected wire delivery. All 585 frontend tests and the canonical roadmap validator passed. The unrestricted backend test selection hit a clip-duration cancellation failure and four stalled process tests. The full run was stopped.
- App Clippy passed with warnings denied and the replay-benchmark feature enabled. Formatting passed for the changed Rust files. The crate-wide formatting check reported existing edits in `league_index/import.rs`.
- The focused follow-up passed four parser tests on generated video-only and finite-AAC fixtures, nine protected playback server tests, and 57 frontend tests. The Windows release reproduced lease exhaustion on the ninth open after eight reloads before the fix. With the fix all nine opens retained the index. A reload clears the departed page's leases, preserves the cache, and rejects earlier preparations. The frontend also releases the lease when descriptor validation fails.

## Deviations

The viewer opts into preparation, indexes are reused for unchanged recordings, and active leases survive cache eviction. Complete-opening measurements include the descriptor and index preparation.

## Decisions

- Build the virtual layout from verified original fragment bytes. Keep exact per-track timing and the original AAC priming edit. The complete video SIDX goes last, with zero first offset, and references through the last original `mdat`.
- Use the existing game route, capability, opened-handle scope and range mechanics. Associate the optional virtual layout with the descriptor URL, game, approved root and opened file facts. Unsupported layouts select raw delivery before a URL is issued.

## Blockers

The four baseline video-recovery timeouts did not recur in the indexed release run. Their cause remains unverified.

## Next action

Keep the complete-opening evidence and video-recovery observations separate in any future playback work.
