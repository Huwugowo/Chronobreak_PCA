// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { render } from "solid-js/web";
import LibraryScreen from "./LibraryScreen";
import type { DdragonStatus, GameSummary } from "../types";

afterEach(() => document.body.replaceChildren());

it("renders Games from the core snapshot when Data Dragon enrichment fails", () => {
  const game: GameSummary = {
    timestamp: "100",
    champion: "Ahri",
    game_mode: "CLASSIC",
    duration_ms: 120_000,
    recorded_at: "2026-01-01T00:00:00Z",
    kills: 2,
    deaths: 1,
    assists: 3,
    summoner_spells: [],
    keystone_id: null,
    items: [],
    saved: false,
    incomplete: false,
    video_size_bytes: 1,
    video_available: true,
  };
  const ddragon: DdragonStatus = {
    state: "offline",
    version: null,
    asset_base_url: null,
    item_count: 0,
    champion_count: 0,
    cache_directory: "",
    error: "Data Dragon unavailable",
  };
  const host = document.createElement("div");
  document.body.append(host);
  const dispose = render(() => <LibraryScreen
    tab="games"
    games={[game]}
    clips={[]}
    ddragon={ddragon}
    actionable={true}
    busyId={null}
    snapshotOrigin={{ root: "A", rootEpoch: 1, request: 1, token: "token-a", navigation: 1 }}
    onOpenGame={() => {}}
    onToggleSaved={() => {}}
    onDeleteGame={() => {}}
    onOpenClip={() => {}}
    onDeleteClip={() => {}}
    onOpenClipsFolder={() => {}}
  />, host);
  expect(host.textContent).toContain("Ahri");
  expect(host.textContent).toContain("MATCH ARCHIVE");
  dispose();
});
