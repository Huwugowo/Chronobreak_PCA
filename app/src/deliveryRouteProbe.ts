import { invoke } from "@tauri-apps/api/core";

type Target = { label: string; url: string; content_type: string; kind: "image" | "audio" | "video" };
type Probe = { imported_token: string; targets: Target[] };

const nativeLoad = (target: Target): Promise<boolean> => new Promise((resolve) => {
  const element = document.createElement(target.kind === "image" ? "img" : target.kind);
  let settled = false;
  const timer = window.setTimeout(() => finish(false), 10_000);
  const finish = (loaded: boolean) => {
    if (settled) return;
    settled = true;
    window.clearTimeout(timer);
    element.removeAttribute("src");
    if (element instanceof HTMLMediaElement) { element.pause(); element.load(); }
    resolve(loaded);
  };
  element.addEventListener(target.kind === "image" ? "load" : "loadedmetadata", () => finish(true), { once: true });
  element.addEventListener("error", () => finish(false), { once: true });
  if (element instanceof HTMLMediaElement) { element.preload = "metadata"; element.muted = true; }
  element.src = target.url;
});

/** Only labels and observations leave this function; capability URLs stay in memory. */
export const runDeliveryRouteProbe = async (): Promise<Record<string, unknown>[]> => {
  const probe = await invoke<Probe>("benchmark_delivery_probe", { releaseToken: null });
  const observations: Record<string, unknown>[] = [];
  try {
    for (const target of probe.targets) {
      const head = await fetch(target.url, { method: "HEAD" });
      const length = Number(head.headers.get("content-length"));
      if (head.status !== 200 || length < 16 || head.headers.get("content-type") !== target.content_type) throw new Error("header contract");
      const range = await fetch(target.url, { headers: { Range: "bytes=0-15" } });
      const bytes = await range.arrayBuffer();
      if (range.status !== 206 || bytes.byteLength !== 16 || range.headers.get("content-range") !== `bytes 0-15/${length}`) throw new Error("range contract");
      const nativeLoaded = await nativeLoad(target);
      if (!nativeLoaded && target.label !== "probe") throw new Error("native asset load");
      observations.push({ route: target.label, head_status: head.status, range_status: range.status,
        length, content_type: target.content_type, native_loaded: nativeLoaded });
    }
    return observations;
  } catch {
    // Browser network errors may contain URLs; never retain the original exception.
    throw new Error("Local delivery route verification failed");
  } finally {
    await invoke("benchmark_delivery_probe", { releaseToken: probe.imported_token });
  }
};
