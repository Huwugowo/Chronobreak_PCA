# Chronobreak architecture

These documents record durable boundaries and invariants discovered from the repository. They describe the implementation; canonical work status remains in `feature-list.json`.

- `overview.md` — process boundaries, ownership, and shared contracts.
- `recorder-lifecycle.md` — League process watching, ffmpeg capture, Live Client synchronization, and failure behavior.
- `media-library.md` — filesystem game/clip bundles, atomic metadata, safety, and re-indexability.
- `desktop-replay.md` — Tauri commands, loopback playback, Data Dragon, viewer state, and clip export.

- `replay-time.md` - schema-v2 replay coordinates, validation, browser presentation state, and frame-addressed clips.

Detailed UI and module behavior remains in `RECORDER.md`, `APP-LIBRARY.md`, `APP-VIEWER.md`, and `APP-CLIP.md`. Those specifications do not replace the canonical roadmap.
