# Targeted simplification and roadmap reconciliation — 2026-09-28

This is a dated report of this pass. `feature-list.json` remains the lifecycle
authority; planned-feature checkpoints retain detailed execution evidence.

The scope was the unfinished export-retry handoff, UI work already integrated
into the roadmap, and one small viewer extraction. No benchmarks were run.
Existing Rust, recorder, collector, performance evidence and unrelated working
changes were preserved. Work stayed in the main checkout on `main`.

## Feature-list changes

| Item | Before → after | What changed and what remains |
| --- | --- | --- |
| **QB-CLIP-009 — Export retry / re-export** | in-progress → **done** | Completed acceptance reconciliation of the existing implementation. Fixed the remaining stale-dispatch gap: failed library refresh or missing source now produces a recovery message and disables another export. Added navigation/root round-trip and refresh supersession/failure coverage. Valid same-root retry preserves the draft/options and uses the refreshed token after media identity validation. |
| **QB-MAINT-003 — Separate Viewer Benchmark Actions** | new → **done** | One direct child under the existing code-quality epic. Moved scripted benchmark actions into `viewerBenchmarkActions.ts`; playback ownership, readiness, cancellation, media release, completion and fullscreen effects stay in the viewer. |
| **EPIC-CODE-QUALITY** | not-started → **in-progress** | Records the completed bounded child. Larger controller/analyzer refactors remain future work. |
| **QB-REPLAY-005 — Replay Navigation Shortcuts** | not-started → **in-progress** | Acknowledges existing Space/Arrow/F/Escape shortcuts and focused timeline keys. Documentation, viewer-wide mapping coverage, event traversal and manual acceptance remain open. Coarse seeking is distinct from exact frame stepping. |
| **QB-REPLAY-016 — Zoomable Timeline and Adaptive LOD** | not-started → **in-progress** | Acknowledges the shared viewport, zoom/pan, bounds and display collision grouping already integrated and tested. Full filter composition, bookmark/preview alignment and responsiveness/manual acceptance remain open. Remaining planned implementation still needs a finalized design. |
| **QB-REPLAY-013 — Versioned Replay Index** | not-started → not-started | Shortened the name and corrected stale evidence: QB-REPLAY-010 already provides descriptor-first opening with optional details. Future index/cache work must reuse that path. The index, identity/rebuild/cleanup contracts and measured benefit remain unimplemented. Existing acceptance criteria are unchanged. |
| **QB-CLIP-001 — Smart Event Clustering** | not-started → not-started | Clarified that visual marker grouping and current heuristic clip defaults do not supply deterministic semantic multikill sequences with reusable bounds. |
| **QB-LIB-003 — Source Match and Clip Linking** | not-started → not-started | Recorded the existing source identity/labels while retaining the missing match-to-clips and clip-to-source-at-time navigation work. |

Eight entries changed, including one new maintenance child: **68 → 69 items**.
No existing acceptance criterion or dependency was removed or weakened.

## Existing work retained and boundaries clarified

- **QB-CLIP-008/010** remain done on their existing owned-export process/filesystem
  evidence. This pass did not reimplement or reverify their Rust/media work.
- **QB-CLIP-002** already records the editable heuristic baseline; its full
  sequence-derived smart boundaries remain future work.
- **QB-REPLAY-002/003/014/015** retain their existing statuses. Event-category
  filtering, thumbnail previews, encoder-policy work and exact presented-frame
  stepping are not completed by the newer timeline appearance.
- **QB-LIB-004/008/009** retain their existing statuses: the scan/retention
  baselines do not complete byte-budget storage management.
- **QB-REPLAY-009/010** retain their unfinished statuses and outstanding
  human/performance gates. This pass did not run or waive those gates.
- **QB-LIB-012** initial library-scan recovery through Settings remains open.
  Fixing export retry does not resolve that separate startup problem.
- The preserved backend stash was not applied. No branch/worktree changes,
  commits or pushes were made during this pass.

This was reconciliation of the relevant overlap, not a fresh audit of every
roadmap item or proof of end-to-end release readiness.

## Code and documentation result

`ViewerScreen.tsx` decreased from **1,277 to 1,009 lines**. The extracted module is
328 lines, including its explicit typed boundary: the extraction adds 60 total
production lines while removing a separate responsibility from the view. It is
a maintainability change, with no performance claim. Eight deterministic tests
exercise action routing, failure/abort handling, canonical event/clip positions,
seek/layout ordering, scrubbing and rate observations using fake operations.

Export admission uses the existing library controller and reactive navigation
state. No parallel token cache, refresh mechanism or backend state was added.
The actual exporter integration tests prove both valid retries and disabled stale
dispatch after failed refresh or source removal.

Updated the export checkpoint and durable architecture. Also corrected the stale
desktop architecture paragraph that described packaged FFmpeg/ffprobe resolution
as future work; the locked portable runtime already exists, while installer work
remains separate.

## Verification

- Export/controller/App focused tests: **37 passed**.
- Full suite after the retry fix: **216 tests in 25 files passed**.
- Extracted runner plus viewer lifecycle tests: **15 passed**.
- Final frontend suite: **224 tests in 26 files passed**.
- TypeScript and production Vite build: **passed**. The existing mixed
  static/dynamic `api.ts` import warning remains nonfatal.
- Mechanical comparison confirmed the moved action body changed only for the
  explicit controller/generation/abort accessors and seek-reason type alias.
- Focused independent review: **no actionable regressions found**.
- Canonical roadmap validation: **69 items / 11 plan-checkpoint pairs passed**.
- `git diff --check`: **passed**.

Logs and the before-state snapshot are under
`build/targeted-simplification-20260928/`. Test fixture/type issues encountered
during the work were corrected before the passing final results. No benchmark
campaign, native capture/encode test, manual A/V session or desktop executable
rebuild was performed. The frontend build alone does not update an installed app.
