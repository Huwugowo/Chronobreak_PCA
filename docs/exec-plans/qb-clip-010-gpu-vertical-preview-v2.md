# QB-CLIP-010 — GPU vertical preview v2

## Purpose

Preserve the existing sharp foreground and blurred full-frame context with one
source video, while meeting the unchanged measured CPU/drop budget. Supersedes
qb-clip-010-single-source-vertical-preview.md; preserve that immutable plan.

## Relevant planning-time architecture

ClipExporterScreen owns the primary native video, 100ms coalesced seeks, looping,
source audio, optional music and export requests. verticalPreviewBackdrop.ts owns
one source-bound display surface, callback generation, pending-seek admission,
visibility suspension and cleanup. Preserve those ownership contracts.

The current 640x360 2D surface plus CSS blur/brightness/saturation is measurably
more costly than the preserved dual-video executable. Existing CSS cover geometry
uses an absolute backdrop with -26px inset under the sharp native video and safe
area. Keep this geometry, focus/framing, music and export behavior.

playback_policy.rs already grants anonymous CORS only to exact admitted frontend
origins (production http://tauri.localhost), with capability and exact Host checks.
The primary video currently omits crossOrigin and therefore taints texture input.
Set crossOrigin=anonymous before assigning src. No server, CSP, credential, token,
origin allowlist, media URL or Rust change is required.

## Scope and non-goals

Replace only the backdrop drawing implementation and its filter placement.
Retain one video, native transport/audio, current source identity/security and
export contracts. No WebCodecs, capture backend, pixel extraction, serialized
images, fallback second video or new polling clock. Do not alter performance
thresholds or reduce background animation rate to make the gate pass.

## Exploration findings

Five valid interleaved windows rejected the CSS-filtered 2D design: baseline
median .403508 cores; median paired increase .316595 vs .060526 allowed. Removing
CSS filters reduced a diagnostic to .3363 cores; no-copy with filters still cost
1.0007. Layer promotion's initial .3952 did not reproduce across complete pairs.
In-context filtering, wrapper filters and backdrop-filter also exceeded budget.
These findings motivate replacing that path; they do not prove GPU internals or
that the replacement will be faster. Actual protected-source GPU upload and
performance must pass before completion.

## Chosen design and rationale

Keep the source-bound drawing owner and its existing public interface. Use a
WebGL canvas with alpha disabled, depth/stencil/antialias disabled and no retained
pixel buffer. Compile a full-screen triangle vertex shader and two-pass separable
Gaussian fragment shader. Allocate one source texture, one intermediate texture
and framebuffer, one bounded vertex buffer and shader/program resources per
binding. Intermediate/output backing dimensions preserve source aspect ratio,
cap at source dimensions and 640 on longest side; resize only on source binding.
Upload the primary decoded video once per admitted RVFC using texImage2D or
texSubImage2D with RGBA/UNSIGNED_BYTE and linear clamp-to-edge sampling. The first
pass downsamples and blurs horizontally into the intermediate framebuffer; the
second blurs vertically and applies brightness .48 and saturation .82 into the
visible canvas. Use normalized Gaussian weights and a fixed bounded tap count
(at least 13 symmetric samples spanning approximately three sigma); no per-frame
JS arrays, shaders, textures, readbacks or unbounded buffering.

Remove CSS filter, will-change and layer-promotion workaround from backdrop.
Retain object-fit:cover and existing overscan. Match 17 CSS-pixel isotropic blur:
observe the displayed backdrop box with one ResizeObserver, derive cover scale
max(cssWidth/backingWidth,cssHeight/backingHeight), and convert sigma to backing
pixels. Update uniforms on box/source changes and refresh a settled paused frame.
Observe valid dimensions before admitting draw; zero/hidden boxes do not invent
geometry. Color math uses CSS saturation luminance weights .2126/.7152/.0722;
brightness precedes saturation as in the old filter chain.

Declare crossorigin="anonymous" on the primary video as a static media attribute
before its reactive src binding selects or replaces the protected URL, for all
presets, without remounting on preset switches. Do not rely on ref callback order
or change crossOrigin after a media load. Integration must instrument source
assignment and prove anonymous mode is already present on initial and replacement
loads; if JSX generation cannot establish this ordering, explicitly own source
assignment after setting anonymous mode rather than retaining competing bindings. Texture upload
must work against the existing protected URL; never log URLs/capabilities. Preserve
all paused-seek/metadata/pending native seek gates, source generation checks,
one pending RVFC, pause/hidden suspension and owner disposal. No GL work continues
outside vertical or after local failure. Loading/unload clears old pixels.

Fail locally on unavailable WebGL, shader/link/framebuffer/upload failure or
context loss; stop scheduling, disconnect ResizeObserver, release all owned GPU
resources and show the existing backdrop error. Do not preventDefault context
loss or auto-retry; explicit preset re-entry/new binding creates a fresh owner.
Cleanup is idempotent; late frame/resize/context callbacks cannot mutate a new
binding. Check GPU setup/upload errors without synchronous per-frame readbacks;
use thrown upload failures/context loss and bounded setup validation rather than
polling getError every frame. Foreground playback and export remain available.

## Rejected alternatives and planning decisions

Reject the measured 2D/CSS filter path and its layer-promotion workaround.
In-canvas and sibling/wrapper filters have not met the resource goal. Keeping the
old dual-video view would violate this feature's single-source criterion.
Do not weaken CORS or bypass media admission to enable texture upload; existing
exact-origin anonymous policy is sufficient. No GPU/CPU saving is assumed before
actual matched acceptance. Shader quality requires exposed-backdrop screenshot
comparison, not just successful compile or foreground motion.

## Milestones

1. Implement bounded GPU renderer behind existing owner; set anonymous media mode
   before source and remove CSS filtering. Focused tests cover GPU lifecycle,
   source/seek/visibility scheduling and integration.
2. Build production app; prove protected-source texture upload and non-black
   moving backdrop, old/new geometry/blur, resize and all editor interactions.
3. Complete unchanged interleaved resource gate, frontend/canonical checks and
   durable evidence. If GPU design fails, retain failure and keep feature open.

## Verification design

Use existing changing synthetic 30s 1080p60 H.264/AAC fixture (1800 distinct
frames), separate sentinel editor library under build/perf/qb-clip-010, preserved
old executable/dist, and staged media runtime. Never use real recordings.
Retain source SHA256 5e624be7d4d59211b6a229c9c7d130e59df4e8de6b133c6986be6dd4aa431589.
Preserve failed trial roots by renaming within the owned build directory before
new runs; do not overwrite them. Use tools/vertical_preview/run-pairs.ps1,
trial.mjs, cpu.ps1, cpu-worker.ps1 and summarize.py. Administrator event collection
was user-authorized; launch hidden UAC worker if necessary. Keep diagnostics
identical between arms and no competing build/tests/capture during measurement.

Five interleaved A/B pairs, one production app at a time, same 1440x900 visible
window, same 0–25s clip, 5s warmup followed by 10s measurement near seconds5–15.
Require stable process tree keyed PID+creation time, start/stop event observation,
250ms CPU samples with no gap over500ms, no missing/regressing counters, no
seek/pause/loop/hidden state and positive foreground decoded/total deltas. Any
invalid arm invalidates its pair; retain and rerun complete pairs without cherry
picking. Integrate process-tree CPU seconds/wall seconds as logical cores. Compare
median of five paired B−A differences to max(15% of median A, .05 cores).
Foreground drop ratio is delta dropped/delta total; increase must be <=1pp.

Unit tests mock GL to verify shader/setup failure, framebuffer completeness,
texture upload/draw passes, bounded allocations, cleanup/context loss and failed
anonymous source. Existing owner tests still cover <=1 callback, metadata→seek,
rapid pending seeks with intermediate events, no-op/failed native seeks, pause,
hidden/resume, source unload/rebind, stale callbacks and disposal. Integration
asserts one source video in all presets and anonymous mode established before src;
music remains independent. Mocks cannot establish actual GPU/CORS correctness.

In actual production WebView inspect GL error/local alert behavior and protected
upload, and old/new settled 0/1/4s screenshots. Exposed background-only regions
above/below foreground must be non-black and visibly change while crop, 17px blur,
brightness/saturation and safe-area geometry remain comparable. Test both1440x900
and1000x700, resize paused/playing, framing/focus extremes, rapid seeks, loop,
restart, preset switch while playing and return to replay. Test source mute/volume
and built-in music playback/properties, encoded H.264/AAC and full decode for
synthetic exports, and unchanged source hash. No readPixels/getImageData/export
of canvas or security relaxation. Retain raw under ignored build and sanitized
results in docs/performance/vertical-preview-decoding.md.

Run npm.cmd run test --prefix app, npm.cmd run check --prefix app,
npm.cmd run build --prefix app and npm.cmd run desktop:build --prefix app.
Run canonical validator from docs/development/VERIFICATION.md and git diff --check.
No Rust change is designed; investigate scope before any Rust implementation.

## Performance and reliability gates

Exactly one source video and visibly current blurred context; sharp foreground,
transport/audio/export and protected delivery unchanged. At most one pending RVFC;
no continuous draws paused/hidden/outside vertical/disposed. Finite GPU resources
and one observer, complete cleanup on every terminal path, no old-source frame.
Local renderer failure must not add a video or disable export.

CPU median paired increase <=max(15% baseline,.05 logical cores); foreground
dropped-frame increase <=1pp. All five complete pairs valid. No weakened gate,
static source substitution, frame-rate reduction or unsupported general GPU/FPS
claim. All feature acceptance criteria and required verification must pass before
done; unavailable/failed evidence stays non-done.
