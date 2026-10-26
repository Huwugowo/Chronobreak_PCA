// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { ensureHevcCapability } from "./api";

vi.mock("@tauri-apps/api/core", async original => ({
  ...await original<typeof import("@tauri-apps/api/core")>(), invoke: vi.fn(), isTauri: vi.fn(),
}));

let video: HTMLVideoElement;
let frame: VideoFrameRequestCallback | undefined;
let currentTime = 0;
let seekCompletes = true;

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_hevc_probe_status") return { tested: false, supported: false, probe_url: "http://localhost/probe.mp4" };
    if (command === "record_hevc_probe_result") return { tested: true, supported: (args as { supported: boolean }).supported, probe_url: "" };
    throw new Error(`Unexpected command ${command}`);
  });
  video = document.createElement("video");
  currentTime = 0;
  frame = undefined;
  seekCompletes = true;
  Object.defineProperties(video, {
    duration: { configurable: true, value: 2 },
    readyState: { configurable: true, value: HTMLMediaElement.HAVE_CURRENT_DATA },
    currentTime: { configurable: true, get: () => currentTime, set: (value: number) => {
      currentTime = value;
      if (seekCompletes) Promise.resolve().then(() => video.dispatchEvent(new Event("seeked")));
    } },
    requestVideoFrameCallback: { configurable: true, value: vi.fn((callback: VideoFrameRequestCallback) => { frame = callback; return 17; }) },
    cancelVideoFrameCallback: { configurable: true, value: vi.fn() },
  });
  vi.spyOn(video, "load").mockImplementation(() => {
    if (video.hasAttribute("src")) Promise.resolve().then(() => video.dispatchEvent(new Event("loadedmetadata")));
  });
  vi.spyOn(video, "play").mockResolvedValue(undefined);
  vi.spyOn(video, "pause").mockImplementation(() => {});
  vi.spyOn(video, "addEventListener");
  vi.spyOn(video, "removeEventListener");
  const create = document.createElement.bind(document);
  vi.spyOn(document, "createElement").mockImplementation((tag, options) => tag === "video" ? video : create(tag, options));
});

afterEach(() => { document.body.replaceChildren(); vi.restoreAllMocks(); vi.resetAllMocks(); vi.useRealTimers(); });

async function start() {
  const result = ensureHevcCapability();
  await vi.advanceTimersByTimeAsync(0);
  return { result };
}

async function verdict(result: Promise<unknown>, supported: boolean) {
  await expect(result).resolves.toMatchObject({ tested: true, supported });
  expect(invoke).toHaveBeenCalledWith("record_hevc_probe_result", { supported });
  expect(vi.mocked(invoke).mock.calls.filter(([name]) => name === "record_hevc_probe_result")).toHaveLength(1);
  expect(video.isConnected).toBe(false);
  expect(video.hasAttribute("src")).toBe(false);
  expect(video.pause).toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
  for (const [event, listener] of vi.mocked(video.addEventListener).mock.calls) {
    expect(video.removeEventListener).toHaveBeenCalledWith(event, listener);
  }
}

describe("HEVC presented-frame capability", () => {
  it("persists success only after a presented frame and releases its observation", async () => {
    const { result } = await start();
    expect(frame).toBeTypeOf("function");
    frame!(0, {} as VideoFrameCallbackMetadata);
    await verdict(result, true);
    expect(video.cancelVideoFrameCallback).toHaveBeenCalledWith(17);
  });

  it("fails closed when the clock and timeupdate advance without a frame", async () => {
    const { result } = await start();
    video.currentTime = 1.5;
    video.dispatchEvent(new Event("timeupdate"));
    await vi.advanceTimersByTimeAsync(2_000);
    await verdict(result, false);
    expect(video.cancelVideoFrameCallback).toHaveBeenCalledWith(17);
    frame!(0, {} as VideoFrameCallbackMetadata);
    expect(vi.mocked(invoke).mock.calls.filter(([name]) => name === "record_hevc_probe_result")).toHaveLength(1);
  });

  it("cannot establish support without the frame callback API", async () => {
    Object.defineProperty(video, "requestVideoFrameCallback", { value: undefined });
    const { result } = await start();
    video.dispatchEvent(new Event("timeupdate"));
    await vi.advanceTimersByTimeAsync(2_000);
    await verdict(result, false);
  });

  it("cleans up immediately when decoding fails during presentation", async () => {
    const { result } = await start();
    video.dispatchEvent(new Event("error"));
    await vi.advanceTimersByTimeAsync(0);
    expect(video.isConnected).toBe(false);
    await verdict(result, false);
    expect(video.cancelVideoFrameCallback).toHaveBeenCalledWith(17);
  });

  it("bounds a play promise that never settles", async () => {
    vi.mocked(video.play).mockImplementation(() => new Promise(() => {}));
    const { result } = await start();
    await vi.advanceTimersByTimeAsync(2_000);
    expect(video.isConnected).toBe(false);
    await verdict(result, false);
  });

  it("fails closed and releases resources if frame registration throws", async () => {
    vi.mocked(video.requestVideoFrameCallback).mockImplementation(() => { throw new Error("unavailable"); });
    const { result } = await start();
    await verdict(result, false);
  });

  it("fails closed if playback rejects", async () => {
    vi.mocked(video.play).mockRejectedValue(new Error("decode failed"));
    const { result } = await start();
    await verdict(result, false);
  });

  it("times out a seek without retaining listeners", async () => {
    seekCompletes = false;
    const { result } = await start();
    await vi.advanceTimersByTimeAsync(5_000);
    await verdict(result, false);
    expect(video.requestVideoFrameCallback).not.toHaveBeenCalled();
  });

  it("reuses only a backend-admitted current probe result", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ tested: true, supported: true, probe_url: "" });
    await expect(ensureHevcCapability()).resolves.toMatchObject({ tested: true, supported: true });
    expect(video.play).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
