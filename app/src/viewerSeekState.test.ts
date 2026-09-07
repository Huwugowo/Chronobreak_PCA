import { describe, expect, it } from "vitest";
import { type ReplayTick } from "./replayTime";
import {
  beginRecovery,
  createViewerSeekState,
  dispatchSeek,
  observePresentation,
  observeSeeked,
} from "./viewerSeekState";

const tick = (value: number) => value as ReplayTick;

describe("viewer seek epochs", () => {
  it("supersedes earlier same-generation seeks and ignores stale observations", () => {
    const first = dispatchSeek(createViewerSeekState(7), tick(100), 4);
    const second = dispatchSeek(first, tick(200), 4);
    expect(second.dispatched?.epoch).toBe(2);
    expect(observeSeeked(second, 7, 1, tick(100))).toBe(second);
    expect(observeSeeked(second, 7, 2, tick(210))).toBe(second);
    const seeked = observeSeeked(second, 7, 2, tick(203));
    expect(seeked.seekedObservation).toBe(203);
    expect(observePresentation(seeked, 7, 1, tick(100), "rvfc")).toBe(seeked);
    const presented = observePresentation(seeked, 7, 2, tick(204), "rvfc");
    expect(presented.presented).toBe(204);
    expect(presented.dispatched).toBeNull();
  });

  it("does not let approximate media clock settle authoritative presentation", () => {
    const state = dispatchSeek(createViewerSeekState(), tick(100), 2);
    expect(observePresentation(state, 0, 1, tick(100), "media-clock-approximate")).toBe(state);
  });

  it("invalidates old-generation work during recovery", () => {
    const state = dispatchSeek(createViewerSeekState(3), tick(100), 2);
    const recovered = beginRecovery(state);
    expect(recovered).toMatchObject({ generation: 4, dispatched: null, presented: null });
    expect(observePresentation(recovered, 3, 1, tick(100), "rvfc")).toBe(recovered);
  });
});
