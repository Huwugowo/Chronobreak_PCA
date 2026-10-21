/** Opt-in rendering fixture, never imported by the application entry point. */
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import ReplayTimeline from "../components/ReplayTimeline";
import { parseMediaId, REPLAY_TICKS_PER_SECOND, type ReplayTick } from "../replayTime";
import { wholeTimelineViewport } from "../replayTimelineGeometry";
import type { ViewerEvent } from "../types";
import "../styles/global.css";

if (!import.meta.env.DEV) throw new Error("Timeline fixture requires the development server");
const tick = (seconds: number) => Math.round(seconds * REPLAY_TICKS_PER_SECOND) as ReplayTick;
const duration = tick(1200);
const events = Array.from({ length: 512 }, (_, index): ViewerEvent & { replay_tick: ReplayTick } => ({
  event_type: "ChampionKill", replay_tick: tick(index * 2), game_tick: "0",
  killer: null, victim: null, assisters: [], dragon_type: null, stolen: null,
  recipient: null, kill_streak: null, acer: null, acing_team: null, turret: null,
  inhibitor: null, result: null, relation: "neutral", actors: {}, local_participant: 1,
  source: { media_id: parseMediaId("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"),
    run_id: parseMediaId("11111111-1111-1111-1111-111111111111"), epoch: 1 },
}));
const [playhead, setPlayhead] = createSignal(tick(1));
const [viewport, setViewport] = createSignal(wholeTimelineViewport(duration));
let playheadReads = 0;
const host = document.getElementById("root")!;
host.style.cssText = "max-width:1120px;margin:80px auto;padding:0 24px";
const props = {
  duration, events, localPlayerName: null, clip: null,
  get playhead() { playheadReads++; return playhead(); },
  get viewport() { return viewport(); },
  onViewportChange: setViewport, onSeek: setPlayhead,
  onEventSelect: (event: typeof events[number]) => setPlayhead(event.replay_tick),
  onClipEndpointEditStart: () => {}, onClipEndpointPreview: () => {},
  onClipEndpointEditFinish: () => {}, onClipEndpointKeyDown: () => {},
  onClipEndpointKeyUp: () => {}, onClipEndpointBlur: () => {},
};
render(() => <ReplayTimeline {...props} />, host);

declare global {
  interface Window { runTimelineBenchmark: () => Promise<object>; }
}
window.runTimelineBenchmark = async () => {
  setViewport(wholeTimelineViewport(duration)); setPlayhead(tick(1));
  await new Promise(requestAnimationFrame);
  playheadReads = 0;
  const updates: number[] = [], gaps: number[] = [];
  let previous = performance.now();
  for (let frame = 1; frame <= 240; frame++) {
    await new Promise<void>(resolve => requestAnimationFrame(now => {
      gaps.push(now - previous); previous = now;
      const start = performance.now();
      setPlayhead(tick(1 + frame / 60));
      updates.push(performance.now() - start);
      resolve();
    }));
  }
  const percentile = (values: number[], ratio: number) => [...values].sort((a, b) => a - b)[Math.floor((values.length - 1) * ratio)];
  return { frames: 240, anchors: host.querySelectorAll('[data-event-anchor]').length,
    elapsed_anchors: host.querySelectorAll('[data-event-anchor][data-elapsed="true"]').length,
    playhead_reads: playheadReads, update_total_ms: updates.reduce((sum, value) => sum + value, 0),
    update_p95_ms: percentile(updates, .95), update_max_ms: Math.max(...updates),
    frame_gap_p95_ms: percentile(gaps, .95), frame_gap_max_ms: Math.max(...gaps) };
};
host.dataset.ready = "true";
