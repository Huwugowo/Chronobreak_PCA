// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { readClipGridPage, type ClipFilters, type ClipGridCard, type ClipGridPage } from "../managedClips";
import { ManagedClips } from "./ManagedClips";
import styles from "./ManagedClips.module.css";

vi.mock("../managedClips", async original => ({
  ...await original<typeof import("../managedClips")>(),
  readClipGridPage: vi.fn(),
}));

const fixture = (id: string): ClipGridCard => ({
  id, title: `Clip ${id}`, champion: "Ahri", duration: "240000000",
  thumbnail_url: `https://fixture/${id}.jpg`, highlights: [], custom_tags: [],
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
  const onOpen = vi.fn();
  dispose = render(() => <ManagedClips
    token="snapshot" ready indexError={null} onRetry={() => {}}
    ddragon={{ state: "ready", asset_base_url: "https://fixture/assets", version: "1",
      champion_count: 1, item_count: 0, cache_directory: "", error: null }}
    filters={filters()} onFilters={setFilters} onOpen={onOpen} onOpenFolder={() => {}}
  />, host);
  return {
    host, onOpen,
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
  const tagged = {
    ...fixture("newer"),
    highlights: [{ kind: "pentakill", attribution: "you", actor: null, event_index: 4 }] as const,
    custom_tags: ["ranked", "outplay", "final fight"],
  };
  const page: ClipGridPage = { clips: [{ ...tagged, highlights: [...tagged.highlights] }, fixture("middle")], next: [100, "middle"] };
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
    ["Pentakill · You", styles.highlightBadge],
    ["ranked", styles.customBadge],
    ["+2", styles.customBadge],
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

it("debounces search and match date and discards superseded pages", async () => {
  let complete!: (page: ClipGridPage) => void;
  vi.mocked(readClipGridPage).mockImplementationOnce(() => new Promise(resolve => { complete = resolve; }))
    .mockResolvedValueOnce({ clips: [fixture("filtered")], next: null });
  const { cards, input } = mount();
  await vi.waitFor(() => expect(readClipGridPage).toHaveBeenCalledOnce());
  input('input[type="search"]', "tower");
  input('input[type="date"]', "2026-07-01");
  await new Promise(resolve => setTimeout(resolve, 210));
  expect(readClipGridPage).toHaveBeenCalledOnce();
  complete({ clips: [fixture("stale")], next: [200, "stale"] });
  await vi.waitFor(() => expect(cards().map(card => card.dataset.managedClip)).toEqual(["filtered"]));
  expect(readClipGridPage).toHaveBeenLastCalledWith("snapshot", { search: "tower", from: "2026-07-01" }, null);
});
