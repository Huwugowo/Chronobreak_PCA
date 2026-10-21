import { createRequire } from 'node:module';
import { mkdir, writeFile } from 'node:fs/promises';
const [playwrightModule, executablePath, label] = process.argv.slice(2);
if (!playwrightModule || !executablePath || !/^(before|after)$/.test(label ?? '')) {
  throw new Error('Expected Playwright module path, Chromium executable path, before|after');
}
const { chromium } = createRequire(import.meta.url)(playwrightModule);
const browser = await chromium.launch({ executablePath, headless: true });
try {
  const report = { label, browser: browser.version(), viewport: { width: 1280, height: 800 }, runs: [] };
  for (let repetition = 1; repetition <= 3; repetition++) {
    const context = await browser.newContext({ viewport: report.viewport });
    try {
      const page = await context.newPage();
      await page.goto('http://127.0.0.1:5173/timeline-benchmark.html');
      await page.waitForSelector('#root[data-ready="true"]');
      report.runs.push({ repetition, ...await page.evaluate(() => window.runTimelineBenchmark()) });
    } finally { await context.close(); }
  }
  await mkdir('build/perf/qb-replay-023', { recursive: true });
  await writeFile(`build/perf/qb-replay-023/${label}.json`, JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} finally { await browser.close(); }
