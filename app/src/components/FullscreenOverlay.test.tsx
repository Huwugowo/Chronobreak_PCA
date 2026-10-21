// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createSignal, type ComponentProps } from "solid-js";
import { render } from "solid-js/web";
import FullscreenOverlay from "./FullscreenOverlay";
import styles from "./FullscreenOverlay.module.css";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "../replayTime";

vi.mock("./ReplayTimeline", () => ({ default: () => <button type="button">Timeline</button> }));
vi.mock("./ChampionFilter", () => ({ default: () => <button type="button">Champion</button> }));
vi.mock("./PlaybackControls", () => ({ default: () => null }));

const disposers: Array<() => void> = [];
afterEach(() => { disposers.splice(0).forEach(dispose => dispose()); document.body.replaceChildren(); vi.useRealTimers(); });
const mount = (overrides: Partial<ComponentProps<typeof FullscreenOverlay>> = {}) => {
  vi.useFakeTimers();
  const noop = () => {};
  const [props, setProps] = createSignal<ComponentProps<typeof FullscreenOverlay>>({
    champion: "Ahri", localPlayerName: null, events: [],
    mediaTimeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"), video: {
      codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
      frameRate: { numerator: 60n, denominator: 1n }, frameCount: 600 as FrameBoundary,
      onePastLastPts: 600n, replayEnd: 480_000_000 as ReplayTick }, audio: { present: false } },
    replayTick: 0 as ReplayTick, presentedTick: null, timelineViewport: { start: 0 as ReplayTick, end: 480_000_000 as ReplayTick },
    onTimelineViewportChange: noop, isPlaying: true, fullscreen: true, playback: undefined,
    onRate: noop, onMuted: noop, onVolume: noop, mediaAvailable: true, participants: [], selectedPlayers: [], clipRange: null,
    onPlayerToggle: noop, onPlayerClear: noop, onTogglePlayback: noop, onSeek: noop, onActivateClip: noop,
    onEventSelect: noop, onClipEndpointEditStart: noop, onClipEndpointPreview: noop, onClipEndpointEditFinish: noop,
    onClipEndpointKeyDown: noop, onClipEndpointKeyUp: noop, onClipEndpointBlur: noop, onExportClip: noop,
    onCancelClip: noop, onFullscreenToggle: noop, ...overrides,
  });
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <FullscreenOverlay {...props()} />, host); disposers.push(dispose);
  const hud = host.querySelector<HTMLElement>('[data-testid="fullscreen-hud"]')!;
  return { host, hud, dispose, hidden: () => hud.classList.contains(styles.hudHidden),
    update: (patch: Partial<ComponentProps<typeof FullscreenOverlay>>) => setProps(value => ({ ...value, ...patch })) };
};

it.each(["paused", "windowed", "clip"])("only auto-hides idle fullscreen playback, revealing immediately when %s", async state => {
  const h = mount();
  await vi.advanceTimersByTimeAsync(3200); expect(h.hidden()).toBe(true);
  if (state === "paused") h.update({ isPlaying: false });
  else if (state === "windowed") h.update({ fullscreen: false });
  else h.update({ clipRange: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"), startFrame: 0 as FrameBoundary, endFrameExclusive: 60 as FrameBoundary } });
  expect(h.hidden()).toBe(false);
  await vi.advanceTimersByTimeAsync(6400); expect(h.hidden()).toBe(false);
  h.update({ isPlaying: true, fullscreen: true, clipRange: null });
  await vi.advanceTimersByTimeAsync(3199); expect(h.hidden()).toBe(false);
  await vi.advanceTimersByTimeAsync(1); expect(h.hidden()).toBe(true);
});

it("holds hover and keyboard focus independently, including pointer activity and focus transfers", async () => {
  const h = mount();
  const buttons = h.hud.querySelectorAll<HTMLButtonElement>("button");
  h.hud.dispatchEvent(new Event("pointerenter"));
  buttons[0].focus();
  window.dispatchEvent(new Event("pointermove"));
  await vi.advanceTimersByTimeAsync(6400); expect(h.hidden()).toBe(false);
  buttons[1].focus();
  h.hud.dispatchEvent(new Event("pointerleave"));
  await vi.advanceTimersByTimeAsync(6400); expect(h.hidden()).toBe(false);
  buttons[1].blur();
  await vi.advanceTimersByTimeAsync(3200); expect(h.hidden()).toBe(true);
});

it.each(["pointerup", "pointercancel", "lostpointercapture"])("holds a pointer interaction outside the HUD until %s", async ending => {
  const h = mount();
  h.hud.querySelector("button")!.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  window.dispatchEvent(new Event("pointermove"));
  await vi.advanceTimersByTimeAsync(6400); expect(h.hidden()).toBe(false);
  window.dispatchEvent(new Event(ending));
  await vi.advanceTimersByTimeAsync(3200); expect(h.hidden()).toBe(true);
});

it("ignores unrelated pointer endings and releases stale interaction holds on window blur", async () => {
  const h = mount();
  const button = h.hud.querySelector<HTMLButtonElement>("button")!;
  h.hud.dispatchEvent(new Event("pointerenter")); button.focus();
  button.dispatchEvent(Object.assign(new Event("pointerdown", { bubbles: true }), { pointerId: 7 }));
  window.dispatchEvent(Object.assign(new Event("pointerup"), { pointerId: 8 }));
  h.hud.dispatchEvent(new Event("pointerleave")); button.blur();
  await vi.advanceTimersByTimeAsync(6400); expect(h.hidden()).toBe(false);
  window.dispatchEvent(new Event("blur")); window.dispatchEvent(new Event("focus"));
  await vi.advanceTimersByTimeAsync(3200); expect(h.hidden()).toBe(true);
});

it("wakes from video activity without an overlay hit surface and cleans up listeners/timers", async () => {
  const h = mount();
  await vi.advanceTimersByTimeAsync(3200); expect(h.hidden()).toBe(true);
  const root = h.host.querySelector('[data-testid="fullscreen-overlay"]')!;
  expect(root.className).toBe(styles.overlayRoot);
  const video = document.createElement("video"); h.host.append(video);
  const click = vi.fn(); video.addEventListener("click", click);
  video.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  video.click();
  expect(click).toHaveBeenCalledOnce(); expect(h.hidden()).toBe(false);
  expect(vi.getTimerCount()).toBe(1);
  h.dispose();
  expect(vi.getTimerCount()).toBe(0);
  window.dispatchEvent(new Event("pointermove"));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab" }));
  expect(vi.getTimerCount()).toBe(0);
});
