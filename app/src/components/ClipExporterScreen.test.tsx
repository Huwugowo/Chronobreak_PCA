import { createVerticalPreviewGpu } from "../verticalPreviewGpu";
vi.mock("../verticalPreviewGpu", () => ({ createVerticalPreviewGpu: vi.fn() }));
// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import ClipExporterScreen from "./ClipExporterScreen";
import {
  chooseMusicFile,
  loadBuiltInMusic,
  loadReplayDescriptor,
  prepareImportedMusicPreview,
  releaseImportedMusicPreview,
} from "../api";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "../replayTime";

vi.mock("../api", () => ({
  chooseMusicFile: vi.fn(),
  exportClip: vi.fn(),
  loadBuiltInMusic: vi.fn(),
  loadReplayDescriptor: vi.fn(),
  prepareImportedMusicPreview: vi.fn(),
  releaseImportedMusicPreview: vi.fn(),
}));

afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); document.body.replaceChildren(); });

  const descriptor = {
    game: { media_id: parseMediaId("11111111-2222-4333-8444-555555555555"), timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-14T00:00:00Z",
      kills: 1, deaths: 2, assists: 3, duration_ms: 240_000, summoner_spells: [], keystone_id: null,
      items: [], participants: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true, league_result: null },
    video_url: "http://127.0.0.1:123/games/1/video.mp4",
    media_timeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
      video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
        frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14_400 as FrameBoundary,
        onePastLastPts: 14_400n, replayEnd: 11_520_000_000 as ReplayTick }, audio: { present: true } },
  };
it("retries the same imported file after preparation fails and releases the successful preview", async () => {
  const path = "C:\\Music\\same.wav";
  const preview = { url: "http://127.0.0.1:123/music-preview/retry", token: "retry" };
  vi.mocked(loadReplayDescriptor).mockResolvedValue({snapshot_token:"test-snapshot",game_timestamp:"1",media_timeline:descriptor.media_timeline,video_url:descriptor.video_url});
  vi.mocked(loadBuiltInMusic).mockResolvedValue([]);
  vi.mocked(chooseMusicFile).mockResolvedValue(path);
  vi.mocked(prepareImportedMusicPreview)
    .mockRejectedValueOnce(new Error("Temporary preview failure"))
    .mockResolvedValueOnce(preview);
  vi.mocked(releaseImportedMusicPreview).mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const host = document.createElement("div");
  document.body.append(host);
  const dispose = render(() => <ClipExporterScreen admissionError={null}
    draft={{ gameTimestamp: "1", mediaId: descriptor.media_timeline.mediaId,
      startFrame: 0 as FrameBoundary, endFrameExclusive: 60 as FrameBoundary }}
    outputPath="C:\\output" snapshotToken="test-snapshot" snapshotOrigin={{ root: "C:\\output", rootEpoch: 1, request: 1, token: "test-snapshot", navigation: 1 }} onBack={() => {}} onExported={() => {}}
    onOpenClips={() => {}} onOpenFolder={() => {}}
    readReplay={work => work("test-snapshot")}
  />, host);
  let audio: HTMLAudioElement | null = null;
  try {
    const browse = () => [...host.querySelectorAll("button")].find((button) => button.textContent === "BROWSE");
    await vi.waitFor(() => expect(browse()).toBeDefined());
    audio = host.querySelector("audio");
    expect(audio).not.toBeNull();
    browse()!.click();
    await vi.waitFor(() => expect(host.textContent).toContain("Temporary preview failure"));
    expect(prepareImportedMusicPreview).toHaveBeenCalledExactlyOnceWith(path);
    expect(audio!.hasAttribute("src")).toBe(false);

    // Cancelling the picker preserves the failed selection without retrying it.
    vi.mocked(chooseMusicFile).mockResolvedValueOnce(null);
    browse()!.click();
    await vi.waitFor(() => expect(chooseMusicFile).toHaveBeenCalledTimes(2));
    expect(prepareImportedMusicPreview).toHaveBeenCalledTimes(1);
    expect(host.textContent).toContain("Temporary preview failure");

    browse()!.click();
    await vi.waitFor(() => expect(prepareImportedMusicPreview).toHaveBeenCalledTimes(2));
    expect(prepareImportedMusicPreview).toHaveBeenNthCalledWith(2, path);
    await vi.waitFor(() => expect(audio!.getAttribute("src")).toBe(preview.url));
    expect(host.textContent).not.toContain("Temporary preview failure");
    expect(releaseImportedMusicPreview).not.toHaveBeenCalled();
  } finally { dispose(); }
  await vi.waitFor(() => expect(releaseImportedMusicPreview).toHaveBeenCalledExactlyOnceWith(preview.token));
  expect(audio!.hasAttribute("src")).toBe(false);
});

it("uses one source in every preset and admits only settled paused frames to the backdrop", async () => {
  vi.mocked(loadReplayDescriptor).mockResolvedValue({snapshot_token:"test-snapshot",game_timestamp:"1",media_timeline:descriptor.media_timeline,video_url:descriptor.video_url});
  vi.mocked(loadBuiltInMusic).mockResolvedValue([]);
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "readyState", "get").mockReturnValue(2);
  vi.spyOn(HTMLVideoElement.prototype, "videoWidth", "get").mockReturnValue(1920);
  vi.spyOn(HTMLVideoElement.prototype, "videoHeight", "get").mockReturnValue(1080);
  const drawImage = vi.fn(), clearRect = vi.fn();
  vi.mocked(createVerticalPreviewGpu).mockReturnValue({draw:drawImage,clear:clearRect,dispose:vi.fn()});
  vi.spyOn(HTMLCanvasElement.prototype, "clientWidth", "get").mockReturnValue(276);
  vi.spyOn(HTMLCanvasElement.prototype, "clientHeight", "get").mockReturnValue(450);
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  const previous = Object.getOwnPropertyDescriptor(HTMLVideoElement.prototype, "requestVideoFrameCallback");
  Object.defineProperty(HTMLVideoElement.prototype, "requestVideoFrameCallback", { configurable: true, value: vi.fn(() => 1) });
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ClipExporterScreen admissionError={null}
    draft={{gameTimestamp:"1",mediaId:descriptor.media_timeline.mediaId,startFrame:60 as FrameBoundary,endFrameExclusive:300 as FrameBoundary}}
    outputPath="C:\\output" snapshotToken="test-snapshot" snapshotOrigin={{root:"C:\\output",rootEpoch:1,request:1,token:"test-snapshot",navigation:1}}
    onBack={() => {}} onExported={() => {}} onOpenClips={() => {}} onOpenFolder={() => {}} readReplay={work=>work("test-snapshot")} />,host);
  try {
    await vi.waitFor(() => expect(host.querySelector("video")).not.toBeNull());
    const video = host.querySelector("video")!;
    let nativeTime = 0, nativeSeeking = false, failSeek = false;
    Object.defineProperty(video, "currentTime", { configurable: true, get: () => nativeTime, set: (value: number) => {
      if (failSeek) throw new Error("native seek failed");
      if (value !== nativeTime) { nativeTime = value; nativeSeeking = true; }
    }});
    Object.defineProperty(video, "seeking", { configurable: true, get: () => nativeSeeking });
    const preset = (name: string) => [...host.querySelectorAll('div[role="group"] > div')].find(e=>e.textContent?.includes(name))!.querySelector("button")!.click();
    preset("TikTok");
    expect(host.querySelectorAll("video")).toHaveLength(1);
    expect(host.querySelector("video")).toBe(video);
    expect(host.querySelector("canvas")).not.toBeNull();
    video.dispatchEvent(new Event("loadedmetadata"));
    drawImage.mockClear(); video.dispatchEvent(new Event("loadeddata"));
    expect(drawImage).not.toHaveBeenCalled();
    nativeSeeking = false; video.dispatchEvent(new Event("seeked")); expect(drawImage).toHaveBeenCalled();
    const slider = host.querySelector('input[aria-label="Clip preview position"]') as HTMLInputElement;
    slider.value = "2000"; slider.dispatchEvent(new Event("input", {bubbles:true}));
    drawImage.mockClear(); video.dispatchEvent(new Event("canplay")); expect(drawImage).not.toHaveBeenCalled();
    nativeSeeking = false; video.dispatchEvent(new Event("seeked")); expect(video.currentTime).toBe(2); expect(drawImage).toHaveBeenCalled();
    await new Promise(resolve => setTimeout(resolve, 110));
    slider.value = "2000"; slider.dispatchEvent(new Event("input", {bubbles:true}));
    drawImage.mockClear(); video.dispatchEvent(new Event("canplay")); expect(drawImage).toHaveBeenCalled();
    await new Promise(resolve => setTimeout(resolve, 110));
    failSeek = true; slider.value = "3000"; slider.dispatchEvent(new Event("input", {bubbles:true}));
    expect(host.textContent).toContain("could not seek");
    preset("Adaptive quality"); expect(host.querySelectorAll("video")).toHaveLength(1); expect(host.querySelector("canvas")).toBeNull();
    drawImage.mockClear(); video.dispatchEvent(new Event("seeked")); expect(drawImage).not.toHaveBeenCalled();
    preset("1080p publish"); expect(host.querySelectorAll("video")).toHaveLength(1); expect(host.querySelector("canvas")).toBeNull();
  } finally {
    dispose();
    if (previous) Object.defineProperty(HTMLVideoElement.prototype, "requestVideoFrameCallback", previous);
    else delete (HTMLVideoElement.prototype as unknown as Record<string, unknown>).requestVideoFrameCallback;
  }
});

it("sets anonymous mode before initial and replacement video sources", async () => {
  const assignments: Array<{src:string,mode:string|null}> = [];
  const setAttribute = Element.prototype.setAttribute;
  vi.spyOn(Element.prototype,"setAttribute").mockImplementation(function(this:Element,name:string,value:string) {
    if(this instanceof HTMLVideoElement && name === "src") assignments.push({src:value,mode:this.crossOrigin});
    setAttribute.call(this,name,value);
  });
  const native = Object.getOwnPropertyDescriptor(HTMLMediaElement.prototype,"src")!;
  vi.spyOn(HTMLMediaElement.prototype,"src","set").mockImplementation(function(this:HTMLMediaElement,value:string) {
    if(this instanceof HTMLVideoElement) assignments.push({src:value,mode:this.crossOrigin});
    native.set!.call(this,value);
  });
  vi.spyOn(HTMLMediaElement.prototype,"pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype,"load").mockImplementation(() => {});
  vi.mocked(loadBuiltInMusic).mockResolvedValue([]);
  vi.mocked(loadReplayDescriptor).mockImplementation(async id => ({snapshot_token:"test-snapshot",game_timestamp:id,
    media_timeline:descriptor.media_timeline,video_url:`http://127.0.0.1:123/games/${id}/video.mp4`}));
  const [id,setId]=createSignal("1");
  const host=document.createElement("div");document.body.append(host);
  const dispose=render(() => <ClipExporterScreen admissionError={null}
    draft={{gameTimestamp:id(),mediaId:descriptor.media_timeline.mediaId,startFrame:0 as FrameBoundary,endFrameExclusive:60 as FrameBoundary}}
    outputPath="output" snapshotToken="test-snapshot" snapshotOrigin={{root:"output",rootEpoch:1,request:1,token:"test-snapshot",navigation:1}}
    onBack={() => {}} onExported={() => {}} onOpenClips={() => {}} onOpenFolder={() => {}} readReplay={work=>work("test-snapshot")} />,host);
  try {
    await vi.waitFor(() => expect(assignments.some(a=>a.src.includes("/games/1/"))).toBe(true));
    setId("2");
    await vi.waitFor(() => expect(assignments.some(a=>a.src.includes("/games/2/"))).toBe(true));
    expect(assignments.every(a=>a.mode === "anonymous")).toBe(true);
  } finally {dispose();}
});
