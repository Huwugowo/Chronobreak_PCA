# QB-CAP-009 — Process-owned recording

Revised by explicit user instruction on 2026-11-01; supersedes the earlier oversized planning/verification requirements in this file. Proceed directly through implementation, focused checks and commit.

## Design

The original exact process owns the recording. Retain the existing limited-rights Windows `OwnedHandle` in `Arc<OriginalProcess>` through startup, capture and stop. No unsafe Send/Sync implementation or extra access rights.

Keep one GPU worker, D3D/MTA context, converter, NVENC encoder, mux and CFR clock. Make only the WGC leaf optional/replaceable. Detach after strict callback quiescence, retaining the last GPU image. Reattach only an HWND belonging to the still-live original process on the same adapter. Reuse existing resize handling for its first fresh frame. Keep bounded two-tick catch-up and at most one attachment attempt per two seconds.

Recoverable absence is a typed window-gone/ownership-changed result or WGC Closed. Window replacement does not stop output. Other WGC/COM/driver, callback, encoder, mux and validation failures are terminal; no broad catch/retry. Unproven callback cleanup retains the guarded leaf/device/MTA together and disables subsequent capture until recorder relaunch.

There is no arbitrary lost-source timeout. Healthy last-image CFR continues until original process exit or intentional user stop. Source freshness alone pauses while unavailable/minimized; encoder/mux watchdogs and actual storage/write errors remain enforced. Lost-source recording can grow on disk until stopped, like any process-owned recording; discovery cadence and in-memory resources stay bounded. Waiting for a source does not invalidate an otherwise healthy recording. A real failure preserves partials and prevents publication; later process exit cannot erase it.

Aggregate all retired/current leaf counters, preserve first error and exact frame accounting. Keep existing strict final media/publication validation. Do not infer complete gameplay coverage from process exit or a valid MP4. Internal tray status reports waiting/resumed; observer protocol stays compatible.

## Focused verification

Use targeted Rust check/test/Clippy. Extend existing generated fixtures only enough to exercise window-first exit, same-process recreation with decoded fresh frames, minimize/restore and process identity boundaries. Reuse callback quiescence and encoder/mux failure tests to prove false publication is rejected. Short real-service hardware runs are useful where available. Report unverified League behavior; long soaks, elaborate evidence schemas and formal gameplay/performance gates are not prerequisites for committing this fix.

Startup admission, audio capture, library membership/refresh and general device recovery remain separate features. Preserve unrelated checkout changes and user recordings.
