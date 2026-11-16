import { Show, createMemo } from "solid-js";
import type { SeekOptions } from "../playbackController";
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
  ScissorsIcon,
} from "../ui/icons";
import ReplayTimeline from "./ReplayTimeline";
import ChampionFilter from "./ChampionFilter";
import type { PlaybackControlsProps } from "./PlaybackControls";
import MediaTransport from "./MediaTransport";
import { createPlaybackHud } from "./createPlaybackHud";
import styles from "./FullscreenOverlay.module.css";
import type { ParticipantSpellAssets, RecordedParticipantState, RecordedReplayState } from "../replayProjection";

type Props = {
  scoreboardOpen?: boolean;
  onToggleScoreboard?: () => void;
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
  onSeek: (replayTick: ReplayTick, options?: Pick<SeekOptions, "scrubCommit">) => void;
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
  const hud = createPlaybackHud(() => props.fullscreen && props.isPlaying && !props.clipRange);
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
  return (
    <div
      ref={hud.setRoot}
      class={styles.overlayRoot}
      data-testid="fullscreen-overlay"
    >
      <div classList={{[styles.recordedState]:true,[styles.hudHidden]:!hud.visible()}} aria-label="Recorded state" title="Last available recorded sample">
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
          [styles.hudHidden]: !hud.visible(),
        }}
        data-hud-region
        {...hud.regionEvents}
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
          [styles.hudHidden]: !hud.visible(),
        }}
        data-hud-region
        {...hud.regionEvents}
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
            participants={props.participants}
            assetBase={props.ddragonAssetBaseUrl}
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

        <MediaTransport
          playing={props.isPlaying}
          playLabel={props.isPlaying ? "Pause fullscreen replay" : props.clipRange ? "Preview selected clip" : "Play fullscreen replay"}
          playTitle={props.isPlaying ? "Pause" : props.clipRange ? "Preview selected clip" : "Play"}
          mediaAvailable={props.mediaAvailable}
          playback={props.playback}
          onTogglePlayback={props.onTogglePlayback}
          onRate={props.onRate}
          onMuted={props.onMuted}
          onVolume={props.onVolume}
          currentMs={replaySecond() * 1_000}
          durationMs={replayTickToMilliseconds(replayEnd())}
          fullscreen={props.fullscreen}
          onFullscreenToggle={props.onFullscreenToggle}
          beforeTime={
            <Show when={props.onToggleScoreboard}>
              <button class={styles.cancelClipControl} type="button" aria-label="Toggle scoreboard" aria-keyshortcuts="Tab"
                aria-pressed={props.scoreboardOpen ?? false} title="Scoreboard (Tab)" onClick={() => props.onToggleScoreboard?.()}>
                TAB
              </button>
            </Show>
          }
        >
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

        </MediaTransport>
      </section>
    </div>
  );
}

export default FullscreenOverlay;
