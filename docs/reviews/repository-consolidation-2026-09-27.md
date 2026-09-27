# Local repository consolidation — 2026-09-27

The user requested one simple local development setup before broader code and
roadmap reconciliation. The other UI session was confirmed stopped.

## Integration

Local `main` was fast-forwarded to the accepted replay/UI integration `a840c645`,
then merged with `ui/app-wide-foundations-v1` at `bb9041ee`. The resulting merge
is `13e68091`. No published history was rewritten and no remote was changed.

Seven conflicts were resolved while retaining the newer HUD, semantic timeline,
hover behavior and visual styles together with descriptor-first replay loading,
controller ownership and pointer capture cleanup. The timeline tests retain both
semantic and lifecycle coverage. Viewer tests now exercise the persistent HUD
across fullscreen changes instead of expecting the former separate rail to unmount.

The primary checkout's pre-existing backend/export, benchmark, documentation and
roadmap edits were restored as uncommitted work. Two restore conflicts were caused
by CRLF/LF differences; normalized three-way merges preserved both changes without
further content conflicts. The old library/poller stash remains deliberately
unapplied pending reconciliation with the current backend and roadmap.

## Verification

- UI integration: 203 frontend tests passed.
- UI plus restored primary work: 210 frontend tests across 25 files passed.
- TypeScript check and production Vite build passed for both states.
- Canonical validator passed: 68 items and 11 plan/checkpoint pairs.
- `git diff --check` passed; no source conflict markers remain.
- Backend source changes were restored, not modified by this integration. Rust
  tests, native playback acceptance and performance benchmarks were not rerun.
- Existing non-fatal Vite static/dynamic import warning remains.

Command logs and preservation receipts are under
`build/repository-consolidation-20260927/`. Neither these checks nor branch
consolidation mark a product feature done. QB-CLIP-009's stale “no implementation”
statement was corrected in its existing checkpoint and feature evidence; its
status remains in progress. Broader feature overlap reconciliation remains open.

## Recovery and cleanup

Recovery directory: `build/repository-consolidation-20260927/verified/`.
It contains both source ZIPs with SHA-256 manifests, the original merge index and
linked-worktree administration, original-ref and pre-cleanup Git bundles, and
recovery instructions. The complete UI archive additionally preserves ignored
dependencies, build output and the packaged runtime.

Cleanup completed after SHA-256 verification of all 17,605 UI files (12,459,552,301
source bytes; 3,157,315,982-byte archive). Automatic approval initially rejected
deletion because the source-only archive omitted ignored files; the complete
archive and matching final inventory resolved that concern before deletion.

The final setup has one registered worktree, the original `Chronobreak` folder,
and one local branch, `main`. Ten other local branches were removed only after
each tip was proven an ancestor of `main` and preserved in Git bundles. The UI
worktree was removed. Existing uncommitted feature work remains in the primary
checkout. Its temporary recovery stash was dropped only after restoration and
bundling; the original library/poller stash `b412cd04` remains as `stash@{0}`.

The local workflow is recorded in `docs/development/WORKFLOW.md`; feature
lifecycle authority remains unchanged. Remote branches were left untouched;
nothing was pushed. The next task is to reconcile feature overlap and remaining
uncommitted work against this single tree, including the separately retained stash.
