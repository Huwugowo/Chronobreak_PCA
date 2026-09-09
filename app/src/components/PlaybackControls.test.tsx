// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import PlaybackControls, { type PlaybackControlsProps } from "./PlaybackControls";

afterEach(() => { document.body.replaceChildren(); });

it("exposes six native speed choices and independent keyboard-accessible audio controls", () => {
  const [state, setState] = createSignal<NonNullable<PlaybackControlsProps["state"]>>({
    rate: { selected: 1, effective: 1, applied: 1, observed: null, outcome: "suspended", limitation: null },
    muted: false, volume: 0.75, audio: "available-but-unverified", desiredPlaying: false, readiness: "ready",
  });
  const changed = vi.fn();
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <PlaybackControls state={state()}
    onRate={(selected) => { changed(selected); setState((old) => ({ ...old, rate: { ...old.rate, selected } })); }}
    onMuted={(muted) => setState((old) => ({ ...old, muted }))}
    onVolume={(volume) => setState((old) => ({ ...old, volume }))} />, host);
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
    expect(state()).toMatchObject({ volume: 0.37, muted: true });
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
