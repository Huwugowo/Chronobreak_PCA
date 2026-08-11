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

The format selector has three deliberately small presets. One or more formats can be
selected in the same export; a separate **Preview** action chooses which selected format
is shown in the live preview.

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

The exporter uses the source `<video>` directly and restricts playback to the selected
clip range. It does not capture a Canvas/JPEG frame.

- Horizontal and Discord show the live source in their output aspect ratio.
- Vertical uses a synchronized muted background video plus the foreground video to
  reproduce the exported hybrid composition without lowering source quality.
- The preview loops the selected range and includes a bounded scrubber.
- Rapid scrub input is coalesced to at most ten native seeks per second.
- Duration, start, end, and the current preview offset remain visible.
- If preview playback fails, export remains available.

## 5. Music and Audio Mix

Three mutually exclusive choices:

- **No music** — original game audio at 100%.
- **Built-in** — one of the bundled, publishing-cleared tracks.
- **Import file** — an absolute path selected with the native picker; MP3 and WAV are
  accepted and the file is not copied.

Built-in and imported tracks play against the live clip preview, starting at music time
zero when the clip begins and looping with the same timing used by export. Game audio
defaults to 80% and Music to 100%. Both sliders apply live and map directly to export
multipliers from 0.0 to 1.0. Imported files are exposed only through a short-lived opaque
localhost token; arbitrary filesystem paths are never accepted by the media server.

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
2. If that encoder cannot initialise, retry with `libx264` (`medium` for publish
   masters, `veryfast` for the size-constrained Discord preset).

Horizontal and Vertical use a high-quality 24 Mbps average / 36 Mbps peak video budget
plus 192 kbps audio. NVENC uses the P6 high-quality VBR/multipass path with adaptive
quantization and 32-frame look-ahead; equivalent slower quality presets are used for
AMF, QSV, and the software fallback. This materially reduces
second-generation H.264 damage in high-motion 1080p60 footage, but cannot restore detail
already absent from the source recording. Discord calculates its video bitrate from the
selected duration after reserving audio and container overhead. The completed file is
measured; an oversized Discord result is re-encoded once with a corrected bitrate and
rejected rather than returned if it still reaches 10,000,000 bytes.

Multiple selected formats are submitted as one batch and encoded sequentially. Every
format needs its own H.264 encode because its dimensions and bitrate constraints differ;
sequential execution avoids competing hardware-encoder sessions and keeps fallback/error
handling deterministic.

Until Phase 8 bundles the media tools, `ffmpeg` and `ffprobe` must be available on the system path.

## 7. Progress, Errors, and Atomic Output

ffmpeg runs with `-progress pipe:1`. The backend converts `out_time_us` into encoding
progress and sends compact progress messages to the exporter. Thumbnail generation is
a separate final stage.

Exports are written as temporary `.part.mp4` and `.part.jpg` files. For a multi-format
batch, all selected outputs and thumbnails must succeed before any are published. Only
then are they renamed to:

```text
{output_path}/clips/{game_timestamp}_{clip_timestamp}.mp4
{output_path}/clips/{game_timestamp}_{clip_timestamp}.jpg
```

Failed temporary files are removed. A completed MP4 is never exposed without its
thumbnail sidecar.

On success the screen lists every format, filename, individual size, total elapsed time,
and total size, with actions to open the clips folder, copy all absolute MP4 paths, or go
to the Clips tab. On failure it shows the last concise ffmpeg error and allows the same
batch to be retried.

## 8. Library Contract

The filename is the only source-game association. The Clips tab discovers the pair by
scanning `/clips/`, probes MP4 duration, and looks up `game_timestamp` for champion and
date. Clips are never auto-deleted. Manual deletion removes both the MP4 and JPEG.
