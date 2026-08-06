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

export type ServerMetrics = {
  requests: number;
  range_requests: number;
  response_bytes: number;
};
