export type GameSummary = {
  timestamp: string;
  champion: string;
  game_mode: string;
  duration_ms: number;
  recorded_at: string;
  kills: number;
  deaths: number;
  assists: number;
  summoner_spells: string[];
  keystone_id: number | null;
  items: GameItemSummary[];
  saved: boolean;
  incomplete: boolean;
  video_size_bytes: number;
  video_available: boolean;
};

export type GameItemSummary = {
  item_id: number;
  slot: number;
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

export type ClipDraft = {
  gameTimestamp: string;
  clipStartMs: number;
  clipEndMs: number;
};

export type ClipRange = {
  startMs: number;
  endMs: number;
};

export type ClipExportPreset = "discord" | "horizontal" | "vertical";

export type ClipMusicSource =
  | { kind: "none" }
  | { kind: "builtin"; filename: string }
  | { kind: "file"; path: string };

export type ClipExportRequest = {
  game_timestamp: string;
  clip_start_ms: number;
  clip_end_ms: number;
  preset: ClipExportPreset;
  vertical_focus: number;
  vertical_position: number;
  music: ClipMusicSource;
  game_audio_volume: number;
  music_volume: number;
};

export type ClipExportProgress = {
  stage: "encoding" | "thumbnail" | "complete";
  percent: number;
};

export type ClipExportResult = {
  filename: string;
  output_path: string;
  thumbnail_path: string;
  elapsed_ms: number;
  file_size_bytes: number;
};

export type BuiltInMusicTrack = {
  filename: string;
  display_name: string;
  mood: string;
  duration_s: number;
  preview_url: string;
};

export type ViewerEvent = {
  event_type: string;
  game_time_ms: number;
  video_time_ms: number;
  killer: string | null;
  victim: string | null;
  assisters: string[];
  dragon_type: string | null;
  kill_streak: number | null;
  acer: string | null;
  acing_team: string | null;
  turret: string | null;
  inhibitor: string | null;
  result: string | null;
  relation: "ally" | "enemy" | "neutral";
};

export type PlayerTimelinePoint = {
  game_time_ms: number;
  video_time_ms: number;
  cs: number;
  level: number;
};

export type KdaTimelinePoint = {
  video_time_ms: number;
  kills: number;
  deaths: number;
  assists: number;
};

export type ReplayParticipant = {
  summoner_name: string;
  champion: string;
  relation: "ally" | "enemy" | "neutral";
};

export type PlaybackProbe = {
  game: GameSummary;
  video_url: string;
  game_start_video_offset_ms: number;
  local_player_name: string | null;
  participants: ReplayParticipant[];
  player_timeline: PlayerTimelinePoint[];
  kda_timeline: KdaTimelinePoint[];
  events: ViewerEvent[];
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
  asset_base_url: string | null;
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
  | { screen: "viewer"; gameTimestamp: string; clipDraft?: ClipDraft }
  | { screen: "clip-export"; draft: ClipDraft };

export type NavigationState =
  | ReturnNavigationState
  | { screen: "settings"; returnTo: ReturnNavigationState };
