import { afterEach, describe, expect, it, vi } from "vitest";
import { createHtmlVideoPlaybackAdapter } from "./htmlVideoPlaybackAdapter";
import { type FrameBoundary, type MediaTimelineV2, parseMediaId, type ReplayTick } from "./replayTime";

afterEach(() => vi.unstubAllGlobals());

describe("HTML video boundary conversions", () => {
  it("converts rational frames at a nonzero video PTS origin and releases the same node", () => {
    const document = new EventTarget();
    Object.assign(document, { hidden: false }); vi.stubGlobal("document", document);
    const timeline: MediaTimelineV2 = {
      mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
      video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 30_000n },
        firstPts: 30_000n, frameRate: { numerator: 30_000n, denominator: 1_001n },
        frameCount: 3_000 as FrameBoundary, onePastLastPts: 3_033_000n, replayEnd: 4_804_800_000 as ReplayTick },
      audio: { present: false },
    };
    const video = Object.assign(new EventTarget(), {
      dataset: {}, src: "", currentSrc: "", currentTime: 0, readyState: 4, networkState: 1,
      error: null, paused: true, ended: false, seeking: false, playbackRate: 1, muted: false,
      volume: 1, duration: 101.1, videoWidth: 1920, videoHeight: 1080,
      load: vi.fn(), pause: vi.fn(), play: vi.fn(() => Promise.resolve()),
      removeAttribute: vi.fn(), cancelVideoFrameCallback: vi.fn(),
      requestVideoFrameCallback: vi.fn(),
    });
    const adapter = createHtmlVideoPlaybackAdapter(video as unknown as HTMLVideoElement);
    adapter.open("http://127.0.0.1/video", timeline);
    const frame = 1_601_600 as ReplayTick;
    adapter.seek(frame);
    expect(video.currentTime).toBeCloseTo(1 + 1001 / 30_000, 12);
    expect(adapter.read().tick).toBe(frame);
    const listener = vi.fn(); const remove = adapter.listen("seeked", listener);
    video.dispatchEvent(new Event("seeked")); remove(); video.dispatchEvent(new Event("seeked"));
    expect(listener).toHaveBeenCalledTimes(1);
    adapter.release(); expect(video.pause).toHaveBeenCalledOnce();
    expect(video.removeAttribute).toHaveBeenCalledWith("src"); expect(video.load).toHaveBeenCalledTimes(2);
  });
});
