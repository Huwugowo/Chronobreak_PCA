import { describe, expect, it } from "vitest";
import { parseFrameBoundary, parseReplayTick, validateMediaTimeline } from "./replayTime";
import {
  alignClipRangeToFrames,
  clipEndpointPreviewTick,
  defaultClipRange,
  latestIndexAt,
  moveClipEndpoint,
  nearestIndexAt,
  timelineValue,
} from "./viewerUtils";

const tick = (value: string) => parseReplayTick(value);

const timeline = () =>
  validateMediaTimeline({
    schema_version: 2,
    replay_ticks_per_second: "48000000",
    media_id: "11111111-2222-4333-8444-555555555555",
    video: {
      codec: "h264",
      profile: "High",
      time_base: { numerator: "1", denominator: "15360" },
      first_pts: "15360",
      frame_rate: { numerator: "60", denominator: "1" },
      frame_count: "1200",
      one_past_last_pts: "322560",
      replay_end: "960000000",
      exact_cfr: true,
    },
    audio: { present: false, codec: null, sample_rate: null, time_base: null, first_pts: null, replay_start: null, replay_end: null },
    container: { start_seconds: { numerator: "1", denominator: "1" }, duration_seconds: { numerator: "20", denominator: "1" } },
    producer: { backend: "native", expected_frame_rate: { numerator: "60", denominator: "1" }, expected_frame_count: "1200", media_runtime_id: "queueback-r6" },
  });

describe("viewer replay-time utilities", () => {
  it("searches only canonical replay ticks", () => {
    const entries = [{ replay_tick: tick("10") }, { replay_tick: tick("20") }, { replay_tick: tick("40") }];
    expect(latestIndexAt(entries, tick("20"))).toBe(1);
    expect(nearestIndexAt(entries, tick("31"))).toBe(2);
    expect(timelineValue(entries, tick("9"))).toBeUndefined();
  });

  it("keeps clip endpoints on the source frame grid and bound to media identity", () => {
    const media = timeline();
    const range = defaultClipRange(tick("959000000"), media, []);
    expect(range).toMatchObject({ mediaId: media.mediaId, startFrame: 718, endFrameExclusive: 1200 });
    expect(clipEndpointPreviewTick(range, "end", media)).toBe(tick("959200000"));

    const moved = moveClipEndpoint(range, "start", tick("800000000"), media);
    expect(moved).toMatchObject({ startFrame: 900, endFrameExclusive: 1200 });
    expect(() => alignClipRangeToFrames({ ...range, mediaId: "aaaaaaaa-2222-4333-8444-555555555555" as typeof range.mediaId }, media)).toThrow("media_identity_mismatch");
  });

  it("rejects a range below the exact five-second minimum", () => {
    const media = timeline();
    expect(() =>
      alignClipRangeToFrames(
        { mediaId: media.mediaId, startFrame: parseFrameBoundary("0"), endFrameExclusive: parseFrameBoundary("299") },
        media,
      ),
    ).toThrow("minimum_clip_duration");
  });
});
