# QB-CLIP-010 execution checkpoint

Feature: `QB-CLIP-010`
ExecPlan: `docs/exec-plans/qb-clip-010-gpu-vertical-preview-v2.md`
Updated: 2026-10-21

## Current milestone

V2 GPU renderer implemented and functionally verified; final resource gate FAILED.

## Active unit

Implement the reviewed v2 shader renderer behind the existing source-bound owner.
GPU renderer, anonymous source binding and CSS replacement implemented and tested; CPU acceptance failed;
preserve unrelated UI/index/performance work.

## Completed

- Original single-source lifecycle/integration implemented and tested; primary
  seek admission, music independence and one-video ownership remain reusable.
- Changing synthetic 30s 1080p60 fixture, preserved old executable/dist and isolated
  editor library retained under build/perf/qb-clip-010. Source SHA256 unchanged.
- Full five-pair series rejected original 2D/CSS-filter design and layer promotion.
- Additional valid diagnostics rejected context filtering (1.5231 cores), wrapper
  filter (1.4349), backdrop-filter (1.0701), and 160/320px context filters
  (1.5266/1.1913). No proposed filter rearrangement met the goal.
- Targeted security scout confirmed existing exact-origin anonymous CORS supports
  WebGL video texture upload without Rust/server/CSP/token changes.
- V2 shader design independently reviewed. Required correction incorporated:
  static anonymous media mode before reactive source binding, with instrumented
  initial/replacement ordering tests. Original plan preserved unchanged.

## In flight

V2 only changes the backdrop rendering path; actual protected video texture
upload, visual match, context loss/resources and performance remain unverified.
Current CSS layer workaround is failed v1 state and must be removed by v2.
Administrator measurement authorization persists; worker queues have stop files.
A fresh owned queue/UAC worker may be needed for the new complete measurement.

## Remaining

1. Implement WebGL owner, anonymous source binding and remove CSS filters.
2. Focused GPU/lifecycle/integration tests; real protected-source visual proof.
3. Full frontend/build, five valid paired resource windows, export/source safety,
   durable architecture/evidence and canonical completion validation.

## Verification

- Final layer-promoted production build succeeded. Complete five interleaved
  pairs all valid: old cores [.4458,.3519,.4035,.3628,.6045]; new
  [.8366,.7729,.5731,.4632,.9210]. Baseline median .403508; median paired
  increase .316595 exceeds allowed .060526. CPU FAIL; all foreground dropped
  deltas zero, drop gate PASS. summarize.py intentionally exits 1 on failed gate.
  Raw build/perf/qb-clip-010/paired-summary.json and trials/a-1..b-5 retained.
- Layer promotion's initial .3952 diagnostic was not reproducible across the full
  series and is not an accepted fix. No product completion claim.

- Regression investigation: valid original pair .4543 cores old / 1.1493 new;
  new foreground 3/600 dropped. Full series stopped on later sample-gap failure.
- Controlled production diagnostics (not acceptance pairs): copy-off 1.0007 cores;
  CSS filters removed .3363; canvas capped320 .4958; explicit CSS layer promotion
  with all filters retained .3952. These windows were valid and had zero new drops.
  Blur-only/colour-only runs had sample gaps and are invalid; do not use them for
  numerical conclusions. Evidence under trials/diagnostic-*.
- Main attribution: filtered canvas compositing/layer selection, not simply the
  per-frame drawImage call. Layer promotion retains visual filters and source;
  internal GPU/readback implementation is not established by CPU counters.


- QB-CLIP-009: six actual production three-second trials; horizontal decoded
  deltas 180/180/181, vertical foreground 182/181/181 plus backdrop 185/184/185.
  Pause 1.2 seconds adds no frames; paused 4s/1s seeks exact; switching horizontal
  pauses detached video; return leaves one replay video. Source hash unchanged.
- Measurement tool Node syntax check passed. An earlier trial setup timed out
  because RESTART asynchronously starts playback; the tool now waits for the
  pause control before pausing. That attempt is not included in the six trials.
- QB-CLIP-010 focused tests: 9 passed; full frontend: 289 passed. Fresh production
  build embeds current frontend; linker emitted only its existing library warning.
- Initial integration test failed because its native mock did not model seeking;
  updated it to model synchronous native seeking and explicit no-op/failure cases.
  Final full suite passed. An earlier suite run from the wrong root failed its
  preview fixture lookup; the correct app-directory run passed.
- Actual changing-source before/after media windows pass their media-only gates;
  neither is a valid complete performance pair because the CPU observer failed.
- Strict five-pair CPU and aggregate dropped-frame acceptance: environment-blocked,
  not passed. No general CPU/GPU/FPS saving claim.

- Three synthetic formats exported successfully in 39.4s; packaged FFmpeg full
  decode passed for every H.264/AAC output. Source SHA-256 unchanged. Return to
  replay leaves one video/no canvas. Raw export-verification.json retained.
- Canonical roadmap validation passed (80 items, 14 plan/checkpoint pairs);
  git diff --check passed. Detailed sanitized evidence appended to
  docs/performance/vertical-preview-decoding.md.

- V2 plan review challenged architecture/security, lifecycle/resource bounds,
  blur geometry and test proof. Anonymous-before-src correction incorporated.
- New shader implementation and its actual performance: not run.

## Deviations

V1's measured performance invalidated its filter/compositing choice. V2 supersedes
that design without changing the feature criteria or resource budgets. Predecessor
plan and all failed raw trials remain retained. New trial/CPU collector is separate
from retained QB-CLIP-009 characterization to preserve historical behavior.

## Decisions

- Preserve existing primary/source/seek/music ownership; replace drawing only.
- Use existing exact-origin anonymous CORS; no new media/security admission path.
- Shader separable blur/color replaces CSS filters; match 17 CSSpx blur via cover
  geometry, fixed bounded source-aspect surface, one ResizeObserver and one RVFC.
- No readback, second video, automatic context-loss retry or frame-rate reduction.

## Blockers

No permission blocker remains. Performance and visual correctness of the shader
replacement are unproven; retain in-progress until the unchanged full gates pass.

## Next action

Investigate a materially different presentation strategy before further candidate edits. The approved v2 path has failed its unchanged resource gate; preserve current results and keep the feature open.

V2 implementation boundary: two-pass 49-tap separable shader, 640px source-aspect
backing, resize-derived 17px sigma, no CSS filter; owner retains seek/visibility
gates and terminal context-loss cleanup. Initial/replacement anonymous-before-src
instrumented integration passed. Full frontend 294 tests passed before added
callback-registration failure test; that focused 9-test owner suite also passed.
Review found callback-registration exception cleanup, now fixed/tested. Latest
production rebuild includes that fix. CPU collector now snapshots the complete
warmed-up tree immediately before timed sampling, including warmup children.

Actual protected texture upload passed with no alert, one video and 640x360 canvas.
Initial 13-tap shader showed visible blur sampling bands in exposed screenshot
regions; increased fixed symmetric kernel to 49 taps across the same three sigma.
This preserves the planned blur rather than accepting a visual regression.

GPU 49-tap first full five-pair gate failed: baseline median .565304 cores,
paired median increase .192871 versus allowed .084796; all foreground drop deltas
<=0. Valid raw retained as *-gpu49-teximage and paired-summary-gpu49-teximage.json.
Range setup and collector stale-name failure pairs retained separately. Trial now
scrolls the clip endpoint into view before a stepped drag and uses unique queue
request names. Texture storage is now allocated once and updated with texSubImage2D
per admitted frame (within approved v2 design), pending focused/build/new full gate.

Subimage candidate has two valid adverse pairs (.5185→.8136 and .3686→.8066),
then invalid range setup stopped the series; not a complete gate. Preserved as
*-gpu49-subimage-incomplete. Scout found deterministic 60fps endpoint key steps;
trial now sets 5→25s with 1200 complete ArrowRight presses and exact aria assertion.
Three valid diagnostics: upload-off .8316, draw-off .6867, native unpack/no-flip
.6074 cores. These single arms do not prove savings or isolate one sole cost.
Next candidate keeps native unpack orientation and flips source UV in the first
shader pass; source storage remains fixed. Build and fresh full matched gate next.


Final v2 boundary (2026-10-21): source storage allocated once, texSubImage2D updates,
native unpack orientation with source flip in shader, 49 fixed Gaussian taps.
Fresh Tauri build and focused GPU tests passed. Full frontend passed 295 tests.
Final complete five-pair native-unpack gate: A [.512947,.454840,.453662,.497193,
.505537], B [.818979,1.646683,.725898,.874118,1.094873]. Baseline median .497193;
paired median increase .376924 versus allowed .074579. CPU FAILED. All foreground
drop ratios were zero, so drops PASSED. paired-summary.json and a-1..b-5 retain
current raw; fifth pair was repeated in full after an EBUSY result publication race.
Previous failed pair retained with -gpu-native-unpack-result-race suffix. Collector
now publishes complete JSON via same-directory temporary file then rename.

Paired final settled-4s screenshots prove correct upright current blur, no local
error and one source canvas/video. Actual GL reports NVIDIA RTX3050Ti ANGLE/D3D11,
error 0; low/high-power requests both select that same hardware adapter. No claim
that software rendering caused the regression, or that native unpack solved it.
The GPU replacement is rejected for performance completion. No permission blocker;
no criteria weakened; no done claim. No source recordings modified. Next design
must address measured presentation cost, not repeat filter/layout/texture tweaks
without new causal evidence. Current product edits remain uncommitted for review.
