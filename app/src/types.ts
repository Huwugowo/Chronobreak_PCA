export type GameSummary = {
  timestamp: string;
  champion: string;
  game_mode: string;
  duration_ms: number;
  recorded_at: string;
  kills: number;
  deaths: number;
  assists: number;
  win: boolean | null;
  win_method: "matchv5" | "derived" | "unknown";
  saved: boolean;
  incomplete: boolean;
  matchv5_fetched: boolean;
  video_size_bytes: number;
  video_available: boolean;
};

export type ClipSummary = {
  filename: string;
  game_timestamp: string;
  clip_timestamp: string;
  duration_ms: number;
  file_size_bytes: number;
  thumbnail_path: string | null;
  thumbnail_url: string | null;
  video_url: string;
  source_champion: string | null;
  source_date: string | null;
};

export type EventMarker = {
  video_time_ms: number;
  event_type: string;
};

export type PlaybackProbe = {
  game: GameSummary;
  video_url: string;
  markers: EventMarker[];
};

export type StorageUsage = {
  games_bytes: number;
  clips_bytes: number;
  game_count: number;
  clip_count: number;
};

export type AppSettings = {
  output_path: string;
  auto_delete_days: number;
  hevc_playback_supported: boolean;
};

export type SettingsUpdate = Pick<AppSettings, "output_path" | "auto_delete_days">;

export type AutoDeleteResult = {
  deleted_count: number;
};

export type HevcProbeStatus = {
  tested: boolean;
  supported: boolean;
  probe_url: string;
};

export type DdragonStatus = {
  state: "loading" | "ready" | "offline";
  version: string | null;
  item_count: number;
  champion_count: number;
  cache_directory: string;
  error: string | null;
};

export type ServerMetrics = {
  requests: number;
  range_requests: number;
  response_bytes: number;
};

export type LibraryTab = "games" | "clips";

export type ReturnNavigationState =
  | { screen: "library"; tab: LibraryTab }
  | { screen: "viewer"; gameTimestamp: string };

export type NavigationState =
  | ReturnNavigationState
  | { screen: "settings"; returnTo: ReturnNavigationState };
