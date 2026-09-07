import { describe, expect, it } from "vitest";
import { decodePlaybackProbe } from "./api";

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
