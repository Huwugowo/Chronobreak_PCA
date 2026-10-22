# Vertical preview decoding — QB-CLIP-009

Measured 2026-10-21 in the current release Tauri application, WebView2
155.0.4283.62, with the isolated `build/preprod` configuration/library. Sources
are synthetic test media; no personal recording was used or changed. The source
is H.264/AAC, 1920×1080 at 60 FPS, duration 1006.633 seconds, size 7,336,507 bytes.
It is low-complexity synthetic footage, not representative gameplay complexity.

## Method

Open a five-second clip at source zero, enter the actual clip workbench and use
its Horizontal/Vertical preview buttons. Temporarily launch the isolated app with
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`.

```powershell
node tools/vertical_preview/measure.mjs <absolute-playwright-module-path>
```

The diagnostic attaches to the existing production WebView, requires the isolated
preprod output path, and plays/pauses through the existing UI. It runs three
three-second trials per mode after restart/seek settlement. Raw bounded reports
stay in ignored `build/perf/qb-clip-009/presentation.json`; capability URL tokens
are redacted. Remove the diagnostic port by restarting normally afterward.

Per-element `webkitDecodedFrameCount` and playback-quality frame counters are
sampled before/after; a video-frame callback counts presented notifications.
100-ms samples inspect inter-layer media-clock drift. No GPU decoder identity,
process CPU, power or memory savings are inferred from these counters. Callback
counts are observer-sensitive and are not a complete frame-delivery accounting.

## Observations

All six trials remained document-visible and lasted 3010–3025 ms.

| Mode / trial | Foreground decoded frames | Backdrop decoded frames | Presented callbacks foreground / backdrop | Dropped-frame delta | Maximum sampled clock drift |
| --- | --- | --- | --- | --- | --- |
| Horizontal 1 | 180 | — | 174 / — | 0 | — |
| Horizontal 2 | 180 | — | 179 / — | 0 | — |
| Horizontal 3 | 181 | — | 173 / — | 0 | — |
| Vertical 1 | 182 | 185 | 179 / 173 | 0 / 0 | 26.7 ms |
| Vertical 2 | 181 | 184 | 177 / 170 | 0 / 0 | 13.2 ms |
| Vertical 3 | 181 | 185 | 168 / 147 | 0 / 0 | 13.9 ms |

Both vertical elements load the same source path. Each independently advances
its decoded-frame counter and presents frames. Combined median decoded work is
366 frames versus 180 horizontally, about 2.03×. This establishes duplicate
per-element media work, not two proven physical hardware decoder allocations or
twice the total application cost. Each vertical trial also observed one backdrop
seek/seeked pair, consistent with the explicit forced synchronization at play.

Pause: both element frame counters remained unchanged across 1.2 seconds.
Paused seeks to four seconds then one second settled both elements at the exact
requested media time. Switching back to horizontal while playing left one mounted
video and paused the detached backdrop. Returning to replay removed the workbench
and left the single replay video. No lifecycle leak was demonstrated.

Source SHA-256 before/after:
`BE85EDA1CFF0FD2BA2EB0E5A1021D4107EC852DDA4D2552C7447E102639A5824`.

## Disposition

`ClipExporterScreen.tsx` uses a second muted video as the blurred full-frame
context beneath the sharp adjustable foreground. Both views are useful; simply
removing the background changes the requested composition. The existing exporter
uses an animation-frame monitor and drift corrections for this secondary media.

Plan QB-CLIP-010 to draw the backdrop from the primary video's presented frames
instead of opening another video source. Preserve blur, focus/framing, audio,
paused seeking and export requests. Measure that implementation against a changing
synthetic source before claiming any broader performance benefit: canvas drawing
adds upload/composition work and could negate decode savings. This characterization
does not change product media behavior or complete that implementation.


## QB-CLIP-010 implementation verification — 2026-10-21

The replacement uses one primary video and a 640x360 display-only canvas for the
same blurred context. The foreground and export pipeline retain their contracts.
The actual native synthetic fixture is 30 seconds of changing 1080p60 H.264/AAC:
1800 fully decoded frames, 1800 distinct frame hashes. The isolated corpus is
`build/perf/qb-clip-010/editor`; source SHA-256 before/after is
`5e624be7d4d59211b6a229c9c7d130e59df4e8de6b133c6986be6dd4aa431589`.

Preserved old/new production runs use the same 0–25s clip, five-second warmup and
ten-second visible playing window near source seconds 5–15:

| Observation | Old a-1 | New b-1 |
| --- | ---: | ---: |
| Mounted source videos | 2 | 1 |
| Foreground decoded / total frame delta | 601 / 601 | 601 / 601 |
| Secondary video decoded frame delta | 601 | No secondary video |
| Foreground dropped frame delta | 0 | 0 |
| Seeks / pauses in window | 0 / 0 | 0 / 0 |
| Canvas | None | 640x360 |

These are **media-only diagnostic windows, not valid performance acceptance
pairs**. Windows rejected the required Win32_ProcessStartTrace event subscription
with 0x80041003 even outside the sandbox. No process-tree CPU comparison or
five-pair aggregate dropped-frame acceptance is claimed. Feature stays in progress
until the unchanged prescribed complete interleaved pairs pass.

Actual WebView screenshots at settled 1s/4s show non-black changing exposed
backdrop-only regions above/below the sharp layer, matching baseline composition,
blur/crop and safe-area placement. Framing/focus extremes and 1000x700 layout were
inspected. Rapid seeks 7s→2s→4s settled at 4s paused; restart and end loop resumed
near zero. Preset switches while playing retained one video and disposed the
canvas outside vertical mode. Built-in music advanced independently with source
unmuted/volume .8 and music volume 1; this checks transport/properties, not an
audible listening judgment. Returning to replay left one video and no canvas.

Three synthetic exports completed in 39.4 seconds: Discord 1280x720 (4,723,699
bytes), horizontal 1920x1080 (6,594,949 bytes), vertical 1080x1920 (31,681,490
bytes). All are 25.016667s H.264/AAC; packaged FFmpeg fully decoded all three with
exit 0 and no errors. Only the dedicated test library was written.

Final frontend suite: 289 tests / 32 files passed; focused renderer/exporter:
9 tests passed. TypeScript, Vite and fresh Tauri production build passed.
Lifecycle tests cover callback bounds, paused/native/queued seeks, stale callbacks,
source unload, hidden state, disposal and local drawing failures. Canonical
validation and diff whitespace checks passed. Raw trials, screenshots and export
verification are retained under ignored `build/perf/qb-clip-010`.


### Regression isolation

Administrator collection was explicitly authorized and resolved event-observer
access. A valid original A/B pair measured .4543 / 1.1493 logical cores. This is a
regression, not an optimization; a later sample-gap failure invalidated the full
series, which was stopped and retained.

Single-variable production diagnostics retained identical source/range/window:

| Diagnostic | Logical cores | Valid window | New foreground drops |
| --- | ---: | --- | ---: |
| Suppress canvas drawImage; retain CSS filters | 1.0007 | Yes | 0 |
| Remove CSS filters; retain frame copies | .3363 | Yes | 0 |
| Reduce destination canvas to 320px | .4958 | Yes | 0 |
| Promote filtered canvas with will-change + translateZ(0) | .3952 | Yes | 0 |

These are diagnostic arms, not five-pair acceptance. Blur-only and colour-only
arms had sample gaps and are numerically inadmissible. The evidence points to
filtered-canvas compositing/layer selection as the dominant regression; it does
not establish Chromium's internal GPU/readback mechanism. Layer promotion retains
all original filters, dimensions and frame updates and is the selected minimal
fix for final validation. No product quality threshold or CPU budget is weakened.


### Complete layer-promotion acceptance result

All ten final windows had valid process observation, CPU sampling and continuous
visible playback. Old CPU cores: .4458, .3519, .4035, .3628, .6045. New CPU cores:
.8366, .7729, .5731, .4632, .9210. Baseline median .403508; median paired increase
.316595; permitted increase .060526. **CPU gate failed.** Every foreground drop
delta was zero, so the drop gate passed. Raw `paired-summary.json` retains the
paired calculations; the summarizer correctly exits 1 on acceptance failure.

The early layer-promotion diagnostic did not reproduce reliably. Current evidence
isolates the regression to the filtered-canvas presentation path, but does not
prove a specific internal Chromium software/GPU mechanism. The chosen 2D design
is rejected for completion; it needs a versioned replacement design and fresh
acceptance, not another threshold/tolerance change. The feature remains unfinished.


### Replacement-path discovery

Further valid single-variable 10-second diagnostics on the same changing protected
source rejected in-context blur (1.5231 cores), wrapper CSS filtering (1.4349),
sibling backdrop-filter (1.0701), and 160/320-pixel in-context filtering
(1.5266 / 1.1913). These are diagnostic arms, not matched acceptance series;
none supplies evidence of a passing replacement. No security policy was altered.

The production WebView reports GPU compositing, canvas, WebGL and video decode
enabled, with NVIDIA RTX3050Ti/Intel UHD adapters present. This capability report
does not prove GPU work placement. Existing loopback exact-origin CORS already
supports anonymous video texture input; the current primary omits anonymous mode.
A GPU two-pass shader renderer is the replacement design candidate. It must prove
actual protected texture upload, blur quality, lifecycle and unchanged CPU/drop
gates before acceptance. No improvement is claimed from its existence or design.


### GPU shader implementation and first acceptance series

Production protected-source texture upload passed with anonymous mode established
before initial and replacement source assignment. A fixed two-pass 49-tap kernel
covers three sigma in each direction; 13 taps initially produced visible sampling
bands and was rejected before acceptance. Settled 0/1/4-second screenshots show
current non-black backdrop and comparable crop/blur/color, including 1000x700 and
framing/focus extremes. Rapid seeks, restart/autoplay, loop, resize, preset
switch/reentry, music and adjusted .4/.6 source/music volume passed. Forced native
WebGL context loss showed the local error, kept export enabled and one video;
explicit preset reentry cleared the error. Return to replay left one video/no canvas.
No frame readback or security relaxation was used. Raw gpu49 screenshots and
interaction JSON are retained under the isolated evidence root.

First complete shader series (texImage2D source update each frame) had valid old
CPU [.5102,.5370,.7430,.5653,.6677] and new [.8354,.8948,.6068,.7044,.8606] cores.
Median baseline .565304; median paired increase .192871; allowed .084796. CPU
failed; all new foreground drop fractions were zero and the drop gate passed.
Retained paired-summary-gpu49-teximage.json and trial suffix -gpu49-teximage.
Invalid range-drag and observer-queue runs are separately retained, never counted.

The renderer now allocates source texture storage once and updates it with
texSubImage2D, which the approved design permits. Tests prove no repeated source
allocation. New production resource measurements are pending; no saving is claimed.


### Final native-unpack shader acceptance: failed

Subimage-only candidate had two valid adverse pairs (.5185→.8136, .3686→.8066),
then a failed clip-range setup; this incomplete series is not a completed gate.
The fixture setup now uses 1,200 complete ArrowRight presses to advance the
60fps endpoint from 5s to exactly25s, with exact endpoint assertion before export.
Queue requests use unique names and CPU results are atomically published after
an observed stale-name collision and result-read EBUSY; every invalid pair is
retained and repeated in full rather than selectively retaining a valid arm.

Valid single diagnostic arms: upload disabled .8316, draw disabled .6867, unpack
flip disabled .6074 cores. These do not prove a causal saving. The final renderer
keeps native unpack orientation and flips source UV in its first shader pass.
Actual paired settled4s screenshots confirm upright current blur and geometry.

All five final pairs are valid. Old CPU cores [.512947,.454840,.453662,.497193,
.505537]; new [.818979,1.646683,.725898,.874118,1.094873]. Baseline median .497193;
median paired increase .376924; allowed .074579. **CPU gate failed.** All foreground
drop ratios zero; **drop gate passed**. Raw paired-summary.json and a-1..b-5 retained;
fifth pair was repeated in full following the result publication race.

The actual canvas reports ANGLE/NVIDIA GeForce RTX3050Ti/D3D11 and GL error0.
Low-power and high-performance context requests select the same NVIDIA adapter.
This rules out a software-renderer explanation for this observed candidate;
it does not locate all browser/compositor cost or prove general GPU savings.
The v2 design remains rejected for performance completion; QB-CLIP-010 stays open.


## Production disposition — 2026-10-22

QB-CLIP-011 restores the synchronized dual-video preview on the normal application
path. It is the better measured option, not an asserted ideal long-term design.
The rejected single-source GPU code, tests, immutable designs and failed results
remain for research and are not imported by production. QB-CLIP-010 stays incomplete.
Revisit only with a materially different credible performance hypothesis; do not
weaken thresholds or continue isolated shader tuning. The next performance focus
is whole-application playback/seeking/search/resize responsiveness, memory/VRAM,
and recorder contention under realistic use.
