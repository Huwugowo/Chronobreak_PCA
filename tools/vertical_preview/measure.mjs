// Read-only media characterization of an already-open isolated production editor.
import { createRequire } from 'node:module';
import { mkdir, writeFile } from 'node:fs/promises';
const [playwrightModule, endpoint = 'http://127.0.0.1:9223'] = process.argv.slice(2);
if (!playwrightModule) throw new Error('Expected absolute Playwright module path');
const { chromium } = createRequire(import.meta.url)(playwrightModule);
const browser = await chromium.connectOverCDP(endpoint);
const page = browser.contexts()[0].pages().find(p => p.url().startsWith('http://tauri.localhost'));
if (!page) throw new Error('Expected production Tauri page');
const body = await page.locator('body').innerText();
if (!body.includes('CLIP WORKBENCH') || !/build[\\/]preprod[\\/]library[\\/]clips/.test(body)) {
  throw new Error('Open the isolated preproduction clip editor first');
}
const report = { measured_at: new Date().toISOString(), runtime: browser.version(), duration_ms: 3000, trials: [] };
for (const mode of ['horizontal', 'vertical']) {
  const card = page.getByRole('group', { name: 'Export formats' }).locator('div').filter({ has: page.locator(':scope > label') }).filter({ hasText: mode === 'vertical' ? 'TikTok' : '1080p publish' });
  await card.getByRole('button').click({ force: true });
  await page.waitForFunction(() => [...document.querySelectorAll('video')].length === (document.querySelector('video[aria-label="vertical clip preview"]') ? 2 : 1) && [...document.querySelectorAll('video')].every(v => v.readyState >= 2));
  for (let repetition = 1; repetition <= 3; repetition++) {
    const pause = page.getByRole('button', { name: 'PAUSE', exact: true });
    if (await pause.count()) await pause.click({ force: true });
    await page.getByRole('button', { name: 'RESTART', exact: true }).click({ force: true });
    // RESTART also plays; pause and settle before the observed user playback.
    await pause.waitFor();
    await pause.click({ force: true });
    await page.waitForFunction(() => [...document.querySelectorAll('video')].every(v => !v.seeking && v.paused));
    await page.evaluate(() => {
      const videos = [...document.querySelectorAll('video')];
      const snapshot = v => ({ time: v.currentTime, paused: v.paused, seeking: v.seeking, ready: v.readyState, frames: v.getVideoPlaybackQuality().totalVideoFrames, dropped: v.getVideoPlaybackQuality().droppedVideoFrames, decoded: v.webkitDecodedFrameCount ?? null });
      const rows = videos.map(v => {
        const row = { role: v.hasAttribute('aria-label') ? 'foreground' : 'backdrop', source_path: new URL(v.currentSrc).pathname.replace(/\/cap\/[^/]+/, '/cap/[redacted]'), width: v.videoWidth, height: v.videoHeight, start: snapshot(v), callbacks: 0, seeking_events: 0, seeked_events: 0, callback: 0 };
        row.onSeeking = () => row.seeking_events++;
        row.onSeeked = () => row.seeked_events++;
        v.addEventListener('seeking', row.onSeeking); v.addEventListener('seeked', row.onSeeked);
        const frame = () => { row.callbacks++; row.callback = v.requestVideoFrameCallback(frame); };
        row.callback = v.requestVideoFrameCallback(frame);
        return row;
      });
      const drift = [];
      const timer = setInterval(() => { if (videos.length === 2) drift.push(Math.abs(videos[0].currentTime - videos[1].currentTime)); }, 100);
      window.__previewObservation = { videos, rows, snapshot, drift, timer, started: performance.now(), visibility: document.visibilityState };
    });
    await page.getByRole('button', { name: 'PLAY CLIP', exact: true }).click({ force: true });
    await new Promise(resolve => setTimeout(resolve, report.duration_ms));
    const result = await page.evaluate(() => {
      const o = window.__previewObservation;
      clearInterval(o.timer);
      const rows = o.rows.map((r, i) => {
        const v = o.videos[i]; v.cancelVideoFrameCallback(r.callback);
        v.removeEventListener('seeking', r.onSeeking); v.removeEventListener('seeked', r.onSeeked);
        const end = o.snapshot(v);
        return { role: r.role, source_path: r.source_path, width: r.width, height: r.height, start: r.start, end, callbacks: r.callbacks, seeking_events: r.seeking_events, seeked_events: r.seeked_events, frame_delta: end.frames - r.start.frames, decoded_delta: end.decoded === null ? null : end.decoded - r.start.decoded, dropped_delta: end.dropped - r.start.dropped };
      });
      const result = { elapsed_ms: performance.now() - o.started, visibility: o.visibility, end_visibility: document.visibilityState, videos: rows, drift_samples: o.drift.length, max_drift_seconds: o.drift.length ? Math.max(...o.drift) : null };
      delete window.__previewObservation; return result;
    });
    await page.getByRole('button', { name: 'PAUSE', exact: true }).click({ force: true });
    report.trials.push({ mode, repetition, ...result });
  }
}
await mkdir('build/perf/qb-clip-009', { recursive: true });
await writeFile('build/perf/qb-clip-009/presentation.json', JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
// Disconnect the diagnostic client without closing the user's application.
await browser.close();
