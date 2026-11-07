# QB-REPLAY-030 execution checkpoint

Feature: `QB-REPLAY-030`
ExecPlan: `docs/exec-plans/qb-replay-030-front-fragment-index.md`
Updated: 2026-11-07

## Current milestone

Planning handoff. Milestone 1 is ready to start.

## Active unit

The first implementation unit is the selected-game MP4 eligibility reader and SIDX serializer. Its app-side inputs are a contained source handle and the descriptor's exact `MediaTimelineV2`.

## Completed

- Reviewed fragment relocation, AAC timing and edit handling, bounded range streaming, source replacement, descriptor publication, and existing clip and benchmark consumers.
- Proved the virtual layout on all five original gameplay sources with the standalone Edge control. The probe reads fragment metadata and serves original initialization and sample data around the added indexes. It keeps the source file open read-only and does not remux.
- Built and retained the pre-index desktop release. Recorded a fresh complete-opening baseline on the same five prepared recordings.

## In flight

The plan handoff contains no product-code changes.

## Remaining

Implement the reader, integrate the protected range path, and verify production opening, A/V timing, seeking, raw compatibility and export according to the ExecPlan.

## Verification

- `build/perf/complete-open-20261106/index-only-probe.json` records one request through frame zero for both indexed originals, versus 351–427 requests. First presentation was 95–135 ms indexed versus 2.22–2.77 s original. Browser duration was identical on each pair.
- `build/perf/complete-open-20261106/index-seek-probe.json` covers all five sources. Frame-zero presentation was 73–124 ms indexed versus 2.69–3.67 s original. Duration and all 15 matched early/middle/late RVFC timestamps agree. Total indexed requests were six or seven including the three seeks. The first middle and late seeks commonly took longer indexed, approximately 195–333 ms for those slow cases, so keep seek timing separate from opening. This is standalone video-element evidence.
- `npm run desktop:build --prefix app` passed. The overlay-optimized pre-index executable is `build/perf/complete-open-20261106/pre-index.exe`, SHA-256 `1D42288738C8D2CA3B3FCDD9F15330236E499D5C9DB4E0493B6AD79AF0D1F56B`. The frontend import warning and linker-output warning were present.
- Fresh production baseline in `build/perf/complete-open-20261106/pre-index-2/summary.json`. Graves (1792962427) reached complete opening in 5,131 ms, first frame in 5,068 ms, metadata in 4,801 ms and TAB in 507 ms. It issued 363 requests before frame zero. The other four recordings reached details and TAB in 688–835 ms, cycled `emptied`/`loadstart` and timed out without metadata or presented video within 12 s. They issued 832–1,084 requests through timeout across load attempts. Keep each failed recording and phase visible. The previous trusted report had two completed pairs and three recoveries.
- The first contained Windows launch failed during app setup with `local data directory unavailable` and could not resolve the packaged runtime from the copied executable location. The successful baseline used `measure-indexed-normal.mjs`, the locked runtime via `QUEUEBACK_MEDIA_RUNTIME_DIR`, Windows launch outside that restriction, the existing disposable config/app-data and a new isolated profile with normal WebView sandboxing. The script is a copy of `measure-local.mjs` with the game identity extracted in memory instead of retaining the capability URL, and the normal WebView flags restored. Run it from the repo root with the Playwright module path, executable, new result directory and new profile, and use matching settings for the indexed release. Raw command output is `pre-index-probe-2.log` under the same artifact root.
- Recorded SHA-256 fingerprints of the five canonical videos and their metadata in `build/perf/complete-open-20261106/pre-index-source-hashes.json` for the candidate source-preservation comparison.

## Deviations

The planning control uses Edge over a diagnostic HTTP path. The actual desktop route and complete-opening improvement require the candidate checks.

## Decisions

- Build the virtual layout from verified original fragment bytes. Keep exact per-track timing and the original AAC priming edit. The complete video SIDX goes last, with zero first offset, and references through the last original `mdat`.
- Use the existing game route, capability, opened-handle scope and range mechanics. Associate the optional virtual layout with the descriptor URL, game, approved root and opened file facts. Unsupported layouts select raw delivery before a URL is issued.

## Blockers

The observed video-recovery failures are unresolved. The standalone control demonstrates the index effect on the original bytes and does not establish a root cause for recovery in the app. No implementation blocker remains at the planning handoff.

## Next action

Start milestone 1 by implementing the bounded eligibility reader and SIDX serializer for the approved layout. Verify synthetic exact-timing and malformed-layout cases before wiring descriptor and range delivery.
