// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { Show, type ComponentProps } from "solid-js";
import App from "./App";
import type ViewerScreen from "./components/ViewerScreen";
import type LibraryScreen from "./components/LibraryScreen";
import type ClipExporterScreen from "./components/ClipExporterScreen";
import { ensureHevcCapability, resolveClipDurations } from "./api";
import { initializeReplayBenchmark } from "./benchmark";
import { parseMediaId, type FrameBoundary } from "./replayTime";
import { indexCatalog, indexRecording } from "./leagueIndex";
import type { CatalogPage, ImportProgress } from "./leagueIndex";
import { editManagedClip, readClipDetail, readClipGridPage, readClipPage, readClipPlayback, reconcileClips,
  type ClipCard, type ClipGridCard, type ClipPage, type ClipPlayback } from "./managedClips";

const harness = vi.hoisted(() => ({ root: "A", revision: 0,
  viewer: null as ComponentProps<typeof ViewerScreen> | null,
  exporter: null as ComponentProps<typeof ClipExporterScreen> | null,
  clip: null as ClipCard | null, sourceMissing: false, requireHistoryPage: false, failReplay: false,
  renderManagedGrid: false,
}));
vi.mock("./managedClips", async original => ({ ...await original<typeof import("./managedClips")>(),
  usesManagedClips: () => harness.clip !== null,
  reconcileClips: vi.fn(async () => true),
  readClipPage: vi.fn(async (): Promise<ClipPage> => ({ groups: [], next: null })),
  readClipGridPage: vi.fn(),
  readClipPlayback: vi.fn(),
  readClipDetail: vi.fn(),
  editManagedClip: vi.fn(async () => {}),
}));
vi.mock("./benchmark", async original => ({ ...await original<typeof import("./benchmark")>(),
  initializeReplayBenchmark: vi.fn(async () => null) }));
vi.mock("./leagueIndex", async original => ({ ...await original<typeof import("./leagueIndex")>(),
  indexCatalog: vi.fn(async (token: string) => ({ snapshot_token: token, items: [], next: null, catalog_recordings: 1 })),
  indexRecording: vi.fn(async () => ({ status: "ready", batches: 0, position: null, tail_bytes: 0 })),
  queryHistory: vi.fn(async () => ({ rows: [], next: null, indexed_recordings: 0, catalog_recordings: 1, unknown_recordings: 1 })) }));
vi.mock("./api", async original => ({ ...await original<typeof import("./api")>(),
  loadSettings: async () => ({ output_path: harness.root, auto_delete_days: 0 }),
  loadDdragonStatus: () => new Promise(() => {}),
  loadReplayDescriptor: vi.fn(async () => { throw new Error("Fixture recording unavailable"); }),
  ensureHevcCapability: vi.fn(() => new Promise(() => {})),
  chooseOutputFolder: async () => "B",
  persistSettings: async (value: { output_path: string }) => { harness.root = value.output_path; return value; },
  refreshLibrary: async () => {
    const game = {
    timestamp: "1", champion: harness.root, game_mode: "CLASSIC", recorded_at: "2026-09-21T00:00:00Z",
    kills: 0, deaths: 0, assists: 0, duration_ms: 10000, summoner_spells: [], keystone_id: null,
    items: [], participants: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true,
    league_result: null,
    };
    const source = harness.clip?.header;
    return { token: `${harness.root}-${++harness.revision}`,
      games: source && !harness.sourceMissing ? [game, {
        ...game, timestamp: source.source_timestamp, media_id: source.range.media_id,
      }] : [game],
      clips: [{ filename: "1_2.mp4" }], managed_clip_count: source ? 1 : 0,
      usage: { games_bytes: 1, clips_bytes: 1, game_count: 1, clip_count: 1 } };
  },
  resolveClipDurations: vi.fn(() => new Promise(() => {})),
}));
vi.mock("./components/LibraryScreen", () => ({ default: (props: ComponentProps<typeof LibraryScreen>) =>
  <>
    <button data-testid="open" onClick={() => props.onOpenGame("1", props.snapshotOrigin)}>Open recording</button>
    <Show when={harness.renderManagedGrid && props.tab === "clips"}>{props.managedClips}</Show>
    <Show when={harness.clip && (!harness.requireHistoryPage || props.history?.page)}>
      <button data-testid="play-clip" onClick={event => props.onOpenManagedClip?.(harness.clip!, event.currentTarget)}>Play clip</button>
    </Show>
    {harness.clip && <button data-testid="play-single" disabled={!props.actionable || !props.snapshotOrigin} onClick={event => props.onPlaySingleClip?.(
      harness.clip!.header.game.match_group!, props.snapshotOrigin!, event.currentTarget)}>Play clip from History</button>}
  </> }));
vi.mock("./components/SettingsScreen", () => ({ default: (props: { onBack: () => void; onChooseFolder: () => void }) => <>
  <button data-testid="settings-back" onClick={props.onBack}>Back</button>
  <button data-testid="root" onClick={props.onChooseFolder}>Change root</button>
</> }));
vi.mock("./components/StorageIndicator", () => ({ default: () => null }));
vi.mock("./components/ViewerScreen", async original => {
  const { default: RealViewerScreen } = await original<typeof import("./components/ViewerScreen")>();
  return { default: (props: ComponentProps<typeof ViewerScreen>) => {
    harness.viewer = props;
    return harness.failReplay ? <RealViewerScreen {...props} /> : <div data-testid="viewer">{props.game.champion}</div>;
  } };
});
vi.mock("./components/ClipExporterScreen", () => ({ default: (props: ComponentProps<typeof ClipExporterScreen>) => {
  harness.exporter = props;
  return <div data-testid="exporter" />;
} }));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.(); dispose = undefined;
  document.body.replaceChildren(); localStorage.clear(); vi.clearAllMocks(); vi.restoreAllMocks();
  vi.mocked(readClipGridPage).mockReset();
  vi.mocked(readClipDetail).mockReset();
  vi.mocked(reconcileClips).mockReset();
  harness.root = "A"; harness.revision = 0; harness.viewer = null; harness.exporter = null;
  harness.clip = null; harness.sourceMissing = false; harness.requireHistoryPage = false; harness.failReplay = false;
  harness.renderManagedGrid = false;
});
const mount = async () => {
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => <App />, host);
  await vi.waitFor(() => expect(host.querySelector('[data-testid="open"]')).not.toBeNull());
  const headerControls: Record<string, string> = {
    home: 'header button[aria-label^="Back to"]',
    settings: 'header button[aria-label="Open settings"]',
    clips: 'nav[aria-label="Library"] button:nth-of-type(2)',
    games: 'nav[aria-label="Library"] button:nth-of-type(1)',
  };
  return { host, click: (id: string) =>
    host.querySelector<HTMLButtonElement>(headerControls[id] ?? `[data-testid="${id}"]`)!.click() };
};

const media = "22222222-3333-4444-8555-666666666666";
const makeClip = (): ClipCard => ({
    title: "Last fight", video_url: "http://fixture/clip.mp4", thumbnail_url: "http://fixture/thumb.jpg",
    header: { version: 1, id: "clip", source_timestamp: "1-2", preset: "horizontal", exported_at: 0,
      range: { media_id: media, start_frame: "420", end_frame_exclusive: "720" },
      game: { match_group: "fixture", champion: "A", played_at: null, part_recorded_at: null, account: null,
        queue_id: null, mode: null, outcome: null, final_kda: null },
      duration: "240000000", events: "unavailable", highlights: [], custom_tags: [] },
});
const gridCard = (clip: ClipCard): ClipGridCard => {
  const { id, duration, custom_tags, highlights } = clip.header;
  return { id, duration, custom_tags, highlights, title: clip.title,
    champion: clip.header.game.champion, thumbnail_url: clip.thumbnail_url };
};

it.each([false, true])("keeps the clip playable and only opens its exact source part (missing: %s)", async sourceMissing => {
  harness.sourceMissing = sourceMissing;
  harness.clip = makeClip();
  harness.requireHistoryPage = true;
  const { host, click } = await mount();
  await vi.waitFor(() => expect(host.querySelector('[data-testid="play-clip"]')).not.toBeNull());
  const trigger = host.querySelector<HTMLButtonElement>('[data-testid="play-clip"]')!;
  click("play-clip");
  expect(host.querySelector("video")?.src).toBe("http://fixture/clip.mp4");
  const source = [...host.querySelectorAll("button")].find(button => button.textContent?.toLowerCase().includes("source replay"))!;
  expect(source.disabled).toBe(sourceMissing);
  if (sourceMissing) { expect(harness.viewer).toBeNull(); return; }
  source.click();
  expect(harness.viewer!.game.timestamp).toBe("1-2");
  expect(harness.viewer!.initialSourcePosition).toMatchObject({ gameTimestamp: "1-2", mediaId: media,
    startFrame: 420, endFrameExclusive: 720 });
  expect(harness.viewer!.initialClipDraft).toBeUndefined();
  click("home");
  await vi.waitFor(() => expect(document.activeElement).toBe(trigger));
  expect(host.querySelector('[data-testid="play-clip"]')).toBe(trigger);
});

it.each([
  { tab: "games", destination: "Match History" },
  { tab: "clips", destination: "Clips" },
])("uses the same return destination in the header and source failure from $destination", async ({ tab, destination }) => {
  harness.clip = makeClip();
  harness.failReplay = true;
  const { host, click } = await mount();
  click(tab);
  const trigger = host.querySelector<HTMLButtonElement>('[data-testid="play-clip"]')!;
  trigger.click();
  [...host.querySelectorAll("button")].find(button => button.textContent === "Open source replay")!.click();
  const label = "Back to " + destination;
  await vi.waitFor(() => expect(host.querySelector('[role="alert"] h1')?.textContent).toBe("Source replay unavailable."));
  expect(host.querySelector("header button[aria-label='" + label + "']")).not.toBeNull();
  const back = [...host.querySelectorAll<HTMLButtonElement>('[role="alert"] button')]
    .find(button => button.textContent === label)!;
  back.click();
  expect(host.querySelector('nav [aria-current="page"]')?.textContent).toBe(destination);
  await vi.waitFor(() => expect(document.activeElement).toBe(trigger));
});

it("keeps focus in the managed player and its nested confirmation", async () => {
  harness.clip = makeClip();
  const { host, click } = await mount();
  const trigger = host.querySelector<HTMLButtonElement>('[data-testid="play-clip"]')!;
  click("play-clip");
  const player = host.querySelector('[aria-labelledby="managed-clip-title"]')!;
  const close = player.querySelector("header button")!;
  const deletion = [...player.querySelectorAll("button")].find(button => button.textContent === "Delete clip")!;
  const press = (key: string, shiftKey = false) => document.activeElement!.dispatchEvent(
    new KeyboardEvent("keydown", { key, shiftKey, bubbles: true, cancelable: true }));
  expect(document.activeElement).toBe(close);
  press("Tab", true);
  expect(document.activeElement).toBe(player.querySelector("summary"));
  press("Tab");
  expect(document.activeElement).toBe(close);

  deletion.click();
  const confirmation = host.querySelector('[role="alertdialog"]')!;
  const [cancel, confirm] = confirmation.querySelectorAll("button");
  expect(document.activeElement).toBe(cancel);
  press("Tab", true);
  expect(document.activeElement).toBe(confirm);
  press("Tab");
  expect(document.activeElement).toBe(cancel);
  trigger.focus();
  expect(document.activeElement).toBe(cancel);
  press("Escape");
  expect(host.querySelector('[role="alertdialog"]')).toBeNull();
  await vi.waitFor(() => expect(document.activeElement).toBe(deletion));
  press("Escape");
  expect(host.querySelector('[role="dialog"]')).toBeNull();
  await vi.waitFor(() => expect(document.activeElement).toBe(trigger));
});

it("opens the sole clip once, discards a lookup after leaving History and restores focus", async () => {
  const clip = makeClip();
  harness.clip = clip;
  const page = { groups: [{ key: "fixture", clips: [clip], next: null }], next: null };
  let release!: (value: ClipPage) => void;
  vi.mocked(readClipPage).mockImplementationOnce(() => new Promise(resolve => { release = resolve; }))
    .mockResolvedValueOnce(page);
  const { host, click } = await mount();
  expect(readClipPage).not.toHaveBeenCalled();
  click("play-single");
  click("play-single");
  expect(readClipPage).toHaveBeenCalledExactlyOnceWith("A-1", { match_group: "fixture" });
  click("settings");
  release(page);
  await Promise.resolve();
  expect(host.querySelector("video")).toBeNull();
  click("settings-back");
  await vi.waitFor(() => expect(host.querySelector<HTMLButtonElement>('[data-testid="play-single"]')?.disabled).toBe(false));
  click("play-single");
  await vi.waitFor(() => expect(host.querySelector("video")?.src).toBe(clip.video_url));
  host.querySelector<HTMLButtonElement>('[aria-labelledby="managed-clip-title"] header button')!.click();
  await vi.waitFor(() => expect(document.activeElement).toBe(host.querySelector('[data-testid="play-single"]')));
});

it.each([false, true])("opens a grid clip in place with saved context and accessible actions (missing metadata: %s)", async missing => {
  harness.renderManagedGrid = true;
  harness.sourceMissing = true;
  const clip = makeClip();
  harness.clip = clip;
  if (missing) clip.header.game.champion = null;
  else Object.assign(clip.header.game, {
    champion: "Ahri", outcome: "win", queue_id: 420, played_at: "2026-07-03T12:00:00Z",
    final_kda: [7, 0, 3], summoner_spells: ["4", "14"], final_items: [1001], duration_seconds: 1_503,
  });
  clip.header.custom_tags = ["one", "two", "three"];
  const card = gridCard(clip);
  vi.mocked(readClipGridPage).mockResolvedValueOnce({ clips: [card], next: null });
  vi.mocked(readClipPlayback).mockResolvedValueOnce({ clip, file_size_bytes: 3_500_000 });
  const { host, click } = await mount();
  click("clips");
  const lookup = () => host.querySelector<HTMLButtonElement>(`[data-managed-clip="${card.id}"]`);
  await vi.waitFor(() => expect(lookup()).not.toBeNull());
  const trigger = lookup()!;
  const thumbnail = trigger.querySelector("img");
  expect(readClipPlayback).not.toHaveBeenCalled();
  trigger.focus();
  trigger.click();
  const dialog = () => host.querySelector('[role="dialog"]');
  await vi.waitFor(() => expect(dialog()).not.toBeNull());
  expect(readClipPlayback).toHaveBeenCalledExactlyOnceWith("A-1", card.id);
  const player = dialog()!;
  const summary = player.querySelector('[aria-label="Match summary"]')!;
  const video = player.querySelector("video")!;
  const details = player.querySelector("#managed-clip-title")!.parentElement!;
  expect(summary.compareDocumentPosition(video) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(video.compareDocumentPosition(details) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(video.src).toBe(clip.video_url);
  expect(details.textContent).toContain("0:05 · 3.50 MB");
  if (missing) {
    expect(summary.textContent).toMatch(/Unknown result.*Queue unavailable.*KDA unavailable.*Final items unavailable.*Date unavailable.*Duration unavailable/);
    expect(summary.querySelector('[aria-label="Champion unavailable"]')).not.toBeNull();
  } else {
    expect(summary.textContent).toMatch(/Victory.*Ranked Solo\/Duo.*7 \/ 0 \/ 3.*25m 3s/);
    expect(summary.querySelector("[aria-label='Ahri']")).not.toBeNull();
    expect(summary.querySelector("[aria-label='4']")).not.toBeNull();
  }
  expect(player.textContent).toContain("onetwothree");
  const buttons = [...player.querySelectorAll("button")];
  for (const label of ["Rename", "Edit tags", "Show in folder", "Delete clip"]) {
    expect(buttons.find(button => button.textContent === label)?.disabled).toBe(false);
  }
  expect(buttons.find(button => button.textContent === "Source replay unavailable")?.disabled).toBe(true);

  document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(document.activeElement).toBe(trigger));
  expect(dialog()).toBeNull();
  expect(lookup()).toBe(trigger);
  expect(trigger.querySelector("img")).toBe(thumbnail);
  expect(readClipGridPage).toHaveBeenCalledExactlyOnceWith("A-1", {}, null);
});

it("preserves the loaded Clips page and restores its scroll position on return from the source replay", async () => {
  harness.renderManagedGrid = true;
  const clip = makeClip();
  harness.clip = clip;
  const card = gridCard(clip);
  vi.mocked(readClipGridPage).mockResolvedValueOnce({ clips: [{ ...card, id: "first" }], next: [1, "first"] })
    .mockResolvedValueOnce({ clips: [card], next: null });
  vi.mocked(readClipPlayback).mockResolvedValueOnce({ clip, file_size_bytes: 512 });
  let scrollY = 0;
  vi.spyOn(window, "scrollY", "get").mockImplementation(() => scrollY);
  const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  const { host, click } = await mount();
  const button = (label: string) => [...host.querySelectorAll("button")].find(element => element.textContent?.trim() === label);
  click("clips");
  await vi.waitFor(() => expect(button("More clips")).toBeDefined());
  button("More clips")!.click();
  const opener = () => host.querySelector<HTMLButtonElement>('[data-managed-clip="clip"]');
  await vi.waitFor(() => expect(opener()).not.toBeNull());
  const trigger = opener()!;
  scrollY = 920;
  trigger.click();
  await vi.waitFor(() => expect(button("Open source replay")).toBeDefined());
  button("Open source replay")!.click();
  expect(harness.viewer?.initialSourcePosition?.gameTimestamp).toBe(clip.header.source_timestamp);
  expect(trigger.isConnected).toBe(true);
  scrollY = 0;
  click("home");
  await vi.waitFor(() => expect(document.activeElement).toBe(trigger));
  expect(scrollTo).toHaveBeenCalledExactlyOnceWith({ top: 920, behavior: "instant" });
  expect(opener()).toBe(trigger);
  expect(readClipGridPage).toHaveBeenCalledTimes(2);
});

it("discards a pending grid activation after navigating away", async () => {
  harness.renderManagedGrid = true;
  const clip = makeClip();
  harness.clip = clip;
  vi.mocked(readClipGridPage).mockResolvedValueOnce({ clips: [gridCard(clip)], next: null });
  let complete!: (value: ClipPlayback) => void;
  vi.mocked(readClipPlayback).mockImplementationOnce(() => new Promise(resolve => { complete = resolve; }));
  const { host, click } = await mount();
  click("clips");
  await vi.waitFor(() => expect(host.querySelector("[data-managed-clip]")).not.toBeNull());
  host.querySelector<HTMLButtonElement>("[data-managed-clip]")!.click();
  expect(readClipPlayback).toHaveBeenCalledOnce();
  click("games");
  complete({ clip, file_size_bytes: 512 });
  await Promise.resolve();
  expect(host.querySelector('[role="dialog"]')).toBeNull();
});

it.each(["rename", "tags", "delete"] as const)("reconciles %s edits without clearing the open, filtered grid", async operation => {
  harness.renderManagedGrid = true;
  const clip = makeClip();
  harness.clip = clip;
  clip.header.custom_tags = ["Keep"];
  const edited = gridCard(clip);
  const first = { ...edited, id: "first", title: "Keep first", thumbnail_url: "http://fixture/first.jpg" };
  const replacement = { ...first, id: "replacement", title: "Keep next" };
  const tail = { ...first, id: "tail", title: "Keep last" };
  const renamed = { ...edited, title: "Renamed fight" };
  const second = operation === "rename" ? renamed : replacement;
  const refreshedCursor: [number, string] = operation === "rename" ? [3, edited.id] : [2, replacement.id];
  vi.mocked(readClipGridPage).mockImplementation(async (token, query, after) => {
    if (!query.search) return { clips: [], next: null };
    if (!after) return { clips: [first], next: [4, first.id] };
    if (token === "A-1") return { clips: [edited], next: [3, edited.id] };
    if (after[1] === first.id) return { clips: [second], next: refreshedCursor };
    return { clips: [tail], next: null };
  });
  vi.mocked(readClipPlayback).mockResolvedValueOnce({ clip, file_size_bytes: 512 });
  if (operation === "tags") {
    vi.mocked(readClipDetail).mockResolvedValueOnce({ header: clip.header, candidates: [], events: { availability: "unavailable", events: [] } });
  }
  const { host, click } = await mount();
  const button = (label: string, within: ParentNode = host) => [...within.querySelectorAll("button")]
    .find(element => element.textContent?.trim() === label);
  const card = (id: string) => host.querySelector<HTMLButtonElement>(`[data-managed-clip="${id}"]`);
  click("clips");
  const search = host.querySelector<HTMLInputElement>('input[type="search"]')!;
  search.value = "keep";
  search.dispatchEvent(new Event("input", { bubbles: true }));
  await vi.waitFor(() => expect(card(first.id)).not.toBeNull());
  button("More clips")!.click();
  await vi.waitFor(() => expect(card(edited.id)).not.toBeNull());
  const firstButton = card(first.id)!;
  const firstThumbnail = firstButton.querySelector("img");
  const editButton = card(edited.id)!;
  const editThumbnail = editButton.querySelector("img");
  const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  editButton.click();
  await vi.waitFor(() => expect(button("Rename")).toBeDefined());

  let acceptIndex!: (ready: boolean) => void;
  vi.mocked(reconcileClips).mockImplementationOnce(() => new Promise(resolve => { acceptIndex = resolve; }));
  if (operation === "rename") {
    button("Rename")!.click();
    const title = host.querySelector<HTMLInputElement>('[role="dialog"] input[required]')!;
    title.value = renamed.title;
    title.dispatchEvent(new Event("input", { bubbles: true }));
    title.form!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  } else if (operation === "tags") {
    button("Edit tags")!.click();
    await vi.waitFor(() => expect(button("Save tags")?.disabled).toBe(false));
    host.querySelector<HTMLButtonElement>('[aria-label="Remove tag Keep"]')!.click();
    button("Save tags")!.click();
  } else {
    button("Delete clip")!.click();
    button("Delete clip", host.querySelector('[role="alertdialog"]')!)!.click();
  }

  const edit = operation === "rename" ? { kind: "rename", title: renamed.title } : operation === "tags"
    ? { kind: "tags", custom_tags: [], highlights: [] } : { kind: "delete" };
  expect(editManagedClip).toHaveBeenCalledExactlyOnceWith("A-1", edited.id, edit);
  await vi.waitFor(() => expect(acceptIndex).toBeDefined());
  expect(host.querySelector('[role="dialog"]')).toBeNull();
  expect(card(first.id)).toBe(firstButton);
  expect(card(edited.id)).toBe(editButton);
  expect(button("More clips")?.getAttribute("aria-disabled")).toBe("true");
  acceptIndex(true);

  await vi.waitFor(() => expect(card(second.id)?.getAttribute("aria-label")).toBe(`Play ${second.title}`));
  expect(card(first.id)).toBe(firstButton);
  expect(firstButton.querySelector("img")).toBe(firstThumbnail);
  if (operation === "rename") {
    expect(card(edited.id)).toBe(editButton);
    expect(editButton.querySelector("img")).toBe(editThumbnail);
  } else expect(card(edited.id)).toBeNull();
  expect(search.value).toBe("keep");
  expect(scrollTo).not.toHaveBeenCalled();
  expect(vi.mocked(readClipGridPage).mock.calls.slice(-2)).toEqual([
    ["A-2", { search: "keep" }, null], ["A-2", { search: "keep" }, [4, first.id]],
  ]);
  button("More clips")!.click();
  await vi.waitFor(() => expect(card(tail.id)).not.toBeNull());
  expect(readClipGridPage).toHaveBeenLastCalledWith("A-2", { search: "keep" }, refreshedCursor);
});

it("shares the actual App writer wiring across early replay open and return", async () => {
  let releaseCatalog!: (page: CatalogPage) => void;
  vi.mocked(indexCatalog).mockImplementationOnce(() => new Promise(resolve => { releaseCatalog = resolve; }));
  const { click } = await mount();
  await vi.waitFor(() => expect(indexCatalog).toHaveBeenCalledOnce());
  click("open");
  const replay = harness.viewer!.indexRecording!("ignored", "1");
  expect(indexRecording).not.toHaveBeenCalled();
  releaseCatalog({ snapshot_token: "A-1", items: [], next: null, catalog_recordings: 1 });
  await replay; expect(indexRecording).toHaveBeenCalledOnce();
  let releaseReplay!: (progress: ImportProgress) => void;
  vi.mocked(indexRecording).mockImplementationOnce(() => new Promise(resolve => { releaseReplay = resolve; }));
  const active = harness.viewer!.indexRecording!("ignored", "1");
  const stale = expect(active).rejects.toThrow("stale");
  await vi.waitFor(() => expect(indexRecording).toHaveBeenCalledTimes(2));
  click("games");
  await new Promise(resolve => setTimeout(resolve, 25));
  expect(indexCatalog).toHaveBeenCalledOnce();
  releaseReplay({ media_id: parseMediaId("11111111-2222-4333-8444-555555555555"), status: "ready", batches: 0, position: null, tail_bytes: 0 });
  await stale;
  expect(indexCatalog).toHaveBeenCalledOnce();
});

it("opens independently of held assets/durations and reacquires origins on Settings/exporter return", async () => {
  const { host, click } = await mount();
  click("clips");
  await vi.waitFor(() => expect(resolveClipDurations).toHaveBeenCalledTimes(1));
  click("games"); click("open");
  expect(host.querySelector('[data-testid="viewer"]')?.textContent).toBe("A");
  const first = harness.viewer!;
  let release!: (value: string) => void;
  const read = first.readReplay(() => new Promise<string>(resolve => { release = resolve; }));
  const stale = expect(read).rejects.toThrow("stale");
  click("settings"); click("settings-back");
  const second = harness.viewer!;
  expect(second).not.toBe(first);
  let entered = false;
  const next = second.readReplay(async token => { entered = true; return token; });
  expect(entered).toBe(false);
  release("old details"); await stale;
  await expect(next).resolves.toMatch(/^A-/);
  await expect(first.readReplay(async () => "late retry")).rejects.toThrow("stale");
  const draft = { gameTimestamp: "1", mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
    startFrame: 0 as FrameBoundary, endFrameExclusive: 600 as FrameBoundary };
  second.onExportClip(draft);
  expect(host.querySelector('[data-testid="exporter"]')).not.toBeNull();
  await expect(harness.exporter!.readReplay(async token => token)).resolves.toMatch(/^A-/);
  harness.exporter!.onBack(draft);
  expect(harness.viewer!.initialClipDraft).toEqual(draft);
  await expect(harness.viewer!.readReplay(async token => token)).resolves.toMatch(/^A-/);
  await expect(second.readReplay(async () => "late retry")).rejects.toThrow("stale");
});

it("does not reopen a retained viewer selection after Settings changes to an overlapping-ID root", async () => {
  const { host, click } = await mount(); click("open");
  const old = harness.viewer!;
  let release!: (value: string) => void;
  const stale = expect(old.readReplay(() => new Promise<string>(resolve => { release = resolve; }))).rejects.toThrow("stale");
  click("settings"); click("root");
  await vi.waitFor(() => expect(host.textContent).toContain("Settings saved."));
  click("settings-back");
  await vi.waitFor(() => expect(host.querySelector('[data-testid="open"]')).not.toBeNull());
  expect(host.querySelector('[data-testid="viewer"]')).toBeNull();
  click("open");
  expect(host.querySelector('[data-testid="viewer"]')?.textContent).toBe("B");
  const next = harness.viewer!.readReplay(async token => token);
  release("root A descriptor"); await stale;
  await expect(next).resolves.toMatch(/^B-/);
});


it.each(["stable", "away", "away-back"])("admits benchmark library milestones only for the painted origin (%s)", async navigation => {
  const emit = vi.fn();
  const complete = vi.fn().mockResolvedValue(undefined);
  const observer = { emit, complete, scenario: () => ({ id: "qb010-library-v2-fixture", kind: "app_idle" }) };
  vi.mocked(initializeReplayBenchmark).mockResolvedValueOnce(observer as unknown as NonNullable<Awaited<ReturnType<typeof initializeReplayBenchmark>>>);
  vi.mocked(ensureHevcCapability).mockResolvedValueOnce({ tested: true, supported: true, probe_url: "fixture" });
  vi.mocked(resolveClipDurations).mockImplementation(async token => ({ snapshot_token: token,
    clips: [{ clip_id: "1_2.mp4", duration: { state: "available", duration_ms: 1000 } }] }));
  const frames: FrameRequestCallback[] = [];
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.push(callback); return frames.length; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
  const { click } = await mount();
  await vi.waitFor(() => expect(emit.mock.calls.some(([kind]) => kind === "library_view_admitted")).toBe(true));
  frames.shift()!(performance.now());
  if (navigation !== "stable") click("clips");
  if (navigation === "away-back") click("games");
  frames.shift()!(performance.now());
  if (navigation === "stable") {
    await vi.waitFor(() => expect(emit.mock.calls.some(([kind]) => kind === "library_useful")).toBe(true));
    const events = emit.mock.calls.map(([kind]) => kind);
    expect(events.indexOf("games_library_usable")).toBeLessThan(events.indexOf("library_useful"));
    expect(events).toContain("scenario_started");
    expect(complete).not.toHaveBeenCalled();
  } else {
    await vi.waitFor(() => expect(complete).toHaveBeenCalledTimes(1));
    expect(complete).toHaveBeenCalledWith("failed", expect.any(String), { phase: "library_paint" });
    const events = emit.mock.calls.map(([kind]) => kind);
    for (const kind of ["games_library_usable", "library_useful", "scenario_started"]) expect(events).not.toContain(kind);
  }
});


it("resumes only the returned viewer after held details outlive an exporter round trip", async () => {
  const { click } = await mount(); click("open");
  const first = harness.viewer!;
  let release!: (value: string) => void;
  const stale = expect(first.readReplay(() => new Promise<string>(resolve => { release = resolve; }))).rejects.toThrow("stale");
  const draft = { gameTimestamp: "1", mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
    startFrame: 60 as FrameBoundary, endFrameExclusive: 600 as FrameBoundary };
  first.onExportClip(draft);
  const exporterWork = vi.fn(async () => "exporter details");
  const cancelledExporter = expect(harness.exporter!.readReplay(exporterWork)).rejects.toThrow("stale");
  harness.exporter!.onBack(draft);
  const returned = harness.viewer!;
  expect(returned.initialClipDraft).toEqual(draft);
  const returnedWork = vi.fn(async token => token);
  const latest = returned.readReplay(returnedWork);
  await cancelledExporter;
  expect(exporterWork).not.toHaveBeenCalled();
  expect(returnedWork).not.toHaveBeenCalled();
  release("old optional details");
  await stale;
  await expect(latest).resolves.toMatch(/^A-/);
  expect(returnedWork).toHaveBeenCalledTimes(1);
  expect(exporterWork).not.toHaveBeenCalled();
});
