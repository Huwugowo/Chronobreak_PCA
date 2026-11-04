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

it("steps through six speeds in one click and preserves accessible audio and fallback feedback", () => {
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
    const speed = host.querySelector("output")!;
    const decrease = host.querySelector<HTMLButtonElement>('[aria-label="Decrease replay speed"]')!;
    const increase = host.querySelector<HTMLButtonElement>('[aria-label="Increase replay speed"]')!;
    expect(host.querySelector("select")).toBeNull();
    expect(speed.textContent).toBe("1x");
    decrease.click(); expect(speed.textContent).toBe("0.5x");
    decrease.click(); expect(speed.textContent).toBe("0.25x");
    expect(decrease.disabled).toBe(true);
    decrease.click(); expect(changed).toHaveBeenCalledTimes(2);
    for (const expected of ["0.5x", "1x", "2x", "4x", "8x"]) {
      increase.click(); expect(speed.textContent).toBe(expected);
    }
    expect(increase.disabled).toBe(true);
    increase.click(); expect(changed).toHaveBeenCalledTimes(7);
    for (const expected of ["4x", "2x", "1x", "0.5x", "0.25x"]) {
      decrease.click(); expect(speed.textContent).toBe(expected);
    }
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
    increase.focus();
    for (const key of ["Enter", " ", "ArrowUp", "ArrowDown"]) {
      const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      increase.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(false);
    }
    expect(globalKey).not.toHaveBeenCalled();
    setState((old) => ({ ...old, desiredPlaying: true, rate: { selected: 8, applied: 1, effective: 1,
      observed: 1, outcome: "limited", limitation: "Presented video stopped advancing" } }));
    expect(speed.textContent).toBe("8x");
    expect(host.querySelector('[role="status"]')!.textContent).toBe("8x unavailable; playing at 1x");
    setState((old) => ({ ...old, readiness: "recovering" }));
    expect(host.querySelector('[role="status"]')!.textContent).toBe("8x unavailable; fallback 1x");
    decrease.click();
    expect(changed).toHaveBeenLastCalledWith(4);
    expect(speed.textContent).toBe("4x");
  } finally { window.removeEventListener("keydown", globalKey); dispose(); }
});

it("applies rapid adjacent steps through the real controller and disables unavailable controls", () => {
  const { video, controller, host, cleanup } = renderControllerControls();
  try {
    const decrease = host.querySelector<HTMLButtonElement>('[aria-label="Decrease replay speed"]')!;
    const increase = host.querySelector<HTMLButtonElement>('[aria-label="Increase replay speed"]')!;
    for (const expected of [2, 4, 8]) {
      increase.click();
      expect(controller.snapshot().rate.selected).toBe(expected);
      expect(video.playbackRate).toBe(expected);
    }
    for (const expected of [4, 2, 1, 0.5, 0.25]) {
      decrease.click();
      expect(controller.snapshot().rate.selected).toBe(expected);
      expect(video.playbackRate).toBe(expected);
    }
    controller.dispose();
    expect(decrease.disabled).toBe(true);
    expect(increase.disabled).toBe(true);
  } finally { cleanup(); }
});

it.each([undefined, "closed", "disposed"] as const)("disables speed steps when controls are %s", (readiness) => {
  const changed = vi.fn();
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <PlaybackControls state={readiness === undefined ? undefined : {
    rate: { selected: 1, effective: 1, applied: 1, observed: null, outcome: "suspended", limitation: null },
    media: { muted: false, volume: 1 }, audio: "available-but-unverified", desiredPlaying: false, readiness,
  }} onRate={changed} onMuted={vi.fn()} onVolume={vi.fn()} />, host);
  try {
    for (const label of ["Decrease replay speed", "Increase replay speed"]) {
      const button = host.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!;
      expect(button.disabled).toBe(true);
      button.click();
    }
    expect(changed).not.toHaveBeenCalled();
  } finally { dispose(); }
});
it("moving volume while muted restores audible playback", () => {
  const [state, setState] = createSignal<NonNullable<PlaybackControlsProps["state"]>>({
    rate: {
      selected: 1,
      effective: 1,
      applied: 1,
      observed: null,
      outcome: "suspended",
      limitation: null,
    },
    media: {
      muted: true,
      volume: 0.75,
    },
    audio: "available-but-unverified",
    desiredPlaying: false,
    readiness: "ready",
  });

  const mutedChanges = vi.fn();
  const volumeChanges = vi.fn();

  const host = document.createElement("div");
  document.body.append(host);

  const dispose = render(
    () => (
      <PlaybackControls
        state={state()}
        onRate={(selected) =>
          setState((old) => ({
            ...old,
            rate: { ...old.rate, selected },
          }))
        }
        onMuted={(muted) => {
          mutedChanges(muted);
          setState((old) => ({
            ...old,
            media: { ...old.media, muted },
          }));
        }}
        onVolume={(volume) => {
          volumeChanges(volume);
          setState((old) => ({
            ...old,
            media: { ...old.media, volume },
          }));
        }}
      />
    ),
    host,
  );

  try {
    expect(host.querySelector('[aria-label="Unmute replay"]')).not.toBeNull();

    const volume = host.querySelector<HTMLInputElement>(
      '[aria-label="Replay volume"]',
    )!;

    volume.value = "37";
    volume.dispatchEvent(new Event("input", { bubbles: true }));

    expect(mutedChanges).toHaveBeenCalledWith(false);
    expect(volumeChanges).toHaveBeenCalledWith(0.37);

    expect(state().media).toEqual({
      muted: false,
      volume: 0.37,
    });

    expect(host.querySelector('[aria-label="Mute replay"]')).not.toBeNull();
  } finally {
    dispose();
  }
});
it("shows zero output while muted and restores the previous volume", () => {
  const [state, setState] = createSignal<NonNullable<PlaybackControlsProps["state"]>>({
    rate: {
      selected: 1,
      effective: 1,
      applied: 1,
      observed: null,
      outcome: "suspended",
      limitation: null,
    },
    media: {
      muted: false,
      volume: 0.65,
    },
    audio: "available-but-unverified",
    desiredPlaying: false,
    readiness: "ready",
  });

  const host = document.createElement("div");
  document.body.append(host);

  const dispose = render(
    () => (
      <PlaybackControls
        state={state()}
        onRate={vi.fn()}
        onMuted={(muted) =>
          setState((old) => ({
            ...old,
            media: { ...old.media, muted },
          }))
        }
        onVolume={(volume) =>
          setState((old) => ({
            ...old,
            media: { ...old.media, volume },
          }))
        }
      />
    ),
    host,
  );

  try {
    const volume = host.querySelector<HTMLInputElement>(
      '[aria-label="Replay volume"]',
    )!;
    const mute = host.querySelector<HTMLButtonElement>(
      '[aria-label="Mute replay"]',
    )!;

    expect(volume.value).toBe("65");
    expect(volume.getAttribute("aria-valuetext")).toBe("65%");

    mute.click();

    expect(state().media).toEqual({
      muted: true,
      volume: 0.65,
    });
    expect(volume.value).toBe("0");
    expect(volume.getAttribute("aria-valuetext")).toBe("0%");
    expect(
      host.querySelector('[aria-label="Unmute replay"]'),
    ).not.toBeNull();

    host
      .querySelector<HTMLButtonElement>('[aria-label="Unmute replay"]')!
      .click();

    expect(state().media).toEqual({
      muted: false,
      volume: 0.65,
    });
    expect(volume.value).toBe("65");
    expect(volume.getAttribute("aria-valuetext")).toBe("65%");
  } finally {
    dispose();
  }
});
