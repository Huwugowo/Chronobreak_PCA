// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
// @ts-expect-error Node filesystem is available only in the test runner.
import { readFileSync } from "node:fs";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { refreshLibrary, loadReplayDescriptor, resolveClipDurations,
  setGameSaved, prepareImportedMusicPreview, releaseImportedMusicPreview, loadDdragonStatus, resolveItemName } from "./api";
import { queryHistory, queryOverlayPage, queryReplayWindow, sourceScopes } from "./leagueIndex";
import { replayEvent, replayParticipants } from "./replayProjection";
import { parseReplayTick } from "./replayTime";
vi.mock("@tauri-apps/api/core", async original => ({ ...await original<typeof import("@tauri-apps/api/core")>(), invoke: vi.fn(), isTauri: vi.fn() }));
afterEach(() => { vi.unstubAllEnvs(); vi.resetAllMocks(); });
it("uses one development mock selection for snapshots, media, optional details and writes, including desktop preview", async () => {
  vi.mocked(isTauri).mockReturnValue(true); vi.stubEnv("VITE_UI_MOCKS", "1");
  const snapshot = await refreshLibrary(); const game = snapshot.games[0];
  const descriptor = await loadReplayDescriptor(game.timestamp, snapshot.token);
  expect(descriptor.video_url).toContain("/mock-replay.mp4");
  expect(descriptor.media_timeline.video.timeBase.denominator).toBe(15360n);
  expect(descriptor.media_timeline.video.onePastLastPts).toBe(27648000n);
  expect(descriptor.media_timeline.video.profile).toBe("Constrained Baseline");
  expect((await resolveClipDurations(snapshot.token, snapshot.clips.map(c => c.filename))).snapshot_token).toBe(snapshot.token);
  await setGameSaved(game.timestamp, !game.saved, snapshot.token);
  expect((await refreshLibrary()).games.find(g => g.timestamp === game.timestamp)?.saved).toBe(!game.saved);
  const music = await prepareImportedMusicPreview("preview.mp3"); await releaseImportedMusicPreview(music.token);
  expect(invoke).not.toHaveBeenCalled();
});
it("ignores the development mock switch in production and retains tokenized native mutations/releases", async () => {
  vi.mocked(isTauri).mockReturnValue(true); vi.stubEnv("DEV", false); vi.stubEnv("VITE_UI_MOCKS", "1");
  vi.mocked(invoke).mockResolvedValue(undefined);
  await setGameSaved("1", true, "native"); await releaseImportedMusicPreview("owned");
  expect(invoke).toHaveBeenCalledWith("save_game", { gameTimestamp: "1", saved: true, snapshotToken: "native" });
  expect(invoke).toHaveBeenCalledWith("release_imported_music_preview", { token: "owned" });
});
it("keeps mock event identity and time fixed when replay windows move", async () => {
  vi.mocked(isTauri).mockReturnValue(true); vi.stubEnv("VITE_UI_MOCKS", "1");
  const snapshot = await refreshLibrary(); const game = snapshot.games[0];
  const [{ source }] = await sourceScopes(snapshot.token, game.timestamp);
  const query = { scope: source, start: parseReplayTick("0"), end: parseReplayTick("4800000000"), at: parseReplayTick("2880000000"), limit: 1 };
  const first = await queryReplayWindow(snapshot.token, game.timestamp, query);
  const later = await queryReplayWindow(snapshot.token, game.timestamp, { ...query, start: parseReplayTick("2400000000"), at: parseReplayTick("3000000000") });
  expect(first.events[0].revision).toEqual(later.events[0].revision);
  expect(first.events[0].replay_tick).toBe(later.events[0].replay_tick);
  expect(later.roster?.observed_replay_tick).toBe(2880000000);
  expect(later.roster?.age_replay_ticks).toBe(120000000);
  const overlay = await queryOverlayPage(snapshot.token, game.timestamp, { scope: source, after: null, revision: null, limit: 128 });
  const continuation = await queryOverlayPage(snapshot.token, game.timestamp, { scope: source, after: overlay.next, revision: overlay.revision, limit: 128 });
  expect(overlay.samples).toHaveLength(128);
  expect(continuation.samples).toHaveLength(53);
  expect(overlay.identities).toHaveLength(10);
  expect(continuation.identities).toBeNull();
  expect(continuation.next).toBeNull();
  expect(overlay.samples[6].replay_tick).toBe(later.roster?.observed_replay_tick);
  expect(overlay.samples[6].players[0].level).toBe(later.roster?.players[0].level);
  expect((await queryReplayWindow(snapshot.token, game.timestamp, { ...query, start: parseReplayTick("3000000000") })).events).toEqual([]);
  await expect(queryReplayWindow(snapshot.token, game.timestamp, { ...query, scope: { ...source, epoch: 1 } })).rejects.toThrow("source changed");
  const unknown = await queryHistory(snapshot.token, { limit: 50, event_kind: "DragonKill" });
  expect(unknown.unknown_recordings).toBeGreaterThan(0);
  expect(invoke).not.toHaveBeenCalled();
});
it("restores diverse timeline events, paginates stable identities, and supplies filterable roster state", async () => {
  vi.mocked(isTauri).mockReturnValue(true); vi.stubEnv("VITE_UI_MOCKS", "1");
  const snapshot = await refreshLibrary(); const game = snapshot.games[0];
  const [scope] = await sourceScopes(snapshot.token, game.timestamp);
  const query = { scope: scope.source, start: parseReplayTick("0"), end: parseReplayTick("86400000000"), at: parseReplayTick("43200000000"), limit: 512 };
  const full = await queryReplayWindow(snapshot.token, game.timestamp, query);
  expect(full.events.length).toBeGreaterThan(10);
  expect(new Set(full.events.map(e => e.revision.event.name)).size).toBeGreaterThan(4);
  expect(full.events[full.events.length - 1].replay_tick).toBeGreaterThan(1500 * 48_000_000);
  const participants = replayParticipants(full, full.identities);
  expect(participants).toHaveLength(10);
  expect(participants.filter(p => p.relation === "enemy")).toHaveLength(5);
  expect(full.roster?.players[0].items).toHaveLength(7);
  expect(full.roster?.players[0].scores?.kills).toBeGreaterThan(0);
  expect(full.events.map(e => replayEvent(e, scope, full.roster)).some(e => e.relation === "enemy")).toBe(true);
  const first = await queryReplayWindow(snapshot.token, game.timestamp, { ...query, limit: 3 });
  const next = await queryReplayWindow(snapshot.token, game.timestamp, { ...query, after: first.next, limit: 3 });
  expect([...first.events, ...next.events]).toEqual(full.events.slice(0, 6));
  expect(invoke).not.toHaveBeenCalled();
});
it("provides offline icon URLs and item names for both browser and desktop previews", async () => {
  for (const desktop of [false, true]) {
    vi.mocked(isTauri).mockReturnValue(desktop); vi.stubEnv("VITE_UI_MOCKS", "1");
    const status = await loadDdragonStatus();
    expect(new URL(status.asset_base_url!).origin).toBe(window.location.origin);
    expect(status.asset_base_url).toContain("/mock-ddragon");
    expect(await resolveItemName("3089")).toBe("Rabadon's Deathcap");
    const games = (await refreshLibrary()).games.filter(game => !game.incomplete);
    const assets = new Set(games.flatMap(game => [
      `champion/${game.champion}`, ...game.participants.map(p => `champion/${p.champion}`),
      ...game.items.map(item => `item/${item.item_id}`), ...game.summoner_spells.map(spell => `spell/${spell}`),
      `rune/${game.keystone_id}`,
    ]));
    for (const asset of assets) {
      const bytes = readFileSync(`public/mock-ddragon/${asset}`);
      expect([...bytes.subarray(0, 8)], asset).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
    }
  }
  expect(invoke).not.toHaveBeenCalled();
});
