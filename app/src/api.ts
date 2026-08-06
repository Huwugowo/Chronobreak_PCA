import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppSettings,
  AutoDeleteResult,
  BuiltInMusicTrack,
  ClipExportProgress,
  ClipExportRequest,
  ClipExportResult,
  ClipSummary,
  DdragonStatus,
  GameSummary,
  GoldTimelinePoint,
  HevcProbeStatus,
  KdaTimelinePoint,
  PlaybackProbe,
  PlayerTimelinePoint,
  ReplayParticipant,
  ServerMetrics,
  SettingsUpdate,
  StorageUsage,
  ViewerEvent,
} from "./types";

const MOCK_VIDEO_OFFSET_MS = 42_369;

const mockEvent = (
  eventType: string,
  videoTimeMs: number,
  details: Partial<ViewerEvent> = {},
): ViewerEvent => ({
  event_type: eventType,
  game_time_ms: Math.max(0, videoTimeMs - MOCK_VIDEO_OFFSET_MS),
  video_time_ms: videoTimeMs,
  killer: null,
  victim: null,
  assisters: [],
  dragon_type: null,
  kill_streak: null,
  acer: null,
  acing_team: null,
  turret: null,
  inhibitor: null,
  result: null,
  relation: "neutral",
  ...details,
});

const mockEvents = (): ViewerEvent[] => {
  const generated = Array.from({ length: 48 }, (_, index) => {
    const videoTimeMs = 248_000 + index * 29_500;
    if (index % 11 === 4) {
      return mockEvent("DragonKill", videoTimeMs, {
        killer: index % 2 === 0 ? "SUPERSTAR" : "Enemy Jungler",
        assisters: ["Ally Jungler", "SUPERSTAR"],
        dragon_type: ["Air", "Fire", "Water", "Earth"][index % 4],
        relation: index % 2 === 0 ? "ally" : "enemy",
      });
    }
    if (index % 13 === 7) {
      return mockEvent("TurretKilled", videoTimeMs, {
        killer: index % 2 === 0 ? "SUPERSTAR" : "Enemy Carry",
        turret: "Turret_T2_C_03_A",
        relation: index % 2 === 0 ? "ally" : "enemy",
      });
    }
    if (index % 17 === 9) {
      return mockEvent("Multikill", videoTimeMs, {
        killer: "SUPERSTAR",
        kill_streak: 2,
        relation: "ally",
      });
    }
    const enemyKill = index % 3 === 0 && index % 5 !== 0;
    return mockEvent("ChampionKill", videoTimeMs, {
      killer: index % 5 === 0 ? "SUPERSTAR" : enemyKill ? "Enemy Carry" : "Ally Jungler",
      victim: index % 5 === 0 ? "Enemy Carry" : enemyKill ? "SUPERSTAR" : "Enemy Jungler",
      assisters: index % 4 === 0 ? ["SUPERSTAR"] : ["Ally Support"],
      relation: enemyKill ? "enemy" : "ally",
    });
  });

  return [
    mockEvent("GameStart", MOCK_VIDEO_OFFSET_MS),
    mockEvent("MinionsSpawning", 107_000),
    mockEvent("FirstBlood", 199_402, {
      killer: "SUPERSTAR",
      victim: "Enemy Carry",
      relation: "ally",
    }),
    mockEvent("ChampionKill", 199_402, {
      killer: "SUPERSTAR",
      victim: "Enemy Carry",
      assisters: ["Ally Jungler"],
      relation: "ally",
    }),
    ...generated,
    mockEvent("BaronKill", 1_501_000, {
      killer: "Ally Jungler",
      assisters: ["SUPERSTAR", "Ally Support"],
      relation: "ally",
    }),
    mockEvent("GameEnd", 1_770_000, { result: "Win" }),
  ].sort((left, right) => left.video_time_ms - right.video_time_ms);
};

const mockPlayerTimeline = (): PlayerTimelinePoint[] =>
  Array.from({ length: 174 }, (_, index) => {
    const gameTimeMs = 1_282 + index * 10_000;
    return {
      game_time_ms: gameTimeMs,
      video_time_ms: MOCK_VIDEO_OFFSET_MS + gameTimeMs,
      cs: Math.min(200, Math.floor(gameTimeMs / 8_450)),
      level: Math.min(15, 1 + Math.floor(gameTimeMs / 118_000)),
    };
  });

const mockKdaTimeline = (): KdaTimelinePoint[] => [
  { video_time_ms: 199_402, kills: 1, deaths: 0, assists: 0 },
  { video_time_ms: 396_000, kills: 2, deaths: 0, assists: 1 },
  { video_time_ms: 534_000, kills: 2, deaths: 1, assists: 2 },
  { video_time_ms: 711_000, kills: 3, deaths: 1, assists: 4 },
  { video_time_ms: 890_000, kills: 5, deaths: 2, assists: 5 },
  { video_time_ms: 1_114_000, kills: 6, deaths: 3, assists: 8 },
  { video_time_ms: 1_409_000, kills: 7, deaths: 4, assists: 10 },
  { video_time_ms: 1_698_000, kills: 8, deaths: 4, assists: 11 },
];

const mockParticipants = (): ReplayParticipant[] => [
  { summoner_name: "SUPERSTAR#VOID", champion: "Syndra", relation: "ally" },
  { summoner_name: "Ally Top#EUW", champion: "Ornn", relation: "ally" },
  { summoner_name: "Ally Jungler#EUW", champion: "Lee Sin", relation: "ally" },
  { summoner_name: "Ally Carry#EUW", champion: "Ezreal", relation: "ally" },
  { summoner_name: "Ally Support#EUW", champion: "Nami", relation: "ally" },
  { summoner_name: "Enemy Top#EUW", champion: "Fiora", relation: "enemy" },
  { summoner_name: "Enemy Jungler#EUW", champion: "Vi", relation: "enemy" },
  { summoner_name: "Enemy Mid#EUW", champion: "Viktor", relation: "enemy" },
  { summoner_name: "Enemy Carry#EUW", champion: "Jinx", relation: "enemy" },
  { summoner_name: "Enemy Support#EUW", champion: "Thresh", relation: "enemy" },
];

const mockGoldTimeline = (durationMs: number): GoldTimelinePoint[] =>
  Array.from(
    { length: Math.max(2, Math.floor((durationMs - MOCK_VIDEO_OFFSET_MS) / 60_000) + 1) },
    (_, index) => {
      const gameTimeMs = index * 60_000;
      const goldDiff = Math.round(Math.sin(index / 2.5) * 1_700 + (index - 10) * 55);
      const baseGold = 2_500 + index * 2_850;
      return {
        game_time_ms: gameTimeMs,
        video_time_ms: MOCK_VIDEO_OFFSET_MS + gameTimeMs,
        ally_gold: baseGold + Math.max(0, goldDiff),
        enemy_gold: baseGold + Math.max(0, -goldDiff),
        gold_diff: goldDiff,
      };
    },
  );

let mockGames: GameSummary[] = [
  {
    timestamp: "1786005616",
    champion: "Syndra",
    game_mode: "CLASSIC",
    duration_ms: 1_788_316,
    recorded_at: "2026-08-06T08:40:16Z",
    kills: 8,
    deaths: 4,
    assists: 11,
    win: true,
    win_method: "matchv5",
    saved: true,
    incomplete: false,
    matchv5_fetched: true,
    video_size_bytes: 4_446_112_713,
    video_available: true,
  },
  {
    timestamp: "1786004189",
    champion: "Viktor",
    game_mode: "CLASSIC",
    duration_ms: 2_142_000,
    recorded_at: "2026-08-05T19:18:00Z",
    kills: 5,
    deaths: 7,
    assists: 14,
    win: null,
    win_method: "unknown",
    saved: false,
    incomplete: false,
    matchv5_fetched: false,
    video_size_bytes: 2_866_000_000,
    video_available: true,
  },
  {
    timestamp: "1785999000",
    champion: "Jinx",
    game_mode: "ARAM",
    duration_ms: 1_327_000,
    recorded_at: "2026-08-04T21:52:00Z",
    kills: 13,
    deaths: 9,
    assists: 22,
    win: false,
    win_method: "matchv5",
    saved: false,
    incomplete: false,
    matchv5_fetched: true,
    video_size_bytes: 1_790_000_000,
    video_available: true,
  },
  {
    timestamp: "1785900000",
    champion: "Orianna",
    game_mode: "CLASSIC",
    duration_ms: 1_954_000,
    recorded_at: "2026-08-03T16:14:00Z",
    kills: 4,
    deaths: 3,
    assists: 16,
    win: null,
    win_method: "unknown",
    saved: false,
    incomplete: false,
    matchv5_fetched: false,
    video_size_bytes: 2_440_000_000,
    video_available: true,
  },
  {
    timestamp: "1785800000",
    champion: "Ahri",
    game_mode: "CLASSIC",
    duration_ms: 2_342_000,
    recorded_at: "2026-08-02T12:03:00Z",
    kills: 10,
    deaths: 6,
    assists: 8,
    win: true,
    win_method: "matchv5",
    saved: false,
    incomplete: false,
    matchv5_fetched: true,
    video_size_bytes: 3_120_000_000,
    video_available: true,
  },
  {
    timestamp: "1785700000",
    champion: "Unknown",
    game_mode: "Unknown",
    duration_ms: 0,
    recorded_at: "",
    kills: 0,
    deaths: 0,
    assists: 0,
    win: null,
    win_method: "unknown",
    saved: false,
    incomplete: true,
    matchv5_fetched: false,
    video_size_bytes: 612_000_000,
    video_available: true,
  },
];

let mockClips: ClipSummary[] = [
  {
    filename: "1786005616_1786005900",
    game_timestamp: "1786005616",
    clip_timestamp: "1786005900",
    duration_ms: 31_000,
    file_size_bytes: 74_000_000,
    thumbnail_path: null,
    thumbnail_url: null,
    video_url: "",
    source_champion: "Syndra",
    source_date: "2026-08-06T08:40:16Z",
  },
  {
    filename: "1785999000_1785999500",
    game_timestamp: "1785999000",
    clip_timestamp: "1785999500",
    duration_ms: 18_000,
    file_size_bytes: 41_000_000,
    thumbnail_path: null,
    thumbnail_url: null,
    video_url: "",
    source_champion: "Jinx",
    source_date: "2026-08-04T21:52:00Z",
  },
];

let mockSettings: AppSettings = {
  output_path: "~/LeagueReplays",
  auto_delete_days: 30,
  hevc_playback_supported: true,
};

const pausePreview = () => new Promise((resolve) => window.setTimeout(resolve, 60));

export const loadGames = async (): Promise<GameSummary[]> => {
  if (!isTauri()) return structuredClone(mockGames);
  return invoke<GameSummary[]>("list_games");
};

export const loadClips = async (): Promise<ClipSummary[]> => {
  if (!isTauri()) return structuredClone(mockClips);
  return invoke<ClipSummary[]>("list_clips");
};

export const loadPlaybackProbe = async (gameTimestamp: string): Promise<PlaybackProbe> => {
  if (!isTauri()) {
    const game = mockGames.find((candidate) => candidate.timestamp === gameTimestamp);
    if (!game) throw new Error("Recording not found");
    return {
      game: structuredClone(game),
      video_url: "",
      game_start_video_offset_ms: MOCK_VIDEO_OFFSET_MS,
      local_player_name: "SUPERSTAR#VOID",
      participants: mockParticipants(),
      player_timeline: mockPlayerTimeline(),
      kda_timeline: mockKdaTimeline(),
      gold_timeline: game.matchv5_fetched ? mockGoldTimeline(game.duration_ms) : [],
      events: mockEvents(),
    };
  }
  return invoke<PlaybackProbe>("get_playback_probe", { gameTimestamp });
};

export const setGameSaved = async (gameTimestamp: string, saved: boolean): Promise<void> => {
  if (!isTauri()) {
    await pausePreview();
    mockGames = mockGames.map((game) =>
      game.timestamp === gameTimestamp ? { ...game, saved } : game,
    );
    return;
  }
  await invoke("save_game", { gameTimestamp, saved });
};

export const removeGame = async (gameTimestamp: string): Promise<void> => {
  if (!isTauri()) {
    await pausePreview();
    mockGames = mockGames.filter((game) => game.timestamp !== gameTimestamp);
    return;
  }
  await invoke("delete_game", { gameTimestamp });
};

export const removeClip = async (clipFilename: string): Promise<void> => {
  if (!isTauri()) {
    await pausePreview();
    mockClips = mockClips.filter((clip) => clip.filename !== clipFilename);
    return;
  }
  await invoke("delete_clip", { clipFilename });
};

export const loadBuiltInMusic = async (): Promise<BuiltInMusicTrack[]> => {
  if (!isTauri()) {
    return [
      {
        filename: "momentum.mp3",
        display_name: "Momentum",
        mood: "electronic",
        duration_s: 12,
        preview_url: "",
      },
    ];
  }
  return invoke<BuiltInMusicTrack[]>("list_built_in_music");
};

export const chooseMusicFile = async (): Promise<string | null> => {
  if (!isTauri()) return "C:\\Music\\highlight.mp3";
  const selected = await open({
    multiple: false,
    filters: [{ name: "Audio", extensions: ["mp3", "wav"] }],
  });
  return typeof selected === "string" ? selected : null;
};

export const exportClip = async (
  request: ClipExportRequest,
  onProgress: (progress: ClipExportProgress) => void,
): Promise<ClipExportResult> => {
  if (!isTauri()) {
    for (const percent of [8, 24, 46, 69, 88, 96]) {
      await pausePreview();
      onProgress({ stage: percent === 96 ? "thumbnail" : "encoding", percent });
    }
    const clipTimestamp = Math.floor(Date.now() / 1_000).toString();
    const filename = `${request.game_timestamp}_${clipTimestamp}`;
    const result: ClipExportResult = {
      filename,
      output_path: `~/LeagueReplays/clips/${filename}.mp4`,
      thumbnail_path: `~/LeagueReplays/clips/${filename}.jpg`,
      elapsed_ms: 912,
      file_size_bytes: request.preset === "discord" ? 9_200_000 : 43_000_000,
    };
    mockClips = [
      {
        filename,
        game_timestamp: request.game_timestamp,
        clip_timestamp: clipTimestamp,
        duration_ms: request.clip_end_ms - request.clip_start_ms,
        file_size_bytes: result.file_size_bytes,
        thumbnail_path: null,
        thumbnail_url: null,
        video_url: "",
        source_champion:
          mockGames.find((game) => game.timestamp === request.game_timestamp)?.champion ?? null,
        source_date:
          mockGames.find((game) => game.timestamp === request.game_timestamp)?.recorded_at ?? null,
      },
      ...mockClips,
    ];
    onProgress({ stage: "complete", percent: 100 });
    return result;
  }
  const progress = new Channel<ClipExportProgress>();
  progress.onmessage = onProgress;
  return invoke<ClipExportResult>("export_clip", { request, progress });
};

export const loadStorageUsage = async (): Promise<StorageUsage> => {
  if (!isTauri()) {
    return {
      games_bytes: mockGames.reduce((total, game) => total + game.video_size_bytes, 0),
      clips_bytes: mockClips.reduce((total, clip) => total + clip.file_size_bytes, 0),
      game_count: mockGames.length,
      clip_count: mockClips.length,
    };
  }
  return invoke<StorageUsage>("get_storage_usage");
};

export const loadSettings = async (): Promise<AppSettings> => {
  if (!isTauri()) return structuredClone(mockSettings);
  return invoke<AppSettings>("get_settings");
};

export const persistSettings = async (settings: SettingsUpdate): Promise<AppSettings> => {
  if (!isTauri()) {
    await pausePreview();
    mockSettings = { ...mockSettings, ...settings };
    return structuredClone(mockSettings);
  }
  return invoke<AppSettings>("save_settings", { settings });
};

export const chooseOutputFolder = async (currentPath: string): Promise<string | null> => {
  if (!isTauri()) return currentPath;
  const selected = await open({ directory: true, multiple: false, defaultPath: currentPath });
  return typeof selected === "string" ? selected : null;
};

export const cleanUpNow = async (): Promise<AutoDeleteResult> => {
  if (!isTauri()) return { deleted_count: 0 };
  return invoke<AutoDeleteResult>("run_auto_delete");
};

export const openOutputFolder = async (): Promise<void> => {
  if (!isTauri()) return;
  await invoke("open_output_folder");
};

export const openClipsFolder = async (): Promise<void> => {
  if (!isTauri()) return;
  await invoke("open_clips_folder");
};

export const loadDdragonStatus = async (): Promise<DdragonStatus> => {
  if (!isTauri()) {
    return {
      state: "ready",
      version: "16.16.1",
      item_count: 527,
      champion_count: 172,
      cache_directory: "browser preview",
      error: null,
    };
  }
  return invoke<DdragonStatus>("get_ddragon_status");
};

export const resolveItemName = async (itemId: string): Promise<string | null> => {
  if (!isTauri()) return itemId === "1001" ? "Boots" : null;
  return invoke<string | null>("resolve_item_name", { itemId });
};

export const ensureHevcCapability = async (): Promise<HevcProbeStatus> => {
  if (!isTauri()) return { tested: true, supported: true, probe_url: "" };
  const status = await invoke<HevcProbeStatus>("get_hevc_probe_status");
  if (status.tested) return status;
  let supported = false;
  try {
    supported = await probeHevcPlayback(status.probe_url);
  } catch {
    supported = false;
  }
  return invoke<HevcProbeStatus>("record_hevc_probe_result", { supported });
};

export const loadServerMetrics = async (): Promise<ServerMetrics> => {
  if (!isTauri()) return { requests: 0, range_requests: 0, response_bytes: 0 };
  return invoke<ServerMetrics>("get_playback_server_metrics");
};

const probeHevcPlayback = async (source: string): Promise<boolean> => {
  const video = document.createElement("video");
  video.muted = true;
  video.preload = "auto";
  video.playsInline = true;
  video.style.cssText =
    "position:fixed;width:2px;height:2px;left:-10px;top:-10px;opacity:0;pointer-events:none";
  document.body.append(video);
  try {
    await waitForMediaEvent(video, "loadedmetadata", source, 8_000);
    if (!Number.isFinite(video.duration) || video.duration < 1) return false;
    const seeked = waitForMediaEvent(video, "seeked", undefined, 5_000);
    video.currentTime = Math.min(video.duration - 0.2, Math.max(0.5, video.duration * 0.6));
    await seeked;
    await video.play();
    const framePresented = await waitForPresentedFrame(video, 2_000);
    video.pause();
    return framePresented && video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA;
  } finally {
    video.pause();
    video.removeAttribute("src");
    video.load();
    video.remove();
  }
};

const waitForMediaEvent = (
  video: HTMLVideoElement,
  event: "loadedmetadata" | "seeked",
  source?: string,
  timeoutMs = 5_000,
): Promise<void> =>
  new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => finish(new Error(`HEVC ${event} timed out`)), timeoutMs);
    const finish = (error?: Error) => {
      window.clearTimeout(timeout);
      video.removeEventListener(event, onSuccess);
      video.removeEventListener("error", onError);
      error ? reject(error) : resolve();
    };
    const onSuccess = () => finish();
    const onError = () => finish(new Error("HEVC media decoding failed"));
    video.addEventListener(event, onSuccess, { once: true });
    video.addEventListener("error", onError, { once: true });
    if (source) {
      video.src = source;
      video.load();
    }
  });

const waitForPresentedFrame = (video: HTMLVideoElement, timeoutMs: number): Promise<boolean> =>
  new Promise((resolve) => {
    let settled = false;
    const finish = (presented: boolean) => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timeout);
      resolve(presented);
    };
    const timeout = window.setTimeout(() => finish(video.currentTime > 0), timeoutMs);
    if (typeof video.requestVideoFrameCallback === "function") {
      video.requestVideoFrameCallback(() => finish(true));
    } else {
      video.addEventListener("timeupdate", () => finish(true), { once: true });
    }
  });

export const isDesktopRuntime = isTauri;
