import { expect, it, vi } from "vitest";
import { decodeOverlayPage, type OverlayPage, type OverlayQuery, type OverlaySample } from "./leagueIndex";
import { ReplayOverlayController } from "./replayOverlayController";
import { parseMediaId, type ReplayTick } from "./replayTime";
import { overlayParticipantStates } from "./replayProjection";
import { participantKey } from "./leagueSource";

const media = parseMediaId("11111111-2222-4333-8444-555555555555");
const source = { media_id: media, run_id: media, epoch: 0 };
const scope = { source, local_participant: 1 };
const tick = (s: number) => s * 48_000_000 as ReplayTick;
const revision = { end_offset: 10, last_sequence: 1, checksum: Array(32).fill(0) };
const sample = (s: number, observation = s): OverlaySample => ({ replay_tick: tick(s), observation_id: observation,
  game_tick: String(s * 1_000_000), local_scores: { kills: 0, deaths: 1, assists: null, creep_score: 20 },
  players: [{ participant: 1, level: s, is_dead: true, respawn_timer: 12.4, spells: ["Flash", "6"] }] });
const page = (samples: OverlaySample[], next: OverlayPage["next"] = null): OverlayPage => ({ ...scope, revision,
  identities: [{ participant: 1, riot_id: "Player#TEST", champion_name: "Ahri", team: "ORDER" }], samples, next });
const deferred = <T>() => { let resolve!: (value: T) => void; const promise = new Promise<T>(r => { resolve = r; }); return { promise, resolve }; };
const setup = (queryOverlayPage: (token: string, game: string, query: OverlayQuery) => Promise<OverlayPage>) =>
  new ReplayOverlayController({ gameTimestamp: "1", replayEnd: tick(1000), readReplay: work => work("snapshot"), queryOverlayPage });

it("loads once, preserves equal-tick page frontiers and selects exact state across distant seeks", async () => {
  const tail = deferred<OverlayPage>();
  const early = sample(2), first = sample(20, 1), tie = sample(20, 2), late = sample(900, 3);
  const cursor = { replay_tick: first.replay_tick, observation_id: first.observation_id };
  const query = vi.fn().mockResolvedValueOnce(page([early, first], cursor)).mockReturnValueOnce(tail.promise);
  const controller = setup(query);
  controller.select(scope);
  expect(controller.value.initialLoadout).toBeNull();
  await vi.waitFor(() => expect(query).toHaveBeenCalledTimes(2));
  expect(controller.value.initialLoadout).toBeNull(); // No context from an incomplete timeline.
  expect(controller.sampleAt(tick(0))).toBeNull();
  expect(controller.sampleAt(tick(19))?.replay_tick).toBe(tick(2));
  expect(controller.sampleAt(tick(20))).toBeNull(); // Equal-tick successor has not arrived.
  expect(controller.sampleAt(tick(950))).toBeNull(); // A loaded predecessor is not yet proof.
  tail.resolve({ ...page([tie, late]), identities: null });
  await vi.waitFor(() => expect(controller.value.status).toBe("ready"));
  expect(controller.value.initialLoadout).toEqual({ replay_tick: early.replay_tick,
    players: [{ participant: 1, spells: ["Flash", "6"] }] });
  for (const [target, expected] of [[950, late], [3, early], [900, late], [19, early], [20, tie], [2, early]] as const)
    expect(controller.sampleAt(tick(target))).toBe(expected);
  expect(controller.sampleAt(tick(20))).toBe(tie);
  expect(query).toHaveBeenCalledTimes(2);
  expect(query.mock.calls[1][2]).toMatchObject({ after: cursor, revision });
  controller.dispose();
  expect(controller.sampleAt(tick(950))).toBeNull();
  expect(controller.value.initialLoadout).toBeNull();
});

it("serializes source replacement and rejects stale completions and changed revisions", async () => {
  const old = deferred<OverlayPage>(), fresh = deferred<OverlayPage>();
  const query = vi.fn().mockReturnValueOnce(old.promise).mockReturnValueOnce(fresh.promise);
  const controller = setup(query);
  controller.select(scope);
  const nextScope = { ...scope, source: { ...source, epoch: 1 } };
  controller.select(nextScope);
  expect(query).toHaveBeenCalledTimes(1); // No second in-flight SQL request.
  old.resolve(page([sample(2)]));
  await vi.waitFor(() => expect(query).toHaveBeenCalledTimes(2));
  expect(controller.value.identities).toEqual([]);
  fresh.resolve({ ...page([sample(900)]), source: nextScope.source });
  await vi.waitFor(() => expect(controller.value.status).toBe("ready"));
  expect(controller.sampleAt(tick(900))?.replay_tick).toBe(tick(900));
  controller.select(scope);
  expect(controller.value.initialLoadout).toBeNull();
  controller.dispose();
  const badQuery = vi.fn().mockResolvedValueOnce(page([sample(2)], sample(2)))
    .mockResolvedValueOnce({ ...page([sample(3)]), identities: null, revision: { ...revision, last_sequence: 2 } });
  const bad = setup(badQuery); bad.select(scope);
  await vi.waitFor(() => expect(bad.value.status).toBe("error"));
  expect(bad.sampleAt(tick(3))).toBeNull();
  expect(bad.value.initialLoadout).toBeNull();
  expect(bad.value.sampleCount).toBe(0); bad.dispose();
});

it("strictly checks compact page order, identity, shape, clocks and unavailable fields", () => {
  const query: OverlayQuery = { scope: source, after: null, revision: null, limit: 128 };
  const wire = () => ({ ...page([]), samples: [sample(2), sample(3)].map(s => ({ ...s, replay_tick: String(s.replay_tick) })) });
  expect(decodeOverlayPage(wire(), query).samples[0].local_scores?.assists).toBeNull();
  for (const mutate of [
    (w: any) => { w.source = { ...source, epoch: 1 }; },
    (w: any) => { w.samples.reverse(); },
    (w: any) => { w.samples.push(w.samples[1]); },
    (w: any) => { w.samples[0].players[0].is_dead = 1; },
    (w: any) => { w.samples[0].players[0].respawn_timer = -1; },
    (w: any) => { w.samples[0].players[0].spells = ["Flash"]; },
    (w: any) => { w.samples[0].players.push(w.samples[0].players[0]); },
    (w: any) => { w.samples[0].items = []; },
    (w: any) => { w.samples = Array(129).fill(w.samples[0]); },
    (w: any) => { w.next = { replay_tick: "0", observation_id: 0 }; },
  ]) { const w = wire(); mutate(w); expect(() => decodeOverlayPage(w, query)).toThrow(); }
});

it("projects dead state and timer from recorded replay time without guessing a respawn", () => {
  const s = sample(2), key = participantKey({ source, participant: 1 });
  const at = (seconds: number) => overlayParticipantStates(scope, s, tick(seconds)).get(key);
  expect(at(1)).toBeUndefined();
  expect(at(2)).toEqual({ level: 2, dead: true, respawnSeconds: 13, spells: ["Flash", "6"] });
  expect(at(3.5)?.respawnSeconds).toBe(11);
  expect(at(100)).toMatchObject({ dead: true, respawnSeconds: 0 });
  expect(at(2)?.respawnSeconds).toBe(13);
  s.players[0].is_dead = false;
  expect(at(3)).toMatchObject({ dead: false, respawnSeconds: undefined });
});

it("shows only first-recorded spells before the first sample and fully replaces them with timed slots", () => {
  const first = sample(25), key = participantKey({ source, participant: 1 });
  const initial = { replay_tick: first.replay_tick,
    players: [{ participant: 1, spells: ["Teleport", null] as const }] };
  for (const position of [null, tick(0), tick(24)]) {
    expect(overlayParticipantStates(scope, null, position, initial).get(key))
      .toEqual({ spells: ["Teleport", null], spellsAreInitial: true });
  }
  // A missing sample later (e.g. an unproven page frontier) cannot reuse opening context.
  expect(overlayParticipantStates(scope, null, tick(25), initial).size).toBe(0);
  first.players[0].spells = [null, "Unleashed Teleport"];
  expect(overlayParticipantStates(scope, first, tick(25), initial).get(key))
    .toEqual({ level: 25, dead: true, respawnSeconds: 13, spells: [null, "Unleashed Teleport"] });
  expect(overlayParticipantStates(scope, null, tick(0)).size).toBe(0);
});
