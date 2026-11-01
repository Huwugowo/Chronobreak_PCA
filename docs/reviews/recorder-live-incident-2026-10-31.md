# Live recorder startup delay and target-close finalization error — 2026-10-31

The recorder automatically began capturing after passive readiness admitted the
game, then preserved a partial recording when the selected League window closed
before a single post-teardown check confirmed the original process had exited.
This report retains investigation evidence; it does not implement a fix, certify
normal match completion, or mark a feature done.

## Scope and provenance

- Related features: QB-CAP-006 (startup readiness), QB-CAP-002 (stop/publication
  safeguards), with QB-CAP-001 and QB-DIST-002 providing lifecycle context.
- Source inspected: `a6a3068f23a2386a385c1eae3f1ab04a8d6ab36e`.
- Recorder: `recorder/target/release/recorder.exe`, PID 6488, Windows session 7,
  ordinary tray mode, started at 22:57:07 Europe/Paris. Its executable hash and
  the evidence snapshot hashes are in the [manifest](evidence/recorder-2026-10-31/manifest.json).
  The executable identity is recorded without claiming reproducible-build equivalence.
- Local recording bundle: `<PROFILE>/LeagueReplays/games/1793484051`.
- The unrelated working-tree change in `app/src-tauri/Cargo.toml` is excluded.
- No media, original player journal, credentials, or user configuration is committed.
  The [recorder log](evidence/recorder-2026-10-31/recorder.log) replaces repository
  and user-profile path prefixes, retaining timing, process IDs, decisions,
  counters and errors. Original logs remain local under
  `build/recorder-launch-20261031/`.

## Observed timeline

Times below are Europe/Paris (UTC+01:00). The attached log uses UTC (`Z`).

| Local time | Observation |
| --- | --- |
| 22:57:08 | Service initialized. No loopback device was found, so audio uses the logged silent stereo fallback. |
| 22:59:04 | League PID 524 appeared. Readiness subsequently alternated among unavailable signal/window, unstable clock and changed window. |
| 22:59:46 | A new League generation, PID 228 with creation-time identity 1793483985, appeared. The log does not establish why the earlier generation ended. |
| 22:59:48 | PID 228 readiness: `SignalUnavailable`. |
| 22:59:52 | Readiness: `ClockUnstable`. |
| 23:00:20 | Readiness: `WindowUnavailable`. |
| 23:00:47 | Readiness: `Collecting`, one sample, game clock 32.598454 s. |
| 23:00:51 | Readiness: `Ready`, three samples, game clock 36.624569 s. Only then were a bundle and native resources created. |
| 23:00:54 | `recording started`; source, encoded-frame and mux-byte progress advanced afterward. One early 2×2 WGC content-size warning was handled by retaining the valid source image. |
| 23:12:48 | Last ordinary progress record: 42,973 encoded frames and 1,086,811,921 mux bytes. |
| 23:12:55.527 | Native worker cleanup finished with `cleanup_ok=true`. |
| 23:12:56.152 | Selected League HWND closed; service checked process termination after teardown. |
| 23:12:56.167 | Terminal progress: 43,420 encoded frames and 1,098,648,666 mux bytes. Journal writer stopped with 1,799 committed records and no journal/checkpoint/manifest failure. |
| 23:12:56.168 | Error: `native capture target was lost without confirmed League process termination`; partial MP4 preserved. |
| 23:13:35, approximately | Read-only process inspection found recorder PID 6488 still running; League PID 228 and FFmpeg had exited. The actual League exit time was not captured. |

## Startup-delay diagnosis

`recorder/src/launch_readiness.rs:74-108` admits three valid, advancing GameStats
samples spanning at least four seconds. Game time must be at least five seconds;
map/mode, window identity and request-interval clock consistency must remain valid.
The loop samples every two seconds and revalidates the full descriptor before
admission (`:132-175`). No native startup occurred before `Ready` in this run.

`ClockUnstable` combines several causes: game time below five seconds, empty mode,
changed map/mode, non-advancing/regressing clock, or inconsistent clock offsets.
`WindowUnavailable` combines unsuccessful window discovery/usability with failed
original-process identity validation (`recorder/src/service.rs:1097`). Windows
discovery requires a visible, non-minimized window with valid dimensions, DPI,
monitor and PID (`recorder/src/platform/windows.rs:107-132`). These aggregate
decisions cannot identify whether the user minimized the window or which clock
condition failed. Only decision transitions are logged (`launch_readiness.rs:184-193`).

The delay was passive deferral, followed by successful native initialization;
the log does not show an encoder startup failure. Opening footage was missed.
This one live run does not satisfy QB-CAP-006's controlled repeated
absent/passive/native launch-comparison acceptance gate.

## Finalization-error diagnosis

`recorder/src/native/lifecycle.rs:401-436` joins the worker and checks successful
worker completion and terminal capture/mux evidence before calling the supplied
`process_ended()` callback once. `recorder/src/service.rs:1594` supplies the existing
watcher's original-generation check. `recorder/src/watcher.rs:125-133` refreshes
the full process snapshot and requires a known original creation time plus absence
of that original PID/creation-time generation.

This error therefore establishes that the process-end check did not confirm
termination at that instant. It does not establish an NVENC failure or a stalled
recording. The log reports zero watchdog stalls and zero backend exits in the
preceding telemetry; native cleanup succeeded. Later process absence is consistent
with an HWND-first normal shutdown, but does not establish its timing or distinguish
normal game exit from a crash. The recorded journal contains no `GameEnd` event;
that absence is not proof of a crash either.

The current documented contract conservatively preserves partial output when the
process outlives video teardown. There is no delayed recheck/publication for this
attempt. The target-loss branch consumes the active recording; later process
disappearance consequently emits no successful-recording `Idle` event to clear the
tray error (`recorder/src/service.rs:638-645,794-812`; `recorder/src/tray.rs:225-235`).
The service remains running, and a new League generation can attempt recording.

Existing focused tests cover target closure without process end, process end during
teardown, terminal-evidence rejection and worker-error precedence in
`recorder/src/native/lifecycle.rs`. They were inspected, not rerun in this incident.
The current limitation is also documented in
[`recorder-lifecycle.md`](../architecture/recorder-lifecycle.md).

## Preserved media and verification

The packaged FFprobe completed with exit code 0 and empty stderr. Its exact
arguments are in the manifest and its output is in
[media-probe.json](evidence/recorder-2026-10-31/media-probe.json):

- H.264 video, 1920×1080, 60 fps, 723.666667 seconds.
- AAC stream, 723.669333 seconds; startup explicitly selected silent audio.
- Container size: 1,098,648,666 bytes; duration: 723.669333 seconds (12m 03.669s).
- `video.partial.mp4` remains in place; canonical `video.mp4`/`metadata.json` were
  not published. This probe is not a full decode or proof of complete gameplay.

[Process observations](evidence/recorder-2026-10-31/process-observations.json) and
[event-name/tick summary](evidence/recorder-2026-10-31/events-summary.json) retain
the bounded non-destructive checks without player identities. No media repair,
rename, deletion, recovery, test-library mutation or product-code change was made.
No performance, formal capture-reliability or full feature-completion claim follows.

## Follow-up boundary

QB-CAP-002 owns the observed shutdown-classification limitation. Any correction
must distinguish genuine target loss from process termination while preserving
worker-error precedence and partial-output protection. Simply accepting HWND
closure would reintroduce the previously rejected truncated-publication path.
The exact normal-exit ordering remains unmeasured; no retry/grace policy is approved
by this report. QB-CAP-006 retains the startup observation and its diagnostic limits.

This evidence is intentionally frozen before the user-requested recorder restart.
The restart is operational follow-up, not a fix or a successful-recording claim.
