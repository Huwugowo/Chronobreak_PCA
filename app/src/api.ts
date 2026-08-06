import { invoke, isTauri } from "@tauri-apps/api/core";
import type { EventMarker, PlaybackProbe, ServerMetrics } from "./types";

const mockMarkers = (): EventMarker[] =>
  Array.from({ length: 86 }, (_, index) => ({
    video_time_ms: 42_000 + index * 23_600,
    event_type: index % 5 === 0 ? "ChampionKill" : "ObjectiveEvent",
  }));

const browserPreview = (): PlaybackProbe => ({
  game: {
    timestamp: "1786005616",
    champion: "Syndra",
    game_mode: "CLASSIC",
    duration_ms: 1_788_316,
    recorded_at: "2026-08-06T08:40:16Z",
    kills: 8,
    deaths: 4,
    assists: 11,
    win: null,
    win_method: "unknown",
    saved: false,
    incomplete: false,
    matchv5_fetched: false,
    video_size_bytes: 4_294_967_296,
  },
  video_url: "",
  markers: mockMarkers(),
});

export const loadPlaybackProbe = async (): Promise<PlaybackProbe | null> => {
  if (!isTauri()) {
    return browserPreview();
  }
  return invoke<PlaybackProbe | null>("get_playback_probe");
};

export const loadServerMetrics = async (): Promise<ServerMetrics> => {
  if (!isTauri()) {
    return { requests: 0, range_requests: 0, response_bytes: 0 };
  }
  return invoke<ServerMetrics>("get_playback_server_metrics");
};

export const isDesktopRuntime = isTauri;
