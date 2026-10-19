// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { refreshLibrary, loadReplayDescriptor, resolveClipDurations,
  setGameSaved, prepareImportedMusicPreview, releaseImportedMusicPreview } from "./api";
import { queryHistory, queryReplayWindow, sourceScopes } from "./leagueIndex";
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
  expect(later.roster?.observed_replay_tick).toBe(0);
  expect(later.roster?.age_replay_ticks).toBe(query.at + 120000000);
  expect((await queryReplayWindow(snapshot.token, game.timestamp, { ...query, start: parseReplayTick("3000000000") })).events).toEqual([]);
  await expect(queryReplayWindow(snapshot.token, game.timestamp, { ...query, scope: { ...source, epoch: 1 } })).rejects.toThrow("source changed");
  const unknown = await queryHistory(snapshot.token, { limit: 50, event_kind: "DragonKill" });
  expect(unknown.unknown_recordings).toBeGreaterThan(0);
  expect(invoke).not.toHaveBeenCalled();
});
