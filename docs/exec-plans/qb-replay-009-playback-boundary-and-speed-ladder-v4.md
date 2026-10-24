# QB-REPLAY-009: Playback boundary and natural seek presentation (v4)

This self-contained revision supersedes
`docs/exec-plans/qb-replay-009-playback-boundary-and-speed-ladder-v3.md` only for
the seek-presentation nudge and the focused correction/verification described
here. Preserve v1-v3 as immutable history. The user approved the startup diagnosis
and authorized this narrow correction on 2026-10-23. Feature acceptance remains
in `feature-list.json`; actual progress, prior evidence and remaining gates belong
only to `docs/execution/qb-replay-009.md`.

## Purpose

Complete a paused replay seek at the requested frame without briefly playing at
1/16x and perturbing the following playback startup. Preserve responsive latest
intent, truthful presented time, normal playing seeks, clip loops and bounded
failure handling. The correction must remove the demonstrated startup-burst
mechanism without concealing remaining native dropped frames.

## Relevant planning-time architecture

The dependencies QB-REPLAY-006/008/012 supply validated replay descriptors,
loopback media transport and the exact 48,000,000-tick timeline. These boundaries
are retained; no dependency reimplementation or media preparation is required.

| Owner | Contract relevant to this correction |
| --- | --- |
| `app/src/playbackController.ts` | Sole primary-media lifecycle owner. `seek` captures intent, `dispatchPending` pauses/assigns the target and arms a frame callback, `onSeeked` releases the native slot, `armFrame` accepts authoritative presentation, `resumeDesired` restores current play intent, and `recover` reloads the same element with bounded attempts. |
| `app/src/htmlVideoPlaybackAdapter.ts` | Concrete HTML-video seam, native event/readiness/error access and frame-time conversion; no alternate playback backend. `play()` is invoked synchronously to preserve user activation. |
| `app/src/viewerSeekState.ts`, `replayTime.ts` | Distinct requested/dispatched/seeked/presented state, generation/epoch validation, rational frame grid and PTS-origin conversion. A currentTime observation is not presentation. |
| `app/src/components/ViewerScreen.tsx` | One persistent primary JSX video, layout/clip policy, controller subscriptions, and existing benchmark actions. Details/assets arrive independently of the media. Fullscreen uses the same video/controller. |
| `PlaybackControls.tsx`, `FullscreenOverlay.tsx` | Shared six native rates and applied sound controls. They do not mutate or own media. |
| Existing benchmark path | `app/src/benchmark.ts`, native benchmark writer and `tools/replay_benchmark/` provide sentinel isolation, lifecycle actions, quality counters, process ownership, fixture hashing and bounded event retention. Temporary diagnostic attachments are removable observation seams. |

The scheduler permits one native seek and one replaceable pending request, a
100 ms dispatch interval and half-frame deduplication after actual presentation.
It retains a separate presentation waiter after native completion. A callback
can settle presentation before `seeked`; the native slot must nevertheless remain
occupied until native completion. A newer intent settles older outcomes and
invalidates callback ownership without issuing parallel currentTime assignments.

The native seek and subsequent presentation deadlines are each 1500 ms. Recovery
uses the same element, a new generation/session URL and current target/preferences,
with two attempts within ten seconds and two consecutive unsuccessful attempts;
only ten seconds of healthy advancement resets the consecutive budget. Readiness
has a five-second deadline. Exhaustion leaves an explicit degraded state and manual
retry. Disposal cancels callbacks, listeners, timers and pending operations.

## Scope and non-goals

Change the internal primary seek-completion policy and its deterministic tests.
Remove obsolete nudge state/timers/branches rather than leaving a disabled parallel
path. Update the small durable ownership description and canonical evidence.
Public commands, typed outcomes, time tolerances, native seek bounds, recovery
limits, speed ladder and observable benchmark meanings remain unchanged.

No speculative replacement play pulse, delay, timer tuning, startup exclusion,
counter reset, decoder switch, extra monitoring, initialization optimization,
backend migration, recording/codec change, export change or machine-specific
configuration. General stepping remains QB-REPLAY-015. The clip editor's separate
multi-element preview and completed QB-PERF-005 memory work are outside scope.
Human audible/A/V verification remains a separate requirement; synthetic fixture
video and property values cannot satisfy it.

## Exploration findings

The approved focused diagnosis used one 1080p60 H.264/AAC moving fixture and
WebView2 155.0.4283.62. Normal replay reported 121 dropped frames across six 12s
windows, 97 within their first two seconds and 24 over the later 60s combined.
Frame metadata showed brief source-time jumps and gaps between submissions to
the compositor. This was transient presentation disruption, not sustained loss
at the original short-window percentage or merely cumulative-counter accounting.

The isolated unchanged controller reproduced the burst in the two cycles taking
the low-rate nudge; four naturally completed cycles had no drops. A native-video
contrast reproduced 30/5/36 startup drops with 1/16x pulses and 0/0/0 with 1x
controls. Replay UI initialization was not required. The precise browser clock,
queue or decoder mechanism and sparse residual normal loss were not established.
This evidence supports removal of the eager pulse, not a guarantee of zero drops
on every device or adoption of a 1x replacement pulse.

The defective policy treats `seeked` arriving before its frame callback as reason
to play immediately at 1/16x. The already-armed paused callback may be imminent;
the independent event order is not evidence that paused presentation cannot occur.
Dispatch already pauses both initially paused and initially playing seeks. Playing
and loop seeks therefore also require real target presentation before restoring
their current desired play state; removing the nudge must retain that continuation.

## Chosen design and rationale

### Natural target presentation

Keep dispatch, target qualification and the callback already armed by the
controller. On qualifying `seeked`, release the native slot and preserve the
existing pending-request and already-presented branches. For an unsettled latest
request with RVFC, retain it as the presentation waiter and start the existing
1500 ms presentation deadline. Do not call play or set playbackRate merely because
`seeked` arrived first. Do not rearm/cancel a valid current callback at that boundary.

A qualifying callback settles the target once, clears the presentation deadline
and waiter, and resumes current desired play state when no native seek is active.
If presentation wins the race, leave the native slot occupied and resume only
when native completion arrives. If the user currently wants pause, remain paused;
if playing is desired, invoke the ordinary selected-rate play path. User play
commands during the wait remain activation-sensitive and synchronous; they do not
turn currentTime into presentation or transfer ownership to stale callbacks.

For an RVFC-backed seek, automatic resumption requires both the qualifying target
frame and native `seeked` for the same current intent, in either order. Before the
second fact, there is no automatic play; after it, there is exactly one ordinary
selected-rate play if the user still wants playback. A user-originated synchronous
`play()` is the explicit activation-sensitive exception.

There is no new fallback pulse. Missing target presentation still fails and enters
the existing bounded recovery policy at the same deadline. RVFC-unavailable
behavior stays explicitly unavailable for authoritative seeks while approximate
display/ordinary playback remain available. A demonstrated regression in genuine
paused presentation would require a recorded causal finding before a fallback is
designed; do not silently reinstate a pulse or increase a timeout to pass a test.

### Remove only obsolete nudge machinery

Remove the nudge request/token, 750 ms nudge timer, temporary rate assignment,
internal nudge play promise and stop/restore helper. Remove nudge-only conditions
from rate eligibility, frame health, buffering checks, public commands and native
rate observations. Keep public play ownership, seek cancellation and generation
guards intact. The controller must neither hold a selected rate behind an internal
nudge nor restore a stale speed: setRate applies the current user selection through
the existing rate policy, including while waiting for presentation.

Rate measurements remain suspended across active/pending/waiting seeks and restart
after discontinuity. Keep selected/applied/observed distinctions, the six selectable
rates (0.25/0.5/1/2/4/8), visible limitation/fallback, no rate-emulating seeks and
no automatic reapplication of failed 8x. Audio preference/application semantics
are unchanged. No new subscriber, media mutation API or benchmark event is needed.

### Retained ownership and diagnostics

Keep one video across layout/recovery, frame-driven semantic state, half-open clip
loops and explicit-seek precedence. Capture generation/intent ownership when jobs
and callbacks are armed; cancel older work on recovery/open/dispose. Invalid open
inputs do not affect the current generation. Preserve mute/volume/rate intent and
exact timeline conversion. Transport/server bounds and source integrity are unchanged.

Hardware remains the normal canonical path with ordinary WebView GPU acceleration.
Existing controlled-load hardware acceptance remains scoped to that observed
fixture/device/runtime/load. Runtime current-load decoder status stays Unknown
when CDP player properties lack load provenance. This correction does not change
diagnostic bindings, acquisition, native subscriptions, or decoder configuration;
no decoder acceptance recapture or native diagnostic work is required by it.

## Rejected alternatives and planning decisions

- Delaying the 1/16x pulse still introduces the proven perturbation and a new
  timing heuristic; the existing presentation deadline already bounds waiting.
- Replacing it with an unconditional 1x pulse would make paused seeks play and
  has not been validated as a general fallback or target-accuracy policy.
- Waiting an arbitrary warm-up interval, resetting counters, or dropping early
  samples hides the defect instead of correcting its producer.
- Optimizing asset/details initialization or changing decoders is not justified
  by reproduction in the isolated native video. Residual loss is measured and
  reported separately; it is not assumed to share the nudge cause.
- Broad rate/decoder/corpus/resource campaigns do not answer a new decision here.
  Reuse unaffected evidence and expand only for a concrete failed invariant.

## Milestones

### C1: Correct the seek-completion producer

Remove eager nudge behavior and its obsolete state in playbackController.ts.
Update meaningful deterministic regression cases and retain the existing adapter
and state contracts. First validate the controller/time/seek dependency cone.

### C2: Validate startup and affected shared flows in production

Use the existing benchmark-feature release and safe fixture to repeat the six
normal startup windows with the same bounded probe. Add one bounded direct check
of paused/playing seeks, rapid latest intent, clip looping, selected speed during
seek and same-element fullscreen transitions. Preserve raw failures and analyzer
dispositions. Investigate only evidence that could change the correction.

### C3: Detach probes and reconcile durable state

Restore temporary attachments exactly, run the ordinary production build and
applicable frontend checks, update durable architecture and concise evidence,
and validate canonical linkage. Preserve the diagnostic report as the before
record. Full feature completion still depends on every unchanged acceptance gate,
including separately deferred human observations.

## Verification design

Deterministic tests must exercise both `seeked`/RVFC orderings, with no rate or
play writes for an ordinary paused completion. Include delayed qualifying and
nonqualifying frames, unchanged native/presentation deadlines, missing-frame
bounded recovery/exhaustion, pending replacement, stale callback rejection and
dispose during a presentation wait. For `playAfter: true`, assert no automatic play
before the second fact and exactly one selected-rate play after it in both event
orders. User pause during either wait must prevent that resumption. Also verify
loop continuation and selected-rate updates during a wait. Retain public play
supersession and no-RVFC behavior.
Do not delete a failure/lifetime invariant just because its old test mentions nudge.

Run focused tests first and the complete frontend suite against final product
source. Exact commands (Windows may use npm.cmd):

```powershell
npm run test --prefix app -- playbackController.test.ts htmlVideoPlaybackAdapter.test.ts viewerSeekState.test.ts
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
npm run desktop:build:benchmark --prefix app
npm run desktop:build --prefix app
```

The benchmark build applies only when temporarily instrumented; the ordinary build
follows detachment. Follow docs/development/VERIFICATION.md for canonical validation
and whitespace checks. No Rust/adapter interface/tooling change is designed; retain
their unaffected verification. If that dependency cone actually changes, run its
corresponding required checks and explicitly record why scope changed.

Production uses the existing disposable p5motion sentinel library, game 1792431584,
synthetic H.264 High/AAC 1920x1080 60 fps, video SHA256
`5a286d9ab489d5fe4ec813e1f68c210c9e64a05558cda512649b21510919aaee`.
Reuse offline DataDragon/index preparation and fresh run/profile/result identities;
never overwrite fixtures or prior results. Freeze the subject and record binary,
source, fixture/runtime/device identities. Use run.ps1 -PreflightOnly followed by
bounded desktop execution. No simultaneous build/measurement or decoder tuning.

The retained drop-normal-02 procedure provides the before reference: six cycles
with readiness, paused seek to 52.433s, fullscreen entry/render-ready/exit, 12s
playback, pause/dispose and library settling. Reuse the same temporary native
quality/RVFC/phase/long-task tap, 50ms samples, 48-row batching and 12,000-row cap.
Keep startup windows included. Retain first 0.5s/1s/2s and subsequent 10s drop
deltas, frame media-time/presentation gaps and presentedFrames, native events,
visibility/focus, seek outcomes, errors/recovery and ownership/fixture receipts.
Counter-detection times and callback timestamps are not physical-display capture.

The bounded affected-flow check uses controller commands and actual RVFC in the
normal viewer: directed early/middle/late forward/backward paused and playing
seeks, one latest-wins scrub burst, two short-loop wraps, one paused/playing
fullscreen transition including pending work, and selected-rate retention through
a seek at representative non-1x speeds. Existing deterministic six-rate/fallback
tests cover unchanged rate capability policy; no new 8x capability/audio claim is
made. Check terminal cleanup and source hashes. Direct assertions may supply
evidence when the standard analyzer rejects an intentionally different scenario;
retain the rejection and do not label that run a standard benchmark pass.

## Performance and reliability gates

All seek/presentation ownership, target tolerance, one-active-plus-latest, finite
deadlines/recovery, persistent element and disposal bounds remain hard gates.
No new errors, unbounded wait, recovery loop, seek amplification or loss of user
pause/rate intent may remain. A callback result must demonstrate target presentation;
currentTime, property values and UI labels alone cannot pass these cases.

Normal after playback must remove the reproduced low-rate startup pattern and
show no material regression in startup/steady drops or responsiveness against the
retained before record. Quantify residual loss without requiring fabricated zero
counts or declaring every isolated sample a regression. Repeated large media jumps,
presentation stalls, failed target frames or changed recovery are material and
require the smallest investigation that can change the implementation decision.
Do not expand scope merely to improve statistics or report formatting.

Keep probe retention bounded and detach it before ordinary build. Existing owned
process cleanup, source hashes and controller lifetime observations establish the
affected resource bound; no new memory campaign is required. Stop after the
correction and focused evidence satisfy these gates and canonical documentation
is truthful. Unavailable required observations remain unverified, never passes.
