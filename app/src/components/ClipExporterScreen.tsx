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
} from "../api";
import { formatBytes, formatDuration } from "../format";
import type {
  BuiltInMusicTrack,
  ClipDraft,
  ClipExportPreset,
  ClipExportProgress,
  ClipExportResult,
} from "../types";
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

const captureFrame = async (source: string, timeMs: number): Promise<string> => {
  const video = document.createElement("video");
  video.crossOrigin = "anonymous";
  video.muted = true;
  video.playsInline = true;
  video.preload = "auto";

  const waitFor = (event: "loadedmetadata" | "seeked"): Promise<void> =>
    new Promise((resolve, reject) => {
      const timeout = window.setTimeout(() => finish(new Error("Preview frame timed out")), 8_000);
      const finish = (error?: Error) => {
        window.clearTimeout(timeout);
        video.removeEventListener(event, ready);
        video.removeEventListener("error", failed);
        error ? reject(error) : resolve();
      };
      const ready = () => finish();
      const failed = () => finish(new Error("Preview frame could not be decoded"));
      video.addEventListener(event, ready, { once: true });
      video.addEventListener("error", failed, { once: true });
    });

  try {
    const metadata = waitFor("loadedmetadata");
    video.src = source;
    video.load();
    await metadata;
    const targetSeconds = clamp(timeMs / 1_000, 0, Math.max(0, video.duration - 0.05));
    if (Math.abs(video.currentTime - targetSeconds) > 0.01) {
      const seeked = waitFor("seeked");
      video.currentTime = targetSeconds;
      await seeked;
    }

    const width = Math.min(video.videoWidth || 1_280, 1_280);
    const height = Math.max(1, Math.round(width * ((video.videoHeight || 720) / (video.videoWidth || 1_280))));
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Preview canvas is unavailable");
    context.drawImage(video, 0, 0, width, height);
    const blob = await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob(
        (value) => (value ? resolve(value) : reject(new Error("Preview frame capture failed"))),
        "image/jpeg",
        0.86,
      ),
    );
    return URL.createObjectURL(blob);
  } finally {
    video.pause();
    video.removeAttribute("src");
    video.load();
  }
};

function ClipExporterScreen(props: Props) {
  let audioPreview!: HTMLAudioElement;
  let previewTimer: number | undefined;

  const [probe] = createResource(() => props.draft.gameTimestamp, loadPlaybackProbe);
  const [tracks] = createResource(loadBuiltInMusic);
  const [preset, setPreset] = createSignal<ClipExportPreset>("discord");
  const [verticalFocus, setVerticalFocus] = createSignal(0.72);
  const [verticalPosition, setVerticalPosition] = createSignal(0.5);
  const [musicMode, setMusicMode] = createSignal<MusicMode>("none");
  const [builtInFilename, setBuiltInFilename] = createSignal("");
  const [importedPath, setImportedPath] = createSignal("");
  const [gameVolume, setGameVolume] = createSignal(0.8);
  const [musicVolume, setMusicVolume] = createSignal(1);
  const [previewFrame, setPreviewFrame] = createSignal<string | null>(null);
  const [previewError, setPreviewError] = createSignal(false);
  const [playingMusic, setPlayingMusic] = createSignal(false);
  const [exporting, setExporting] = createSignal(false);
  const [progress, setProgress] = createSignal<ClipExportProgress>({
    stage: "encoding",
    percent: 0,
  });
  const [result, setResult] = createSignal<ClipExportResult | null>(null);
  const [exportError, setExportError] = createSignal<string | null>(null);

  const durationMs = createMemo(() => props.draft.clipEndMs - props.draft.clipStartMs);
  const selectedTrack = createMemo(() =>
    tracks()?.find((track) => track.filename === builtInFilename()),
  );
  const foregroundRatio = createMemo(() => 16 / 9 - verticalFocus() * (16 / 9 - 1));
  const estimatedSize = createMemo(() => {
    if (preset() === "discord") return "< 10 MB";
    const bitrate = 12_192_000;
    return `~${formatBytes((durationMs() / 1_000) * (bitrate / 8))}`;
  });

  const stopMusicPreview = () => {
    if (previewTimer !== undefined) window.clearTimeout(previewTimer);
    previewTimer = undefined;
    setPlayingMusic(false);
    if (!audioPreview) return;
    audioPreview.pause();
    audioPreview.currentTime = 0;
  };

  const playMusicPreview = async () => {
    const track = selectedTrack();
    if (!track?.preview_url || !audioPreview) return;
    if (playingMusic()) {
      stopMusicPreview();
      return;
    }
    audioPreview.src = track.preview_url;
    audioPreview.currentTime = 0;
    try {
      await audioPreview.play();
      setPlayingMusic(true);
      previewTimer = window.setTimeout(stopMusicPreview, 10_000);
    } catch {
      setPlayingMusic(false);
    }
  };

  createEffect(() => {
    const available = tracks();
    if (available?.length && !builtInFilename()) setBuiltInFilename(available[0].filename);
  });

  createEffect(() => {
    musicMode();
    builtInFilename();
    stopMusicPreview();
  });

  createEffect(() => {
    const playback = probe();
    if (!playback?.video_url) {
      setPreviewFrame(null);
      return;
    }
    let disposed = false;
    let frameUrl: string | null = null;
    setPreviewError(false);
    void captureFrame(playback.video_url, props.draft.clipStartMs)
      .then((url) => {
        frameUrl = url;
        if (disposed) URL.revokeObjectURL(url);
        else setPreviewFrame(url);
      })
      .catch(() => {
        if (!disposed) setPreviewError(true);
      });
    onCleanup(() => {
      disposed = true;
      if (frameUrl) URL.revokeObjectURL(frameUrl);
    });
  });

  onCleanup(stopMusicPreview);

  const chooseImport = async () => {
    const path = await chooseMusicFile();
    if (!path) return;
    setImportedPath(path);
    setMusicMode("file");
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
    setExporting(true);
    setExportError(null);
    setResult(null);
    setProgress({ stage: "encoding", percent: 0 });
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
          clip_start_ms: props.draft.clipStartMs,
          clip_end_ms: props.draft.clipEndMs,
          preset: preset(),
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
    const path = result()?.output_path;
    if (path) await navigator.clipboard.writeText(path);
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
          <div><dt>START</dt><dd>{formatDuration(props.draft.clipStartMs)}</dd></div>
          <div><dt>END</dt><dd>{formatDuration(props.draft.clipEndMs)}</dd></div>
          <div><dt>DURATION</dt><dd>{formatDuration(durationMs())}</dd></div>
        </dl>
      </header>

      <Switch>
        <Match when={probe.loading}>
          <div class={styles.loadingState}>Preparing clip frame...</div>
        </Match>
        <Match when={probe.error}>
          <div class={styles.errorState} role="alert">{String(probe.error)}</div>
        </Match>
        <Match when={probe()}>
          <div class={styles.exportGrid}>
            <section class={styles.previewPanel}>
              <div class={styles.sectionHeading}>
                <span>01 / FORMAT</span>
                <strong>{probe()!.game.champion} · {preset().toUpperCase()}</strong>
              </div>
              <div class={styles.presetGrid} role="radiogroup" aria-label="Export format">
                <For each={PRESETS}>
                  {(option) => (
                    <button
                      type="button"
                      role="radio"
                      aria-checked={preset() === option.id}
                      classList={{ [styles.presetActive]: preset() === option.id }}
                      onClick={() => setPreset(option.id)}
                    >
                      <span>{option.meta}</span>
                      <strong>{option.label}</strong>
                      <small>{option.detail}</small>
                    </button>
                  )}
                </For>
              </div>

              <div
                classList={{
                  [styles.previewStage]: true,
                  [styles.previewStageVertical]: preset() === "vertical",
                }}
              >
                <Show
                  when={previewFrame()}
                  fallback={
                    <div class={styles.framePlaceholder}>
                      <strong>{previewError() ? "FRAME UNAVAILABLE" : "LR"}</strong>
                      <span>{previewError() ? "The clip can still be exported." : "CAPTURE PREVIEW"}</span>
                    </div>
                  }
                >
                  {(frame) => (
                    <Show
                      when={preset() === "vertical"}
                      fallback={<img class={styles.horizontalFrame} src={frame()} alt="Clip start frame" />}
                    >
                      <div class={styles.verticalCanvas}>
                        <img class={styles.verticalBackdrop} src={frame()} alt="" />
                        <div
                          class={styles.verticalForeground}
                          style={{ "aspect-ratio": foregroundRatio() }}
                          onPointerDown={beginFocusDrag}
                          title="Drag to reposition the action"
                        >
                          <img
                            src={frame()}
                            alt="Vertical clip start frame"
                            style={{ "object-position": `${verticalPosition() * 100}% 50%` }}
                          />
                        </div>
                        <span class={styles.safeZone} aria-hidden="true" />
                        <small>PLATFORM SAFE AREA · PREVIEW ONLY</small>
                      </div>
                    </Show>
                  )}
                </Show>
              </div>

              <Show when={preset() === "vertical"}>
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
                      <button type="button" disabled={!selectedTrack()?.preview_url} onClick={() => void playMusicPreview()}>
                        {playingMusic() ? "STOP" : "▶ 10S"}
                      </button>
                    </div>
                  </Show>
                  <label classList={{ [styles.musicActive]: musicMode() === "file" }}>
                    <input type="radio" name="music" checked={musicMode() === "file"} onChange={() => importedPath() && setMusicMode("file")} />
                    <span><strong>Import file</strong><small>{importedPath() ? importedFilename(importedPath()) : "MP3 or WAV"}</small></span>
                    <button type="button" onClick={(event) => { event.preventDefault(); void chooseImport(); }}>BROWSE</button>
                  </label>
                </div>
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
                  <div><dt>FORMAT</dt><dd>{preset() === "vertical" ? "1080 × 1920" : preset() === "horizontal" ? "16:9 · up to 1080p" : "Adaptive · up to 720p"}</dd></div>
                  <div><dt>EST. SIZE</dt><dd>{estimatedSize()}</dd></div>
                  <div><dt>OUTPUT</dt><dd title={`${props.outputPath}/clips/`}>{props.outputPath}/clips/</dd></div>
                </dl>

                <Show when={exporting()}>
                  <div class={styles.progressBlock} role="status">
                    <div><span>{progress().stage === "thumbnail" ? "GENERATING THUMBNAIL" : "ENCODING CLIP"}</span><strong>{Math.round(progress().percent)}%</strong></div>
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
                      <span>{formatBytes(complete().file_size_bytes)} · {complete().filename}.mp4</span>
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
                  {exporting() ? "EXPORTING..." : exportError() ? "TRY AGAIN" : "EXPORT MP4"}
                  <span aria-hidden="true">→</span>
                </button>
              </section>
            </aside>
          </div>
        </Match>
      </Switch>
      <audio ref={audioPreview} onEnded={stopMusicPreview} />
    </section>
  );
}

export default ClipExporterScreen;
