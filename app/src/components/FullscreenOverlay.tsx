import { Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { formatDuration } from "../format";
import type {
  ClipRange,
  KdaTimelinePoint,
  ReplayParticipant,
  ViewerEvent,
} from "../types";
import {
  eventCategory,
  eventSummary,
  eventTitle,
  nearestIndexAt,
  replayTickAtFrameBoundary,
} from "../viewerUtils";
import {
  REPLAY_TICKS_PER_SECOND,
  type MediaTimelineV2,
  type ReplayTick,
} from "../replayTime";
import type { TimelineViewport } from "../replayTimelineGeometry";
import ReplayTimeline from "./ReplayTimeline";
import ChampionFilter from "./ChampionFilter";
import PlaybackControls, { type PlaybackControlsProps } from "./PlaybackControls";
import styles from "./FullscreenOverlay.module.css";

type Props = {
  champion: string;
  localPlayerName: string | null;
  events: readonly ViewerEvent[];
  mediaTimeline: MediaTimelineV2;
  replayTick: ReplayTick;
  timelineViewport: TimelineViewport;
  onTimelineViewportChange: (viewport: TimelineViewport) => void;
  presentedTick: ReplayTick | null;
  gameTick: string;
  beforeGameStart: boolean;
  currentKda: KdaTimelinePoint | undefined;
  isPlaying: boolean;
  playback: PlaybackControlsProps["state"];
  onRate: PlaybackControlsProps["onRate"];
  onMuted: PlaybackControlsProps["onMuted"];
  onVolume: PlaybackControlsProps["onVolume"];
  mediaAvailable: boolean;
  participants: readonly ReplayParticipant[];
  selectedPlayers: readonly string[];
  clipRange: ClipRange | null;
  onPlayerToggle: (summonerName: string) => void;
  onPlayerClear: () => void;
  onTogglePlayback: () => void;
  onSeek: (replayTick: ReplayTick) => void;
  onActivateClip: () => void;
  onEventSelect: (event: ViewerEvent) => void;
  onClipEndpointEditStart: (endpoint: "start" | "end") => void;
  onClipEndpointPreview: (endpoint: "start" | "end", requestedTick: ReplayTick) => void;
  onClipEndpointEditFinish: () => void;
  onClipEndpointKeyDown: (event: KeyboardEvent, endpoint: "start" | "end") => void;
  onClipEndpointKeyUp: (event: KeyboardEvent, endpoint: "start" | "end") => void;
  onClipEndpointBlur: (endpoint: "start" | "end") => void;
  onExportClip: () => void;
  onCancelClip: () => void;
  onExit: () => void;
};

function FullscreenOverlay(props: Props) {
  let idleTimer: number | undefined;
  let dismissTimer: number | undefined;
  let unmountTimer: number | undefined;
  let lastTriggeredEvent: ViewerEvent | undefined;

  const [topBarVisible, setTopBarVisible] = createSignal(true);
  const [displayedEvent, setDisplayedEvent] = createSignal<ViewerEvent | null>(null);
  const [eventCardVisible, setEventCardVisible] = createSignal(false);

  const replayEnd = createMemo(() => props.mediaTimeline.video.replayEnd);
  const replayTickToMilliseconds = (tick: ReplayTick): number =>
    Math.floor((tick * 1_000) / REPLAY_TICKS_PER_SECOND);
  const replaySecond = createMemo(() =>
    Math.floor(replayTickToMilliseconds(props.replayTick) / 1_000),
  );
  const gameTickToMilliseconds = (tick: string): number => Number(BigInt(tick) / 1_000n);
  const frameBoundaryTick = (frame: ClipRange["startFrame"]): ReplayTick =>
    replayTickAtFrameBoundary(frame, props.mediaTimeline);
  const mappedEvents = createMemo(() =>
    props.events.filter(
      (event): event is ViewerEvent & { replay_tick: ReplayTick } => event.replay_tick !== undefined,
    ),
  );
  const nearestEventIndex = createMemo(() => props.presentedTick === null ? -1 : nearestIndexAt(mappedEvents(), props.presentedTick));
  const proximityEventIndex = createMemo(() => {
    const index = nearestEventIndex();
    return index >= 0 && Math.abs(mappedEvents()[index].replay_tick - (props.presentedTick ?? 0)) <= REPLAY_TICKS_PER_SECOND
      ? index
      : -1;
  });
  const noteActivity = () => {
    setTopBarVisible(true);
    if (idleTimer !== undefined) window.clearTimeout(idleTimer);
    idleTimer = window.setTimeout(() => setTopBarVisible(false), 3_200);
  };
  const holdHud = () => {
    setTopBarVisible(true);
    if (idleTimer !== undefined) {
      window.clearTimeout(idleTimer);
      idleTimer = undefined;
    }
  };

  const releaseHud = () => {
    noteActivity();
  };

  const dismissEventCard = () => {
    setEventCardVisible(false);
    if (unmountTimer !== undefined) window.clearTimeout(unmountTimer);
    unmountTimer = window.setTimeout(() => setDisplayedEvent(null), 220);
  };

  const showEventCard = (event: ViewerEvent) => {
    if (dismissTimer !== undefined) window.clearTimeout(dismissTimer);
    if (unmountTimer !== undefined) window.clearTimeout(unmountTimer);
    setDisplayedEvent(event);
    setEventCardVisible(false);
    requestAnimationFrame(() => requestAnimationFrame(() => setEventCardVisible(true)));
    dismissTimer = window.setTimeout(dismissEventCard, 3_500);
  };

  createEffect(() => {
    const index = proximityEventIndex();
    if (index < 0) {
      lastTriggeredEvent = undefined;
      return;
    }
    const event = mappedEvents()[index];
    if (event === lastTriggeredEvent) return;
    lastTriggeredEvent = event;
    showEventCard(event);
  });

  onMount(() => {
    noteActivity();
    window.addEventListener("mousemove", noteActivity, { passive: true });
    onCleanup(() => {
      window.removeEventListener("mousemove", noteActivity);
      if (idleTimer !== undefined) window.clearTimeout(idleTimer);
      if (dismissTimer !== undefined) window.clearTimeout(dismissTimer);
      if (unmountTimer !== undefined) window.clearTimeout(unmountTimer);
    });
  });

  return (
    <div class={styles.overlayRoot} data-testid="fullscreen-overlay">
      <header
        classList={{
          [styles.topBar]: true,
          [styles.topBarHidden]: !topBarVisible(),
        }}
        onPointerEnter={holdHud}
        onPointerLeave={releaseHud}
        onFocusIn={holdHud}
        onFocusOut={releaseHud}
        data-testid="fullscreen-topbar"
      >
        <div class={styles.topIdentity}>
          <span class={styles.replayMark}>REPLAY</span>
          <span class={styles.topDivider} />
          <div>
            <small>{props.localPlayerName ?? "LOCAL PLAYER"}</small>
            <strong>{props.champion}</strong>
          </div>
          <span class={styles.topKda}>
            {props.currentKda?.kills ?? 0} / {props.currentKda?.deaths ?? 0} / {props.currentKda?.assists ?? 0}
          </span>
        </div>
        <div class={styles.topStatus}>
          <strong>{props.beforeGameStart ? "--:--" : formatDuration(gameTickToMilliseconds(props.gameTick))}</strong>
          <span class={styles.recStatus}><i aria-hidden="true" /> REC</span>
          <button type="button" onClick={props.onExit} aria-label="Exit fullscreen replay">EXIT <span aria-hidden="true">×</span></button>
        </div>
      </header>

      <Show when={displayedEvent()}>
        {(event) => (
          <article
            classList={{
              [styles.eventCard]: true,
              [styles.eventCardVisible]: eventCardVisible(),
              [styles.eventCardTopHidden]: !topBarVisible(),
              [styles.eventAlly]: event().relation === "ally",
              [styles.eventEnemy]: event().relation === "enemy",
              [styles.eventNeutral]: event().relation === "neutral",
            }}
            data-testid="fullscreen-event-card"
          >
            <span>{eventCategory(event())}</span>
            <strong>{eventTitle(event())}</strong>
            <small>{eventSummary(event())}</small>
          </article>
        )}
      </Show>

      <div
        classList={{
          [styles.rosterHud]: true,
          [styles.hudHidden]: !topBarVisible(),
        }}
        onPointerEnter={holdHud}
        onPointerLeave={releaseHud}
        onFocusIn={holdHud}
        onFocusOut={releaseHud}
      >
        <ChampionFilter
          participants={props.participants}
          selectedPlayers={props.selectedPlayers}
          localPlayerName={props.localPlayerName}
          mode="fullscreen"
          onToggle={props.onPlayerToggle}
          onClear={props.onPlayerClear}
        />
      </div>

      <section
        classList={{
          [styles.bottomHud]: true,
          [styles.hudHidden]: !topBarVisible(),
        }}
        onPointerEnter={holdHud}
        onPointerLeave={releaseHud}
        onFocusIn={holdHud}
        onFocusOut={releaseHud}
        onPointerMove={noteActivity}
        aria-label="Fullscreen replay controls"
        data-testid="fullscreen-hud"
      >
        <div class={styles.timelineHud}>
          <ReplayTimeline
            duration={replayEnd()}
            playhead={props.replayTick}
            viewport={props.timelineViewport}
            onViewportChange={props.onTimelineViewportChange}
            events={mappedEvents()}
            clip={
              props.clipRange
                ? {
                    start: frameBoundaryTick(props.clipRange.startFrame),
                    end: frameBoundaryTick(props.clipRange.endFrameExclusive),
                  }
                : null
            }
            onSeek={props.onSeek}
            onEventSelect={props.onEventSelect}
            onClipEndpointEditStart={props.onClipEndpointEditStart}
            onClipEndpointPreview={props.onClipEndpointPreview}
            onClipEndpointEditFinish={props.onClipEndpointEditFinish}
            onClipEndpointKeyDown={props.onClipEndpointKeyDown}
            onClipEndpointKeyUp={props.onClipEndpointKeyUp}
            onClipEndpointBlur={props.onClipEndpointBlur}
          />
        </div>

        <div class={styles.fullscreenControls}>
          <button
            class={styles.fullscreenPlay}
            type="button"
            disabled={!props.mediaAvailable}
            onClick={props.onTogglePlayback}
            aria-label={
              props.isPlaying
                ? "Pause fullscreen replay"
                : props.clipRange
                  ? "Preview selected clip"
                  : "Play fullscreen replay"
            }
          >
            <span aria-hidden="true">{props.isPlaying ? "Ⅱ" : "▶"}</span>
            {props.isPlaying ? "PAUSE" : props.clipRange ? "PREVIEW CLIP" : "PLAY"}
          </button>

          <Show
            when={props.clipRange}
            fallback={
              <button
                class={styles.clipControl}
                type="button"
                onClick={() => props.onActivateClip()}
              >
                <span aria-hidden="true">✦</span> CLIP
              </button>
            }
          >
            <button
              class={styles.cancelClipControl}
              type="button"
              onClick={props.onCancelClip}
            >
              CANCEL
            </button>
            <button
              class={styles.exportClipControl}
              type="button"
              onClick={props.onExportClip}
            >
              EXPORT CLIP <span aria-hidden="true">→</span>
            </button>
          </Show>

          <span class={styles.controlsDivider} />

          <PlaybackControls
            dark
            state={props.playback}
            onRate={props.onRate}
            onMuted={props.onMuted}
            onVolume={props.onVolume}
          />

          <span class={styles.fullscreenTime}>
            <strong>{formatDuration(replaySecond() * 1_000)}</strong>
            {" / "}
            {formatDuration(replayTickToMilliseconds(replayEnd()))}
          </span>
        </div>
      </section>
    </div>
  );
}

export default FullscreenOverlay;
