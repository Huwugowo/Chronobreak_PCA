// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { batch, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { readClipGridChampions, readClipGridPage, tagChoices, type ClipFilters, type ClipGridCard, type ClipGridPage,
  type ClipTagUpdate, type Tag } from "../managedClips";
import { assigned, customTag, pentakill } from "../tagFixtures";
import { ManagedClips } from "./ManagedClips";
import styles from "./ManagedClips.module.css";

vi.mock("../managedClips", async original => ({
  ...await original<typeof import("../managedClips")>(),
  readClipGridPage: vi.fn(),
  readClipGridChampions: vi.fn(),
  tagChoices: vi.fn(),
}));

const fixture = (id: string): ClipGridCard => ({
  id, title: `Clip ${id}`, champion: "Ahri", duration: "240000000",
  thumbnail_url: `https://fixture/${id}.jpg`, tags: [],
});

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.replaceChildren();
  vi.resetAllMocks();
});

function mount() {
  const host = document.createElement("div");
  document.body.append(host);
  const [filters, setFilters] = createSignal<ClipFilters>({});
  const [token, setToken] = createSignal("snapshot");
  const [ready, setReady] = createSignal(true);
  const [indexError, setIndexError] = createSignal<string | null>(null);
  const [tagUpdate, editTags] = createSignal<ClipTagUpdate>();
  const [renamedTag, renameTag] = createSignal<Tag>();
  const onOpen = vi.fn();
  dispose = render(() => <ManagedClips
    root="fixture" token={token()} ready={ready()} indexError={indexError()} onRetry={() => {}}
    ddragon={{ state: "ready", asset_base_url: "https://fixture/assets", version: "1",
      champion_count: 1, item_count: 0, cache_directory: "", error: null }}
    filters={filters()} onFilters={setFilters} onOpen={onOpen} tagUpdate={tagUpdate()}
    renamedTag={renamedTag()} onOpenFolder={() => {}}
  />, host);
  return {
    host, onOpen, setIndexError, editTags, renameTag,
    accept: (newToken: string, actionable: boolean) => batch(() => { setToken(newToken); setReady(actionable); }),
    cards: () => [...host.querySelectorAll<HTMLButtonElement>("[data-managed-clip]")],
    button: (label: string) => [...host.querySelectorAll("button")].find(button => button.textContent === label)!,
    input: (selector: string, value: string) => {
      const input = host.querySelector<HTMLInputElement>(selector)!;
      input.value = value;
      input.dispatchEvent(new Event("input", { bubbles: true }));
    },
  };
}

it("appends cursor pages in server order and keeps the existing images mounted", async () => {
  const tagged: ClipGridCard = {
    ...fixture("newer"),
    tags: [assigned(pentakill, "detected"), assigned({ id: "builtin:baron_steal", label: "Baron Steal", builtin: true, tier: null }),
      ...["outplay", "final fight"].map(label => assigned(customTag(label)))],
  };
  const page: ClipGridPage = { clips: [tagged, fixture("middle")], next: [100, "middle"] };
  vi.mocked(readClipGridPage).mockResolvedValueOnce(page)
    .mockRejectedValueOnce(new Error("Temporary failure"))
    .mockResolvedValueOnce({ clips: [fixture("older")], next: null });
  const { host, cards, button, onOpen } = mount();
  await vi.waitFor(() => expect(cards()).toHaveLength(2));
  const [first, second] = cards();
  const image = first.querySelector<HTMLImageElement>('img[src$="newer.jpg"]')!;
  expect(image.getAttribute("loading")).toBe("lazy");
  expect(first.querySelector('[role="img"]')?.getAttribute("aria-label")).toBe("Ahri");
  expect(first.querySelector("em")?.textContent).toBe("0:05");
  const tags = first.querySelector(`.${styles.clipTags}`)!;
  expect([...tags.children].map(tag => [tag.textContent, tag.className])).toEqual([
    ["Pentakill", styles.detectedBadge],
    ["Baron Steal", styles.manualBadge],
    ["+2", styles.manualBadge],
  ]);
  const descriptions = (card: HTMLElement) => card.getAttribute("aria-describedby")!
    .split(" ").map(id => document.getElementById(id));
  expect(descriptions(first)).toEqual([first.querySelector('[role="img"]'), first.querySelector("em"), tags]);
  expect(descriptions(second)).toEqual([second.querySelector('[role="img"]'), second.querySelector("em")]);
  expect(second.querySelector(`.${styles.clipTags}`)).toBeNull();

  button("More clips").click();
  await vi.waitFor(() => expect(host.querySelector('[role="alert"]')?.textContent).toContain("Temporary failure"));
  expect(cards()).toEqual([first, second]);
  const retry = button("Try again");
  retry.focus();
  retry.click();
  await vi.waitFor(() => expect(cards().map(card => card.dataset.managedClip)).toEqual(["newer", "middle", "older"]));
  expect(document.activeElement).toBe(cards()[2]);
  expect(readClipGridPage).toHaveBeenNthCalledWith(1, "snapshot", {}, null);
  expect(readClipGridPage).toHaveBeenNthCalledWith(2, "snapshot", {}, [100, "middle"]);
  expect(readClipGridPage).toHaveBeenNthCalledWith(3, "snapshot", {}, [100, "middle"]);
  expect(cards()[0]).toBe(first);
  expect(first.querySelector('img[src$="newer.jpg"]')).toBe(image);
  first.click();
  expect(onOpen).toHaveBeenCalledWith(page.clips[0], first);
});

it("keeps loaded pages through a catalog error and rejects a late continuation from the old catalog", async () => {
  const first = fixture("first");
  const second = fixture("second");
  const replacement = { ...second, title: "Renamed clip", tags: [assigned(customTag("Highlights"))] };
  let complete!: (page: ClipGridPage) => void;
  vi.mocked(readClipGridPage)
    .mockResolvedValueOnce({ clips: [first], next: [4, first.id] })
    .mockResolvedValueOnce({ clips: [second], next: [3, second.id] })
    .mockImplementationOnce(() => new Promise(resolve => { complete = resolve; }))
    .mockResolvedValueOnce({ clips: [first], next: [4, first.id] })
    .mockResolvedValueOnce({ clips: [replacement], next: [3, second.id] });
  const { host, cards, button, accept, setIndexError, onOpen } = mount();
  await vi.waitFor(() => expect(cards()).toHaveLength(1));
  button("More clips").click();
  await vi.waitFor(() => expect(cards()).toHaveLength(2));
  const [firstButton, secondButton] = cards();
  const image = secondButton.querySelector("img");
  button("More clips").click();
  await vi.waitFor(() => expect(readClipGridPage).toHaveBeenCalledTimes(3));

  accept("snapshot", false);
  accept("new", false);
  setIndexError("Index temporarily unavailable");
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("Index temporarily unavailable");
  expect(cards()).toEqual([firstButton, secondButton]);
  expect(button("More clips").getAttribute("aria-disabled")).toBe("true");
  secondButton.click();
  expect(onOpen).not.toHaveBeenCalled();
  setIndexError(null);
  accept("new", true);
  complete({ clips: [fixture("stale")], next: null });

  await vi.waitFor(() => expect(secondButton.getAttribute("aria-label")).toBe("Play Renamed clip"));
  expect(cards()).toEqual([firstButton, secondButton]);
  expect(secondButton.querySelector("img")).toBe(image);
  expect(secondButton.querySelector(`.${styles.manualBadge}`)?.textContent).toBe("Highlights");
  expect(secondButton.getAttribute("aria-describedby")).toContain("grid-clip-second-tags");
  expect(button("More clips").getAttribute("aria-disabled")).toBe("false");
  expect(readClipGridPage).toHaveBeenNthCalledWith(4, "new", {}, null);
  expect(readClipGridPage).toHaveBeenNthCalledWith(5, "new", {}, [4, first.id]);
});

it("patches an unfiltered tag edit without reading the loaded pages or disturbing the cursor", async () => {
  const clip = fixture("first");
  vi.mocked(readClipGridPage).mockResolvedValueOnce({ clips: [clip], next: [10, clip.id] })
    .mockResolvedValueOnce({ clips: [fixture("second")], next: null });
  const { cards, button, editTags } = mount();
  await vi.waitFor(() => expect(cards()).toHaveLength(1));
  const card = cards()[0];
  const image = card.querySelector("img");
  editTags({ id: clip.id, tags: [assigned(customTag("New label"))] });
  expect(card.textContent).toContain("New label");
  expect(cards()[0]).toBe(card);
  expect(card.querySelector("img")).toBe(image);
  expect(readClipGridPage).toHaveBeenCalledOnce();
  button("More clips").click();
  await vi.waitFor(() => expect(cards()).toHaveLength(2));
  expect(readClipGridPage).toHaveBeenLastCalledWith("snapshot", {}, [10, clip.id]);
});

it("patches tag edits that preserve the exact-ID filter without rereading pages or remounting cards", async () => {
  const tag = customTag("River fight");
  const clip = { ...fixture("first"), tags: [assigned(tag)] };
  vi.mocked(tagChoices).mockResolvedValue([tag]);
  vi.mocked(readClipGridPage).mockResolvedValue({ clips: [clip], next: null });
  const { host, cards, button, renameTag, editTags } = mount();
  await vi.waitFor(() => expect(cards()).toHaveLength(1));
  const card = cards()[0];
  const image = card.querySelector("img");
  button("Tags").click();
  await vi.waitFor(() => expect(host.querySelector('input[type="checkbox"]')).not.toBeNull());
  host.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  button("Tags").click();
  await vi.waitFor(() => expect(readClipGridPage).toHaveBeenLastCalledWith("snapshot", { tags: [tag.id] }, null));
  await vi.waitFor(() => expect(card.getAttribute("aria-disabled")).toBe("false"));
  renameTag({ ...tag, label: "River play" });
  expect(card.textContent).toContain("River play");
  expect(host.querySelector('[aria-label="Remove River play"]')).not.toBeNull();
  editTags({ id: clip.id, tags: [assigned({ ...tag, label: "River play" }), assigned(customTag("Great timing"))] });
  expect(card.textContent).toContain("Great timing");
  expect(cards()[0]).toBe(card);
  expect(card.querySelector("img")).toBe(image);
  expect(readClipGridPage).toHaveBeenCalledTimes(2);
  expect(tagChoices).toHaveBeenCalledOnce();
});

it("debounces search and match date and discards superseded pages", async () => {
  let complete!: (page: ClipGridPage) => void;
  vi.mocked(readClipGridPage).mockImplementationOnce(() => new Promise(resolve => { complete = resolve; }))
    .mockResolvedValueOnce({ clips: [fixture("filtered")], next: null });
  const { host, cards, input, button } = mount();
  await vi.waitFor(() => expect(readClipGridPage).toHaveBeenCalledOnce());
  input('[aria-label="Search clips"]', "tower");
  button("Filters").click();
  const select = host.querySelector<HTMLSelectElement>('select')!;
  select.value = "custom";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  input('input[type="date"]', "2026-07-01");
  button("Apply filters").closest("form")!.requestSubmit();
  await new Promise(resolve => setTimeout(resolve, 210));
  expect(readClipGridPage).toHaveBeenCalledOnce();
  complete({ clips: [fixture("stale")], next: [200, "stale"] });
  await vi.waitFor(() => expect(cards().map(card => card.dataset.managedClip)).toEqual(["filtered"]));
  expect(readClipGridPage).toHaveBeenLastCalledWith("snapshot", { search: "tower", from: "2026-07-01" }, null);
});

it("keeps a tag edit when a pending filter page contains older metadata", async () => {
  const clip = fixture("first");
  const tags = [assigned(customTag("River fight"))];
  let complete!: (page: ClipGridPage) => void;
  vi.mocked(readClipGridPage)
    .mockResolvedValueOnce({ clips: [clip], next: null })
    .mockImplementationOnce(() => new Promise(resolve => { complete = resolve; }))
    .mockResolvedValueOnce({ clips: [{ ...clip, tags }], next: null });
  const { cards, input, editTags } = mount();
  await vi.waitFor(() => expect(cards()).toHaveLength(1));
  const card = cards()[0];
  const thumbnail = card.querySelector("img");
  input('[aria-label="Search clips"]', "first");
  await vi.waitFor(() => expect(readClipGridPage).toHaveBeenCalledTimes(2));

  editTags({ id: clip.id, tags });
  complete({ clips: [clip], next: null });
  await vi.waitFor(() => expect(card.getAttribute("aria-disabled")).toBe("false"));
  expect(readClipGridPage).toHaveBeenCalledTimes(3);
  expect(card.textContent).toContain("River fight");
  expect(cards()[0]).toBe(card);
  expect(card.querySelector("img")).toBe(thumbnail);
});

it("combines queue, champion, exact tags and advanced filters and preserves matching cards", async () => {
  const kept = fixture("kept");
  vi.mocked(readClipGridPage).mockResolvedValue({ clips: [kept], next: null });
  vi.mocked(readClipGridChampions).mockResolvedValue(["Ahri", "Kai'Sa"]);
  vi.mocked(tagChoices).mockResolvedValue([customTag("River fight"), pentakill]);
  const { host, cards, input, button } = mount();
  await vi.waitFor(() => expect(cards()).toHaveLength(1));
  const card = cards()[0];
  const image = card.querySelector("img");
  expect(readClipGridChampions).not.toHaveBeenCalled();
  expect(tagChoices).not.toHaveBeenCalled();

  const ranked = button("Ranked");
  ranked.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  expect(document.activeElement).toBe(button("Ranked Solo/Duo"));
  expect([...document.getElementById(ranked.getAttribute("aria-controls")!)!.querySelectorAll("button")].map(choice => choice.textContent))
    .toEqual(["Ranked Solo/Duo", "Ranked Flex", "Ranked 5s"]);
  button("Ranked Solo/Duo").click();
  expect(ranked.getAttribute("data-active")).toBe("true");
  expect(document.activeElement).toBe(ranked);
  const champion = host.querySelector<HTMLInputElement>('[role="combobox"]')!;
  champion.focus();
  input('[role="combobox"]', "kaisa");
  const option = () => host.querySelector('[role="option"]');
  await vi.waitFor(() => expect(option()?.textContent).toBe("Kai'Sa"));
  expect(option()?.querySelector("img")?.src).toContain("champion/Kai'Sa");
  champion.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(readClipGridChampions).toHaveBeenCalledExactlyOnceWith("snapshot");

  const tags = button("Tags");
  tags.click();
  const tagPanel = document.getElementById(tags.getAttribute("aria-controls")!)!;
  const tag = (label: string) => [...tagPanel.querySelectorAll("label")]
    .filter(element => element.textContent === label).map(element => element.querySelector("input")!);
  await vi.waitFor(() => expect(tag("River fight")).toHaveLength(1));
  expect(tag("Pentakill")).toHaveLength(1);
  tag("Pentakill")[0].click();
  tag("River fight")[0].click();
  expect(tagChoices).toHaveBeenCalledExactlyOnceWith("snapshot", "");
  expect(tagPanel.textContent).not.toMatch(/ally|enemy|actor/i);

  button("Filters").click();
  expect(tags.getAttribute("aria-expanded")).toBe("false");
  const [date, result] = host.querySelectorAll("select");
  date.value = "week";
  date.dispatchEvent(new Event("change", { bubbles: true }));
  result.value = "loss";
  result.dispatchEvent(new Event("change", { bubbles: true }));
  button("Apply filters").closest("form")!.requestSubmit();
  expect(document.activeElement).toBe(button("Filters"));
  expect(host.querySelector('[aria-label="Remove Last 7 days"]')).not.toBeNull();
  await vi.waitFor(() => expect(readClipGridPage).toHaveBeenLastCalledWith("snapshot", expect.objectContaining({
    queue: "ranked_solo_duo", champions: ["Kai'Sa"], tags: [pentakill.id, customTag("River fight").id], from: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
    to: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/), outcome: "loss",
  }), null));
  expect(cards()[0]).toBe(card);
  expect(cards()[0].querySelector("img")).toBe(image);

  host.querySelector<HTMLButtonElement>('[aria-label="Remove River fight"]')!.click();
  await vi.waitFor(() => expect(readClipGridPage).toHaveBeenLastCalledWith("snapshot", expect.objectContaining({ tags: [pentakill.id] }), null));
  button("Clear all").click();
  await vi.waitFor(() => expect(readClipGridPage).toHaveBeenLastCalledWith("snapshot", {}, null));
  expect(button("All").getAttribute("aria-pressed")).toBe("true");
  expect(cards()[0]).toBe(card);
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Search clips");
});
