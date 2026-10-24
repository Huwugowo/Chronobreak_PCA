# QB-REPLAY-009: startup dropped-frame investigation

Date: 2026-10-23. Owner: `QB-REPLAY-009`. Trigger: the completed
`QB-PERF-005` replay-memory observation. This is diagnosis and a proposed
correction, not a product fix or feature-completion claim.

## Finding

The large loss is concentrated at playback startup after a paused seek. It is
not sustained at the original short-window percentage. It also is not explained
away as lifetime-counter accounting: native dropped counters increase after play,
and frame metadata shows real presentation discontinuities, including a roughly
half-second source-time jump followed by a roughly half-second gap between frame
submissions to the compositor.
These are consistent with a perceptible transient skip/freeze. This investigation
did not obtain a human visual rating or physical-display capture.

The application-level cause of the reproducible large burst is the controller's eager
paused-seek nudge: `seeked` can arrive before the already-armed frame callback,
causing a temporary `playbackRate = 1/16`, play, pause, and restoration to 1x.
The same isolated video with the unchanged controller reproduces the burst,
without replay UI initialization. A separate native-video intervention reproduces
it with 1/16x pulses and not with otherwise equivalent 1x pulses. The exact
WebView internal clock/queue/decoder mechanism remains unproven; a hardware or
software decoder diagnosis is neither supplied nor needed for this conclusion.
This identifies a correction target in the tested fixture/runtime; it does not
assign every normal-replay dropped frame to the same cause.

Proposed correction: let the already-armed callback complete an ordinary paused
seek without an immediate low-rate play pulse. Retain authoritative target-frame
validation and the finite presentation timeout/recovery contract. Any necessary
fallback for a genuinely missing paused presentation must be separately designed
and validated; the zero-drop 1x control is not approval to substitute an untested
fallback throughout the product.

## What the original observation measured

`memory-01` sampled six 2-second windows: the initial open and five reopens.
Each window followed full replay readiness, a paused seek to about 52.433 seconds,
fullscreen entry, roster/asset render readiness, and fullscreen exit. Playback
then stopped before the library's settling/sampling interval. Thus the label
"post-reopen" also includes paused-seek and layout preconditioning.

Those windows reported drops/total of `40/118`, `3/119`, `27/118`, `28/118`,
`29/119`, and `28/118`: `155/710`. Their start/end snapshots cannot locate each
drop within a window. The earlier `motion-01` result of `11/1800` covers a
different 30-second moving workflow. The two results are not a matched
before/after regression comparison. Original evidence remains unchanged in
[`QB-PERF-005 report`](../qb-perf-005-replay-20261022/report.md).

## Procedure and measurement limits

The existing production benchmark runner, server, prepared fixture library,
lifecycle actions, counters and event transport were reused. Temporary attachments
extended only these diagnostic playback windows from 2 to 12 seconds and batched
native quality samples, native events, phase marks, frame callback metadata and
long-task observations. The tap sampled counters every 50 ms and on callbacks,
batched 48 rows per existing event, and capped each attachment at 12,000 rows.
No permanent observer, schema change, decoder trace or new benchmark framework
was added. No builds ran concurrently with the measurements.

All successful runs used the same synthetic moving H.264 High/AAC fixture,
1920x1080 at 60 fps, duration 1006.633333 seconds, through the existing replay
HTTP server. The video SHA256 is
`5a286d9ab489d5fe4ec813e1f68c210c9e64a05558cda512649b21510919aaee`.
The two-game disposable library, prepopulated SQLite index and offline DataDragon
cache were reused; every run had a separate app-data/WebView profile. DataDragon
fingerprint:
`974b5d6e8f63fc02344644ed3b2aa057906638803a5d699fb97a3dbe85603af8`.
The host was Windows 10 build 19045, i7-11800H, Intel UHD/RTX 3050 Ti, with
WebView2 `155.0.4283.62`. No device, codec, decoder or runtime tuning was applied.

Normal replay uses its original controller, UI, asynchronous details and actual
fullscreen actions. The isolated path replaces the replay surface with one video,
omits those details/UI, and uses matching synthetic fullscreen-size transitions.
Its normal rectangle is approximately 950x506, versus 1414x770 for normal replay
in the 1440x844 viewport. Its readiness gate waits for a first frame and canplay;
the normal gate also waits for replay data/render readiness. Native seek uses
52.433 seconds; controller seek uses the timeline-aligned 52.433333 seconds.
The normal/native comparison therefore localizes a cause; it does not estimate
the UI's isolated performance cost. The isolated native/controller arms share
geometry, and the pulse contrast shares the exact same native flow and target.

Phase marks report a visible document and one video throughout. The last two
plain-native cycles report `document.hasFocus() = false`; their zero-drop results
are retained with that limitation. The first four focused plain-native cycles
already yield `0/2869`. Normal replay, isolated controller and every pulse/control
cycle report focus at all phase marks. Phase samples do not continuously prove
occlusion/focus state between marks.

Times below are when an increased native counter was first observed, not exact
timestamps of individual discarded frames. Native counters are cumulative within
the media load, so each playback window uses a fresh start/end delta; all four
arms have zero dropped frames at every playback-window baseline. Frame callbacks
describe delivery to composition and may themselves be missed; `presentedFrames`
helps distinguish that from skipped source frames. They do not prove physical
scanout. The relevant primary API contracts are the
[Media Playback Quality specification](https://w3c.github.io/media-playback-quality/)
and [video frame callback specification](https://wicg.github.io/video-rvfc/).

## Results

Each complete arm has six 12-second playback windows, followed by the existing
bounded return-to-library lifecycle. Counts are measured deltas, not extrapolated
rates. The original 2-second observation is retained, not replaced by these runs.

| Arm | Drops / total frames | Drops in first 2 s per cycle | Drops in following 10 s per cycle |
| --- | ---: | ---: | ---: |
| Normal replay (`drop-normal-02`) | 121 / 4302 | 97 | 24 |
| Isolated native video (`drop-native-01`) | 0 / 4306 | 0 | 0 |
| Isolated unchanged controller (`drop-isolated-01`) | 46 / 4296 | 46 | 0 |
| Native pulse contrast (`drop-pulse-01`, both rates) | 71 / 4314 | 71 | 0 |

| Cycle | Normal: first 2 s / later 10 s drops | Plain native drops | Isolated controller drops | Native pulse rate | Pulse drops |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 5 / 5 | 0 | 16 | 1/16x | 30 |
| 2 | 17 / 4 | 0 | 30 | 1x | 0 |
| 3 | 39 / 5 | 0 | 0 | 1x | 0 |
| 4 | 4 / 2 | 0 | 0 | 1/16x | 5 |
| 5 | 16 / 2 | 0 | 0 | 1/16x | 36 |
| 6 | 16 / 6 | 0 | 0 | 1x | 0 |

In normal replay, 90 of the 121 drops were detected in the first 500 ms and 97
in the first 2 seconds. The remaining 24 were spread across 60 seconds of later
playback. Later source-frame spacing had a roughly 16.667 ms median/p95, with
occasional larger gaps; this is not a claim that ordinary playback never drops.
The largest startup source-time jump was 633.334 ms. There was one 87 ms long
JavaScript task during initial preparation, ending more than a second before
playback, and no recorded long task overlapping the large reopen bursts.
Normal cycles 2-6 all contain direct 1/16x samples. In cycles 2 and 6, the queued
`ratechange` handlers run after restoration and read 1x, while earlier `canplay`
and frame samples read 1/16x. Event-handler samples are not a log of the property
value at mutation time. Cycle 1 has no low-rate sample or ratechange event; its
five startup drops remain un-attributed, as do the 24 later normal-replay drops.

The isolated unchanged controller used its low-rate nudge in cycles 1 and 2;
they had 16 and 30 startup drops. In cycles 3-6, the target frame arrived before
`seeked`, the nudge was skipped, and all four windows had zero drops. None of the
isolated-controller drops occurred after the first second. No long tasks were
recorded in any isolated arm.

For the native contrast, each `seeked` event triggers play at the assigned rate;
the next frame callback pauses and restores 1x. The remaining seek/fullscreen/play
sequence is identical. Rates were ordered `1/16, 1, 1, 1/16, 1/16, 1` to avoid
confounding the treatment with a simple first-run versus warm-run distinction.
Low-rate pulses produced `71/2156` drops; 1x controls produced `0/2158`. All 71
drops were detected within the first second and none in the remaining 33 seconds
of low-rate playback. Low-rate pulse lengths were 648.0, 5.7 and 6.3 ms; even the
two short pulses reproduced a discontinuity. The 1x pulses were 5.3, 6.6 and
0.7 ms. This is a focused causal intervention, not a cross-device performance
distribution or a precise estimate of the effect of pulse duration. It uses three
cycles per treatment and a fixed, nonrandom order.

An isolated-controller witness (cycle 2) shows why this is more than accounting:

| Callback after play baseline | Frame media time | `presentedFrames` | Native dropped count |
| ---: | ---: | ---: | ---: |
| 65.9 ms | 52.483333 s | 4 | 5 |
| 90.1 ms | 52.966667 s | 5 | 30 |
| 582.8 ms | 52.983333 s | 6 | 30 |

The first transition skips 483.334 ms of source time while the composition count
advances by only one; the next frame is submitted about 500.1 ms later according
to `presentationTime`. Thus it is not simply the probe missing many intervening
callbacks. Native low-rate cycle 1 independently shows the same 483.334 ms jump
and a 516.7 ms submission-to-compositor gap, with consecutive `presentedFrames`
values. Sparse normal-replay drops outside the reproduced burst mechanism remain
unexplained.

## Root cause and proposed correction

In `app/src/playbackController.ts:586-626`, `onSeeked` releases the native seek
and checks whether its request already settled. If the frame callback won the
race, it resumes the desired state without a nudge. Otherwise it immediately
starts the presentation deadline and a 1/16x play nudge, even when the natural
target-frame callback is imminent. A qualifying callback at lines 475-484 calls
`stopNudge`; lines 248-255 pause and restore the effective selected rate.

The generalizable fault is making ordinary paused-seek completion depend on an
unnecessary rate/play/pause transition, selected by event ordering rather than
evidence that the browser cannot present the target while paused. The measured
browser pipeline subsequently exhibits a startup transient when returning to 1x.
Replay initialization may affect callback ordering and therefore how often the
branch is taken; it is not necessary for the defect. Removing the replay UI and
its details/assets did not remove the burst, and reproducing the low-rate pulse
without the controller was sufficient. This warrants correcting the playback
boundary. It does not exclude UI/initialization or scheduling contributions to
the remaining normal-replay drops, nor does it warrant optimizing those jobs or
tuning the test computer without further evidence.

The evidence does not isolate a Chromium audio clock, renderer deadline queue,
hardware decoder, or software fallback as the internal cause. Callback
`processingDuration` outliers are pipeline timing evidence, not proof that a
decoder is computationally too slow. Current-load decoder status remains Unknown.
No new tracing or forced decoder mode is warranted by this decision.

For a subsequent correction:

1. Prefer the already-armed authoritative frame callback after a paused seek;
   do not immediately change playback rate or start playback on `seeked` alone.
2. Preserve separate native-seek/presentation deadlines, exact target tolerance,
   one-active-plus-latest scheduling, generation/intent invalidation, recovery,
   selected speed and desired paused/playing state. Genuine missing-presentation
   cases must still terminate or recover within the contract.
3. Cover both callback/event orders and assert that ordinary paused completion
   needs no rate/play writes. Retain supersession/disposal and missing-frame
   coverage. Validate affected playing-seek, loop and selected-rate behavior.
4. Reuse these production 1x startup/steady windows plus paused target-frame
   checks to confirm the reproduced burst is removed and quantify residual
   normal-replay loss before accepting a fix. Do not add a
   warm-up sleep, discard startup measurements, hide native counters, switch
   decoders or hardcode a device-specific delay.

The current v3 ExecPlan explicitly prescribes the nudge. Any implementation must
first record a narrowly revised design under the repository workflow; v3 remains
immutable. No correction has been implemented in this investigation.

## Evidence disposition and reproduction

Source base: `e0055df442fcf5bb4545e9c877e5d75b84f0b62f`.
The first three completed arms share instrumented executable SHA256
`e7c4ac56be48cf82f3bf4a98bbfd3de25ba70eed5ea89a331b7c2a1ca53c0a8c`.
The native pulse contrast uses
`623be646ffec659f56501701a62b9b7322618530e49989e17c08f83a55679510`;
only the ignored probe adds the pulse treatment/control flow.

All four completed runs have app exit 0, no timeout or forced termination,
unchanged fixture hashes, zero remaining owned processes, zero unobserved owned
processes, zero lost event/server/process records, and six complete windows.
The normal replay run passed the existing analyzer. The three isolated runs
deliberately omit normal viewer lifecycle events: native runs omit
`playback_payload_ready`, `viewer_mounted`, `media_loadstart`, `metadata_ready`,
`media_canplay`, `first_presented_frame`; the controller run omits the first two.
The generic analyzer rejected them with exit 2 (runner exit 1). Those rejections
are preserved unchanged. Their direct phase/frame/lifecycle evidence is used here;
they are not represented as passing standard lifecycle benchmark runs.

`drop-normal-01` did not start the frontend in the restricted environment and
timed out at 30 seconds. It provides no playback evidence. It is retained beside
the desktop-access replacement. Early manifest/scratch preflight failures were
also retained; none launched a playback trial.

Raw runs and manifests live under
`build/perf/p5motion/.chronobreak-replay-benchmark/{results,manifests}/`.
Ignored reproduction artifacts live under
`build/perf/qb-replay-009-startup-20261023/`: frozen subjects/identities,
`temporary-final.patch`, original/restored source hashes, `dropProbe-r1.ts`,
`dropProbe.ts` (pulse version), build/run logs, `analyze_probe.py`, and
`gather_evidence.py`. Each trial uses its retained absolute manifest with
`tools/replay_benchmark/run.ps1`, first `-PreflightOnly`, then desktop execution.
The helper first analyzes each named run and then produces the accompanying
[`report.json`](report.json), which retains per-cycle timing, visibility/focus,
presentation witnesses, lifecycle receipts and raw-file hashes. Analysis asserts
six bounded windows, advancing media, monotonic window counters, agreement with
existing steady-playback deltas, and no lost records. This derived evidence is
not a replacement benchmark pass gate.

The two temporary product attachments have been restored byte-for-byte. The
ordinary `npm.cmd run desktop:build --prefix app` passed, including the frontend
build and release executable, SHA256
`db2363dc13d00e1edb02593528f689e7d0b0871d61b2c04a24d4e0528b8ac1c9`.
Built frontend assets contain no temporary probe markers. Product code, benchmark
tools and immutable ExecPlans have no diff from the session base. Canonical
validation passed for 86 roadmap items and 15 plan/checkpoint pairs.
`QB-REPLAY-009` remains ready/in-progress for the
unimplemented correction and separately deferred human audio/A/V observations.
`QB-PERF-005` and its completion evidence remain unchanged.
