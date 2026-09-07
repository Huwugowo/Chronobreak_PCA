import { describe, expect, it } from "vitest";
import type { ReplayTick } from "./replayTime";
import {
  createClipPreviewSyncState,
  dispatchClipPreviewSeek,
  endClipPreview,
  musicLoopTickForPrimaryTick,
  observeClipPreviewPresentation,
  setClipPreviewPlaying,
} from "./clipPreviewSync";

const tick = (value: number): ReplayTick => value as ReplayTick;

describe("clip preview synchronization", () => {
  it("suppresses a stale seek epoch until the latest seek presents", () => {
    const first = dispatchClipPreviewSeek(createClipPreviewSyncState(3), tick(100), 4);
    const second = dispatchClipPreviewSeek(first, tick(200), 4);

    expect(observeClipPreviewPresentation(second, 3, 1, tick(100))).toBe(second);
    expect(observeClipPreviewPresentation(second, 3, 2, tick(180))).toBe(second);

    const accepted = observeClipPreviewPresentation(second, 3, 2, tick(203));
    expect(accepted).toMatchObject({ dispatched: null, presentedTick: 203 });
  });

  it("keeps only the latest of rapid seeks authoritative", () => {
    const first = dispatchClipPreviewSeek(createClipPreviewSyncState(), tick(100), 2);
    const second = dispatchClipPreviewSeek(first, tick(200), 2);
    const third = dispatchClipPreviewSeek(second, tick(300), 2);

    expect(observeClipPreviewPresentation(third, 0, 2, tick(200))).toBe(third);
    expect(observeClipPreviewPresentation(third, 0, 3, tick(300)).presentedTick).toBe(300);
  });

  it("models play, pause, and end without making them seek authority", () => {
    const playing = setClipPreviewPlaying(createClipPreviewSyncState(), true);
    expect(playing).toMatchObject({ playing: true, ended: false });
    expect(setClipPreviewPlaying(playing, false)).toMatchObject({ playing: false, ended: false });
    expect(endClipPreview(playing)).toMatchObject({ playing: false, ended: true });
  });

  it("maps looping music from clip-relative primary ticks", () => {
    expect(musicLoopTickForPrimaryTick(tick(1_250), tick(1_000), tick(100))).toBe(50);
    expect(musicLoopTickForPrimaryTick(tick(1_050), tick(1_000), null)).toBe(50);
  });
});
