# QueueBack progress

Current canonical work state lives in `feature-list.json`.

`QB-CAP-002` is not complete. Its experimental recording-state authority and strict client classification were rolled back on 2026-08-12 after they made a valid Practice Tool bundle appear Unknown/non-playable. It is no longer a dependency of the performance work.

Most recent completed prerequisite:

- `QB-DIST-001`: completed at `docs/exec-plans/completed/qb-dist-001-packaged-media-tool-contract.md`. The exact QueueBack FFmpeg/ffprobe source build, shared resolver, packaged release layout, sanitized-PATH smoke, and real existing-driver NVENC diagnostic passed.

Active performance work:

- `QB-PERF-002`: canonical reconciliation is active at `docs/exec-plans/active/qb-perf-002-gpu-agnostic-windows-capture.md`. Native WGC/D3D11/NVENC is the Windows default and external FFmpeg WGC is an explicit alternative, never an automatic fallback. The packaged r6/release gates, matched 240-second A/B, native steady/resize/minimize/occlusion/target-close/NVENC-failure matrix, and product-owner-accepted 30-minute native run are recorded. The valid post-change League matrix remains unavailable and mandatory before completion or a negligible-impact claim.

`QB-PERF-003` and `QB-PERF-004` retain the physical AMD/AMF and Intel/QSV validation work after the GPU-agnostic implementation exists. The current RTX 4060 can validate only the common capture layer plus NVENC.
