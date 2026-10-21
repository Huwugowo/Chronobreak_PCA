import { createVerticalPreviewGpu } from "./verticalPreviewGpu";

/** A display-only copy of the primary preview's decoded frame. Owns no media. */
export function createVerticalPreviewBackdrop(
  video: HTMLVideoElement,
  canvas: HTMLCanvasElement,
  failed: () => void,
  source = video.src,
) {
  let renderer: ReturnType<typeof createVerticalPreviewGpu> | undefined;
  let observer: ResizeObserver | undefined;
  let width = canvas.clientWidth, height = canvas.clientHeight;
  let disposed = false;
  let seekPending = false;
  let callback: number | undefined;
  let generation = 0;
  const listeners: Array<[EventTarget, string, EventListener]> = [];

  const cancel = () => {
    generation++;
    if (callback !== undefined) video.cancelVideoFrameCallback(callback);
    callback = undefined;
  };
  const dispose = () => {
    if (disposed) return;
    disposed = true;
    cancel();
    for (const [target, event, listener] of listeners) target.removeEventListener(event, listener);
    listeners.length = 0;
    observer?.disconnect();
    renderer?.clear();
    renderer?.dispose();
    renderer = undefined;
  };
  const fail = () => { if (disposed) return; dispose(); failed(); };
  const sourceMatches = () => video.src === source && (!video.currentSrc || video.currentSrc === source);
  const draw = () => {
    if (disposed || seekPending || video.seeking || document.hidden ||
      video.readyState < 2 || video.videoWidth <= 0 || video.videoHeight <= 0 || !sourceMatches()) return;
    try {
      if (width > 0 && height > 0) renderer!.draw(video, width, height);
    } catch { fail(); }
  };
  const schedule = () => {
    if (disposed || callback !== undefined || seekPending || video.seeking ||
      video.paused || video.ended || document.hidden || !sourceMatches()) return;
    const binding = generation;
    try { callback = video.requestVideoFrameCallback(() => {
      if (disposed || binding !== generation) return;
      callback = undefined;
      draw();
      schedule();
    }); } catch { fail(); }
  };
  const refresh = () => { draw(); schedule(); };
  const listen = (target: EventTarget, event: string, handler: () => void) => {
    target.addEventListener(event, handler);
    listeners.push([target, event, handler]);
  };
  const owner = {
    refresh,
    setSeekPending(pending: boolean) {
      seekPending = pending;
      if (pending) cancel();
      else refresh();
    },
    dispose,
  };
  try {
    if (typeof video.requestVideoFrameCallback !== "function") throw new Error("Frame callbacks unavailable");
    renderer = createVerticalPreviewGpu(canvas);
    observer = new ResizeObserver(entries => {
      if (disposed) return;
      width = entries[0].contentRect.width; height = entries[0].contentRect.height;
      refresh();
    });
    observer.observe(canvas);
  } catch { fail(); return owner; }

  listen(canvas, "webglcontextlost", fail);
  for (const event of ["loadeddata", "canplay", "seeked", "playing"]) listen(video, event, refresh);
  for (const event of ["pause", "ended"]) listen(video, event, () => { cancel(); draw(); });
  listen(video, "seeking", cancel);
  for (const event of ["loadstart", "emptied"]) listen(video, event, () => {
    cancel(); renderer?.clear();
  });
  listen(document, "visibilitychange", () => { cancel(); if (!document.hidden) refresh(); });
  // The caller admits initial/queued seeks before requesting the first refresh.
  return owner;
}
