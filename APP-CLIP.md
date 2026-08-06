# APP-CLIP.md — Process 2: Clip Exporter

> **Context:** The Viewer owns clip selection and passes a finished time range to this
> screen. This screen owns framing, audio, encoding, and the exported files. See
> `APP-VIEWER.md` §11 for endpoint behaviour and `SPEC.md` §4 for storage paths.

## 1. Entry Contract

```ts
{
  gameTimestamp: string,
  clipStartMs: number,
  clipEndMs: number,
}
```

The range is measured against `video.mp4`. It is already clamped to the recording and
is at least five seconds long. Returning to the Viewer restores the same range.

## 2. Product Goal

Every export is a normal MP4 intended to leave the app and be uploaded directly. The
source recording may be H.264 or HEVC; exported clips are always:

- H.264 High Profile, 4:2:0 progressive video
- AAC stereo audio at 48 kHz
- MP4 with its `moov` atom moved to the front (`+faststart`)
- clean footage only — no League Replay controls or event markers are burned in

Video stream-copy is not used. A source HEVC recording must still produce the same
publishable H.264 output as a source H.264 recording.

## 3. Export Targets

The format selector has three deliberately small presets:

| Preset | Output | Purpose |
|---|---|---|
| Discord | 16:9, adaptive resolution up to 1280×720 | Always less than 10,000,000 bytes |
| Horizontal | 16:9, source resolution capped at 1920 px wide | Regular YouTube and other landscape posts |
| Vertical | 1080×1920 (9:16) | TikTok and YouTube Shorts |

All presets preserve the recording frame rate up to 60 fps when the bitrate budget
allows it. Discord progressively falls back to 30 fps and then 540p/480p for longer
clips rather than exceeding its size ceiling.

### 3.1 Vertical Composition

A fixed centre crop loses too much of a League match, while a complete 16:9 frame is
too small on a phone. The default is therefore a hybrid composition:

1. A full-frame duplicate fills the 9:16 canvas and is strongly blurred and darkened.
2. A sharp foreground keeps the original height and uses an adjustable horizontal
   crop.
3. **Framing** moves continuously from full 16:9 context to a square action crop.
4. **Focus** (or dragging the foreground) positions that crop from left to right.
5. Platform-safe guides appear in the preview only and are never exported.

The default framing favours the central action without committing to a destructive
full-height 9:16 crop. No AI tracking, HUD extraction, captions, or multi-layer editor
is part of this phase.

## 4. Preview

The exporter seeks an off-screen `<video>` to `clipStartMs`, captures one JPEG frame
through Canvas, and then releases the video element. The preview is static; the app
does not decode video continuously on this screen.

- Horizontal and Discord show the frame in their output aspect ratio.
- Vertical renders the exact hybrid composition using the captured frame.
- Duration, start, and end remain visible beside the preview.
- If frame capture fails, export remains available.

## 5. Music and Audio Mix

Three mutually exclusive choices:

- **No music** — original game audio at 100%.
- **Built-in** — one of the bundled, publishing-cleared tracks.
- **Import file** — an absolute path selected with the native picker; MP3 and WAV are
  accepted and the file is not copied.

Built-in tracks have a ten-second preview button. When music is active, Game Audio
defaults to 80% and Music to 100%. Both sliders map directly to multipliers from 0.0
to 1.0.

Music is looped with `-stream_loop -1`. Both sources are trimmed to the selected clip
duration and mixed with `amix=duration=first`, so a short music track never truncates
the clip.

Bundled tracks live under `app/src-tauri/resources/music/`. `music.json` is the
manifest and contains filename, display name, mood, duration, and licence provenance.

## 6. Encoding Pipeline

The frontend sends one validated request to a narrow Rust Tauri command. Rust invokes
ffmpeg directly without a shell and hides child-process windows on Windows.

Encoder order is intentionally small:

1. Reuse the recording hardware family for H.264 (`h264_nvenc`, `h264_amf`,
   `h264_qsv`, or `h264_videotoolbox`).
2. If that encoder cannot initialise, retry with `libx264` using a fast preset.

Horizontal and Vertical target 12 Mbps video plus 192 kbps audio. Discord calculates
its video bitrate from the selected duration after reserving audio and container
overhead. The completed file is measured; an oversized Discord result is re-encoded
once with a corrected bitrate and rejected rather than returned if it still reaches
10,000,000 bytes.

Until Phase 9 bundles ffmpeg, `ffmpeg` must be available on the system path.

## 7. Progress, Errors, and Atomic Output

ffmpeg runs with `-progress pipe:1`. The backend converts `out_time_us` into encoding
progress and sends compact progress messages to the exporter. Thumbnail generation is
a separate final stage.

Exports are written as temporary `.part.mp4` and `.part.jpg` files. Only after both
commands succeed are they renamed to:

```text
{output_path}/clips/{game_timestamp}_{clip_timestamp}.mp4
{output_path}/clips/{game_timestamp}_{clip_timestamp}.jpg
```

Failed temporary files are removed. A completed MP4 is never exposed without its
thumbnail sidecar.

On success the screen shows elapsed time, final size, and actions to open the clips
folder, copy the absolute MP4 path, or go to the Clips tab. On failure it shows the
last concise ffmpeg error and allows the same request to be retried.

## 8. Library Contract

The filename is the only source-game association. The Clips tab discovers the pair by
scanning `/clips/`, probes MP4 duration, and looks up `game_timestamp` for champion and
date. Clips are never auto-deleted. Manual deletion removes both the MP4 and JPEG.
