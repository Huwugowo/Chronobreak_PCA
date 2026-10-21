// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createVerticalPreviewGpu } from "./verticalPreviewGpu";
vi.mock("./verticalPreviewGpu", () => ({ createVerticalPreviewGpu: vi.fn() }));
import { createVerticalPreviewBackdrop } from "./verticalPreviewBackdrop";

beforeEach(() => {
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function fixture() {
  const video = document.createElement("video"); video.src = "http://localhost/source.mp4";
  for (const [key, value] of Object.entries({ readyState: 2, videoWidth: 1920, videoHeight: 1080, paused: true, seeking: false, ended: false, currentSrc: video.src })) {
    Object.defineProperty(video, key, { configurable: true, writable: true, value });
  }
  const canvas = document.createElement("canvas");
  const context = { drawImage: vi.fn(), clearRect: vi.fn() };
  Object.defineProperties(canvas, { clientWidth: { value: 276 }, clientHeight: { value: 450 } });
  vi.mocked(createVerticalPreviewGpu).mockReturnValue({ draw: context.drawImage, clear: context.clearRect, dispose: vi.fn() });
  const pending = new Map<number, VideoFrameRequestCallback>();
  const all = new Map<number, VideoFrameRequestCallback>();
  let id = 0;
  video.requestVideoFrameCallback = vi.fn(cb => { pending.set(++id, cb); all.set(id, cb); return id; });
  video.cancelVideoFrameCallback = vi.fn(id => { pending.delete(id); });
  const failed = vi.fn();
  const owner = createVerticalPreviewBackdrop(video, canvas, failed);
  const state = (changes: Record<string, unknown>) => {
    for (const [key, value] of Object.entries(changes)) Object.defineProperty(video, key, { configurable: true, writable: true, value });
  };
  const event = (name: string) => video.dispatchEvent(new Event(name));
  const frame = () => { const [key, cb] = [...pending][0]; pending.delete(key); cb(0, {} as VideoFrameCallbackMetadata); };
  return { video, canvas, context, owner, pending, all, failed, state, event, frame };
}

it("draws the initial decoded frame with the actual CSS geometry", () => {
  const f = fixture(); f.owner.setSeekPending(false);
  expect(f.context.drawImage).toHaveBeenCalledWith(f.video, 276, 450);
  expect(f.pending.size).toBe(0);
  f.owner.refresh(); expect(f.context.drawImage).toHaveBeenCalledTimes(2);
  f.owner.dispose();
});

it("owns one callback during playback and cancels late callbacks on pause/disposal", () => {
  const f = fixture(); f.state({ paused: false }); f.event("playing");
  f.owner.refresh(); expect(f.pending.size).toBe(1);
  const late = [...f.all.values()][0];
  f.frame(); expect(f.pending.size).toBe(1);
  f.state({ paused: true }); f.event("pause"); expect(f.pending.size).toBe(0);
  const count = f.context.drawImage.mock.calls.length;
  late(0, {} as VideoFrameCallbackMetadata); expect(f.context.drawImage).toHaveBeenCalledTimes(count);
  f.owner.dispose(); f.owner.dispose(); f.event("playing"); f.owner.refresh();
  expect(f.pending.size).toBe(0); expect(f.context.drawImage).toHaveBeenCalledTimes(count);
});

it("does not draw pre-seek data during metadata/queued seeks and draws only the final settled target", () => {
  const f = fixture(); f.state({ readyState: 1 }); f.owner.setSeekPending(true);
  f.event("loadeddata"); expect(f.context.drawImage).not.toHaveBeenCalled();
  f.state({ readyState: 2, seeking: true }); f.event("canplay");
  f.state({ seeking: false }); f.event("seeked"); // A newer coalesced request is still pending.
  expect(f.context.drawImage).not.toHaveBeenCalled();
  f.state({ seeking: true }); f.owner.setSeekPending(false);
  expect(f.context.drawImage).not.toHaveBeenCalled();
  f.state({ seeking: false }); f.event("seeked");
  expect(f.context.drawImage).toHaveBeenCalledTimes(1);
  f.owner.dispose();
});

it("suspends hidden playback and refreshes/rearms only on visibility return", () => {
  const f = fixture(); f.state({ paused: false }); f.owner.refresh();
  vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  document.dispatchEvent(new Event("visibilitychange")); expect(f.pending.size).toBe(0);
  const count = f.context.drawImage.mock.calls.length;
  f.owner.refresh(); expect(f.context.drawImage).toHaveBeenCalledTimes(count);
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  document.dispatchEvent(new Event("visibilitychange")); expect(f.pending.size).toBe(1);
  expect(f.context.drawImage).toHaveBeenCalledTimes(count + 1);
  f.owner.dispose();
});

it("clears source unloads, rejects a replaced source and its queued callback", () => {
  const f = fixture(); f.state({ paused: false }); f.owner.refresh();
  const late = [...f.all.values()][0]; const count = f.context.drawImage.mock.calls.length;
  f.state({ readyState: 0, currentSrc: "" }); f.event("emptied");
  expect(f.context.clearRect).toHaveBeenCalled(); expect(f.pending.size).toBe(0);
  videoSourceChange();
  function videoSourceChange() { f.video.src = "http://localhost/new.mp4"; f.state({ currentSrc: f.video.src, readyState: 2 }); }
  late(0, {} as VideoFrameCallbackMetadata); f.event("loadeddata");
  expect(f.context.drawImage).toHaveBeenCalledTimes(count); expect(f.pending.size).toBe(0);
  f.owner.dispose();
});

it("reports draw failure once and releases all scheduling/listeners", () => {
  const f = fixture(); f.state({ paused: false });
  f.context.drawImage.mockImplementation(() => { throw new Error("draw failed"); });
  f.owner.refresh(); f.owner.refresh(); f.event("playing");
  expect(f.failed).toHaveBeenCalledTimes(1); expect(f.pending.size).toBe(0);
});

it("reports unavailable contexts without opening another source", () => {
  const video = document.createElement("video"), canvas = document.createElement("canvas");
  vi.mocked(createVerticalPreviewGpu).mockImplementation(() => { throw new Error("unavailable"); });
  const failed = vi.fn(); const owner = createVerticalPreviewBackdrop(video, canvas, failed);
  owner.refresh(); owner.dispose(); expect(failed).toHaveBeenCalledTimes(1);
  expect(video.hasAttribute("src")).toBe(false);
});

it("treats context loss as terminal and ignores subsequent events", () => {
  const f = fixture(); f.state({ paused: false }); f.owner.refresh();
  f.canvas.dispatchEvent(new Event("webglcontextlost"));
  f.owner.refresh(); f.event("playing");
  expect(f.failed).toHaveBeenCalledTimes(1); expect(f.pending.size).toBe(0);
});

it("releases the renderer when frame callback registration fails", () => {
  const f = fixture(); f.state({paused:false});
  const results = vi.mocked(createVerticalPreviewGpu).mock.results;
  const renderer = results[results.length - 1].value;
  vi.mocked(f.video.requestVideoFrameCallback).mockImplementation(() => {throw new Error("callback failed");});
  f.owner.refresh(); const count=f.context.drawImage.mock.calls.length;
  f.owner.refresh(); f.event("playing");
  expect(f.failed).toHaveBeenCalledTimes(1); expect(renderer.dispose).toHaveBeenCalledTimes(1);
  expect(f.context.drawImage).toHaveBeenCalledTimes(count); expect(f.pending.size).toBe(0);
});
