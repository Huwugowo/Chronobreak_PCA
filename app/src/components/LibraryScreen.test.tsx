// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
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
    items: [], participants: [{ summoner_name: "Ally", champion: "Lux", relation: "ally" },
      { summoner_name: "Enemy", champion: "Garen", relation: "enemy" },
      { summoner_name: "Unknown team", champion: "Unknown", relation: "neutral" }],
    saved: false,
    incomplete: false,
    video_size_bytes: 1,
    video_available: true,
    league_result: { outcome: "unknown", ended_early: true, partial: true },
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
  const origin = { root: "A", rootEpoch: 1, request: 1, token: "token-a", navigation: 1 };
  const [actionable, setActionable] = createSignal(true);
  const opened = vi.fn(), saved = vi.fn(), deleted = vi.fn();
  const dispose = render(() => <LibraryScreen
    tab="games"
    games={[game]}
    clips={[]}
    ddragon={ddragon}
    actionable={actionable()}
    busyId={null}
    snapshotOrigin={origin}
    onOpenGame={opened}
    onToggleSaved={saved}
    onDeleteGame={deleted}
    onOpenClip={() => {}}
    onDeleteClip={() => {}}
    onOpenClipsFolder={() => {}}
  />, host);
  expect(host.textContent).toContain("Ahri");
  expect(host.textContent).toContain("Unknown");
  expect(host.textContent).not.toContain("Defeat");
  expect(host.textContent).not.toContain("Remake");
  expect(host.querySelector('[aria-label="Match History"]')).not.toBeNull();
  const roster = host.querySelector('[aria-label="Team rosters"]')!;
  expect(roster.querySelector('[data-team="ally"]')?.textContent).toContain("Ally");
  expect(roster.querySelector('[data-team="enemy"]')?.textContent).toContain("Enemy");
  expect(roster.textContent).not.toContain("Unknown team");
  const open = host.querySelector<HTMLButtonElement>('[aria-label^="Open Ahri"]')!;
  const save = host.querySelector<HTMLButtonElement>('[aria-label="Save Ahri recording"]')!;
  const remove = [...host.querySelectorAll("button")].find(button => button.textContent?.includes("Delete recording"))!;
  open.click(); save.click(); remove.click();
  expect(opened).toHaveBeenCalledWith("100", origin);
  expect(saved).toHaveBeenCalledWith(game, origin);
  expect(deleted).toHaveBeenCalledWith(game, origin);
  setActionable(false);
  for (const button of [open, save, remove]) { expect(button.disabled).toBe(true); button.click(); }
  expect(opened).toHaveBeenCalledTimes(1);
  expect(saved).toHaveBeenCalledTimes(1);
  expect(deleted).toHaveBeenCalledTimes(1);
  dispose();
});
