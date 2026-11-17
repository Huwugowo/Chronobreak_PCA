// Run from the repo root with Node 24 and a staged Windows release runtime.
// node tools/library_benchmark/history_lifecycle.mjs <release-exe> <Data Dragon cache>
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { cp, mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { once } from 'node:events';
import { createServer } from 'node:net';
import path from 'node:path';
import { setTimeout as pause } from 'node:timers/promises';

const [executable, assets] = process.argv.slice(2);
assert(executable && assets, 'Supply the release executable and a populated Data Dragon cache');
const source = path.resolve('fixtures/preprod-library/library/games');
const run = await mkdtemp(path.resolve('build/history-lifecycle-'));
const library = path.join(run, 'library');
const games = path.join(library, 'games');
const appData = path.join(run, 'app-data');
const config = path.join(run, 'config.toml');
await mkdir(games, { recursive: true });
await cp(path.join(source, '1792431584'), path.join(games, '1792431584'), { recursive: true });
await cp(path.resolve(assets), path.join(appData, 'ddragon'), { recursive: true });
await writeFile(config, `[storage]\noutput_path = ${JSON.stringify(library.replaceAll('\\', '/'))}\nauto_delete_days = 0\n[app]\nautostart = false\nhevc_playback_supported = false\n`);
const portReservation = createServer().listen(0, '127.0.0.1');
await once(portReservation, 'listening');
const { port } = portReservation.address();
portReservation.close();
await once(portReservation, 'close');
const app = spawn(path.resolve(executable), [], { windowsHide: true, env: {
  ...process.env, CHRONOBREAK_DISABLE_RECORDER_SUPERVISION: '1', LEAGUE_REPLAY_CONFIG: config,
  LEAGUE_REPLAY_OUTPUT_PATH: library, LEAGUE_REPLAY_APP_DATA_DIR: appData,
  WEBVIEW2_USER_DATA_FOLDER: path.join(run, 'webview2'),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
} });
app.stderr.on('data', data => process.stderr.write(data));

async function waitFor(label, probe, timeout = 20_000) {
  const deadline = Date.now() + timeout;
  do {
    assert.equal(app.exitCode, null, 'Desktop app exited');
    const value = await probe();
    if (value) return value;
    await pause(25);
  } while (Date.now() < deadline);
  throw new Error(`Timed out waiting for ${label}`);
}

let cdp, journalLock;
try {
  const target = await waitFor('production WebView', async () => {
    const tabs = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(1000) })
      .then(response => response.json(), () => []);
    return tabs.find(tab => tab.url.startsWith('http://tauri.localhost'));
  });
  cdp = new WebSocket(target.webSocketDebuggerUrl);
  await once(cdp, 'open');
  const pending = new Map();
  let id = 0;
  cdp.addEventListener('message', ({ data }) => {
    const message = JSON.parse(data);
    pending.get(message.id)?.(message);
    pending.delete(message.id);
  });
  const evaluate = async (script, ...args) => {
    const expression = `(${script})(${args.map(arg => JSON.stringify(arg)).join(',')})`;
    const message = await new Promise(resolve => {
      pending.set(++id, resolve);
      cdp.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } }));
    });
    assert(!message.error, JSON.stringify(message.error));
    assert(!message.result.exceptionDetails, message.result.exceptionDetails?.exception.description);
    return message.result.result.value;
  };
  await waitFor('populated card and decoded artwork', () => evaluate(() => {
    const card = document.querySelector('[aria-label^="Open Master Yi recording"]')?.closest('article');
    const fields = card && [card.querySelector('[class*="kda"] strong'), card.querySelector('[aria-label="Final creep score"]')];
    const items = [...(card?.querySelectorAll('[class*="itemSlot"] img') ?? [])];
    const images = [...(card?.querySelectorAll('img') ?? [])].filter(img => !img.hidden);
    if (!fields?.[0]?.textContent.match(/30\s*\/\s*3\s*\/\s*1/) || !fields[1]?.textContent.includes('CS 136') ||
        !items.length || !images.every(img => img.complete && img.naturalWidth > 0) ||
        document.querySelector('[aria-label="Recorded matches"]')?.getAttribute('aria-busy') !== 'false') return false;
    const values = fields.map(field => field.textContent);
    const urls = images.map(img => img.src);
    const failures = [];
    const check = () => {
      if (failures.length) return;
      if (!card.isConnected || fields.some((field, index) => !field.isConnected || field.textContent !== values[index]) ||
          images.some((img, index) => !card.contains(img) || img.src !== urls[index] || img.hidden || !img.complete || !img.naturalWidth)) {
        failures.push('Previously populated History data changed or was removed');
      }
    };
    const observer = new MutationObserver(records => {
      const tracked = [card, ...fields, ...images];
      if (records.some(record => [...record.removedNodes].some(node => tracked.some(element => node.contains(element))))) {
        failures.push('A populated History node was detached');
      }
      if (records.some(record => fields.some(field => field.contains(record.target)) &&
          (record.oldValue?.includes('—') || [...record.addedNodes, ...record.removedNodes].some(node => node.textContent.includes('—'))))) {
        failures.push('An unavailable value was rendered in a populated History field');
      }
      check();
    });
    observer.observe(document.body, { subtree: true, attributes: true, childList: true, characterData: true, characterDataOldValue: true });
    const frame = () => { check(); requestAnimationFrame(frame); };
    requestAnimationFrame(frame);

    const monitor = window.historyRegression = { failures, events: [], calls: [], releases: [], historyResponses: 0 };
    const fetchNative = window.fetch.bind(window);
    window.fetch = async (url, options) => {
      const cmd = new URL(url, location.href);
      if (cmd.hostname !== 'ipc.localhost') return fetchNative(url, options);
      const name = decodeURIComponent(cmd.pathname.slice(1));
      monitor.calls.push(name);
      const response = await fetchNative(url, options);
      if (monitor.holdCatalog && name === 'index_league_catalog') {
        monitor.pendingStatus = (await response.clone().json()).items.find(item => item.game_timestamp === '1792431584')?.progress;
        await new Promise(resolve => monitor.releases.push(resolve));
      }
      if (monitor.holdQuery && name === 'query_league_history') {
        monitor.queryWaiting = true;
        await new Promise(resolve => monitor.releases.push(resolve));
      }
      if (name === 'query_league_history') monitor.historyResponses++;
      return response;
    };
    const tauri = window.__TAURI_INTERNALS__;
    for (const event of ['library-catalog-changed', 'league-data-prepared']) {
      const listeners = window.__internal_unstable_listeners_object_id__[event];
      for (const id of Object.getOwnPropertyNames(listeners)) {
        const listener = listeners[id];
        const handler = listener.handlerId;
        listener.handlerId = tauri.transformCallback(message => {
          const notification = { ...message };
          monitor.events.push({ event, deliver: () => tauri.runCallback(handler, notification) });
        });
      }
    }
    return true;
  }), 90_000);

  const openReplay = async () => {
    await evaluate(() => document.querySelector('[aria-label^="Open Master Yi recording"]').click());
    await waitFor('real replay', () => evaluate(() => document.querySelector('[data-testid="replay-panel"] video')?.readyState >= 2));
  };
  const returnToHistory = async () => {
    await evaluate(() => document.querySelector('[aria-label="Back to Match History"]').click());
    await waitFor('visible Match History', () => evaluate(() => document.querySelector('[aria-label^="Open Master Yi recording"]')?.getClientRects().length));
  };
  await openReplay();
  await cp(path.join(source, '1792432630'), path.join(games, '1792432630'), { recursive: true });
  await waitFor('native catalog notification', () => evaluate(() => window.historyRegression.events.some(item => item.event === 'library-catalog-changed')));
  await returnToHistory();
  await evaluate(() => window.historyRegression.events.filter(item => item.event === 'library-catalog-changed').forEach(item => item.deliver()));
  await waitFor('catalog reconciliation after replay', () => evaluate(() => !!document.querySelector('[aria-label^="Open Akali recording"]')));

  // Block the optional journal read so the actual catalog response reports
  // pending preparation for a recording with a still-valid final result.
  journalLock = spawn('powershell.exe', ['-NoProfile', '-Command',
    '$file=[IO.File]::Open($env:CHRONOBREAK_TEST_JOURNAL, "Open", "Read", "None"); try { "locked"; [Console]::ReadLine() } finally { $file.Dispose() }'],
  { windowsHide: true, env: { ...process.env, CHRONOBREAK_TEST_JOURNAL: path.join(games, '1792431584', 'league_observations.jsonl') } });
  const [locked] = await once(journalLock.stdout, 'data');
  assert.match(String(locked), /locked/);
  await evaluate(() => { window.historyRegression.holdCatalog = true; window.historyRegression.holdQuery = true; });
  const toggle = desired => evaluate(saved => document.querySelector(`[aria-label="${saved ? 'Save' : 'Unsave'} Master Yi recording"]`).click(), desired);
  const savedAndSettled = saved => evaluate(value => {
    const button = document.querySelector(`[aria-label="${value ? 'Unsave' : 'Save'} Master Yi recording"]`);
    return button?.getAttribute('aria-disabled') === 'false';
  }, saved);
  await toggle(true);
  await waitFor('native pending catalog response', () => evaluate(() => window.historyRegression.pendingStatus?.history_ready === false));
  await evaluate(() => { window.historyRegression.holdCatalog = false; window.historyRegression.releases.shift()(); });
  await waitFor('delayed native History response', () => evaluate(() => window.historyRegression.queryWaiting));
  assert.deepEqual(await evaluate(() => window.historyRegression.failures), []);
  await evaluate(() => { window.historyRegression.holdQuery = false; window.historyRegression.releases.splice(0).forEach(resolve => resolve()); });
  await waitFor('favorite to settle', () => savedAndSettled(true));
  await toggle(false);
  await waitFor('unfavorite to settle', () => savedAndSettled(false));
  journalLock.stdin.end('\n');
  await once(journalLock, 'exit');
  journalLock = undefined;
  await openReplay();
  await returnToHistory();
  const proof = await evaluate(() => {
    const { events, calls, historyResponses } = window.historyRegression;
    events.filter(item => item.event === 'league-data-prepared').forEach(item => item.deliver());
    return { historyResponses, events: events.map(item => item.event), calls };
  });
  assert(proof.events.includes('league-data-prepared'), 'No native preparation notification was delayed');
  assert.equal(proof.calls.filter(name => name === 'save_game').length, 2);
  assert.equal(proof.calls.filter(name => name === 'get_replay_descriptor').length, 2);
  await waitFor('History response after delayed preparation notification', () =>
    evaluate(previous => window.historyRegression.historyResponses > previous, proof.historyResponses));
  await evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  assert.deepEqual(await evaluate(() => window.historyRegression.failures), []);
  console.log('Match History remained populated through both replay round trips and both favorite mutations');
} finally {
  journalLock?.kill();
  cdp?.close();
  app.kill();
  console.log(`Isolated test library: ${run}`);
}
