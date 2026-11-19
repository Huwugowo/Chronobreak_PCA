// Run from the repository root with an installed Playwright module, Chromium
// executable and generated landscape MP4 fixture. No user media is needed.
import assert from "node:assert/strict";
import { mkdtemp, rmdir, unlink, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [playwrightModule, executablePath, fixtureVideo] = process.argv.slice(2);
assert.ok(playwrightModule && executablePath && fixtureVideo, "Expected Playwright, Chromium and a fixture MP4");
const { chromium } = createRequire(import.meta.url)(resolve(playwrightModule));
const app = resolve("app");
const { createServer } = await import(pathToFileURL(join(app, "node_modules/vite/dist/node/index.js")));
const fixture = await mkdtemp(join(app, ".clips-layout-"));
const files = [];
let server, browser;
try {
  const write = async (name, contents) => {
    const path = join(fixture, name);
    await writeFile(path, contents, { flag: "wx" });
    files.push(path);
  };
  await write("index.html", '<!doctype html><meta charset="utf-8"><div id="root"></div><script type="module" src="./main.tsx"></script>');
  await write("main.tsx", `
    import { createSignal, Show } from "solid-js";
    import { render } from "solid-js/web";
    import { ManagedClips, ManagedClipPlayer } from "/src/components/ManagedClips";
    import shell from "/src/App.module.css";
    import "/src/styles/global.css";
    const [filters, setFilters] = createSignal({});
    const [selected, setSelected] = createSignal();
    const ddragon = { asset_base_url: "/clip-fixture-image" };
    render(() => <div class={shell.appShell} data-screen="library" data-library-tab="clips">
      <main class={shell.mainContent} id="content">
        <ManagedClips token="fixture" ready indexError={null} onRetry={() => {}}
          ddragon={ddragon} filters={filters()} onFilters={setFilters} onOpenFolder={() => {}}
          onOpen={(clip, button) => setSelected({ clip, button })} />
      </main>
      <Show when={selected()} keyed>{({ clip, button }) =>
        <ManagedClipPlayer token="fixture" opener={button} onClose={() => setSelected(undefined)}
          onEdit={async () => false} onSource={() => {}} library={{ ddragon, fileSizeBytes: 3500000 }}
          clip={{ title: clip.title, video_url: "/clip-fixture-video.mp4", thumbnail_url: clip.thumbnail_url,
            header: { ...clip, game: {
              champion: "Ahri", outcome: "win", queue_id: 420, played_at: "2026-07-03T12:00:00Z",
              final_kda: [7, 0, 3], summoner_spells: ["4", "14"], final_items: [1001], duration_seconds: 1503,
            }, events: "unavailable" } }} />
      }</Show>
    </div>, document.querySelector("#root"));
  `);
  server = await createServer({ root: app, server: { host: "127.0.0.1", port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch({ executablePath, headless: true, ignoreDefaultArgs: ["--hide-scrollbars"] });
  const page = await browser.newPage({ viewport: { width: 1500, height: 830 } });
  page.setDefaultTimeout(8_000);
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  const images = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jYlEAAAAASUVORK5CYII=", "base64");
  await page.route("**/clip-fixture-image/**", route => route.fulfill({ body: images, contentType: "image/png" }));
  await page.route("**/clip-fixture-video.mp4", route => route.fulfill({ path: fixtureVideo, contentType: "video/mp4" }));
  await page.addInitScript(() => {
    window.gridCalls = [];
    const clips = Array.from({ length: 20 }, (_, index) => ({
      id: String(index), title: index === 0 ? "A long title ".repeat(14) : `Clip ${index}`,
      champion: "Ahri", duration: "240000000", thumbnail_url: `/clip-fixture-image/${index}.png`,
      custom_tags: index === 0 ? [] : ["ranked", "outplay", "river"], highlights: [],
    }));
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      if (command !== "clip_grid_page") throw new Error(command);
      window.gridCalls.push(args);
      return args.after ? { clips: clips.slice(12), next: null } : { clips: clips.slice(0, 12), next: [1, "11"] };
    } };
  });
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/${basename(fixture)}/`);
  const grid = page.getByRole("list", { name: "Clips", exact: true });
  const cards = grid.getByRole("button");
  await cards.nth(11).waitFor();
  const first = await cards.first().elementHandle();
  const image = await cards.first().locator("img").first().elementHandle();
  assert.equal(await image.getAttribute("loading"), "lazy");
  await image.waitForElementState("visible");
  assert.ok(await image.evaluate(img => img.complete && img.naturalWidth > 0));

  for (const [width, columns] of [[559, 1], [560, 2], [879, 2], [880, 3], [1319, 3], [1320, 4], [560, 2], [1320, 4]]) {
    await page.locator("#content").evaluate((element, width) => { element.style.width = `${width}px`; }, width);
    const sizing = await grid.evaluate(element => {
      const style = getComputedStyle(element);
      const artwork = element.querySelector("button > span").getBoundingClientRect();
      const portrait = element.querySelector('[role="img"]').getBoundingClientRect();
      const title = element.querySelector("strong");
      return {
        width: element.clientWidth, columns: style.gridTemplateColumns.split(" ").length,
        gap: style.gap, ratio: artwork.width / artwork.height,
        portrait: [portrait.width, portrait.height],
        titleLines: title.offsetHeight / Number.parseFloat(getComputedStyle(title).lineHeight),
      };
    });
    assert.deepEqual([sizing.width, sizing.columns, sizing.gap, sizing.portrait, sizing.titleLines],
      [width, columns, "24px 16px", [36, 36], 2]);
    assert.ok(Math.abs(sizing.ratio - 16 / 9) < 0.001);
    assert.ok(await first.evaluate(node => node.isConnected));
    assert.ok(await image.evaluate(node => node.isConnected));
  }

  await page.locator("#content").evaluate(element => { element.style.removeProperty("width"); });
  await page.setViewportSize({ width: 620, height: 650 });
  await page.getByRole("searchbox", { name: "Search clips" }).fill("ranked");
  await page.waitForFunction(() => window.gridCalls.at(-1)?.query.search === "ranked");
  await page.getByRole("button", { name: "More clips" }).click();
  const opener = page.getByRole("button", { name: "Play Clip 16", exact: true });
  await opener.scrollIntoViewIfNeeded();
  const previous = await opener.elementHandle();
  const scrollBefore = await page.evaluate(() => scrollY);
  assert.ok(scrollBefore > 0);
  await opener.click();
  const dialog = page.getByRole("dialog", { name: "Clip 16" });
  const video = dialog.locator("video");
  await video.waitFor();
  await page.waitForFunction(() => document.querySelector("video")?.readyState >= HTMLMediaElement.HAVE_METADATA);
  assert.equal(await page.evaluate(() => scrollY), scrollBefore);
  await dialog.getByRole("button", { name: "Open fullscreen clip" }).click();
  await page.waitForFunction(() => document.fullscreenElement?.querySelector("video"));
  assert.equal(await page.evaluate(() => document.fullscreenElement.textContent.includes("Victory")), false);
  assert.equal(await page.locator(":fullscreen").getByRole("region", { name: "Clip controls" }).count(), 1);
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !document.fullscreenElement);
  assert.equal(await dialog.count(), 1);
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  assert.equal(await page.evaluate(() => scrollY), scrollBefore);
  assert.ok(await previous.evaluate(node => node === document.activeElement && node.isConnected));
  assert.equal(await cards.count(), 20);
  assert.equal(await page.getByRole("searchbox").inputValue(), "ranked");
  assert.equal(await page.evaluate(() => window.gridCalls.length), 3);

  for (const [width, height] of [[360, 420], [800, 480], [1500, 900]]) {
    await page.setViewportSize({ width, height });
    await opener.click();
    const geometry = await dialog.evaluate(element => {
      const bounds = element.getBoundingClientRect();
      const controls = element.querySelector('[aria-label="Clip controls"]');
      const controlsBounds = controls.getBoundingClientRect();
      return { left: bounds.left, right: bounds.right, bottom: bounds.bottom, width: bounds.width,
        fits: element.scrollWidth === element.clientWidth,
        controlsFit: controls.scrollWidth === controls.clientWidth && controlsBounds.bottom <= bounds.bottom };
    });
    assert.ok(geometry.left >= 0 && geometry.right <= width && geometry.bottom <= height && geometry.fits && geometry.controlsFit,
      JSON.stringify({ width, height, geometry }));
    assert.ok(geometry.width <= 1040);
    if (width === 1500) assert.equal(geometry.width, 1040);
    await dialog.getByRole("button", { name: "Close", exact: true }).click();
  }
  assert.deepEqual(errors, []);
  console.log("Verified eight content-width samples, three modal sizes, pagination, fullscreen and scroll/focus restoration.");
} finally {
  await browser?.close();
  await server?.close();
  for (const path of files) await unlink(path);
  await rmdir(fixture);
}
