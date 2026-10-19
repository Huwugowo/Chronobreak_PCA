# League recording data — 2026-10-19

This dedicated data-only branch archives both completed recordings from the live test session. Product source code and local working-tree changes are not included.

Recorder source commit: `42a14671da790156d486de8c26c65908edecc593`.
Media runtime: `queueback-ffmpeg-8.1.2-windows-x86_64-r6`.
Capture configuration: native WGC/D3D11/NVENC, H.264 High, 1920x1080 at 60 FPS, silent stereo audio fallback.

| Recording directory | Published media duration | Post-game result included |
| --- | ---: | --- |
| `1792431584` | 1006.635 s | Yes |
| `1792432630` | 131.925 s | Yes |

Each `games/<directory>/` retains the original `metadata.json`, `game_log.json` manifest, `league_observations.jsonl` journal, `league_checkpoint.json`, `league_match.json`, and `league_result.json`. The sidecars are copied byte-for-byte. Player identities and match statistics remain part of the recorded League data.

`logs/` contains snapshots of recorder diagnostics and the live recorder stderr log, including capture finalization and post-game collection. The recorder can continue running locally; these logs are snapshots at the export time recorded in `manifest.json`.

Both videos are intentionally omitted (1,729,256,450 bytes in total). No video was deleted or changed locally. This archive cannot play back video on its own; the original `video.mp4` files remain in the local recording library.

`manifest.json` records payload paths, sizes, SHA-256 hashes, journal record counts, recording identifiers, and excluded video sizes. All JSON and JSONL payloads were parsed successfully during packaging, and copied recording sidecars were checked against their originals. No schema acceptance, playback, or performance claim is made by this archive.
