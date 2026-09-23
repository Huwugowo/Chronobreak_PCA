import type { LibraryOrigin, LibraryState } from "./libraryController";
import type { LibrarySnapshot } from "./types";

/** Display-only save intent. Snapshot/token authority stays with the controller. */
export type SavedOverlay = {
  origin: LibraryOrigin;
  snapshot: LibrarySnapshot;
  game: string;
  saved: boolean;
  phase: "pending" | "refresh";
};

const sameView = (a: LibraryOrigin, b: LibraryOrigin | null) => b !== null &&
  a.root === b.root && a.rootEpoch === b.rootEpoch && a.navigation === b.navigation;

export function reconcileSavedOverlay(
  overlay: SavedOverlay | null, current: LibraryOrigin | null, state: LibraryState,
): SavedOverlay | null {
  if (!overlay || !sameView(overlay.origin, current) || state.snapshot !== overlay.snapshot) return null;
  if (state.actionable && current!.request === overlay.origin.request && current!.token === overlay.origin.token)
    return overlay;
  // Only the exact successful save's refresh may bridge the retained old cards.
  if (state.refreshing && state.mutationRefresh?.origin === overlay.origin &&
      state.mutationRefresh.succeeded && current!.request === overlay.origin.request + 1 &&
      current!.token === overlay.origin.token)
    return overlay.phase === "refresh" ? overlay : { ...overlay, phase: "refresh" };
  return null;
}

export function isSaveCompletionCurrent(
  overlay: SavedOverlay, current: LibraryOrigin | null, state: LibraryState,
): boolean {
  return sameView(overlay.origin, current) && current!.request === overlay.origin.request + 1 &&
    state.mutationRefresh?.origin === overlay.origin;
}
