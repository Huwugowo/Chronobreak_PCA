import { Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { formatDuration } from "../format";
import type {
  ClipRange,
  KdaTimelinePoint,
  ReplayParticipant,
  ViewerEvent,
} from "../types";
import { replayTickAtFrameBoundary } from "../viewerUtils";
import {
  REPLAY_TICKS_PER_SECOND,
  type MediaTimelineV2,
  type ReplayTick,
} from "../replayTime";
import type { TimelineViewport } from "../replayTimelineGeometry";
import {
  ArrowRightIcon,
  FullscreenExitIcon,
  FullscreenIcon,
  PauseIcon,
  PlayIcon,
  ScissorsIcon,
} from "../ui/icons";
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
  fullscreen: boolean;
  onFullscreenToggle: () => void;
};

function FullscreenOverlay(props: Props) {
  let idleTimer: number | undefined;

  const [topBarVisible, setTopBarVisible] = createSignal(true);

  const replayEnd = createMemo(() => props.mediaTimeline.video.replayEnd);
  const replayTickToMilliseconds = (tick: ReplayTick): number =>
    Math.floor((tick * 1_000) / REPLAY_TICKS_PER_SECOND);
  const replaySecond = createMemo(() =>
    Math.floor(replayTickToMilliseconds(props.replayTick) / 1_000),
  );
  const frameBoundaryTick = (frame: ClipRange["startFrame"]): ReplayTick =>
    replayTickAtFrameBoundary(frame, props.mediaTimeline);
  const mappedEvents = createMemo(() =>
    props.events.filter(
      (event): event is ViewerEvent & { replay_tick: ReplayTick } => event.replay_tick !== undefined,
    ),
  );
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

  onMount(() => {
    noteActivity();

    // Wake fullscreen chrome from any pointer activity, even when the HUD
    // itself is hidden and therefore cannot receive pointer events.
    window.addEventListener("pointermove", noteActivity, {
      passive: true,
      capture: true,
    });
    window.addEventListener("pointerdown", noteActivity, {
      passive: true,
      capture: true,
    });

    onCleanup(() => {
      window.removeEventListener("pointermove", noteActivity, true);
      window.removeEventListener("pointerdown", noteActivity, true);
      if (idleTimer !== undefined) window.clearTimeout(idleTimer);
    });
  });

  return (
    <div
      classList={{
        [styles.overlayRoot]: true,
        [styles.overlayRootHudWake]: !topBarVisible(),
      }}
      onPointerMove={noteActivity}
      onPointerDown={noteActivity}
      data-testid="fullscreen-overlay"
    >
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
        aria-label="Replay controls"
        data-testid="fullscreen-hud"
      >
        <div class={styles.timelineHud}>
          <ReplayTimeline
            duration={replayEnd()}
            playhead={props.replayTick}
            viewport={props.timelineViewport}
            onViewportChange={props.onTimelineViewportChange}
            events={mappedEvents()}
            localPlayerName={props.localPlayerName}
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
            title={
              props.isPlaying
                ? "Pause"
                : props.clipRange
                  ? "Preview selected clip"
                  : "Play"
            }
          >
            {props.isPlaying ? <PauseIcon size={18} /> : <PlayIcon size={18} />}
            <span class={styles.srOnly}>
              {props.isPlaying
                ? "PAUSE"
                : props.clipRange
                  ? "PREVIEW CLIP"
                  : "PLAY"}
            </span>
          </button>

          <PlaybackControls
            dark
            state={props.playback}
            onRate={props.onRate}
            onMuted={props.onMuted}
            onVolume={props.onVolume}
          />

          <span class={styles.fullscreenTime} data-testid="time-readout">
            <strong class={styles.fullscreenTimeCurrent}>
              {formatDuration(replaySecond() * 1_000)}
            </strong>
            <span class={styles.fullscreenTimeDivider}>/</span>
            <span class={styles.fullscreenTimeDuration}>
              {formatDuration(replayTickToMilliseconds(replayEnd()))}
            </span>
          </span>

          <Show
            when={props.clipRange}
            fallback={
              <button
                class={styles.clipControl}
                type="button"
                onClick={() => props.onActivateClip()}
                title="Create clip"
              >
                <ScissorsIcon size={15} />
                <span>Clip</span>
              </button>
            }
          >
            <button
              class={styles.cancelClipControl}
              type="button"
              onClick={props.onCancelClip}
            >
              Cancel
            </button>

            <button
              class={styles.exportClipControl}
              type="button"
              onClick={props.onExportClip}
            >
              Export clip
              <ArrowRightIcon size={15} />
            </button>
          </Show>

          <button
            class={styles.exitFullscreenControl}
            type="button"
            onClick={props.onFullscreenToggle}
            aria-label={props.fullscreen ? "Exit fullscreen replay" : "Open fullscreen replay"}
            title={props.fullscreen ? "Exit fullscreen" : "Fullscreen"}
          >
            <Show when={props.fullscreen} fallback={<FullscreenIcon size={18} />}>
              <FullscreenExitIcon size={18} />
            </Show>
          </button>
        </div>      </section>
    </div>
  );
}

export default FullscreenOverlay;
