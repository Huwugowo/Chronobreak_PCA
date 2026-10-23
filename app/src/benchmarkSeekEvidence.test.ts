import { expect, it } from "vitest";
import { createBenchmarkSeekEvidence } from "./benchmarkSeekEvidence";
import type { PlaybackEvent } from "./playbackController";

const event = (kind: string, extra: Partial<PlaybackEvent> = {}): PlaybackEvent => ({
  kind, generation: 3, actionId: "seek-1", atMs: 0, payload: { authoritative: true }, ...extra,
});

it.each(["seeked", "seek_presented"])("requires both observations, starting with %s", async first => {
  const gate = createBenchmarkSeekEvidence(3, "seek-1");
  let settled = false;
  void gate.complete.then(() => { settled = true; });
  gate.observe(event("seek_dispatched"));
  gate.observe(event(first));
  await Promise.resolve();
  expect(settled).toBe(false);
  gate.observe(event(first === "seeked" ? "seek_presented" : "seeked"));
  await gate.complete;
  expect(settled).toBe(true);
});

it("does not combine other actions or approximate frames", async () => {
  const gate = createBenchmarkSeekEvidence(3, "seek-1");
  let settled = false;
  void gate.complete.then(() => { settled = true; });
  gate.observe(event("seek_dispatched"));
  gate.observe(event("seeked", { actionId: "seek-0" }));
  gate.observe(event("seek_presented", { payload: { authoritative: false } }));
  await Promise.resolve();
  expect(settled).toBe(false);
  gate.observe(event("seeked"));
  gate.observe(event("seek_presented"));
  await gate.complete;
});

it.each(["action_cancelled_superseded", "action_failed", "seek_timeout", "viewer_disposed", "media_error", "recovery_started"])(
  "rejects incomplete evidence on %s", async kind => {
    const gate = createBenchmarkSeekEvidence(3, "seek-1");
    gate.observe(event("seek_dispatched"));
    gate.observe(event("seek_presented"));
    gate.observe(event(kind));
    await expect(gate.complete).rejects.toThrow("benchmark seek");
  },
);

it.each([event("seeked", { generation: 4 }), event("seek_requested", { actionId: "seek-2" })])(
  "rejects lost ownership even after presentation", async invalidation => {
    const gate = createBenchmarkSeekEvidence(3, "seek-1");
    gate.observe(event("seek_presented"));
    gate.observe(invalidation);
    await expect(gate.complete).rejects.toThrow("benchmark seek");
  },
);

it("retains explicit deduplication without inventing phases", async () => {
  const gate = createBenchmarkSeekEvidence(3, "seek-1");
  gate.observe(event("seek_deduped"));
  await gate.complete;
});
