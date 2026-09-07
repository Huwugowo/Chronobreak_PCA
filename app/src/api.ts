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
import {
  createClipRange,
  frameBoundaryToReplayTick,
  parseFrameBoundary,
  parseMediaId,
  parseReplayTick,
  parseSignedReplayTick,
  validateMediaTimeline,
  type ReplayTick,
} from "./replayTime";

const MOCK_TICKS_PER_MILLISECOND = 48_000;
const mockTick = (milliseconds: number): ReplayTick =>
  parseReplayTick(String(milliseconds * MOCK_TICKS_PER_MILLISECOND));
const MOCK_GAME_START_REPLAY_TICK = mockTick(42_369);
const MOCK_MEDIA_TIMELINE = validateMediaTimeline({
  schema_version: 2,
  replay_ticks_per_second: "48000000",
  media_id: "f22d7d5e-0ce1-4e0f-8318-cbb0b6decb19",
  video: {
    codec: "h264",
    profile: "High",
    time_base: { numerator: "1", denominator: "60" },
    first_pts: "0",
    frame_rate: { numerator: "60", denominator: "1" },
    frame_count: "108000",
    one_past_last_pts: "108000",
    replay_end: "86400000000",
    exact_cfr: true,
  },
  audio: {
    present: false,
    codec: null,
    sample_rate: null,
    time_base: null,
    first_pts: null,
    replay_start: null,
    replay_end: null,
  },
  container: {
    start_seconds: { numerator: "0", denominator: "1" },
    duration_seconds: { numerator: "1800", denominator: "1" },
  },
  producer: {
    backend: "mock",
    expected_frame_rate: { numerator: "60", denominator: "1" },
    expected_frame_count: "108000",
    media_runtime_id: "mock-runtime-v2",
  },
});

const mockEvent = (
  eventType: string,
  replayTick: ReplayTick,
  details: Partial<ViewerEvent> = {},
): ViewerEvent => ({
  event_type: eventType,
  game_tick: String(Math.max(0, replayTick - MOCK_GAME_START_REPLAY_TICK)),
  replay_tick: replayTick,
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
    const replayTick = mockTick(248_000 + index * 29_500);
    if (index % 11 === 4) {
      return mockEvent("DragonKill", replayTick, {
        killer: index % 2 === 0 ? "SUPERSTAR" : "Enemy Jungler",
        assisters: ["Ally Jungler", "SUPERSTAR"],
        dragon_type: ["Air", "Fire", "Water", "Earth"][index % 4],
        relation: index % 2 === 0 ? "ally" : "enemy",
      });
    }
    if (index % 13 === 7) {
      return mockEvent("TurretKilled", replayTick, {
        killer: index % 2 === 0 ? "SUPERSTAR" : "Enemy Carry",
        turret: "Turret_T2_C_03_A",
        relation: index % 2 === 0 ? "ally" : "enemy",
      });
    }
    if (index % 17 === 9) {
      return mockEvent("Multikill", replayTick, {
        killer: "SUPERSTAR",
        kill_streak: 2,
        relation: "ally",
      });
    }
    const enemyKill = index % 3 === 0 && index % 5 !== 0;
    return mockEvent("ChampionKill", replayTick, {
      killer: index % 5 === 0 ? "SUPERSTAR" : enemyKill ? "Enemy Carry" : "Ally Jungler",
      victim: index % 5 === 0 ? "Enemy Carry" : enemyKill ? "SUPERSTAR" : "Enemy Jungler",
      assisters: index % 4 === 0 ? ["SUPERSTAR"] : ["Ally Support"],
      relation: enemyKill ? "enemy" : "ally",
    });
  });

  return [
    mockEvent("GameStart", MOCK_GAME_START_REPLAY_TICK),
    mockEvent("MinionsSpawning", mockTick(107_000)),
    mockEvent("FirstBlood", mockTick(199_402), {
      killer: "SUPERSTAR",
      victim: "Enemy Carry",
      relation: "ally",
    }),
    mockEvent("ChampionKill", mockTick(199_402), {
      killer: "SUPERSTAR",
      victim: "Enemy Carry",
      assisters: ["Ally Jungler"],
      relation: "ally",
    }),
    ...generated,
    mockEvent("BaronKill", mockTick(1_501_000), {
      killer: "Ally Jungler",
      assisters: ["SUPERSTAR", "Ally Support"],
      relation: "ally",
    }),
    mockEvent("GameEnd", mockTick(1_770_000), { result: "Win" }),
  ].sort((left, right) => (left.replay_tick ?? 0) - (right.replay_tick ?? 0));
};

const mockPlayerTimeline = (): PlayerTimelinePoint[] =>
  Array.from({ length: 174 }, (_, index) => {
    const gameTick = mockTick(1_282 + index * 10_000);
    return {
      game_tick: String(gameTick),
      replay_tick: parseReplayTick(String(MOCK_GAME_START_REPLAY_TICK + gameTick)),
      cs: Math.min(200, Math.floor(Number(gameTick) / (8_450 * MOCK_TICKS_PER_MILLISECOND))),
      level: Math.min(15, 1 + Math.floor(Number(gameTick) / (118_000 * MOCK_TICKS_PER_MILLISECOND))),
    };
  });

const mockKdaTimeline = (): KdaTimelinePoint[] => [
  { replay_tick: mockTick(199_402), kills: 1, deaths: 0, assists: 0 },
  { replay_tick: mockTick(396_000), kills: 2, deaths: 0, assists: 1 },
  { replay_tick: mockTick(534_000), kills: 2, deaths: 1, assists: 2 },
  { replay_tick: mockTick(711_000), kills: 3, deaths: 1, assists: 4 },
  { replay_tick: mockTick(890_000), kills: 5, deaths: 2, assists: 5 },
  { replay_tick: mockTick(1_114_000), kills: 6, deaths: 3, assists: 8 },
  { replay_tick: mockTick(1_409_000), kills: 7, deaths: 4, assists: 10 },
  { replay_tick: mockTick(1_698_000), kills: 8, deaths: 4, assists: 11 },
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
    summoner_spells: ["SummonerFlash", "SummonerTeleport"],
    keystone_id: 8214,
    items: [
      { item_id: 6657, slot: 0 },
      { item_id: 3020, slot: 1 },
      { item_id: 3089, slot: 2 },
      { item_id: 3135, slot: 3 },
      { item_id: 3157, slot: 4 },
      { item_id: 3102, slot: 5 },
      { item_id: 3363, slot: 6 },
    ],
    saved: true,
    incomplete: false,
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
    summoner_spells: ["SummonerFlash", "SummonerTeleport"],
    keystone_id: 8369,
    items: [
      { item_id: 6653, slot: 0 },
      { item_id: 3020, slot: 1 },
      { item_id: 3118, slot: 2 },
      { item_id: 3089, slot: 3 },
      { item_id: 3135, slot: 4 },
      { item_id: 3157, slot: 5 },
      { item_id: 3340, slot: 6 },
    ],
    saved: false,
    incomplete: false,
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
    summoner_spells: ["SummonerFlash", "SummonerBarrier"],
    keystone_id: 8005,
    items: [
      { item_id: 3031, slot: 0 },
      { item_id: 3006, slot: 1 },
      { item_id: 3094, slot: 2 },
      { item_id: 3085, slot: 3 },
      { item_id: 3036, slot: 4 },
      { item_id: 6676, slot: 5 },
      { item_id: 3363, slot: 6 },
    ],
    saved: false,
    incomplete: false,
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
    summoner_spells: ["SummonerFlash", "SummonerTeleport"],
    keystone_id: 8214,
    items: [
      { item_id: 6657, slot: 0 },
      { item_id: 3020, slot: 1 },
      { item_id: 3089, slot: 2 },
      { item_id: 3135, slot: 3 },
      { item_id: 3157, slot: 4 },
      { item_id: 3102, slot: 5 },
      { item_id: 3340, slot: 6 },
    ],
    saved: false,
    incomplete: false,
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
    summoner_spells: ["SummonerFlash", "SummonerDot"],
    keystone_id: 8112,
    items: [
      { item_id: 6657, slot: 0 },
      { item_id: 3020, slot: 1 },
      { item_id: 3100, slot: 2 },
      { item_id: 3089, slot: 3 },
      { item_id: 3135, slot: 4 },
      { item_id: 3157, slot: 5 },
      { item_id: 3364, slot: 6 },
    ],
    saved: false,
    incomplete: false,
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
    summoner_spells: [],
    keystone_id: null,
    items: [],
    saved: false,
    incomplete: true,
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
const exactRecord = (
  value: unknown,
  expectedKeys: readonly string[],
  label: string,
): Record<string, unknown> => {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  const record = value as Record<string, unknown>;
  const actualKeys = Object.keys(record).sort();
  const canonicalKeys = [...expectedKeys].sort();
  if (
    actualKeys.length !== canonicalKeys.length ||
    actualKeys.some((key, index) => key !== canonicalKeys[index])
  ) {
    throw new Error(`${label} has an invalid schema`);
  }
  return record;
};

const recordArray = (value: unknown, label: string): Record<string, unknown>[] => {
  if (!Array.isArray(value)) throw new Error(`${label} must be an array`);
  return value.map((entry, index) => exactRecord(entry, Object.keys(entry ?? {}), `${label}[${index}]`));
};

const decodeMappedReplayTick = (value: unknown, label: string): ReplayTick | undefined => {
  const mapping = value as Record<string, unknown> | null;
  const status = mapping?.status;
  switch (status) {
    case "inside_media": {
      const inside = exactRecord(value, ["status", "replay_tick"], label);
      return parseReplayTick(inside.replay_tick, `${label}.replay_tick`);
    }
    case "before_media":
    case "after_media": {
      const outside = exactRecord(value, ["status", "replay_tick"], label);
      parseSignedReplayTick(outside.replay_tick, `${label}.replay_tick`);
      return undefined;
    }
    case "unavailable": {
      const unavailable = exactRecord(value, ["status", "reason"], label);
      if (
        unavailable.reason !== "calibration_unavailable" &&
        unavailable.reason !== "calibration_invalidated"
      ) {
        throw new Error(`${label}.reason is invalid`);
      }
      return undefined;
    }
    default:
      throw new Error(`${label}.status is invalid`);
  }
};

const decodeMappedRows = <T>(
  value: unknown,
  expectedKeys: readonly string[],
  label: string,
): T[] => {
  if (!Array.isArray(value)) throw new Error(`${label} must be an array`);
  return value.map((entry, index) => {
    const row = exactRecord(entry, expectedKeys, `${label}[${index}]`);
    const { mapped_replay_time: mapping, ...fields } = row;
    return {
      ...fields,
      replay_tick: decodeMappedReplayTick(mapping, `${label}[${index}].mapped_replay_time`),
    } as T;
  });
};

export const decodePlaybackProbe = (value: unknown): PlaybackProbe => {
  const wire = exactRecord(
    value,
    [
      "game",
      "video_url",
      "media_timeline",
      "local_player_name",
      "participants",
      "player_timeline",
      "kda_timeline",
      "events",
    ],
    "playback_probe",
  );
  const game = exactRecord(
    wire.game,
    [
      "timestamp",
      "champion",
      "game_mode",
      "duration_ms",
      "recorded_at",
      "kills",
      "deaths",
      "assists",
      "summoner_spells",
      "keystone_id",
      "items",
      "saved",
      "incomplete",
      "video_size_bytes",
      "video_available",
    ],
    "playback_probe.game",
  );
  const items = recordArray(game.items, "playback_probe.game.items");
  for (const [index, item] of items.entries()) {
    exactRecord(item, ["item_id", "slot"], `playback_probe.game.items[${index}]`);
  }
  const participants = recordArray(wire.participants, "playback_probe.participants");
  for (const [index, participant] of participants.entries()) {
    exactRecord(
      participant,
      ["summoner_name", "champion", "relation"],
      `playback_probe.participants[${index}]`,
    );
  }
  if (typeof wire.video_url !== "string") throw new Error("playback_probe.video_url is invalid");
  if (wire.local_player_name !== null && typeof wire.local_player_name !== "string") {
    throw new Error("playback_probe.local_player_name is invalid");
  }
  return {
    game: game as unknown as GameSummary,
    video_url: wire.video_url,
    media_timeline: validateMediaTimeline(wire.media_timeline),
    local_player_name: wire.local_player_name,
    participants: participants as unknown as ReplayParticipant[],
    player_timeline: decodeMappedRows<PlayerTimelinePoint>(
      wire.player_timeline,
      ["game_tick", "mapped_replay_time", "cs", "level"],
      "playback_probe.player_timeline",
    ),
    kda_timeline: decodeMappedRows<KdaTimelinePoint>(
      wire.kda_timeline,
      ["mapped_replay_time", "kills", "deaths", "assists"],
      "playback_probe.kda_timeline",
    ),
    events: decodeMappedRows<ViewerEvent>(
      wire.events,
      [
        "event_type",
        "game_tick",
        "mapped_replay_time",
        "killer",
        "victim",
        "assisters",
        "dragon_type",
        "kill_streak",
        "acer",
        "acing_team",
        "turret",
        "inhibitor",
        "result",
        "relation",
      ],
      "playback_probe.events",
    ),
  };
};


export const loadPlaybackProbe = async (gameTimestamp: string): Promise<PlaybackProbe> => {
  if (!isTauri()) {
    const game = mockGames.find((candidate) => candidate.timestamp === gameTimestamp);
    if (!game) throw new Error("Recording not found");
    return {
      game: structuredClone(game),
      video_url: "",
      media_timeline: MOCK_MEDIA_TIMELINE,
      local_player_name: "SUPERSTAR#VOID",
      participants: mockParticipants(),
      player_timeline: mockPlayerTimeline(),
      kda_timeline: mockKdaTimeline(),
      events: mockEvents(),
    };
  }
  return decodePlaybackProbe(await invoke<unknown>("get_playback_probe", { gameTimestamp }));
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

export const prepareImportedMusicPreview = async (path: string): Promise<string> => {
  if (!isTauri()) return "";
  return invoke<string>("prepare_imported_music_preview", { path });
};

export const exportClip = async (
  request: ClipExportRequest,
  onProgress: (progress: ClipExportProgress) => void,
): Promise<ClipExportResult> => {
  if (!isTauri()) {
    const totalOutputs = request.presets.length;
    for (const [outputIndex, preset] of request.presets.entries()) {
      for (const localPercent of [8, 24, 46, 69, 88, 96, 99]) {
        await pausePreview();
        onProgress({
          stage: localPercent >= 99 ? "thumbnail" : localPercent >= 96 ? "validating" : "encoding",
          percent: Math.floor(((outputIndex * 100 + localPercent) / totalOutputs)),
          preset,
          completed_outputs: outputIndex,
          total_outputs: totalOutputs,
        });
      }
    }
    const range = createClipRange(
      MOCK_MEDIA_TIMELINE,
      parseFrameBoundary(request.start_frame, "start_frame"),
      parseFrameBoundary(request.end_frame_exclusive, "end_frame_exclusive"),
      parseMediaId(request.media_id),
    );
    const firstClipTimestamp = Math.floor(Date.now() / 1_000);
    const durationSeconds =
      Number(range.endFrameExclusive - range.startFrame) /
      Number(MOCK_MEDIA_TIMELINE.video.frameRate.numerator);
    const validatedReplayEnd =
      frameBoundaryToReplayTick(
        range.endFrameExclusive,
        MOCK_MEDIA_TIMELINE.video.frameRate,
      ) -
      frameBoundaryToReplayTick(
        range.startFrame,
        MOCK_MEDIA_TIMELINE.video.frameRate,
      );
    const outputs = request.presets.map((preset, index) => {
      const clipTimestamp = (firstClipTimestamp + index).toString();
      const filename = `${request.game_timestamp}_${clipTimestamp}`;
      return {
        preset,
        filename,
        output_path: `~/LeagueReplays/clips/${filename}.mp4`,
        thumbnail_path: `~/LeagueReplays/clips/${filename}.jpg`,
        file_size_bytes:
          preset === "discord" ? 9_200_000 : Math.round(durationSeconds * (24_192_000 / 8)),
        strategy: "full_reencode" as const,
        encoder_used: "mock-libx264",
        encode_elapsed_ms: 800,
        validation_elapsed_ms: 15,
        validated_frame_count: String(range.endFrameExclusive - range.startFrame),
        validated_video_replay_end: String(validatedReplayEnd),
        validated_audio_replay_start: "0",
        validated_audio_replay_end: String(validatedReplayEnd),
        thumbnail_elapsed_ms: 112,
        retry_count: 0,
        attempts: [
          {
            encoder: "mock-libx264",
            elapsed_ms: 800,
            successful: true,
            error: null,
          },
        ],
      };
    });
    const result: ClipExportResult = {
      outputs,
      elapsed_ms: 912,
      setup_elapsed_ms: 12,
      source_probe_elapsed_ms: 2,
      source_probe_strategy: "recording_metadata",
      finalize_elapsed_ms: 8,
      total_file_size_bytes: outputs.reduce((total, output) => total + output.file_size_bytes, 0),
    };
    mockClips = [
      ...outputs.map((output) => ({
        filename: output.filename,
        game_timestamp: request.game_timestamp,
        clip_timestamp: output.filename.slice(request.game_timestamp.length + 1),
        duration_ms: Math.round(durationSeconds * 1_000),
        file_size_bytes: output.file_size_bytes,
        thumbnail_path: null,
        thumbnail_url: null,
        video_url: "",
        source_champion:
          mockGames.find((game) => game.timestamp === request.game_timestamp)?.champion ?? null,
        source_date:
          mockGames.find((game) => game.timestamp === request.game_timestamp)?.recorded_at ?? null,
      })),
      ...mockClips,
    ];
    onProgress({
      stage: "complete",
      percent: 100,
      preset: null,
      completed_outputs: totalOutputs,
      total_outputs: totalOutputs,
    });
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
      asset_base_url: null,
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
  if (!isTauri()) {
    return {
      requests: 0,
      range_requests: 0,
      response_bytes: 0,
      completed_streams: 0,
      cancelled_streams: 0,
    };
  }
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
