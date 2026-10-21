import { describe, expect, it } from "vitest";
import { decodeReplayWindow, decodeRecordingFacts } from "./leagueIndex";
import { parseMediaId } from "./replayTime";
import { replayEvent, replayParticipants, recordedGameClock } from "./replayProjection";
import { eventInvolvesPlayer } from "./viewerUtils";
import { participantKey, type EventActors } from "./leagueSource";

const media = parseMediaId("11111111-2222-4333-8444-555555555555");
const source = { media_id: media, run_id: media, epoch: 1 };
const coverage = { index_status: "ready", committed: null, uncommitted_tail_bytes: 0, gap_attempts: 1,
  unresolved_actors: 1, unmapped_observations: 0, successful_observations: 2, observed_from: "0", observed_through: "5000000" };
const evidence = (endpoint: string) => ({ id: 1, source, endpoint, timing: { start_us: 0, end_us: 1, received_at: "2026-10-16T00:00:00Z" }, game_stats: 2, calibration: 3 });
const wire = () => ({ source, local_participant: 1, coverage, next: { replay_tick: "48000000", source_id: 7 },
  identities: [{ participant: 1, team: "ORDER", riot_id: "Same#A", champion_name: "Ahri" }, { participant: 2, team: "CHAOS", riot_id: "Same#B", champion_name: "Garen" }],
  events: [{ revision: { event: { source_id: 7, game_tick: "1000000", name: "ChampionKill", killer: "Same#B", victim: "Same#A",
    actors: { killer: [{ participant: 2, status: "resolved" }], victim: [{ participant: 1, status: "resolved" }] } },
    fingerprint: Array(32).fill(1), response_fingerprint: Array(32).fill(2), ingestion: "accepted" }, sequence: 4, ordinal: 0,
    effective_actors: { killer: [{ participant: 2, status: "resolved" }], victim: [{ participant: 1, status: "resolved" }] },
    evidence: evidence("events"), mapped_replay_time: { status: "inside_media", replay_tick: "48000000" } }],
  roster: { source, local_participant: 1, coverage, game_tick: "0", observed_replay_tick: "0", age_replay_ticks: "48000000",
    evidence: evidence("player_list"), players: [
      { participant: 1, riot_id: "Same#A", champion_name: "Ahri", team: "ORDER", scores: { kills: 0, assists: 2 } },
      { participant: 2, riot_id: "Same#B", champion_name: "Garen", team: "CHAOS" },
    ] } });

describe("bounded League replay boundary", () => {
  it("rejects malformed death and spell facts at the replay boundary", () => {
    for (const fields of [{is_dead:1},{respawn_timer:-1},{respawn_timer:Infinity},
      {summoner_spells:{one:{id:"4"}}},{summoner_spells:{one:{display_name:7}}},
      {summoner_spells:{three:{display_name:"Flash"}}}]) {
      const input = wire(); Object.assign(input.roster.players[0],fields);
      expect(()=>decodeReplayWindow(input,source)).toThrow();
    }
  });
  it("preserves recorded discontinuous game clocks", () => {
    expect(recordedGameClock("1290000000")).toBe("21:30");
    expect(recordedGameClock("30000000")).toBe("0:30");
    expect(recordedGameClock("-1000000")).toBe("−0:01");
  });
  it("uses app associations for presentation while retaining unresolved producer evidence", () => {
    const input = wire();
    (input.events[0].revision.event.actors as EventActors).killer = [{participant: null, status:"unresolved"}];
    const window = decodeReplayWindow(input,source);
    const event = replayEvent(window.events[0],window,window.roster);
    expect(window.events[0].revision.event.actors?.killer?.[0].status).toBe("unresolved");
    expect(event.actors?.killer?.[0]).toEqual({participant:2,status:"resolved"});
    expect(event.relation).toBe("enemy");
  });
  it("preserves exact mapping, evidence, roster age and nullable scores with scoped actor keys", () => {
    const window = decodeReplayWindow(wire(), source);
    expect(window.events[0].replay_tick).toBe(48_000_000);
    expect(window.events[0].evidence).toEqual(evidence("events"));
    expect(window.roster?.age_replay_ticks).toBe(48_000_000);
    expect(window.roster?.players[0].scores?.kills).toBe(0);
    expect(window.roster?.players[0].scores?.deaths).toBeUndefined();
    const players = replayParticipants(window, window.identities);
    expect(players[0].is_local).toBe(true); expect(players[1].is_local).toBe(false);
    const event = replayEvent(window.events[0], window, window.roster);
    expect(event.relation).toBe("enemy");
    expect(eventInvolvesPlayer(event, participantKey(players[1].identity!))).toBe(true);
    expect(eventInvolvesPlayer(event, participantKey({ ...players[1].identity!, source: { ...source, epoch: 2 } }))).toBe(false);
    expect(eventInvolvesPlayer(event, "Same")).toBe(false);
  });

  it("keeps filter order and teams fixed across timed samples without inventing state at zero", () => {
    const window = decodeReplayWindow({ ...wire(), roster: null }, source);
    expect(window.roster).toBeNull();
    const players = replayParticipants(window, window.identities);
    expect(players.map(player => player.champion)).toEqual(["Ahri", "Garen"]);
    expect(players[0].is_local).toBe(true);
    expect(players[1].relation).toBe("enemy");
    const later = decodeReplayWindow(wire(), source);
    later.roster!.players.reverse();
    later.identities.reverse();
    expect(replayParticipants(later, later.identities)).toEqual(players);
    const unknownTeam = window.identities.map(player => ({ ...player, team: null }));
    expect(replayParticipants(window, unknownTeam).every(player => player.relation === "neutral")).toBe(true);
    expect(replayEvent(window.events[0], window, window.roster, window.identities).relation).toBe("enemy");
    expect(replayEvent(window.events[0], window, null, window.identities.slice(1)).relation).toBe("neutral");
    expect(replayEvent(window.events[0], window, null, window.identities.slice(0, 1)).relation).toBe("neutral");
    const unresolved = { ...window.events[0], effective_actors: { killer: [{ participant: null, status: "unresolved" as const }] } };
    expect(replayEvent(unresolved, window, null, window.identities).relation).toBe("neutral");
    const ambiguous = { ...window.events[0], effective_actors: { killer: [{ participant: 1, status: "resolved" as const }, { participant: 2, status: "resolved" as const }] } };
    expect(replayEvent(ambiguous, window, null, window.identities).relation).toBe("neutral");
  });

  it("rejects mixed scopes, expanded payloads and oversized event pages", () => {
    for (const change of [
      () => ({ ...wire(), source: { ...source, epoch: 2 } }),
      () => ({ ...wire(), unexpected: true }),
      () => ({ ...wire(), events: Array(513).fill(wire().events[0]) }),
      () => ({ ...wire(), identities: Array(65).fill(wire().identities[0]) }),
      () => ({ ...wire(), identities: [wire().identities[0], wire().identities[0]] }),
      () => ({ ...wire(), identities: [{ ...wire().identities[0], scores: { kills: 30 } }] }),
      () => ({ ...wire(), roster: { ...wire().roster, evidence: evidence("events") } }),
      () => ({ ...wire(), events: [{ ...wire().events[0], evidence: { ...evidence("events"), source: { ...source, epoch: 2 } } }] }),
    ]) expect(() => decodeReplayWindow(change(), source)).toThrow();
  });

  it("keeps missing small facts nullable and rejects a different recording", () => {
    const facts = { media_id: media, local_player_name: null, league_match: null, league_result: null };
    expect(decodeRecordingFacts(facts, media)).toEqual(facts);
    expect(() => decodeRecordingFacts({ ...facts, media_id: "22222222-2222-4333-8444-555555555555" }, media)).toThrow();
  });
});
