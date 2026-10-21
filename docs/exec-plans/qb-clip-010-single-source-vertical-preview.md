# QB-CLIP-010 — Single-source vertical clip preview

## Purpose

Keep the sharp adjustable action view and blurred full-frame context while
decoding the source video once. Preserve the existing clip editor, audio and
export behavior. QB-CLIP-009 establishes duplicate per-element decoded work; it
does not establish hardware decoder allocation or an application CPU/GPU saving.

## Relevant planning-time architecture

`app/src/components/ClipExporterScreen.tsx` owns the editor's primary video,
preview readiness/error state, play/pause, 100-ms coalesced seeking, clip looping
and optional music. It is separate from the replay playback controller. The
primary video's native time remains the preview authority.

The vertical preset currently adds a second muted video with the same protected
loopback URL. `syncSecondaryMedia` seeks that backdrop when forced or when drift
exceeds 80 ms, and independently synchronizes optional music. `resumeSecondaryMedia`
starts both. The existing preset effect pauses the backdrop outside vertical;
cleanup pauses media and clears sources. Preserve music behavior while removing
only the backdrop media paths.

`ClipExporterScreen.module.css` gives `.verticalBackdrop` absolute cover sizing,
blur(17px), brightness(.48) and saturation(.82). `.verticalCanvas` is currently a
div, not a drawing surface. The foreground remains the original native video
with crop/focus through CSS object-position. Framing changes its aspect ratio;
the backdrop is full-frame context, independent of focus.

The descriptor/probe owns the source URL and frame range. Loopback video may taint
a canvas: drawing for display is allowed, but this design never reads its pixels,
serializes it or changes crossOrigin/CSP/server security. No export request,
FFmpeg, replay controller, persisted format or media admission change is needed.

## Scope and non-goals

Replace only the secondary preview video with a canvas fed by the primary video.
Add one small tested drawing owner, integrated with exporter lifecycle. Preserve
foreground layout, background filters, safe area, focus, framing, source audio,
music and all export controls. No new capture backend, shared decoder service,
VideoFrame/WebCodecs pipeline, runtime polling or global playback refactor.

## Exploration findings

The production static synthetic 1080p60 fixture measured about 180 decoded frames
per three seconds horizontally and 366 across the two vertical elements. Both
present frames. Pause and seeks work, and the detached backdrop is already paused
on preset exit. There is no demonstrated lifecycle leak to repair.

Static title-card footage does not prove a new canvas advances with the scene.
Use a changing synthetic fixture for implementation acceptance. Canvas draws add
copy/composition work, so removing a video is insufficient performance evidence.
Existing exporter tests have only simple pause/load mocks; add meaningful owned
frame scheduling and media-event mocks rather than asserting JSX structure alone.

## Chosen design and rationale

Introduce `app/src/verticalPreviewBackdrop.ts` with one explicit owner bound to a
specific primary HTMLVideoElement and canvas. Its interface supports refresh of
the current decoded frame, playback scheduling, suspension and idempotent disposal;
it reports a draw/context failure through a local callback. The exporter creates
it only for the mounted vertical canvas, after refs exist, and destroys it before
changing source or leaving vertical mode. A Solid child/effect cleanup must own
this boundary; avoid leaving a stale ref as the owner.

Allocate one 2D context and a fixed-size backing surface per source binding.
Preserve source aspect ratio, cap backing dimensions at the source dimensions
and 640 pixels on the longest side. Keep existing CSS cover/blur/brightness on
the canvas. Its CSS box retains the old backdrop geometry. Reuse the context and
backing store; do not resize or allocate images on every frame.

Use primary `requestVideoFrameCallback` for continuous drawing. At most one
callback is pending. Each callback verifies binding generation and liveness,
draws the currently decoded primary frame and reschedules only during active
visible-document playback in vertical mode. The supported WebView2 has RVFC; no
RAF decoding substitute is introduced. Drawing must not call play, seek, load,
mutate primary time, or become an audio authority.

Initial metadata alone is insufficient for drawing. Draw when HAVE_CURRENT_DATA
and valid video dimensions are available, the primary is not seeking, and there
is no newer pending coalesced seek. The exporter supplies a seek-pending boundary
to the drawing owner: requested/dispatched seeks invalidate pending draws until
the final target settles. This includes metadata immediately followed by the
existing initial clip-start seek. Loadeddata/canplay/pause must not publish the
pre-seek frame merely because HAVE_CURRENT_DATA remains true. Clear the backdrop
on loadstart/emptied and before rebinding, even when the primary element is reused.
Draw on entering vertical while paused. Pause cancels continuous
scheduling and refreshes the settled frame; a subsequent paused seek redraws from
seeked. Loop/restart uses existing primary seeks and those same events. Hidden
documents suspend scheduling; visibility restoration redraws and resumes only if
the same primary is playing. No timer is needed.

Source replacement, preset exit and component cleanup invalidate the generation,
cancel the pending callback, remove all owner event listeners and clear references.
An already-queued stale callback cannot draw or schedule. Source unload must not
leave the previous frame presented as the new source. Explicit draw/context failure
releases the owner and shows a local backdrop-preview error; retain the foreground
and export controls, with no second-video fallback. Re-entering the preset or
binding a new source can create a fresh owner; do not add automatic retries.

Replace the vertical `<video>` with aria-hidden `<canvas>`. Remove backdrop refs,
its forced/drift seeks, play/pause/source cleanup and metadata hooks. Keep the
existing optional music synchronization, primary seek coalescing and clip loop.

## Rejected alternatives and planning decisions

- Removing the visual background loses useful composition context.
- A CSS background cannot reuse a native video's decoded changing frame.
- Retaining two sources and tweaking preload/synchronization retains duplicate
  playback work. Both layers are visible, so visibility suppression is unsuitable.
- Full-resolution drawing spends unnecessary copy bandwidth on a heavily blurred
  small preview. The 640-pixel cap requires visual verification; foreground stays
  native and sharp. Changing that cap for measured quality reasons is a local
  implementation decision, recorded in the checkpoint with its evidence.
- Do not infer hardware decoder ownership from player-scoped Media properties.

## Milestones

1. Preserve the current executable as an ignored baseline; prepare a changing
   synthetic bundle in a separate owned fixture library. Run the baseline trials
   with the measurement protocol below, before editing product code.
2. Implement/test the bounded drawing owner and integrate the canvas. Remove
   secondary-video operations without altering music or primary transport.
3. Run focused then full frontend verification; build fresh production Tauri and
   run matched visual/media/resource trials and editor interactions. Update durable
   architecture, execution checkpoint and canonical evidence only after gates pass.

## Verification design

Generate non-League moving 1080p60 H.264/AAC with the existing sentinel-owned
`tools/native_backend/run_native_fixture.ps1 -Scenario steady -DurationSeconds 30`
(requires staged runtime, Windows WGC and NVENC). Retain its EVIDENCE root and full
decode/frame-hash validation. Wrap the explicit video in a coherent isolated bundle
using `python tools/replay_benchmark/build_corpus.py --spec <owned-schema-v2-spec>
--media-runtime-root build/media-runtime/windows-x86_64`. Follow the shape of
`tools/replay_benchmark/schemas/corpus-v2.example.json`, with unique media/game IDs,
actual duration/rate and copy transform. Do not replace video underneath unrelated
metadata or overwrite the checked-in preprod fixture. Use a separate sentinel-owned
app config, app-data, WebView profile and zero-retention library; hash sources before
and after. If native generation is unavailable, record the blocker rather than
substitute static footage for the moving-frame gate.

The corpus spec must use an existing sentinel directory with exact leaf
`.chronobreak-replay-time`; its fresh output_root leaf must equal corpus_id. Use
the exact video.mp4 path under the generator's printed EVIDENCE root, a new UUID
and decimal game timestamp, expected H.264/AAC and 60/1, transform copy, and minimum
duration 20 seconds (less than the probed 30-second fixture). Preserve the spec
and builder validation output. An AAC stream proves structure rather than audible
content: use the existing built-in music for audible mix verification and retain
source-audio property checks separately.

Retain the old production executable and baseline frontend before rebuilding.
Use the same generated bundle and runtime for interleaved A/B/A/B/A/B/A/B/A/B trials,
one app at a time, same window size and visible foreground, no capture running.
Use the same 0–25-second clip range in both editors. Warm up five seconds before
a ten-second measurement, without any loop/seek in that timed window; both arms
must cover the same source interval (approximately seconds 5–15). Loop/restart
correctness is exercised separately. Extend `tools/vertical_preview/measure.mjs` to record labels,
RVFC/presentation/decoded/drop deltas, source identity, visibility, canvas drawing
evidence and owned process-tree CPU samples. It currently requires build/preprod;
parameterize the expected isolated output root explicitly rather than weakening
the safety check. Keep diagnostics bounded and identical between arms.

Sample the owned app and descendants every 250 ms with Windows process CPU counters,
keyed by PID plus creation time. Subscribe to process start/stop events before
warmup (Win32_ProcessStartTrace/StopTrace) and retain those events through measurement.
Require a stable owned tree throughout the timed window; any start/exit, missed
sample, counter regression, identity mismatch, event-observer failure or >500-ms
sample gap invalidates that entire pair. Account every initial/final member; do
not silently drop exited children or substitute zero for unavailable CPU.
Integrate CPU seconds across the ten-second window and report logical-core
equivalents (CPU seconds / wall seconds). For each complete pair compute B minus A;
compare the median of five paired differences to the permitted increase calculated
from the median of the five A values. Keep identical diagnostic collection per arm.
Require visible document, continuous playing state, no seek/loop, positive decoded
and total frame deltas and complete counters in every arm; a zero/missing counter
invalidates the pair rather than proving efficiency. Dropped proportion is dropped
delta divided by playback-quality totalVideoFrames delta, foreground only.

Compare old/new screenshots at the same settled timestamps, identifying exposed
backdrop-only regions above/below the foreground. Those regions must change with
the moving source while retaining baseline crop/blur geometry; foreground changes
alone are insufficient. Verify non-black rendering from the protected URL in the
actual WebView: successful drawImage without an exception alone is insufficient.
Do not read canvas pixels across the protected origin. Inspect baseline/new paused
zero, one/four-second seeks, restart/loop, framing/focus extremes and switching
presets while playing. Check both 1440×900 and 1000×700, source audio/mute/mix,
unchanged export options and return to replay.

Unit tests cover pending-callback bound, correct decoded-frame draws, initial
readiness, paused seeking, play/pause, hidden/resume, resize only on binding,
source replacement/loadstart/emptied on a reused element, late callbacks, disposal
and context/draw failure. Specifically test metadata→initial seek and rapid queued
seeks with intermediate loadeddata/canplay, accepting only the final settled frame. Exporter
integration covers one video in all presets, initial/paused/loop events and music
independence. Mocked counters alone cannot satisfy production evidence.

Run from root:

```powershell
npm.cmd run test --prefix app
npm.cmd run check --prefix app
npm.cmd run build --prefix app
npm.cmd run desktop:build --prefix app
```

Run canonical validation from `docs/development/VERIFICATION.md`, `git diff --check`
and applicable fixture/tool tests. No Rust changes are designed; if implementation
requires them, resolve that scope before proceeding. Retain sanitized results in
`docs/performance/vertical-preview-decoding.md`, raw outputs under ignored build.

## Performance and reliability gates

- Exactly one source video in vertical; no backdrop media load/play/seek remains.
  Its decoded-frame delta stays comparable to baseline foreground, while the
  backdrop visibly advances from those same frames and paused seeks are correct.
- No stale frame from a previous source, missing initial paused backdrop, new media
  errors, stuck transport/music, leaked callback/listener or foreground crop change.
- No more than one pending frame callback; zero continuous draws when paused,
  hidden, outside vertical or disposed. Event-driven paused refresh is allowed.
- Median paired total process-tree CPU must not increase by more than the greater
  of 15% of baseline or 0.05 logical cores. Foreground dropped-frame proportion must
  not increase by more than one percentage point. Invalid/incomplete trials cannot
  pass. If variability prevents attribution, retain evidence and rerun complete
  pairs; do not cherry-pick repetitions or loosen the thresholds.
- Source hashes remain unchanged. No live recordings or user libraries are test
  data. Claim only the demonstrated decoder-work reduction and measured resource
  behavior, never general GPU savings or gameplay FPS.
