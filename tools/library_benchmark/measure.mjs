// Run against `npm run dev --prefix app -- --port 5173`.
// Pass the installed Playwright module and Chromium executable; no downloads.
import { createRequire } from 'node:module';
import { mkdir, writeFile } from 'node:fs/promises';
const [playwrightModule, executablePath] = process.argv.slice(2);
if (!playwrightModule || !executablePath) throw new Error('Expected Playwright module path and browser executable path');
const { chromium } = createRequire(import.meta.url)(playwrightModule);
const browser = await chromium.launch({ executablePath, headless: true });
try {
  const report = { browser: browser.version(), viewport: { width: 1280, height: 800 }, runs: {} };
  for (const label of ['before', 'after']) {
    const runs = [];
    for (let repetition = 1; repetition <= 3; repetition++) {
      const context = await browser.newContext({ viewport: report.viewport });
      try {
        const page = await context.newPage();
        await page.addInitScript(() => {
          window.__cardVisibility = new Map();
          document.addEventListener('contentvisibilityautostatechange', event => {
            window.__cardVisibility.set(event.target, event.skipped);
          }, true);
        });
        const session = await context.newCDPSession(page);
        await session.send('Performance.enable');
        await page.goto(`http://127.0.0.1:5173/library-benchmark.html${label === 'before' ? '?baseline' : ''}`);
        await page.waitForSelector('#root[data-ready="true"]');
        await page.waitForFunction(() => Array.from(document.querySelectorAll('article:first-child img'))
          .every(image => image.hidden || (image.complete && image.naturalWidth > 0)));
        const metrics = Object.fromEntries((await session.send('Performance.getMetrics')).metrics.map(m => [m.name, m.value]));
        const initial = await page.evaluate(() => ({
          mount_ms: performance.getEntriesByName('library-mount')[0].duration,
          cards: document.querySelectorAll('article').length, images: document.images.length,
          loaded: Array.from(document.images).filter(image => image.complete && image.naturalWidth > 0).length,
          deferred_cards: Array.from(window.__cardVisibility.values()).filter(Boolean).length,
          height: document.documentElement.scrollHeight, dom: document.getElementsByTagName('*').length,
        }));
        const scroll = await page.evaluate(async () => {
          const gaps = []; let previous = performance.now();
          for (let step = 1; step <= 60; step++) {
            scrollTo(0, (document.documentElement.scrollHeight - innerHeight) * step / 60);
            await new Promise(resolve => requestAnimationFrame(now => { gaps.push(now - previous); previous = now; resolve(); }));
          }
          gaps.sort((a, b) => a - b);
          return { p95_ms: gaps[Math.floor((gaps.length - 1) * .95)], max_ms: gaps.at(-1) };
        });
        runs.push({ repetition, ...initial, layout_ms: metrics.LayoutDuration * 1000,
          style_ms: metrics.RecalcStyleDuration * 1000, scroll });
      } finally { await context.close(); }
    }
    report.runs[label] = runs;
  }
  await mkdir('build/perf/qb-lib-017', { recursive: true });
  await writeFile('build/perf/qb-lib-017/comparison.json', JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} finally { await browser.close(); }
