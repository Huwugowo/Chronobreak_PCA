// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { assertReplayProbeMatches, refreshLibrary, loadReplayDescriptor, loadPlaybackProbe, resolveClipDurations,
  setGameSaved, prepareImportedMusicPreview, releaseImportedMusicPreview } from "./api";
vi.mock("@tauri-apps/api/core", async original => ({ ...await original<typeof import("@tauri-apps/api/core")>(), invoke: vi.fn(), isTauri: vi.fn() }));
afterEach(() => { vi.unstubAllEnvs(); vi.resetAllMocks(); });
it("uses one development mock selection for snapshots, media, optional details and writes, including desktop preview", async () => {
  vi.mocked(isTauri).mockReturnValue(true); vi.stubEnv("VITE_UI_MOCKS", "1");
  const snapshot = await refreshLibrary(); const game = snapshot.games[0];
  const descriptor = await loadReplayDescriptor(game.timestamp, snapshot.token);
  const probe = await loadPlaybackProbe(game.timestamp, snapshot.token);
  expect(assertReplayProbeMatches(descriptor, probe)).toBe(probe);
  expect(descriptor.video_url).toContain("/mock-replay.mp4");
  expect(descriptor.media_timeline.video.timeBase.denominator).toBe(15360n);
  expect(descriptor.media_timeline.video.onePastLastPts).toBe(27648000n);
  expect(descriptor.media_timeline.video.profile).toBe("Constrained Baseline");
  expect(probe.participants).toEqual(game.participants);
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
