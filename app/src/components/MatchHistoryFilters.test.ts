// @vitest-environment jsdom
import { invoke } from "@tauri-apps/api/core";
import { expect, it, vi } from "vitest";
import { readClipGridPage } from "../managedClips";
import { emptyHistoryFilters, historyQueryForFilters } from "./MatchHistoryFilters";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

it.each([
  ["2026-10-25", "2026-10-25", new Date(2026, 9, 25), new Date(2026, 9, 26)],
  ["2026-10-25", "", new Date(2026, 9, 25), undefined],
  ["", "2026-10-25", undefined, new Date(2026, 9, 26)],
] as const)("uses the same local-day boundaries for Clips and Match History (%s to %s)", async (from, to, start, end) => {
  const history = historyQueryForFilters({ ...emptyHistoryFilters(), from, to });
  const seconds = (date?: Date) => date && date.getTime() / 1000;
  expect(history.recorded_from).toBe(seconds(start));
  expect(history.recorded_before).toBe(seconds(end));

  await readClipGridPage("snapshot", { from, to }, null);
  expect(invoke).toHaveBeenLastCalledWith("clip_grid_page", {
    snapshotToken: "snapshot", after: null,
    query: { played_from: history.recorded_from, played_before: history.recorded_before },
  });
});
