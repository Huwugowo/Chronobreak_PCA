// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createSignal, Show } from "solid-js";
import { render } from "solid-js/web";
import ReplayTimeline from "./ReplayTimeline";
import { REPLAY_TICKS_PER_SECOND, type ReplayTick } from "../replayTime";
import type { TimelineViewport } from "../replayTimelineGeometry";

const tick = (seconds: number) => seconds * REPLAY_TICKS_PER_SECOND as ReplayTick;
const disconnect = vi.fn();
let dispose: (() => void) | undefined;
beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 1000, height: 40, left: 0, right: 1000, top: 0, bottom: 40, x: 0, y: 0, toJSON: () => ({}) });
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect = disconnect; });
});
afterEach(() => { dispose?.(); dispose = undefined; vi.restoreAllMocks(); vi.unstubAllGlobals(); disconnect.mockClear(); document.body.replaceChildren(); });
const pointer = (element: Element, type: string, x: number, id = 7) => {
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, button: 0 });
  Object.defineProperty(event, "pointerId", { value: id });
  element.dispatchEvent(event);
};
const capture = (element: Element) => {
  const held = new Set<number>();
  const release = vi.fn((id: number) => held.delete(id));
  Object.assign(element, { setPointerCapture: (id: number) => held.add(id),
    hasPointerCapture: (id: number) => held.has(id), releasePointerCapture: release });
  return { held, release };
};
const mount = () => {
  const host = document.createElement("div"); document.body.append(host);
  const [visible, setVisible] = createSignal(true);
  const [viewport, setViewport] = createSignal<TimelineViewport>({ start: tick(0), end: tick(100) });
  const seek = vi.fn(); const start = vi.fn(); const preview = vi.fn(); const finish = vi.fn();
  dispose = render(() => <Show when={visible()}><ReplayTimeline duration={tick(100)} playhead={tick(30)}
    viewport={viewport()} onViewportChange={setViewport} events={[]} clip={{ start: tick(10), end: tick(50) }}
    onSeek={seek} onEventSelect={() => {}} onClipEndpointEditStart={start} onClipEndpointPreview={preview}
    onClipEndpointEditFinish={finish} onClipEndpointKeyDown={() => {}} onClipEndpointKeyUp={() => {}}
    onClipEndpointBlur={() => {}} /></Show>, host);
  return { host, viewport, seek, start, preview, finish, setVisible,
    rail: host.querySelector<HTMLElement>('[data-testid="replay-timeline"]')!,
    handle: host.querySelector<HTMLButtonElement>('[aria-label="Clip starts at 0:10"]')! };
};

it("seeks, zooms and pans the actual rail using the shared viewport before details exist", () => {
  const h = mount(); const c = capture(h.rail);
  pointer(h.rail, "pointerdown", 400);
  expect(h.seek).toHaveBeenLastCalledWith(tick(40));
  pointer(h.rail, "pointermove", 600, 99);
  expect(h.seek).toHaveBeenCalledTimes(1);
  pointer(h.rail, "pointerup", 500);
  expect(h.seek).toHaveBeenLastCalledWith(tick(50)); expect(c.held.size).toBe(0);
  h.rail.dispatchEvent(new KeyboardEvent("keydown", { key: "+", bubbles: true }));
  expect(h.viewport().end - h.viewport().start).toBe(tick(80));
  const zoomed = h.viewport();
  h.rail.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", shiftKey: true, bubbles: true }));
  expect(h.viewport().start).toBeGreaterThan(zoomed.start);
  expect(h.viewport().end - h.viewport().start).toBe(tick(80));
  h.rail.dispatchEvent(new KeyboardEvent("keydown", { key: "0", bubbles: true }));
  expect(h.viewport()).toEqual({ start: tick(0), end: tick(100) });
});

it.each(["pointerup", "pointercancel", "lostpointercapture", "dispose"])("releases clip capture and listeners on %s", ending => {
  const h = mount(); const c = capture(h.handle);
  const remove = vi.spyOn(h.handle, "removeEventListener");
  pointer(h.handle, "pointerdown", 100);
  expect(h.start).toHaveBeenCalledWith("start"); expect(c.held.has(7)).toBe(true);
  pointer(h.handle, "pointermove", 300);
  expect(h.preview).toHaveBeenLastCalledWith("start", tick(30));
  pointer(h.handle, "pointermove", 900, 99);
  expect(h.preview).toHaveBeenCalledTimes(1);
  if (ending === "dispose") h.setVisible(false);
  else pointer(h.handle, ending, 400);
  expect(h.finish).toHaveBeenCalledTimes(1); expect(c.held.size).toBe(0);
  expect(remove.mock.calls.map(([type]) => type)).toEqual(expect.arrayContaining([
    "pointermove", "pointerup", "pointercancel", "lostpointercapture"]));
  const count = h.preview.mock.calls.length;
  pointer(h.handle, "pointermove", 800); pointer(h.handle, "pointerup", 800);
  expect(h.preview).toHaveBeenCalledTimes(count); expect(h.finish).toHaveBeenCalledTimes(1);
  h.setVisible(false); expect(disconnect).toHaveBeenCalledTimes(1);
});

it("releases seek capture and ResizeObserver when the layout rail is disposed", () => {
  const h = mount(); const c = capture(h.rail);
  pointer(h.rail, "pointerdown", 250); h.setVisible(false);
  expect(c.release).toHaveBeenCalledWith(7); expect(disconnect).toHaveBeenCalledTimes(1);
  pointer(h.rail, "pointermove", 900); expect(h.seek).toHaveBeenCalledTimes(1);
});
