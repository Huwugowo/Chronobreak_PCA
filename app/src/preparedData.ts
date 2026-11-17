import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

function subscribe<T>(name: string, callback: (payload: T) => void): () => void {
  let closed = false;
  let release: (() => void) | undefined;
  if (isTauri()) void listen<T>(name, event => {
    if (!closed) callback(event.payload);
  }).then(unsubscribe => { if (closed) unsubscribe(); else release = unsubscribe; }).catch(() => {});
  return () => { closed = true; release?.(); };
}

/** Subscribe without tying background preparation to the lifetime of a view. */
export const onPreparedData = (callback: (media: string[]) => void) =>
  subscribe<string[]>("league-data-prepared", media => { if (Array.isArray(media)) callback(media); });

export const onLibraryCatalogChanged = (callback: (token: string) => void) =>
  subscribe<string>("library-catalog-changed", token => { if (typeof token === "string") callback(token); });
