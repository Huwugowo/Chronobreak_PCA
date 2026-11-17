// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import ClipModal from "./ClipModal";
import hudStyles from "./FullscreenOverlay.module.css";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.replaceChildren();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

function mountClip() {
  const close = vi.fn();
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => (
    <ClipModal
      clip={{
        filename: "fixture.mp4", video_url: "http://fixture/clip.mp4", duration_ms: 45_000,
        game_timestamp: "1", clip_timestamp: "2", file_size_bytes: 1,
        source_champion: "Ahri", source_date: null, thumbnail_path: null, thumbnail_url: null,
      }}
      onClose={close}
    />
  ), host);
  const video = host.querySelector("video")!;
  const button = (label: string) => host.querySelector<HTMLButtonElement>('button[aria-label="' + label + '"]')!;
  return { host, video, button, close };
}

it("uses shared playback controls with the clip's native media state", async () => {
  const { host, video, button } = mountClip();
  expect(video.controls).toBe(false);
  expect(video.autoplay).toBe(true);

  let paused = true;
  vi.spyOn(video, "paused", "get").mockImplementation(() => paused);
  vi.spyOn(video, "duration", "get").mockReturnValue(45);
  const play = vi.spyOn(video, "play").mockImplementation(async () => {
    paused = false;
    video.dispatchEvent(new Event("play"));
  });
  const pause = vi.spyOn(video, "pause").mockImplementation(() => {
    paused = true;
    video.dispatchEvent(new Event("pause"));
  });
  button("Play clip").click();
  expect(play).toHaveBeenCalledOnce();
  button("Pause clip").click();
  expect(pause).toHaveBeenCalledOnce();

  const position = host.querySelector<HTMLInputElement>('[aria-label="Clip position"]')!;
  expect(position.disabled).toBe(true);
  video.dispatchEvent(new Event("loadedmetadata"));
  expect(position.disabled).toBe(false);
  position.value = "12.5";
  position.dispatchEvent(new Event("input", { bubbles: true }));
  expect(video.currentTime).toBe(12.5);
  position.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", ctrlKey: true, bubbles: true }));
  expect(video.currentTime).toBe(17.5);
  expect(host.querySelector('[data-testid="time-readout"]')?.textContent).toBe("0:17/0:45");

  button("Mute clip").click();
  expect(video.muted).toBe(true);
  const volume = host.querySelector<HTMLInputElement>('[aria-label="Clip volume"]')!;
  volume.value = "35";
  volume.dispatchEvent(new Event("input", { bubbles: true }));
  expect(video.muted).toBe(false);
  expect(video.volume).toBe(0.35);
  const speedUp = button("Increase clip speed");
  speedUp.click();
  speedUp.click();
  expect(video.playbackRate).toBe(4);
  expect(host.querySelector('[aria-label="Selected clip speed"]')?.textContent).toBe("4x");
});

it("keeps the video across fullscreen, hides the idle HUD and consumes Escape before the dialog", async () => {
  vi.useFakeTimers();
  const { host, video, button, close } = mountClip();
  const surface = video.parentElement!;
  const hud = host.querySelector('[aria-label="Clip controls"]')!;
  let fullscreenElement: Element | null = null;
  const setFullscreen = (element: Element | null) => {
    fullscreenElement = element;
    document.dispatchEvent(new Event("fullscreenchange"));
  };
  const original = Object.getOwnPropertyDescriptors(document);
  Object.defineProperty(document, "fullscreenElement", { configurable: true, get: () => fullscreenElement });
  Object.defineProperty(document, "exitFullscreen", {
    configurable: true, value: vi.fn(async () => setFullscreen(null)),
  });
  Object.defineProperty(surface, "requestFullscreen", { value: vi.fn(async () => setFullscreen(surface)) });
  try {
    let paused = false;
    vi.spyOn(video, "paused", "get").mockImplementation(() => paused);
    video.dispatchEvent(new Event("play"));
    await vi.advanceTimersByTimeAsync(3_200);
    expect(hud.classList.contains(hudStyles.hudHidden)).toBe(false);

    hud.dispatchEvent(new Event("pointerenter"));
    button("Open fullscreen clip").click();
    expect(document.fullscreenElement).toBe(surface);
    expect(button("Exit fullscreen clip")).not.toBeNull();
    expect(surface.querySelector("video")).toBe(video);
    // Fullscreen can reposition the hovered HUD without dispatching pointerleave.
    video.dispatchEvent(new Event("pointermove", { bubbles: true }));
    await vi.advanceTimersByTimeAsync(3_200);
    expect(hud.classList.contains(hudStyles.hudHidden)).toBe(true);

    paused = true;
    video.dispatchEvent(new Event("pause"));
    expect(hud.classList.contains(hudStyles.hudHidden)).toBe(false);
    const escape = () => video.dispatchEvent(new KeyboardEvent("keydown", {
      key: "Escape", bubbles: true, cancelable: true,
    }));
    escape();
    expect(document.fullscreenElement).toBeNull();
    expect(surface.querySelector("video")).toBe(video);
    expect(close).not.toHaveBeenCalled();
    escape();
    expect(close).toHaveBeenCalledOnce();
  } finally {
    for (const key of ["fullscreenElement", "exitFullscreen"] as const) {
      const descriptor = original[key];
      if (descriptor) Object.defineProperty(document, key, descriptor);
      else Reflect.deleteProperty(document, key);
    }
  }
});

it("traps Tab and restores focus when the local clip dialog closes", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  const [open, setOpen] = createSignal(false);
  dispose = render(() => <>
    <button type="button" onClick={() => setOpen(true)}>Open clip</button>
    <Show when={open()}>
      <ClipModal clip={{
        filename: "fixture.mp4", video_url: "", duration_ms: 45_000,
        game_timestamp: "1", clip_timestamp: "2", file_size_bytes: 1,
        source_champion: "Ahri", source_date: null, thumbnail_path: null, thumbnail_url: null,
      }} onClose={() => setOpen(false)} />
    </Show>
  </>, host);
  const trigger = host.querySelector("button")!;
  trigger.focus();
  trigger.click();
  const close = host.querySelector<HTMLButtonElement>('[aria-label="Close clip player"]')!;
  expect(document.activeElement).toBe(close);
  for (const shiftKey of [false, true]) {
    const tab = new KeyboardEvent("keydown", { key: "Tab", shiftKey, bubbles: true, cancelable: true });
    expect(close.dispatchEvent(tab)).toBe(false);
    expect(document.activeElement).toBe(close);
  }
  close.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  expect(host.querySelector('[role="dialog"]')).toBeNull();
  await vi.waitFor(() => expect(document.activeElement).toBe(trigger));
});
