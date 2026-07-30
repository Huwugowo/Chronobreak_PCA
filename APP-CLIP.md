# APP-CLIP.md — Process 2: Clip Exporter Screen

> **Context:** This screen is reached from the Viewer after the user sets clip endpoints. For viewer clip mode activation (endpoint handles, smart auto-positioning, scrubber behaviour in clip mode), refer to `APP-VIEWER.md` Section 11. For data schemas and file paths, refer to `SPEC.md`.
>
> **What this screen does:** Lets the user preview a clip window, optionally mix in music, and export a clean H.264 MP4 file to disk. The exported clip appears in the Clips tab of the library (see `APP-LIBRARY.md` Section 4).

---

## 1. Entry Point

The viewer passes the following state when navigating to this screen:

```ts
{
  gameTimestamp: string,   // unix timestamp, identifies the game bundle
  clipStartMs:  number,    // milliseconds into video.mp4
  clipEndMs:    number,    // milliseconds into video.mp4
}
```

Clip duration = `clipEndMs - clipStartMs`. Minimum 5 seconds (enforced in viewer).

---

## 2. Screen Layout

```
┌──────────────────────────────────────────────────────────────┐
│  ← Back to Viewer                                            │
│                                                              │
│  ┌────────────────────────────────────┐                      │
│  │                                    │  Duration: 0:32      │
│  │       Clip preview / thumbnail     │  Start:   14:17      │
│  │         (first frame of clip)      │  End:     14:49      │
│  │                                    │                      │
│  └────────────────────────────────────┘                      │
│                                                              │
│  ──── MUSIC ──────────────────────────────────────────────   │
│  ○  No music                                                 │
│  ●  Built-in library     [ Track name          ▼ ]  [ ▶ ]   │
│  ○  Import file          [ Browse...               ]         │
│                                                              │
│  ──── AUDIO MIX ──────────────────────────────────────────   │
│  Game audio   [────●────────────] 80%                        │
│  Music        [──────────────●──] 100%                       │
│                                                              │
│  ──── EXPORT ─────────────────────────────────────────────   │
│  Output: ~/LeagueReplays/clips/1741267920_1741268352.mp4     │
│                                                              │
│  [ Export MP4 ]                              [ Cancel ]      │
│                                                              │
│  ─── after export ─────────────────────────────────────────  │
│  ✓ Exported (0:04)   [ Open in Finder ]   [ Copy path ]      │
└──────────────────────────────────────────────────────────────┘
```

---

## 3. Clip Preview

- Displays the first frame of the clip window as a static thumbnail
- Achieved by seeking an off-screen `<video>` element to `clipStartMs / 1000` seconds and capturing a canvas frame
- Not a live preview — no playback in this screen
- Shows duration, start time, end time (formatted as `MM:SS`) alongside the thumbnail

---

## 4. Music Options

Three mutually exclusive radio options:

### 4.1 No Music
Default selection. Export uses game audio only at the configured volume.

### 4.2 Built-in Library
- Dropdown of bundled royalty-free tracks
- ~10–15 tracks at launch, various moods
- All tracks pre-cleared for use in content shared on Twitter/Discord/YouTube
- `[ ▶ ]` button plays a 10-second preview of the selected track through the system audio output
- Preview stops when the user picks a different track or changes the music option

### 4.3 Import File
- Opens a native file picker (Tauri dialog) filtered to `.mp3` and `.wav`
- Selected file path is displayed truncated
- File is referenced by path only — it is not copied into the app

---

## 5. Audio Mix Sliders

Two range sliders, 0–100%, shown when a music option is selected (hidden when "No music" is selected):

- **Game audio** — controls volume of the original recording's audio track. Default: 80%.
- **Music** — controls volume of the selected music track. Default: 100%.

Both values are passed directly to ffmpeg as volume multipliers (0.0–1.0).

---

## 6. Export Pipeline

Export is triggered by the "Export MP4" button. The frontend calls a Tauri command which executes ffmpeg on the Rust backend.

### 6.1 Output Path

```
{output_path}/clips/{game_timestamp}_{clip_timestamp}.mp4
```

`clip_timestamp` = Unix timestamp at export time.

### 6.2 ffmpeg Command — With Music

```bash
ffmpeg \
  -ss {clip_start_seconds} \
  -i {game_bundle_path}/video.mp4 \
  -t {clip_duration_seconds} \
  -stream_loop -1 -i {music_file_path} \
  -filter_complex \
    "[0:a]volume={game_audio_vol}[a1]; \
     [1:a]volume={music_vol}[a2]; \
     [a1][a2]amix=inputs=2:duration=first[aout]" \
  -map 0:v \
  -map "[aout]" \
  -vcodec copy \
  -acodec aac -b:a 192k \
  -t {clip_duration_seconds} \
  {output_path}/clips/{clip_filename}.mp4
```

`-stream_loop -1` loops the music track infinitely. `amix=duration=first` cuts the mixed audio to the length of the first input (the game audio, which is exactly `clip_duration_seconds` long). The final `-t {clip_duration_seconds}` on the output is a safety guard ensuring the clip never exceeds the intended duration regardless of muxer behaviour.

### 6.3 ffmpeg Command — No Music

```bash
ffmpeg \
  -ss {clip_start_seconds} \
  -i {game_bundle_path}/video.mp4 \
  -t {clip_duration_seconds} \
  -filter_complex "[0:a]volume={game_audio_vol}[aout]" \
  -map 0:v \
  -map "[aout]" \
  -vcodec copy \
  -acodec aac -b:a 192k \
  {output_path}/clips/{clip_filename}.mp4
```

### 6.4 Thumbnail Sidecar

After the clip MP4 is written, extract the first frame as a JPEG sidecar:

```bash
ffmpeg \
  -ss 0 \
  -i {output_path}/clips/{clip_filename}.mp4 \
  -frames:v 1 \
  -q:v 3 \
  {output_path}/clips/{clip_filename}.jpg
```

This sidecar is used by the Clips tab in the library (see `APP-LIBRARY.md` §4). It is always generated immediately after export — the Clips tab should never need to generate thumbnails at render time.

### 6.5 Key ffmpeg Flags

| Flag | Reason |
|---|---|
| `-ss` before `-i` | Fast seek (input seeking) — much faster than output seeking for large files |
| `-vcodec copy` | Video is not re-encoded. Export is near-instant regardless of clip length. |
| `-stream_loop -1` | Loops music infinitely so it always fills the clip duration |
| `amix=duration=first` | Cuts mixed audio to game audio length — never shorter than the clip |
| `-t {clip_duration_seconds}` | Explicit output duration cap — safety guard |

### 6.6 No Stats Overlay

Exported clips contain **clean footage + audio only**. No stats, no event markers, no UI elements are burned into the video. The viewer overlay exists only in the app.

---

## 7. Export Progress & Completion

During export:
- "Export MP4" button is replaced with a progress indicator
- ffmpeg stdout/stderr is piped to the Rust backend and progress percentage is derived from the `time=` output
- Progress is sent to the frontend via Tauri event

On completion:
- Success state: green checkmark + "Exported (elapsed time)"
- Two action buttons appear: "Open in Finder / Explorer" (opens the output folder), "Copy file path" (copies absolute path to clipboard)
- The clip is **not** auto-opened or auto-shared — the user decides what to do with it

On error:
- Error state: red indicator + ffmpeg error message (truncated, last line)
- "Try again" button re-runs the export with the same parameters

---

## 8. Clip Storage

- Output written to `{output_path}/clips/` as two files: `{clip_filename}.mp4` and `{clip_filename}.jpg` (thumbnail sidecar)
- Clips have `saved: true` by default — they are **never auto-deleted**
- Clips are not linked back to a game bundle in any structured way — the filename convention (`{game_ts}_{clip_ts}`) is the only association
- Exported clips appear in the **Clips tab** of the app library (see `APP-LIBRARY.md` Section 4), not in the Games tab
- Clips and their sidecars are included in the storage usage breakdown shown in Settings
- Deleting a clip must delete both the `.mp4` and the `.jpg` sidecar

---

## 9. Built-in Music Library

- Royalty-free tracks bundled inside the Tauri app under `resources/music/`
- Format: MP3, 192kbps minimum
- Naming convention: `{mood}_{title}.mp3` (e.g. `hype_overdrive.mp3`, `chill_aftermath.mp3`)
- Metadata (display name, mood tag, duration) stored in a bundled `music.json` manifest
- The dropdown in the UI is populated from this manifest

```json
[
  {
    "filename": "hype_overdrive.mp3",
    "display_name": "Overdrive",
    "mood": "hype",
    "duration_s": 142
  }
]
```
