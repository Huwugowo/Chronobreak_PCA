// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import PlaybackControls, { type PlaybackControlsProps } from "./PlaybackControls";
import { createPlaybackController } from "../playbackController";
import { createHtmlVideoPlaybackAdapter } from "../htmlVideoPlaybackAdapter";
import { parseMediaId, type MediaTimelineV2, type FrameBoundary, type ReplayTick } from "../replayTime";

afterEach(() => { document.body.replaceChildren(); });

const timeline: MediaTimelineV2 = {
  mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
  video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
    frameRate: { numerator: 60n, denominator: 1n }, frameCount: 600 as FrameBoundary,
    onePastLastPts: 600n, replayEnd: 480_000_000 as ReplayTick }, audio: { present: true },
};

function renderControllerControls() {
  const video = document.createElement("video");
  video.load = () => {};
  video.pause = () => {};
  Object.defineProperties(video, {
    currentSrc: { get: () => video.src }, readyState: { get: () => 4 },
  });
  const controller = createPlaybackController(createHtmlVideoPlaybackAdapter(video));
  const [state, setState] = createSignal(controller.snapshot());
  const unsubscribe = controller.subscribe(setState);
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <PlaybackControls state={state()} onRate={controller.setRate}
    onMuted={controller.setMuted} onVolume={controller.setVolume} />, host);
  void controller.open({ url: "http://127.0.0.1:123/games/1/video.mp4", mediaId: timeline.mediaId, timeline });
  video.dispatchEvent(new Event("loadstart")); video.dispatchEvent(new Event("loadedmetadata"));
  return { video, controller, host, cleanup: () => { dispose(); unsubscribe(); controller.dispose(); } };
}

it("renders an ignored mute as unmuted while retaining intent for recovery", () => {
  const { video, controller, host, cleanup } = renderControllerControls();
  let applied = false;
  let ignore = true;
  Object.defineProperty(video, "muted", { get: () => applied, set: (value: boolean) => { if (!ignore) applied = value; } });
  try {
    const mute = host.querySelector("button")!;
    mute.click();
    expect(controller.snapshot()).toMatchObject({ muted: true, media: { muted: false }, audio: "runtime-limited" });
    expect(mute.textContent).toBe("Mute");
    expect(mute.getAttribute("aria-label")).toBe("Mute replay");
    expect(mute.getAttribute("aria-pressed")).toBe("false");
    expect(host.textContent).toContain("Sound controls unavailable");
    ignore = false;
    void controller.retry();
    expect(video.muted).toBe(true);
    expect(mute.getAttribute("aria-pressed")).toBe("true");
    mute.click();
    expect(video.muted).toBe(false);
  } finally { cleanup(); }
});

it.each(["ignored", "rejected", "transformed"] as const)("renders the applied volume after a %s assignment", (failure) => {
  const { video, controller, host, cleanup } = renderControllerControls();
  let applied = 1;
  let limited = true;
  Object.defineProperty(video, "volume", { get: () => applied, set: (value: number) => {
    if (!limited) applied = value;
    else if (failure === "rejected") throw new DOMException("Volume rejected", "NotSupportedError");
    else if (failure === "transformed") applied = 0.5;
  } });
  try {
    const volume = host.querySelector("input")!;
    const expected = failure === "transformed" ? 0.5 : 1;
    // Repeat after the applied value has stopped changing: the native thumb must
    // still return to actual volume even when Solid has no changed value to write.
    for (const requested of [37, 20]) {
      volume.value = String(requested); volume.dispatchEvent(new Event("input", { bubbles: true }));
      expect(controller.snapshot()).toMatchObject({ volume: requested / 100, media: { volume: expected }, audio: "runtime-limited" });
      expect(volume.value).toBe(String(expected * 100));
      expect(volume.getAttribute("aria-valuetext")).toBe(`${expected * 100}%`);
      expect(volume.parentElement!.querySelector("span")!.textContent).toBe(`${expected * 100}%`);
      expect(host.textContent).toContain("Sound controls unavailable");
    }
    limited = false;
    void controller.retry();
    expect(video.volume).toBe(0.2);
    expect(volume.value).toBe("20");
  } finally { cleanup(); }
});

it("exposes six native speed choices and independent keyboard-accessible audio controls", () => {
  const [state, setState] = createSignal<NonNullable<PlaybackControlsProps["state"]>>({
    rate: { selected: 1, effective: 1, applied: 1, observed: null, outcome: "suspended", limitation: null },
    media: { muted: false, volume: 0.75 }, audio: "available-but-unverified", desiredPlaying: false, readiness: "ready",
  });
  const changed = vi.fn();
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <PlaybackControls state={state()}
    onRate={(selected) => { changed(selected); setState((old) => ({ ...old, rate: { ...old.rate, selected } })); }}
    onMuted={(muted) => setState((old) => ({ ...old, media: { ...old.media, muted } }))}
    onVolume={(volume) => setState((old) => ({ ...old, media: { ...old.media, volume } }))} />, host);
  const globalKey = vi.fn(); window.addEventListener("keydown", globalKey);
  try {
    const speed = host.querySelector("select")!;
    expect([...speed.options].map((option) => option.textContent)).toEqual(["0.25x", "0.5x", "1x", "2x", "4x", "8x"]);
    speed.value = "4"; speed.dispatchEvent(new Event("change", { bubbles: true }));
    expect(changed).toHaveBeenCalledWith(4); expect(speed.value).toBe("4");
    const volume = host.querySelector("input")!;
    expect(volume.getAttribute("aria-label")).toBe("Replay volume");
    expect([volume.min, volume.max]).toEqual(["0", "100"]);
    volume.value = "37"; volume.dispatchEvent(new Event("input", { bubbles: true }));
    host.querySelector("button")!.click();
    expect(state()).toMatchObject({ media: { volume: 0.37, muted: true } });
    expect(host.querySelector("button")!.getAttribute("aria-pressed")).toBe("true");
    volume.focus();
    const key = new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true });
    volume.dispatchEvent(key);
    expect(key.defaultPrevented).toBe(false); expect(globalKey).not.toHaveBeenCalled();
    setState((old) => ({ ...old, desiredPlaying: true, rate: { selected: 8, applied: 1, effective: 1,
      observed: 1, outcome: "limited", limitation: "Presented video stopped advancing" } }));
    expect(speed.value).toBe("8");
    expect(host.querySelector('[role="status"]')!.textContent).toBe("8x unavailable; playing at 1x");
    setState((old) => ({ ...old, readiness: "recovering" }));
    expect(host.querySelector('[role="status"]')!.textContent).toBe("8x unavailable; fallback 1x");
  } finally { window.removeEventListener("keydown", globalKey); dispose(); }
});
