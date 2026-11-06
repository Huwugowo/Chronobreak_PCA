import { createRequire } from 'node:module';
import { writeFile, readFile } from 'node:fs/promises';
import path from 'node:path';

const [playwrightModule, endpoint, run] = process.argv.slice(2);
if (!playwrightModule || !/^http:\/\/127\.0\.0\.1:\d+$/.test(endpoint ?? '') || !run) throw new Error('Expected Playwright module, loopback CDP endpoint and proof root');
const { chromium } = createRequire(import.meta.url)(playwrightModule);
let browser;
let connectionError;
const deadline = Date.now() + 30000;
while (!browser && Date.now() < deadline) {
  try { browser = await chromium.connectOverCDP(endpoint, { timeout: 2000 }); }
  catch (error) { connectionError = error; await new Promise(resolve => setTimeout(resolve, 250)); }
}
if (!browser) throw new Error('Isolated WebView CDP connection unavailable: ' + connectionError?.message.split('\n')[0]);
try {
  let page;
  const pageDeadline = Date.now() + 15000;
  while (!page && Date.now() < pageDeadline) {
    page = browser.contexts().flatMap(context => context.pages()).find(p => /^https?:\/\/tauri\.localhost(?:\/|$)/.test(p.url()));
    if (!page) await new Promise(resolve => setTimeout(resolve, 200));
  }
  if (!page) throw new Error('Production Tauri page missing');
  await page.waitForFunction(() => typeof window.__TAURI_INTERNALS__?.invoke === 'function');
  const outputPath = await page.evaluate(async () => (await window.__TAURI_INTERNALS__.invoke('get_settings')).output_path);
  if (typeof outputPath !== 'string' || path.resolve(outputPath).toLowerCase() !== path.resolve(run, 'library').toLowerCase())
    throw new Error('Connected app does not own the isolated generated library');
  await page.getByRole('button', { name: /Open .* recording from/ }).first().click({ force: true, timeout: 30000 });
  await page.locator('video[data-qb-primary-playback]').waitFor({ timeout: 15000 });
  await page.waitForFunction(() => document.querySelector('video[data-qb-primary-playback]')?.readyState >= 2);
  const session = await browser.contexts()[0].newCDPSession(page);
  const evaluation = await session.send('Runtime.evaluate', {
    expression: `(${proof.toString()})()`, awaitPromise: true, returnByValue: true, userGesture: true,
  });
  if (evaluation.exceptionDetails) throw new Error('WebView proof evaluation failed: ' + evaluation.exceptionDetails.text);
  const result = evaluation.result.value;
  const fixture = JSON.parse(await readFile(path.join(run, 'fixture.json'), 'utf8'));
  await writeFile(path.join(run, 'result.json'), JSON.stringify({
    schema_version: 1, feature: 'QB-AUDIO-001', observed_at_utc: new Date().toISOString(),
    scope: 'generated four-channel AAC; actual WebView2 capability; standalone native video and Web Audio nodes',
    runtime: browser.version(), ...fixture, ...result,
    product_controls_verified: false, production_capture_timing_proven: false,
  }, null, 2));
  console.log(JSON.stringify({ passed: result.passed, runtime: browser.version(),
    channel_identity_passed: result.channel_identity_passed,
    independent_gain_mute_passed: result.independent_gain_mute_passed,
    native_baseline_rates: result.native_baseline_rates, rates: result.rates, native_loop_passed: result.native_loop_passed,
    reload_passed: result.reload_passed, layout_passed: result.layout_passed }));
  if (!result.passed) process.exitCode = 1;
} finally { await browser.close(); }

async function proof() {
  const primary = document.querySelector('video[data-qb-primary-playback]');
  primary.pause();
  const video = document.createElement('video');
  video.crossOrigin = 'anonymous';
  video.style.cssText = 'position:fixed;left:0;top:0;width:320px;height:180px;z-index:99999';
  video.src = primary.currentSrc;
  document.body.append(video);
  const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
  const bounded = async (operation, promise) => {
    let timer;
    try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(operation + ' deadline')), 4000); })]); }
    finally { clearTimeout(timer); }
  };
  const seek = async target => {
    await bounded('native seek', new Promise(resolve => {
      video.addEventListener('seeked', resolve, { once: true }); video.currentTime = target;
    }));
  };
  const measureRate = async requested => {
    video.pause(); await seek(2); video.playbackRate = requested;
    await bounded('rate play', video.play());
    const frames = [];
    let callback;
    const frame = (_, metadata) => {
      frames.push({ media: metadata.mediaTime, display: metadata.expectedDisplayTime });
      callback = video.requestVideoFrameCallback(frame);
    };
    callback = video.requestVideoFrameCallback(frame);
    try { await wait(850); } finally { video.cancelVideoFrameCallback(callback); }
    const first = frames[0], last = frames.at(-1);
    const observed = frames.length >= 3 ? (last.media - first.media) / ((last.display - first.display) / 1000) : null;
    return { requested, adopted: video.playbackRate, callbacks: frames.length, observed,
      passed: observed !== null && observed / requested > 0.8 && observed / requested < 1.2 };
  };
  const context = new AudioContext({ sampleRate: 48000 });
  await bounded('audio context resume', context.resume());
  await bounded('baseline video play', video.play());
  const nativeBaseline = [];
  for (const requested of [0.25, 0.5, 1, 2, 4, 8]) nativeBaseline.push(await measureRate(requested));
  video.pause(); video.playbackRate = 1; await seek(0);
  const source = context.createMediaElementSource(video);
  const split = context.createChannelSplitter(4);
  const merge = context.createChannelMerger(2);
  const channelAnalysers = Array.from({ length: 4 }, () => context.createAnalyser());
  const gains = Array.from({ length: 4 }, () => context.createGain());
  const mixSplit = context.createChannelSplitter(2);
  const mixAnalysers = Array.from({ length: 2 }, () => context.createAnalyser());
  const output = context.createChannelMerger(2);
  const observations = [];
  const frequencies = [697, 941, 1301, 1709];
  const nodes = [source, split, merge, mixSplit, output, ...channelAnalysers, ...gains, ...mixAnalysers];
  const amplitude = (analyser, hz) => {
    const samples = new Float32Array(analyser.fftSize);
    analyser.getFloatTimeDomainData(samples);
    let real = 0, imaginary = 0;
    for (let i = 0; i < samples.length; i++) {
      const phase = 2 * Math.PI * hz * i / context.sampleRate;
      real += samples[i] * Math.cos(phase); imaginary += samples[i] * Math.sin(phase);
    }
    return 2 * Math.hypot(real, imaginary) / samples.length;
  };
  const sample = name => {
    const channels = channelAnalysers.map(analyser => frequencies.map(hz => amplitude(analyser, hz)));
    const mix = mixAnalysers.map(analyser => frequencies.map(hz => amplitude(analyser, hz)));
    const channelSignals = channelAnalysers.map(analyser => {
      const samples = new Float32Array(analyser.fftSize); analyser.getFloatTimeDomainData(samples);
      const bins = new Float32Array(analyser.frequencyBinCount); analyser.getFloatFrequencyData(bins);
      let square = 0, peakBin = 1;
      for (const value of samples) square += value * value;
      for (let i = 2; i < bins.length; i++) if (bins[i] > bins[peakBin]) peakBin = i;
      return { rms: Math.sqrt(square / samples.length), peak_hz: peakBin * context.sampleRate / analyser.fftSize };
    });
    const observation = { name, channels, mix, channel_signals: channelSignals,
      media_time: video.currentTime, rate: video.playbackRate, paused: video.paused };
    observations.push(observation); return observation;
  };
  try {
    for (const analyser of [...channelAnalysers, ...mixAnalysers]) { analyser.fftSize = 8192; analyser.smoothingTimeConstant = 0; }
    source.connect(split);
    for (let i = 0; i < 4; i++) {
      split.connect(channelAnalysers[i], i); channelAnalysers[i].connect(gains[i]); gains[i].connect(merge, 0, i % 2);
    }
    merge.connect(mixSplit);
    for (let i = 0; i < 2; i++) { mixSplit.connect(mixAnalysers[i], i); mixAnalysers[i].connect(output, 0, i); }
    output.connect(context.destination);
    await bounded('audio context resume', context.resume()); await bounded('initial video play', video.play()); await wait(700);
    const all = sample('both-groups');
    const channelIdentityPassed = all.channels.every((row, i) => row[i] >= 0.02 && row.every((value, j) => i === j || value / row[i] < 0.05));
    gains[0].gain.value = 0; gains[1].gain.value = 0;
    await wait(350); const leagueMuted = sample('league-muted');
    gains[0].gain.value = 0.5; gains[1].gain.value = 0.5; gains[2].gain.value = 0; gains[3].gain.value = 0;
    await wait(350); const otherMuted = sample('other-muted-league-half');
    const mixPassed = [0, 1].every(side =>
      leagueMuted.mix[side][side] < 0.001 && leagueMuted.mix[side][side + 2] >= 0.02 &&
      otherMuted.mix[side][side] / all.mix[side][side] > 0.47 && otherMuted.mix[side][side] / all.mix[side][side] < 0.53 &&
      otherMuted.mix[side][side + 2] < 0.001);
    video.pause(); const pauseTime = video.currentTime; await wait(300);
    const pausePassed = Math.abs(video.currentTime - pauseTime) < 0.001;
    await seek(9);
    const seekPassed = Math.abs(video.currentTime - 9) < 1 / 60;
    gains[0].gain.value = 1; gains[1].gain.value = 1; gains[2].gain.value = 1; gains[3].gain.value = 1;
    await bounded('video play after seek', video.play()); await wait(350); const afterSeek = sample('after-seek');
    const afterSeekPassed = afterSeek.channels.every((row, i) => row[i] >= 0.02);
    const rates = [];
    for (const requested of [0.25, 0.5, 1, 2, 4, 8]) {
      const rate = await measureRate(requested);
      const spectral = sample('rate-' + requested);
      // Native pitch preservation modulates phase at non-unit rates. Presence is
      // measured by signal energy and its dominant frequency, not one narrow
      // fixed-phase projection which can cancel over the analyser window.
      const channelsPresent = spectral.channel_signals.every((signal, i) =>
        signal.rms >= 0.005 && Math.abs(signal.peak_hz - frequencies[i]) <= 15);
      rates.push({ ...rate,
        channels_present: channelsPresent,
        passed: rate.passed && channelsPresent });
    }
    video.pause(); video.playbackRate = 1; video.loop = true; await seek(19.6);
    await bounded('loop play', video.play()); await wait(850);
    const loop = sample('native-loop');
    const loopPassed = loop.media_time < 1.5 && loop.channels.every((row, i) => row[i] >= 0.02);
    video.loop = false; video.pause(); video.load();
    await bounded('reload play', video.play()); await wait(400);
    const reload = sample('same-element-reload');
    const reloadPassed = reload.media_time > 0 && reload.channels.every((row, i) => row[i] >= 0.02);
    const container = document.createElement('div'); document.body.append(container); container.append(video);
    video.style.width = '640px'; video.style.height = '360px'; await wait(350);
    const layout = sample('persistent-element-layout-move');
    const layoutPassed = layout.media_time > reload.media_time && layout.channels.every((row, i) => row[i] >= 0.02);
    container.replaceWith(video);
    // A deliberately swapped channel contract must fail the same identity assertion.
    const swappedIdentityAccepted = all.channels.every((row, i) => row[(i + 2) % 4] >= 0.02);
    return {
      passed: channelIdentityPassed && mixPassed && pausePassed && seekPassed && afterSeekPassed &&
        rates.every(rate => rate.passed) && loopPassed && reloadPassed && layoutPassed && !swappedIdentityAccepted,
      channel_identity_passed: channelIdentityPassed, independent_gain_mute_passed: mixPassed,
      native_pause_passed: pausePassed, native_seek_passed: seekPassed, after_seek_channels_passed: afterSeekPassed,
      swapped_identity_rejected: !swappedIdentityAccepted, context_sample_rate: context.sampleRate,
      native_baseline_rates: nativeBaseline, rates,
      native_loop_passed: loopPassed, reload_passed: reloadPassed, layout_passed: layoutPassed,
      tested_pipeline_media_elements: 1, tested_parallel_audio_renderers: 0, observations,
    };
  } finally {
    video.pause(); video.removeAttribute('src'); video.load(); video.remove();
    for (const node of nodes) node.disconnect();
    await context.close();
  }
}
