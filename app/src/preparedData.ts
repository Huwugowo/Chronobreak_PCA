import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Subscribe without tying background preparation to the lifetime of a view. */
export function onPreparedData(callback: (media: string[]) => void): () => void {
  let closed = false;
  let release: (() => void) | undefined;
  if (isTauri()) void listen<string[]>("league-data-prepared", event => {
    if (!closed && Array.isArray(event.payload)) callback(event.payload);
  }).then(unsubscribe => { if (closed) unsubscribe(); else release = unsubscribe; }).catch(() => {});
  return () => { closed = true; release?.(); };
}
