import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { MediaTimelineV2 } from "./replayTime";
import type { PlaybackSnapshot } from "./playbackController";

export type DecoderSnapshot = Readonly<{
  owner: string;
  session_token: string | null;
  media_id: string | null;
  generation: number | null;
  enabled: boolean;
  status: "hardware-confirmed" | "software-fallback" | "unknown" | "unsupported";
  reason: string;
  expected_codec: string | null;
  expected_profile: string | null;
  expected_pixel_format: string | null;
  decoder_name: string | null;
  platform_decoder: boolean | null;
  runtime: string | null;
  protocol: string | null;
  source: string;
  associated: boolean;
  transitions: readonly string[];
}>;

/** Tauri bridge for the one viewer lifetime. Playback itself never awaits this in normal use. */
export function createPlaybackDiagnostics(onChange: (value: DecoderSnapshot) => void) {
  const owner = crypto.randomUUID();
  let disposed = false;
  let remove: UnlistenFn | undefined;
  let current: PlaybackSnapshot | undefined;
  let boundToken: string | null = null;
  const unavailable = (reason: string): DecoderSnapshot => ({
    owner, session_token: current?.sessionToken ?? null, media_id: current?.mediaId ?? null,
    generation: current?.generation ?? null, enabled: false, status: "unknown", reason,
    expected_codec: null, expected_profile: null, expected_pixel_format: null, decoder_name: null,
    platform_decoder: null, runtime: null, protocol: null, source: "WebView2 CDP Media",
    associated: false, transitions: [],
  });
  const accept = (value: DecoderSnapshot) => {
    if (!disposed && value.owner === owner && value.session_token === current?.sessionToken
      && value.media_id === current?.mediaId && value.generation === current?.generation) onChange(value);
  };
  const ready = (async () => {
    if (!isTauri()) return false;
    try {
      const unlisten = await listen<DecoderSnapshot>("playback-decoder", (event) => accept(event.payload));
      if (disposed) { unlisten(); return false; }
      remove = unlisten;
      const value = await invoke<DecoderSnapshot>("open_playback_diagnostics", { owner });
      if (disposed) { await invoke("close_playback_diagnostics", { owner }); return false; }
      return value.enabled;
    } catch { return false; }
  })();
  return {
    ready,
    bind(value: PlaybackSnapshot, timeline: MediaTimelineV2) {
      if (disposed || !value.sessionToken || value.sessionToken === boundToken) return;
      boundToken = value.sessionToken; current = value;
      onChange(unavailable("acquiring decoder evidence for current media"));
      void ready.then(async (enabled) => {
        if (disposed || current?.sessionToken !== value.sessionToken) return;
        if (!enabled) { onChange(unavailable("native decoder diagnostics unavailable")); return; }
        try {
          accept(await invoke<DecoderSnapshot>("bind_playback_diagnostics", { owner, binding: {
            session_token: value.sessionToken, media_id: value.mediaId, generation: value.generation,
            url: value.sourceUrl, codec: timeline.video.codec, profile: timeline.video.profile,
          } }));
        } catch {
          if (!disposed && current?.sessionToken === value.sessionToken) onChange(unavailable("decoder association unavailable"));
        }
      });
    },
    dispose() {
      if (disposed) return;
      disposed = true; remove?.(); remove = undefined; current = undefined;
      if (isTauri()) void invoke("close_playback_diagnostics", { owner }).catch(() => undefined);
    },
  };
}
