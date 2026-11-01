// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { createSignal, type ComponentProps } from "solid-js";
import { render } from "solid-js/web";
import ReplayScoreboard, { scoreboardSlots } from "./ReplayScoreboard";
import type { OverlaySample, ScoreboardFacts } from "../leagueIndex";
import type { ReplayTick } from "../replayTime";

afterEach(() => document.body.replaceChildren());
const facts = (item: number, count: number | null): ScoreboardFacts => ({ payload_id: item, scores: { kills: 0, deaths: null, assists: 3, creep_score: 12 },
  ward_score: 1.5, items: [{ item_id: item, slot: 0, count }] });
const sample = (seconds: number): OverlaySample => ({ replay_tick: seconds * 48_000_000 as ReplayTick,
  observation_id: seconds, game_tick: String(seconds * 1_000_000), local_scores: null,
  players: Array.from({ length: 10 }, (_, i) => ({ participant: i + 1, level: seconds, is_dead: null, respawn_timer: null, spells: ["Flash", null] })) });

it("shows ten fixed team rows, nullable stats and discrete inventories, reusing visited image nodes on backward seeks", () => {
  const host = document.createElement("div"); document.body.append(host);
  let update!: (patch: Partial<ComponentProps<typeof ReplayScoreboard>>) => void;
  const early = sample(2), late = sample(20), before = facts(2003, 2), after = facts(1001, 1);
  const dispose = render(() => {
    const [props, setProps] = createSignal<ComponentProps<typeof ReplayScoreboard>>({ open: true, assetBaseUrl: "http://assets.test/ddragon/16.20.1",
      identities: Array.from({ length: 10 }, (_, i) => ({ participant: i + 1, riot_id: `Player${i + 1}#TEST`, champion_name: "Ahri", team: i < 5 ? "ORDER" : "CHAOS" })),
      localParticipant: 1, sample: early, snapshot: new Map([[1, before]]), status: "ready" });
    update = patch => setProps(p => ({ ...p, ...patch }));
    return <ReplayScoreboard {...props()} />;
  }, host);
  try {
    const rows = [...host.querySelectorAll('[data-testid="scoreboard-player"]')];
    expect(rows).toHaveLength(10);
    expect(host.querySelectorAll('[aria-label="Blue team"] [data-participant]')).toHaveLength(5);
    expect(host.querySelectorAll('[aria-label="Red team"] [data-participant]')).toHaveLength(5);
    expect(rows[0].textContent).toContain("Player1"); expect(rows[0].textContent).not.toContain("#TEST");
    expect(rows[0].querySelector('[aria-label="Kills / Deaths / Assists"]')?.textContent).toBe("0 / — / 3");
    expect(rows[0].querySelector('[aria-label="Vision score"]')?.textContent).toBe("1.5");
    const slot = rows[0].querySelector('[data-slot="0"]')!;
    expect(slot.getAttribute("aria-label")).toBe("Item 2003 × 2");
    const potion = slot.querySelector("img")!;
    expect(potion.getAttribute("src")).toBe("http://assets.test/ddragon/16.20.1/item/2003");
    expect(rows[1].querySelector('[data-slot="0"]')?.getAttribute("aria-label")).toContain("unavailable");
    update({ sample: late, snapshot: new Map([[1, after]]) });
    expect(host.querySelector('[data-participant="1"]')).toBe(rows[0]);
    expect(potion.hidden).toBe(true);
    expect(slot.querySelectorAll("img")).toHaveLength(2);
    expect(slot.getAttribute("aria-label")).toBe("Item 1001 × 1");
    update({ sample: early, snapshot: new Map([[1, before]]) });
    expect(slot.querySelector('img[src$="/2003"]')).toBe(potion);
    expect(potion.hidden).toBe(false);
    update({ open: false });
    const text = host.textContent;
    update({ sample: late, snapshot: new Map([[1, after]]) });
    expect(host.textContent).toBe(text); // No closed-panel snapshot work or new assets.
    update({ open: true });
    expect(slot.getAttribute("aria-label")).toBe("Item 1001 × 1");
    update({ sample: null, snapshot: null });
    expect(rows[0].querySelector('[aria-label="Level"]')?.textContent).toBe("—");
    expect(rows[0].querySelector('[aria-label="Kills / Deaths / Assists"]')?.textContent).toBe("— / — / —");
    expect(slot.getAttribute("aria-label")).toContain("unavailable");
    expect(host.textContent).not.toMatch(/gold/i);
    expect(host.querySelectorAll("video, canvas")).toHaveLength(0);
  } finally { dispose(); }
});

it("keeps missing, empty, conflicting and unplaceable recorded inventory distinct", () => {
  expect(scoreboardSlots(null)).toEqual(Array(7).fill(null));
  expect(scoreboardSlots([])).toEqual(Array(7).fill("empty"));
  expect(scoreboardSlots([{ item_id: 2003, slot: null, count: 2 }])).toEqual(Array(7).fill(null));
  const slots = scoreboardSlots([{ item_id: 2003, slot: 0, count: null }, { item_id: null, slot: 6, count: 1 }]);
  expect(slots[0]).toEqual({ item_id: 2003, slot: 0, count: null });
  expect(slots[1]).toBe("empty"); expect(slots[6]).toEqual({ item_id: null, slot: 6, count: 1 });
  expect(scoreboardSlots([{ item_id: 2003, slot: 0, count: 2 }, { item_id: 1001, slot: 0, count: 1 }])[0]).toBeNull();
});

it("defers identities and late asset readiness while a newly selected source is closed", () => {
  const host = document.createElement("div"); document.body.append(host);
  let update!: (patch: Partial<ComponentProps<typeof ReplayScoreboard>>) => void;
  const dispose = render(() => {
    const [props, setProps] = createSignal<ComponentProps<typeof ReplayScoreboard>>({ open: false, assetBaseUrl: null,
      identities: [], localParticipant: 1, sample: null, snapshot: null, status: "idle" });
    update = patch => setProps(p => ({ ...p, ...patch }));
    return <ReplayScoreboard {...props()} />;
  }, host);
  try {
    update({ identities: [{ participant: 1, riot_id: "Player#TEST", champion_name: "Ahri", team: "ORDER" }], assetBaseUrl: "http://assets.test" });
    expect(host.querySelectorAll("img")).toHaveLength(0);
    expect(host.querySelectorAll('[data-testid="scoreboard-player"]')).toHaveLength(0);
    update({ open: true });
    const image = host.querySelector("img")!;
    expect(image.getAttribute("src")).toBe("http://assets.test/champion/Ahri");
    update({ open: false }); update({ assetBaseUrl: "http://new-assets.test" });
    expect(host.querySelectorAll("img")).toHaveLength(1);
    expect(host.querySelector("img")).toBe(image);
  } finally { dispose(); }
});
