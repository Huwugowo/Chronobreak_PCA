import { Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { formatDuration } from "../format";
import type {
  ClipRange,
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
import type { ParticipantSpellAssets, RecordedParticipantState, RecordedReplayState } from "../replayProjection";

type Props = {
  ddragonAssetBaseUrl?: string | null;
  champion: string;
  recordedState?: RecordedReplayState | null;
  localPlayerName: string | null;
  events: readonly ViewerEvent[];
  mediaTimeline: MediaTimelineV2;
  replayTick: ReplayTick;
  timelineViewport: TimelineViewport;
  onTimelineViewportChange: (viewport: TimelineViewport) => void;
  hasMoreEvents?: boolean;
  hasEarlierEvents?: boolean;
  eventsLoading?: boolean;
  onNextEvents?: () => void;
  onFirstEvents?: () => void;
  presentedTick: ReplayTick | null;
  isPlaying: boolean;
  playback: PlaybackControlsProps["state"];
  onRate: PlaybackControlsProps["onRate"];
  onMuted: PlaybackControlsProps["onMuted"];
  onVolume: PlaybackControlsProps["onVolume"];
  mediaAvailable: boolean;
  participants: readonly ReplayParticipant[];
  participantStates?: ReadonlyMap<string, RecordedParticipantState>;
  spellAssets?: ParticipantSpellAssets;
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
  let overlay: HTMLDivElement | undefined;
  let heldPointer: number | undefined;

  const [topBarVisible, setTopBarVisible] = createSignal(true);
  const [hovered, setHovered] = createSignal(false);
  const [focused, setFocused] = createSignal(false);
  const [pressed, setPressed] = createSignal(false);
  const canHide = () => props.fullscreen && props.isPlaying && !props.clipRange
    && !hovered() && !focused() && !pressed();

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
    idleTimer = undefined;
    if (canHide()) idleTimer = window.setTimeout(() => {
      idleTimer = undefined;
      if (canHide()) setTopBarVisible(false);
    }, 3_200);
  };
  createEffect(noteActivity);
  const inHud = (target: EventTarget | null) => target instanceof Element
    && !!overlay?.contains(target) && !!target.closest("[data-hud-region]");
  const pointerDown = (event: PointerEvent) => {
    if (inHud(event.target)) { heldPointer = event.pointerId; setPressed(true); }
    noteActivity();
  };
  const pointerUp = (event: PointerEvent) => {
    if (pressed() && event.pointerId !== heldPointer) return;
    heldPointer = undefined;
    setPressed(false);
    noteActivity();
  };
  const windowBlur = () => {
    heldPointer = undefined;
    setPressed(false); setHovered(false); setFocused(false);
    noteActivity();
  };
  const windowFocus = () => {
    setFocused(inHud(document.activeElement));
    noteActivity();
  };

  onMount(() => {
    // Observe the video's events without adding a surface above it.
    window.addEventListener("pointermove", noteActivity, {
      passive: true,
      capture: true,
    });
    window.addEventListener("pointerdown", pointerDown, {
      passive: true,
      capture: true,
    });
    window.addEventListener("pointerup", pointerUp, true);
    window.addEventListener("pointercancel", pointerUp, true);
    window.addEventListener("lostpointercapture", pointerUp, true);
    window.addEventListener("blur", windowBlur);
    window.addEventListener("focus", windowFocus);
    window.addEventListener("keydown", noteActivity, true);

    onCleanup(() => {
      window.removeEventListener("pointermove", noteActivity, true);
      window.removeEventListener("pointerdown", pointerDown, true);
      window.removeEventListener("pointerup", pointerUp, true);
      window.removeEventListener("pointercancel", pointerUp, true);
      window.removeEventListener("lostpointercapture", pointerUp, true);
      window.removeEventListener("blur", windowBlur);
      window.removeEventListener("focus", windowFocus);
      window.removeEventListener("keydown", noteActivity, true);
      if (idleTimer !== undefined) window.clearTimeout(idleTimer);
    });
  });

  return (
    <div
      ref={overlay}
      class={styles.overlayRoot}
      data-testid="fullscreen-overlay"
    >
      <div classList={{[styles.recordedState]:true,[styles.hudHidden]:!topBarVisible()}} aria-label="Recorded state" title="Last available recorded sample">
        <span>Recorded state</span>
        <dl>
          <div><dt>K / D / A</dt><dd>{props.recordedState?.kills ?? "—"} / {props.recordedState?.deaths ?? "—"} / {props.recordedState?.assists ?? "—"}</dd></div>
          <div><dt>CS</dt><dd>{props.recordedState?.creep_score ?? "—"}</dd></div>
          <div><dt>Level</dt><dd>{props.recordedState?.level ?? "—"}</dd></div>
          <div><dt>Game clock</dt><dd>{props.recordedState?.clock ?? "—"}</dd></div>
        </dl>
      </div>
      <div
        classList={{
          [styles.rosterHud]: true,
          [styles.hudHidden]: !topBarVisible(),
        }}
        data-hud-region
        onPointerEnter={() => setHovered(true)}
        onPointerLeave={() => setHovered(false)}
        onFocusIn={() => setFocused(true)}
        onFocusOut={event => setFocused(inHud(event.relatedTarget))}
      >
        <ChampionFilter
          ddragonAssetBaseUrl={props.ddragonAssetBaseUrl ?? null}
          participants={props.participants}
          participantStates={props.participantStates}
          spellAssets={props.spellAssets}
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
        data-hud-region
        onPointerEnter={() => setHovered(true)}
        onPointerLeave={() => setHovered(false)}
        onFocusIn={() => setFocused(true)}
        onFocusOut={event => setFocused(inHud(event.relatedTarget))}
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

          <Show when={props.hasEarlierEvents}>
            <button class={styles.cancelClipControl} type="button" disabled={props.eventsLoading} onClick={props.onFirstEvents}>First events</button>
          </Show>
          <Show when={props.hasMoreEvents}>
            <button class={styles.cancelClipControl} type="button" disabled={props.eventsLoading} onClick={props.onNextEvents}>Next events</button>
          </Show>

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
