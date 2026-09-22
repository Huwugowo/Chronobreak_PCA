import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { assertReplayProbeMatches, decodeClipDurations, decodeLibrarySnapshot, decodePlaybackProbe, decodeReplayDescriptor, loadReplayDescriptor } from "./api";

vi.mock("@tauri-apps/api/core", async original => ({ ...await original<typeof import("@tauri-apps/api/core")>(),
  invoke: vi.fn(), isTauri: vi.fn() }));
afterEach(() => vi.resetAllMocks());

const playbackWire = () => ({
  game: {
    timestamp: "1787904000",
    champion: "Ahri",
    game_mode: "PRACTICETOOL",
    duration_ms: 2_000,
    recorded_at: "2026-08-28T00:00:00Z",
    kills: 1,
    deaths: 1,
    assists: 1,
    summoner_spells: ["Flash", "Teleport"],
    keystone_id: 8214,
    items: [{ item_id: 1001, slot: 0 }],
    saved: false,
    incomplete: false,
    video_size_bytes: 1024,
    video_available: true,
  },
  video_url: "http://127.0.0.1:9000/games/1787904000/video.mp4",
  media_timeline: {
    schema_version: 2,
    replay_ticks_per_second: "48000000",
    media_id: "9c82b929-7c85-4c27-a6d0-4f4e1d8e3311",
    video: {
      codec: "h264",
      profile: "High",
      time_base: { numerator: "1", denominator: "15360" },
      first_pts: "0",
      frame_rate: { numerator: "60", denominator: "1" },
      frame_count: "120",
      one_past_last_pts: "30720",
      replay_end: "96000000",
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
      duration_seconds: { numerator: "2", denominator: "1" },
    },
    producer: {
      backend: "replay-corpus-normalize",
      expected_frame_rate: { numerator: "60", denominator: "1" },
      expected_frame_count: "120",
      media_runtime_id: "queueback-ffmpeg-8.1.2-windows-x86_64-r6",
    },
    capture: null,
  },
  local_player_name: "QB-Blue-1#TEST",
  participants: [
    { summoner_name: "QB-Blue-1#TEST", champion: "Ahri", relation: "ally" },
  ],
  player_timeline: [
    {
      game_tick: "0",
      mapped_replay_time: { status: "inside_media", replay_tick: "0" },
      cs: 0,
      level: 1,
    },
  ],
  kda_timeline: [
    {
      mapped_replay_time: { status: "before_media", replay_tick: "-1" },
      kills: 0,
      deaths: 0,
      assists: 0,
    },
  ],
  events: [
    {
      event_type: "ChampionKill",
      game_tick: "1000000",
      mapped_replay_time: { status: "inside_media", replay_tick: "48000000" },
      killer: "QB-Blue-1#TEST",
      victim: "QB-Red-1#TEST",
      assisters: [],
      dragon_type: null,
      kill_streak: null,
      acer: null,
      acing_team: null,
      turret: null,
      inhibitor: null,
      result: null,
      relation: "ally",
    },
  ],
});

describe("decodeReplayDescriptor", () => {
  const wire = () => ({ snapshot_token: "current", game_timestamp: "1787904000",
    video_url: playbackWire().video_url, media_timeline: playbackWire().media_timeline });
  it("admits media authority independently of semantic arrays", () => {
    const descriptor = decodeReplayDescriptor(wire());
    expect(descriptor.media_timeline.video.replayEnd).toBe(96_000_000);
    expect(Object.keys(descriptor)).toHaveLength(4);
  });
  it("sends the selected token and rejects a descriptor for a different token or game", async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValueOnce(wire());
    await expect(loadReplayDescriptor("1787904000", "current")).resolves.toMatchObject({ snapshot_token: "current" });
    expect(invoke).toHaveBeenCalledWith("get_replay_descriptor", { gameTimestamp: "1787904000", snapshotToken: "current" });
    for (const changed of [{ ...wire(), snapshot_token: "old" }, { ...wire(), game_timestamp: "2" }]) {
      vi.mocked(invoke).mockResolvedValueOnce(changed);
      await expect(loadReplayDescriptor("1787904000", "current")).rejects.toThrow("selection mismatch");
    }
  });
  it("rejects legacy, missing, malformed and expanded descriptor payloads", () => {
    for (const invalid of [{ ...wire(), snapshot_token: "" }, { ...wire(), game_timestamp: "../1" },
      { ...wire(), events: [] }, { ...wire(), video_url: null },
      { ...wire(), media_timeline: { ...wire().media_timeline, schema_version: 1 } }]) {
      expect(() => decodeReplayDescriptor(invalid)).toThrow();
    }
  });
  it("rejects full details from a different recording, URL or exact media timeline", () => {
    const descriptor = decodeReplayDescriptor(wire());
    const probe = decodePlaybackProbe(playbackWire());
    expect(assertReplayProbeMatches(descriptor, probe)).toBe(probe);
    const variants = [
      { ...probe, game: { ...probe.game, timestamp: "2" } },
      { ...probe, video_url: `${probe.video_url}?changed` },
      { ...probe, media_timeline: { ...probe.media_timeline,
        video: { ...probe.media_timeline.video, firstPts: 1n } } },
      { ...probe, media_timeline: { ...probe.media_timeline,
        audio: { present: true } } },
    ];
    for (const changed of variants) expect(() => assertReplayProbeMatches(descriptor, changed)).toThrow("Recording changed");
  });
});

describe("decodePlaybackProbe", () => {
  it("decodes the strict snake-case schema-v2 wire payload for viewer use", () => {
    const probe = decodePlaybackProbe(playbackWire());

    expect(probe.media_timeline.mediaId).toBe("9c82b929-7c85-4c27-a6d0-4f4e1d8e3311");
    expect(probe.media_timeline.video.replayEnd).toBe(96_000_000);
    expect(probe.player_timeline[0]?.replay_tick).toBe(0);
    expect(probe.kda_timeline[0]?.replay_tick).toBeUndefined();
    expect(probe.events[0]?.replay_tick).toBe(48_000_000);
    expect(probe.events[0]).not.toHaveProperty("mapped_replay_time");
  });

  it("rejects unknown mapped-time fields instead of accepting a parallel wire shape", () => {
    const wire = playbackWire();
    Object.assign(wire.events[0]!.mapped_replay_time, { legacy_time_ms: 1000 });

    expect(() => decodePlaybackProbe(wire)).toThrow("invalid schema");
  });
});

describe("decodeLibrarySnapshot", () => {
  const valid = () => ({
    token: "snapshot-a",
    games: [playbackWire().game],
    clips: [{ filename: "1_2", game_timestamp: "1", clip_timestamp: "2", duration_ms: null,
      file_size_bytes: 10, thumbnail_path: null, thumbnail_url: null, video_url: "",
      source_champion: null, source_date: null }],
    usage: { games_bytes: 10, clips_bytes: 10, game_count: 1, clip_count: 1 },
  });
  it("admits one coherent tokenized core snapshot with unknown duration", () => {
    expect(decodeLibrarySnapshot(valid()).clips[0]?.duration_ms).toBeNull();
  });
  it("rejects extra fields in snapshot records", () => {
    const value = valid();
    Object.assign(value.clips[0]!, { unexpected: true });
    expect(() => decodeLibrarySnapshot(value)).toThrow("invalid schema");
  });
});

describe("decodeClipDurations", () => {
  it("keeps unavailable and available results explicit", () => {
    expect(decodeClipDurations({ snapshot_token: "a", clips: [
      { clip_id: "x", duration: { state: "unavailable" } },
      { clip_id: "y", duration: { state: "available", duration_ms: 42 } },
    ] }).clips[1]?.duration).toEqual({ state: "available", duration_ms: 42 });
  });
  it("rejects an unknown optional-duration shape", () => {
    expect(() => decodeClipDurations({ snapshot_token: "a", clips: [{ clip_id: "x", duration: { state: "available" } }] })).toThrow();
  });
  it("rejects duplicate IDs or an empty response token", () => {
    expect(() => decodeClipDurations({ snapshot_token: "", clips: [] })).toThrow();
    expect(() => decodeClipDurations({ snapshot_token: "a", clips: [
      { clip_id: "x", duration: { state: "unavailable" } },
      { clip_id: "x", duration: { state: "unavailable" } },
    ] })).toThrow();
  });
});
