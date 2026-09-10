import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
} from "solid-js";
import {
  chooseMusicFile,
  exportClip,
  loadBuiltInMusic,
  loadPlaybackProbe,
  prepareImportedMusicPreview,
  releaseImportedMusicPreview,
} from "../api";
import { createImportedMusicPreview } from "../importedMusicPreview";
import { formatBytes, formatDuration } from "../format";
import type {
  BuiltInMusicTrack,
  ClipDraft,
  ClipExportPreset,
  ClipExportProgress,
  ClipExportResult,
} from "../types";
import {
  browserSecondsForReplayTick,
  createClipRange,
  frameBoundaryToReplayTick,
} from "../replayTime";
import { clamp } from "../viewerUtils";
import styles from "./ClipExporterScreen.module.css";

type Props = {
  draft: ClipDraft;
  outputPath: string;
  onBack: (draft: ClipDraft) => void;
  onExported: () => void | Promise<void>;
  onOpenClips: () => void;
  onOpenFolder: () => void;
};

type MusicMode = "none" | "builtin" | "file";

const PRESETS: Array<{
  id: ClipExportPreset;
  label: string;
  meta: string;
  detail: string;
}> = [
  {
    id: "discord",
    label: "Discord",
    meta: "SIZE SAFE",
    detail: "Adaptive quality, always below 10 MB",
  },
  {
    id: "horizontal",
    label: "Horizontal",
    meta: "16:9",
    detail: "1080p publish master for YouTube",
  },
  {
    id: "vertical",
    label: "Vertical",
    meta: "9:16",
    detail: "TikTok and YouTube Shorts hybrid frame",
  },
];

const importedFilename = (path: string): string => {
  const segments = path.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] ?? path;
};

const presetLabel = (preset: ClipExportPreset): string =>
  PRESETS.find((option) => option.id === preset)?.label ?? preset;

function ClipExporterScreen(props: Props) {
  let previewVideo!: HTMLVideoElement;
  let previewBackdropVideo: HTMLVideoElement | undefined;
  let musicPreview!: HTMLAudioElement;
  let previewAnimationFrameId: number | undefined;
  let previewSeekTimerId: number | undefined;
  let pendingPreviewSeekMs: number | undefined;
  let lastPreviewSeekAt = Number.NEGATIVE_INFINITY;

  const [probe] = createResource(() => props.draft.gameTimestamp, loadPlaybackProbe);
  const [tracks] = createResource(loadBuiltInMusic);
  const [selectedPresets, setSelectedPresets] = createSignal<readonly ClipExportPreset[]>([
    "horizontal",
  ]);
  const [previewPreset, setPreviewPreset] = createSignal<ClipExportPreset>("horizontal");
  const [verticalFocus, setVerticalFocus] = createSignal(0.72);
  const [verticalPosition, setVerticalPosition] = createSignal(0.5);
  const [musicMode, setMusicMode] = createSignal<MusicMode>("none");
  const [builtInFilename, setBuiltInFilename] = createSignal("");
  const [importedPath, setImportedPath] = createSignal("");
  const [importedPreviewUrl, setImportedPreviewUrl] = createSignal("");
  const [gameVolume, setGameVolume] = createSignal(0.8);
  const [musicVolume, setMusicVolume] = createSignal(1);
  const [previewReady, setPreviewReady] = createSignal(false);
  const [previewPlaying, setPreviewPlaying] = createSignal(false);
  const [previewTimeMs, setPreviewTimeMs] = createSignal(0);
  const [previewError, setPreviewError] = createSignal<string | null>(null);
  const [musicPreviewError, setMusicPreviewError] = createSignal<string | null>(null);
  const [exporting, setExporting] = createSignal(false);
  const [progress, setProgress] = createSignal<ClipExportProgress>({
    stage: "encoding",
    percent: 0,
    preset: null,
    completed_outputs: 0,
    total_outputs: 1,
  });
  const [result, setResult] = createSignal<ClipExportResult | null>(null);
  const [exportError, setExportError] = createSignal<string | null>(null);

  const clipRange = createMemo(() => {
    const timeline = probe()?.media_timeline;
    return timeline
      ? createClipRange(
          timeline,
          props.draft.startFrame,
          props.draft.endFrameExclusive,
          props.draft.mediaId,
        )
      : null;
  });
  const clipStartMs = createMemo(() => {
    const timeline = probe()?.media_timeline;
    const range = clipRange();
    if (!timeline || !range) return 0;
    return browserSecondsForReplayTick(
      timeline,
      frameBoundaryToReplayTick(range.startFrame, timeline.video.frameRate),
    ) * 1_000;
  });
  const clipEndMs = createMemo(() => {
    const timeline = probe()?.media_timeline;
    const range = clipRange();
    if (!timeline || !range) return 0;
    return browserSecondsForReplayTick(
      timeline,
      frameBoundaryToReplayTick(range.endFrameExclusive, timeline.video.frameRate),
    ) * 1_000;
  });
  const durationMs = createMemo(() => clipEndMs() - clipStartMs());
  const frameDurationMs = createMemo(() => {
    const timeline = probe()?.media_timeline;
    if (!timeline) return 1;
    return 1_000 * Number(timeline.video.frameRate.denominator) /
      Number(timeline.video.frameRate.numerator);
  });
  const selectedTrack = createMemo(() =>
    tracks()?.find((track) => track.filename === builtInFilename()),
  );
  const musicPreviewSource = createMemo(() =>
    musicMode() === "builtin"
      ? selectedTrack()?.preview_url ?? ""
      : musicMode() === "file"
        ? importedPreviewUrl()
        : "",
  );
  const foregroundRatio = createMemo(() => 16 / 9 - verticalFocus() * (16 / 9 - 1));
  const estimatedSize = createMemo(() => {
    const bytes = selectedPresets().reduce(
      (total, preset) =>
        total +
        (preset === "discord"
          ? 10_000_000
          : (durationMs() / 1_000) * (24_192_000 / 8)),
      0,
    );
    return `${selectedPresets().length} file${selectedPresets().length === 1 ? "" : "s"} · ${selectedPresets().includes("discord") ? "up to " : "~"}${formatBytes(bytes)}`;
  });

  const clearPreviewClock = () => {
    if (previewAnimationFrameId === undefined) return;
    cancelAnimationFrame(previewAnimationFrameId);
    previewAnimationFrameId = undefined;
  };

  const pausePreview = () => {
    clearPreviewClock();
    previewVideo?.pause();
    previewBackdropVideo?.pause();
    musicPreview?.pause();
    setPreviewPlaying(false);
  };

  const setMediaTime = (element: HTMLMediaElement | undefined, seconds: number) => {
    if (!element || element.readyState < HTMLMediaElement.HAVE_METADATA) return;
    try {
      element.currentTime = clamp(seconds, 0, Math.max(0, element.duration || seconds));
    } catch {
      // The next media event retries synchronization after metadata is available.
    }
  };

  const musicTimeFor = (timeMs: number) => {
    const offsetSeconds = Math.max(0, timeMs - clipStartMs()) / 1_000;
    const musicDuration = musicPreview?.duration;
    return musicDuration && Number.isFinite(musicDuration) && musicDuration > 0
      ? offsetSeconds % musicDuration
      : offsetSeconds;
  };

  const syncSecondaryMedia = (timeMs: number, force = false) => {
    const seconds = timeMs / 1_000;
    if (
      previewBackdropVideo &&
      (force || Math.abs(previewBackdropVideo.currentTime - seconds) > 0.08)
    ) {
      setMediaTime(previewBackdropVideo, seconds);
    }
    if (musicPreviewSource() && musicPreview?.readyState >= HTMLMediaElement.HAVE_METADATA) {
      const musicTime = musicTimeFor(timeMs);
      if (force || Math.abs(musicPreview.currentTime - musicTime) > 0.12) {
        setMediaTime(musicPreview, musicTime);
      }
    }
  };

  const seekPreview = (requestedMs: number) => {
    const targetMs = clamp(
      requestedMs,
      clipStartMs(),
      clipEndMs(),
    );
    setPreviewTimeMs(targetMs);
    setMediaTime(previewVideo, targetMs / 1_000);
    syncSecondaryMedia(targetMs, true);
  };

  const dispatchPreviewSeek = () => {
    if (previewSeekTimerId !== undefined) {
      window.clearTimeout(previewSeekTimerId);
      previewSeekTimerId = undefined;
    }
    if (pendingPreviewSeekMs === undefined || !previewVideo || !previewReady()) return;
    if (previewVideo.seeking) {
      previewSeekTimerId = window.setTimeout(dispatchPreviewSeek, 50);
      return;
    }
    const delay = Math.max(0, lastPreviewSeekAt + 100 - performance.now());
    if (delay > 0) {
      previewSeekTimerId = window.setTimeout(dispatchPreviewSeek, delay);
      return;
    }
    const targetMs = pendingPreviewSeekMs;
    pendingPreviewSeekMs = undefined;
    lastPreviewSeekAt = performance.now();
    seekPreview(targetMs);
  };

  const requestPreviewSeek = (requestedMs: number) => {
    setPreviewTimeMs(
      clamp(requestedMs, clipStartMs(), clipEndMs()),
    );
    pendingPreviewSeekMs = requestedMs;
    dispatchPreviewSeek();
  };

  const resumeSecondaryMedia = () => {
    if (previewPreset() === "vertical" && previewBackdropVideo) {
      void previewBackdropVideo.play().catch(() => undefined);
    }
    if (musicPreviewSource() && musicPreview) {
      void musicPreview.play().catch(() => {
        setMusicPreviewError("The selected music could not be played in the live preview.");
      });
    }
  };

  const monitorPreview = () => {
    previewAnimationFrameId = undefined;
    if (!previewVideo || previewVideo.paused) {
      setPreviewPlaying(false);
      return;
    }
    let currentMs = previewVideo.currentTime * 1_000;
    if (currentMs >= clipEndMs() - 8) {
      currentMs = clipStartMs();
      seekPreview(currentMs);
      resumeSecondaryMedia();
    } else {
      setPreviewTimeMs(currentMs);
      syncSecondaryMedia(currentMs);
    }
    previewAnimationFrameId = requestAnimationFrame(monitorPreview);
  };

  const playPreview = async (restart = false) => {
    if (!previewVideo || !previewReady()) return;
    setPreviewError(null);
    setMusicPreviewError(null);
    if (restart || previewTimeMs() >= clipEndMs() - 20) {
      seekPreview(clipStartMs());
    } else {
      syncSecondaryMedia(previewTimeMs(), true);
    }
    try {
      const primaryPlayback = previewVideo.play();
      // Start every media element while the click's user activation is still live.
      resumeSecondaryMedia();
      await primaryPlayback;
      setPreviewPlaying(true);
      clearPreviewClock();
      previewAnimationFrameId = requestAnimationFrame(monitorPreview);
    } catch (error) {
      pausePreview();
      setPreviewError(
        error instanceof Error ? error.message : "The clip preview could not start.",
      );
    }
  };

  const togglePreview = () => {
    if (previewPlaying()) pausePreview();
    else void playPreview();
  };

  const togglePreset = (preset: ClipExportPreset) => {
    const current = selectedPresets();
    if (current.includes(preset)) {
      if (current.length === 1) return;
      const next = current.filter((selected) => selected !== preset);
      setSelectedPresets(next);
      if (previewPreset() === preset) setPreviewPreset(next[0] ?? "horizontal");
      return;
    }
    setSelectedPresets(
      PRESETS.map((option) => option.id).filter(
        (candidate) => current.includes(candidate) || candidate === preset,
      ),
    );
    setPreviewPreset(preset);
  };

  const activatePreviewPreset = (preset: ClipExportPreset) => {
    if (!selectedPresets().includes(preset)) togglePreset(preset);
    setPreviewPreset(preset);
  };

  createEffect(() => {
    const available = tracks();
    if (available?.length && !builtInFilename()) setBuiltInFilename(available[0].filename);
  });

  createEffect(() => {
    const source = musicPreviewSource();
    pausePreview();
    setMusicPreviewError(null);
    if (!musicPreview) return;
    if (!source) {
      musicPreview.removeAttribute("src");
      musicPreview.load();
      return;
    }
    musicPreview.src = source;
    musicPreview.load();
  });

  const importedPreview = createImportedMusicPreview({
    prepare: prepareImportedMusicPreview,
    release: releaseImportedMusicPreview,
    changed: setImportedPreviewUrl,
    failed: (error) => setMusicPreviewError(
      error instanceof Error ? error.message : "Imported music preview is unavailable.",
    ),
  });
  createEffect(() => importedPreview.select(musicMode() === "file" ? importedPath() : ""));

  createEffect(() => {
    if (previewVideo) {
      previewVideo.volume = musicMode() === "none" ? 1 : gameVolume();
    }
    if (musicPreview) musicPreview.volume = musicVolume();
  });

  createEffect(() => {
    const preset = previewPreset();
    if (preset !== "vertical") {
      previewBackdropVideo?.pause();
      return;
    }
    queueMicrotask(() => {
      syncSecondaryMedia(previewTimeMs(), true);
      if (previewPlaying()) resumeSecondaryMedia();
    });
  });

  onCleanup(() => {
    pausePreview();
    importedPreview.dispose();
    if (previewSeekTimerId !== undefined) window.clearTimeout(previewSeekTimerId);
    previewVideo?.removeAttribute("src");
    previewBackdropVideo?.removeAttribute("src");
    musicPreview?.removeAttribute("src");
    musicPreview?.load();
  });

  const chooseImport = async () => {
    const path = await chooseMusicFile();
    if (!path) return;
    pausePreview();
    setImportedPath(path);
    setMusicMode("file");
    setMusicPreviewError(null);
  };

  const beginFocusDrag = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    event.preventDefault();
    const surface = event.currentTarget;
    const startX = event.clientX;
    const startPosition = verticalPosition();
    const update = (pointerEvent: PointerEvent) => {
      const width = Math.max(surface.getBoundingClientRect().width, 1);
      setVerticalPosition(clamp(startPosition - ((pointerEvent.clientX - startX) / width), 0, 1));
    };
    const finish = (pointerEvent: PointerEvent) => {
      update(pointerEvent);
      surface.removeEventListener("pointermove", update);
      surface.removeEventListener("pointerup", finish);
      surface.removeEventListener("pointercancel", finish);
      if (surface.hasPointerCapture(pointerEvent.pointerId)) {
        surface.releasePointerCapture(pointerEvent.pointerId);
      }
    };
    surface.setPointerCapture(event.pointerId);
    surface.addEventListener("pointermove", update);
    surface.addEventListener("pointerup", finish);
    surface.addEventListener("pointercancel", finish);
  };

  const runExport = async () => {
    if (exporting()) return;
    pausePreview();
    setExporting(true);
    setExportError(null);
    setResult(null);
    setProgress({
      stage: "encoding",
      percent: 0,
      preset: selectedPresets()[0] ?? null,
      completed_outputs: 0,
      total_outputs: selectedPresets().length,
    });
    try {
      const music =
        musicMode() === "builtin"
          ? { kind: "builtin" as const, filename: builtInFilename() }
          : musicMode() === "file"
            ? { kind: "file" as const, path: importedPath() }
            : { kind: "none" as const };
      const exported = await exportClip(
        {
          game_timestamp: props.draft.gameTimestamp,
          media_id: props.draft.mediaId,
          start_frame: props.draft.startFrame.toString(),
          end_frame_exclusive: props.draft.endFrameExclusive.toString(),
          presets: [...selectedPresets()],
          vertical_focus: verticalFocus(),
          vertical_position: verticalPosition(),
          music,
          game_audio_volume: musicMode() === "none" ? 1 : gameVolume(),
          music_volume: musicVolume(),
        },
        setProgress,
      );
      setResult(exported);
      await props.onExported();
    } catch (error) {
      setExportError(error instanceof Error ? error.message : String(error));
    } finally {
      setExporting(false);
    }
  };

  const copyOutputPath = async () => {
    const paths = result()?.outputs.map((output) => output.output_path).join("\n");
    if (paths) await navigator.clipboard.writeText(paths);
  };

  return (
    <section class={styles.exporter} data-testid="clip-exporter">
      <header class={styles.exportHeader}>
        <button
          class={styles.backButton}
          type="button"
          disabled={exporting()}
          onClick={() => props.onBack(props.draft)}
        >
          <span aria-hidden="true">←</span> BACK TO REPLAY
        </button>
        <div>
          <p>CLIP WORKBENCH</p>
          <h1>Publish the moment.</h1>
        </div>
        <dl>
          <div><dt>START</dt><dd>{formatDuration(clipStartMs())}</dd></div>
          <div><dt>END</dt><dd>{formatDuration(clipEndMs())}</dd></div>
          <div><dt>DURATION</dt><dd>{formatDuration(durationMs())}</dd></div>
        </dl>
      </header>

      <Switch>
        <Match when={probe.loading}>
          <div class={styles.loadingState}>Preparing live clip preview...</div>
        </Match>
        <Match when={probe.error}>
          <div class={styles.errorState} role="alert">{String(probe.error)}</div>
        </Match>
        <Match when={probe()}>
          <div class={styles.exportGrid}>
            <section class={styles.previewPanel}>
              <div class={styles.sectionHeading}>
                <span>01 / FORMAT</span>
                <strong>{probe()!.game.champion} · {selectedPresets().length} SELECTED</strong>
              </div>
              <div class={styles.presetGrid} role="group" aria-label="Export formats">
                <For each={PRESETS}>
                  {(option) => (
                    <div
                      classList={{
                        [styles.presetOption]: true,
                        [styles.presetActive]: selectedPresets().includes(option.id),
                        [styles.presetPreviewing]: previewPreset() === option.id,
                      }}
                    >
                      <label>
                        <input
                          type="checkbox"
                          checked={selectedPresets().includes(option.id)}
                          disabled={
                            selectedPresets().length === 1 &&
                            selectedPresets().includes(option.id)
                          }
                          onChange={() => togglePreset(option.id)}
                        />
                        <span>{option.meta}</span>
                        <strong>{option.label}</strong>
                        <small>{option.detail}</small>
                      </label>
                      <button
                        type="button"
                        aria-pressed={previewPreset() === option.id}
                        onClick={() => activatePreviewPreset(option.id)}
                      >
                        {previewPreset() === option.id ? "PREVIEWING" : "PREVIEW"}
                      </button>
                    </div>
                  )}
                </For>
              </div>

              <div
                classList={{
                  [styles.previewStage]: true,
                  [styles.previewStageVertical]: previewPreset() === "vertical",
                }}
              >
                <div
                  classList={{
                    [styles.previewViewport]: true,
                    [styles.verticalCanvas]: previewPreset() === "vertical",
                  }}
                >
                  <Show when={previewPreset() === "vertical"}>
                    <video
                      ref={(element) => {
                        previewBackdropVideo = element;
                      }}
                      class={styles.verticalBackdrop}
                      src={probe()!.video_url || undefined}
                      playsinline
                      muted
                      preload="auto"
                      aria-hidden="true"
                      onLoadedMetadata={() => syncSecondaryMedia(previewTimeMs(), true)}
                    />
                  </Show>
                  <div
                    classList={{
                      [styles.previewForeground]: true,
                      [styles.horizontalFrame]: previewPreset() !== "vertical",
                      [styles.verticalForeground]: previewPreset() === "vertical",
                    }}
                    style={{
                      "aspect-ratio":
                        previewPreset() === "vertical" ? foregroundRatio() : 16 / 9,
                    }}
                    onPointerDown={(event) => {
                      if (previewPreset() === "vertical") beginFocusDrag(event);
                    }}
                    title={
                      previewPreset() === "vertical"
                        ? "Drag to reposition the action"
                        : "Live clip preview"
                    }
                  >
                    <video
                      ref={previewVideo}
                      class={styles.livePreviewVideo}
                      src={probe()!.video_url || undefined}
                      playsinline
                      preload="auto"
                      aria-label={`${previewPreset()} clip preview`}
                      style={{ "object-position": `${verticalPosition() * 100}% 50%` }}
                      onLoadedMetadata={() => {
                        setPreviewReady(true);
                        setPreviewError(null);
                        seekPreview(clipStartMs());
                      }}
                      onCanPlay={() => setPreviewReady(true)}
                      onPlay={() => setPreviewPlaying(true)}
                      onPause={() => {
                        clearPreviewClock();
                        setPreviewPlaying(false);
                      }}
                      onSeeked={() => {
                        setPreviewTimeMs(previewVideo.currentTime * 1_000);
                        syncSecondaryMedia(previewVideo.currentTime * 1_000, true);
                        dispatchPreviewSeek();
                      }}
                      onError={() => {
                        pausePreview();
                        setPreviewReady(false);
                        setPreviewError("The source recording could not be decoded for preview.");
                      }}
                      onClick={() => {
                        if (previewPreset() !== "vertical") togglePreview();
                      }}
                    />
                  </div>
                  <Show when={previewPreset() === "vertical"}>
                    <span class={styles.safeZone} aria-hidden="true" />
                    <small>PLATFORM SAFE AREA · PREVIEW ONLY</small>
                  </Show>
                  <Show when={!previewReady() || previewError()}>
                    <div class={styles.previewMessage} role={previewError() ? "alert" : "status"}>
                      <strong>{previewError() ? "PREVIEW UNAVAILABLE" : "LOADING SOURCE"}</strong>
                      <span>{previewError() ?? "Preparing the selected clip range..."}</span>
                    </div>
                  </Show>
                </div>
                <div class={styles.previewTransport}>
                  <button type="button" disabled={!previewReady()} onClick={togglePreview}>
                    {previewPlaying() ? "Ⅱ PAUSE" : "▶ PLAY CLIP"}
                  </button>
                  <input
                    type="range"
                    min={clipStartMs()}
                    max={clipEndMs()}
                    step={frameDurationMs()}
                    value={previewTimeMs()}
                    aria-label="Clip preview position"
                    onInput={(event) => requestPreviewSeek(Number(event.currentTarget.value))}
                  />
                  <span>
                    {formatDuration(previewTimeMs() - clipStartMs())} / {formatDuration(durationMs())}
                  </span>
                  <button type="button" disabled={!previewReady()} onClick={() => void playPreview(true)}>
                    RESTART
                  </button>
                </div>
                <div class={styles.previewStatusLine}>
                  <span>LIVE SOURCE · {previewPreset().toUpperCase()}</span>
                  <span>{musicMode() === "none" ? "GAME AUDIO" : "GAME + MUSIC MIX"}</span>
                </div>
              </div>

              <Show when={previewPreset() === "vertical"}>
                <div class={styles.reframeControls}>
                  <label>
                    <span><strong>FRAMING</strong><small>Context</small><small>Action</small></span>
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.01"
                      value={verticalFocus()}
                      onInput={(event) => setVerticalFocus(Number(event.currentTarget.value))}
                    />
                  </label>
                  <label>
                    <span><strong>FOCUS</strong><small>Left</small><small>Right</small></span>
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.01"
                      value={verticalPosition()}
                      onInput={(event) => setVerticalPosition(Number(event.currentTarget.value))}
                    />
                  </label>
                  <p>Blurred context stays full-frame; drag the sharp layer to follow the play.</p>
                </div>
              </Show>
            </section>

            <aside class={styles.optionsPanel}>
              <section>
                <div class={styles.sectionHeading}>
                  <span>02 / SOUNDTRACK</span>
                  <strong>Audio mix</strong>
                </div>
                <div class={styles.musicChoices}>
                  <label classList={{ [styles.musicActive]: musicMode() === "none" }}>
                    <input type="radio" name="music" checked={musicMode() === "none"} onChange={() => setMusicMode("none")} />
                    <span><strong>No music</strong><small>Keep original game audio</small></span>
                  </label>
                  <label classList={{ [styles.musicActive]: musicMode() === "builtin" }}>
                    <input type="radio" name="music" checked={musicMode() === "builtin"} onChange={() => setMusicMode("builtin")} />
                    <span><strong>Built-in</strong><small>Cleared for publishing</small></span>
                  </label>
                  <Show when={musicMode() === "builtin"}>
                    <div class={styles.trackPicker}>
                      <select value={builtInFilename()} onChange={(event) => setBuiltInFilename(event.currentTarget.value)}>
                        <For each={tracks() ?? []}>
                          {(track: BuiltInMusicTrack) => <option value={track.filename}>{track.display_name} · {track.mood}</option>}
                        </For>
                      </select>
                      <button type="button" disabled={!selectedTrack()?.preview_url || !previewReady()} onClick={() => void playPreview(true)}>
                        ▶ WITH CLIP
                      </button>
                    </div>
                  </Show>
                  <label classList={{ [styles.musicActive]: musicMode() === "file" }}>
                    <input type="radio" name="music" checked={musicMode() === "file"} onChange={() => importedPath() && setMusicMode("file")} />
                    <span><strong>Import file</strong><small>{importedPath() ? importedFilename(importedPath()) : "MP3 or WAV"}</small></span>
                    <button type="button" onClick={(event) => { event.preventDefault(); void chooseImport(); }}>BROWSE</button>
                  </label>
                </div>
                <Show when={musicPreviewError()}>
                  {(message) => <p class={styles.musicPreviewError} role="alert">{message()}</p>}
                </Show>
                <Show when={musicMode() !== "none"}>
                  <div class={styles.mixControls}>
                    <label>
                      <span>GAME AUDIO <strong>{Math.round(gameVolume() * 100)}%</strong></span>
                      <input type="range" min="0" max="1" step="0.01" value={gameVolume()} onInput={(event) => setGameVolume(Number(event.currentTarget.value))} />
                    </label>
                    <label>
                      <span>MUSIC <strong>{Math.round(musicVolume() * 100)}%</strong></span>
                      <input type="range" min="0" max="1" step="0.01" value={musicVolume()} onInput={(event) => setMusicVolume(Number(event.currentTarget.value))} />
                    </label>
                  </div>
                </Show>
              </section>

              <section class={styles.exportPanel}>
                <div class={styles.sectionHeading}>
                  <span>03 / EXPORT</span>
                  <strong>H.264 · AAC · MP4</strong>
                </div>
                <dl>
                  <div><dt>FORMATS</dt><dd>{selectedPresets().map(presetLabel).join(" + ")}</dd></div>
                  <div><dt>QUALITY</dt><dd>{selectedPresets().some((preset) => preset !== "discord") ? "24 Mbps HQ · up to 1080p60" : "Adaptive size-safe"}</dd></div>
                  <div><dt>EST. SIZE</dt><dd>{estimatedSize()}</dd></div>
                  <div><dt>OUTPUT</dt><dd title={`${props.outputPath}/clips/`}>{props.outputPath}/clips/</dd></div>
                </dl>

                <Show when={exporting()}>
                  <div class={styles.progressBlock} role="status">
                    <div>
                      <span>
                        {progress().stage === "thumbnail"
                          ? "GENERATING THUMBNAIL"
                          : progress().stage === "validating"
                            ? "VALIDATING OUTPUT"
                            : "ENCODING"}
                        {progress().preset ? ` · ${presetLabel(progress().preset!)}` : ""}
                        {` · ${Math.min(progress().completed_outputs + 1, progress().total_outputs)}/${progress().total_outputs}`}
                      </span>
                      <strong>{Math.round(progress().percent)}%</strong>
                    </div>
                    <span><i style={`width:${progress().percent}%`} /></span>
                  </div>
                </Show>

                <Show when={exportError()}>
                  {(message) => <div class={styles.exportError} role="alert"><strong>EXPORT FAILED</strong><span>{message()}</span></div>}
                </Show>

                <Show when={result()}>
                  {(complete) => (
                    <div class={styles.exportSuccess}>
                      <strong>✓ EXPORTED IN {(complete().elapsed_ms / 1_000).toFixed(1)}S</strong>
                      <span>{complete().outputs.length} file{complete().outputs.length === 1 ? "" : "s"} · {formatBytes(complete().total_file_size_bytes)}</span>
                      <ul>
                        <For each={complete().outputs}>
                          {(output) => (
                            <li>
                              <strong>{presetLabel(output.preset)}</strong>
                              <span>{formatBytes(output.file_size_bytes)} · {output.filename}.mp4</span>
                            </li>
                          )}
                        </For>
                      </ul>
                      <div>
                        <button type="button" onClick={props.onOpenFolder}>OPEN FOLDER</button>
                        <button type="button" onClick={() => void copyOutputPath()}>COPY PATH</button>
                        <button type="button" onClick={props.onOpenClips}>VIEW CLIPS</button>
                      </div>
                    </div>
                  )}
                </Show>

                <button
                  class={styles.exportButton}
                  type="button"
                  disabled={exporting() || (musicMode() === "file" && !importedPath()) || (musicMode() === "builtin" && !builtInFilename())}
                  onClick={() => void runExport()}
                >
                  {exporting()
                    ? "EXPORTING..."
                    : exportError()
                      ? "TRY AGAIN"
                      : `EXPORT ${selectedPresets().length} MP4${selectedPresets().length === 1 ? "" : "S"}`}
                  <span aria-hidden="true">→</span>
                </button>
              </section>
            </aside>
          </div>
        </Match>
      </Switch>
      <audio
        ref={musicPreview}
        loop
        onLoadedMetadata={() => syncSecondaryMedia(previewTimeMs(), true)}
        onError={() => {
          if (musicPreviewSource()) {
            setMusicPreviewError("The selected music could not be decoded for preview.");
          }
        }}
      />
    </section>
  );
}

export default ClipExporterScreen;
