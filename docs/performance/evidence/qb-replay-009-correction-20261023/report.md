# QB-REPLAY-009 natural seek presentation correction — 2026-10-23

The approved narrow correction removes the eager 1/16x play pulse after native
seek completion. The production repeat recorded **0/4309 dropped frames** over
six 12-second windows, compared with **121/4302** in the retained before run.
Startup remains included. All six paused seeks completed through natural frame
presentation, with both native event orders observed. A separate 44-step run
passed the affected seek, scrub, fullscreen, loop and rate-intent checks.

This closes the technical startup correction. QB-REPLAY-009 remains in progress
only for the previously deferred human audio/A/V observations, which were neither
performed nor waived. Completed QB-PERF-005 evidence is unchanged.

## Change and causal boundary

The [diagnosis](../qb-replay-009-startup-20261023/report.md) reproduced the startup
burst with the isolated unchanged controller and isolated native 1/16x pulses;
ordinary isolated native playback and 1x pulse controls did not reproduce it.
Replay UI initialization was not required. The general application error was
treating `seeked` before the frame callback as a reason to force playback at a
temporary rate. Those independently delivered events do not establish a missing
paused frame.

`app/src/playbackController.ts` now keeps the already-armed target-frame callback
and the existing 1500 ms presentation deadline after `seeked`. Automatic resume
waits for both facts, in either order, and honors current play/pause/rate intent.
Missing frames still enter the existing bounded recovery. The nudge token, timer,
temporary rate assignment, play promise and restore helper were removed. No new
pulse, fallback, timeout tuning, decoder/initialization change or monitoring was
introduced. Public interfaces, target tolerance and the six-rate policy are intact.

Focused review also identified a directly affected existing ownership edge: a
user Play during the seek could be followed by a second automatic Play, replacing
the user's pending result. Both event-order regressions reproduced it. Automatic
resume now starts native playback only if the element is still paused, preserving
the synchronous user's operation. It does not change public activation handling.

The exact browser clock/queue/decoder mechanism remains unproven and is unnecessary
to choose this correction. No hardware or software decoder setting was changed.
Runtime decoder attribution remains Unknown; earlier controlled hardware-normal
acceptance retains its existing scope.

## Normal startup comparison

Both runs used the existing `settled-memory` lifecycle scenario, six viewer cycles
and fresh process/profile identities. Each cycle waits for readiness, seeks while
paused to 52.433s, enters fullscreen, awaits rendered readiness, exits fullscreen,
plays for 12s, pauses/disposes and returns to the settled library. The bounded r1
native-quality/RVFC/phase/long-task observer was reused byte-for-byte: 50 ms samples,
48-row flushes and a 12,000-row attachment cap. No startup sample was excluded and
no native counter was reset. The normal viewer/controller remained in use.

| Observation | Before `drop-normal-02` | After `drop-corrected-normal-01` |
| --- | ---: | ---: |
| Six windows combined | 121 / 4302 drops | 0 / 4309 drops |
| First two seconds of each window | 97 drops | 0 drops |
| Later ten seconds of each window, 60s total | 24 drops | 0 drops |
| Per-cycle drops | 10, 21, 44, 6, 18, 22 | 0, 0, 0, 0, 0, 0 |
| Largest adjacent frame media-time jump | 633.334 ms | 16.667 ms |
| Largest adjacent compositor-submission gap | about 266 ms | 74.9 ms |
| After-run largest submission gap beyond 2s | — | 18.1 ms |

After windows lasted 12000.5–12003.6 ms and advanced about 11.93–11.95 media
seconds. Existing `steady_playback` frame deltas exactly agree with probe deltas.
All start dropped-frame counters were zero; no callback-count gaps or non-1x
samples occurred. Every phase mark was visible, focused and had one video element.
No captured long task overlapped these windows.

Natural paused presentation completed in 36.2–216.3 ms from request. Five cycles
delivered `seeked` first and one delivered the frame callback first. In the five
seeked-first cycles, presentation followed by 0.4–3.0 ms without a Play call.
The accepted frames were 16.6663 ms before the rounded target, within the existing
one-frame-plus-rounding tolerance; this correction does not tighten frame accuracy.

This single matched procedure supports removal of the demonstrated startup burst.
It is not a statistical campaign or a guarantee of zero drops on other media,
devices or later runs. The before-run's sparse residual normal losses remain
unattributed; none occurred in this after-run. Native counter detection timestamps
are observation times, and RVFC describes submission to the compositor, not
physical display scanout or a human visual inspection.

## Affected production behavior

`drop-corrected-flow-01` used the existing `layout` scenario kind and actual normal
viewer actions, selected by a temporary scenario-id branch. No action/schema,
runner or analyzer extension was added. Forty-four bounded action steps passed:

- Six paused and six playing forward/backward seeks at early, middle and late
  positions: approximately 8s, 503.316s, 996.633s, 986.633s, 452.985s and 5s.
  Paused seeks produced no native Play event; playing seeks resumed once after
  both target-frame and native-completion evidence, retaining selected speed.
- One six-request rapid scrub: two native dispatches, five superseded outcomes
  and one authoritative latest-target presentation at 52.4333s. No amplification
  or leftover seek remained.
- Enter/exit fullscreen while paused and playing, with a seek dispatched before
  each entry. All step checks preserved the same connected primary video, media
  generation and load session; final play/pause intent was retained.
- A seek at selected 2x and a separate active seek changed from 2x to 0.5x while
  its queue was `1 ACTIVE` and the native video was paused. The latter presented
  the target and resumed at selected/applied 0.5x.
- Actual clip mode over the existing 47–60s range at 2x, with two loop wraps.
  Both target presentations were exactly 47s and resumed through the normal path.

Twenty completed target presentations, including both loop wraps, took 2–176.5 ms
from request. Seventeen were seeked-first and three frame-first; target errors
stayed within the unchanged one-frame tolerance. The single additional dispatched
scrub target was explicitly superseded, not a failed presentation. Raw ordering
checks found no native Play event before both facts for any completed seek.

DOM media events are asynchronous. Temporary per-step Play event counts outside
the awaited directed seeks can straddle neighboring action boundaries; they are
not treated as synchronous media mutations. Controller snapshots, source identity,
full seek-event ordering and final intent supplied the layout checks. There were
no media errors, timeouts, recoveries or low-rate pulse samples. All step marks
were visible/focused. This does not repeat or supersede the earlier six-rate
capability/fallback evidence, and supplies no human audibility claim.

## Verification, identities and retained artifacts

- Focused controller/HTML-adapter/seek-state tests: **64 passed**. New cases cover
  both event orders, delayed target frames, current pause/rate, user-play ownership,
  stale waiter callbacks, repeated loops, missing/nonqualifying-frame recovery
  exhaustion, and disposal. Existing public-play, generation, scheduling and rate
  fallback regressions remain. Ten new assertions failed against the old pulse;
  two user-play cases separately reproduced the duplicate-operation edge.
- Final frontend suite: **361 tests / 37 files passed**. TypeScript noEmit, Vite
  production build, both benchmark release builds and the detached ordinary
  Tauri release passed. Rust, adapter interfaces and benchmark tooling have no
  product diff; their unaffected earlier checks were not represented as fresh runs.
- Both runs passed the existing analyzer, exited with code 0, preserved all 14
  prepared fixture identities, retained zero lost records and left zero active
  owned processes or unobserved descendants. Disposal reported zero owned timers
  and listeners. Normal run: six cycles; flow run: 44 actions.
- Temporary ViewerScreen and benchmark attachments were restored byte-for-byte.
  Ordinary generated assets contain no probe markers. The only product changes
  are the controller correction and regression tests; architecture and canonical
  records describe the resulting behavior. V1–V3 ExecPlans remain immutable;
  reviewed [v4](../../../exec-plans/qb-replay-009-playback-boundary-and-speed-ladder-v4.md)
  governs this correction.

Base commit: `e0055df442fcf5bb4545e9c877e5d75b84f0b62f` plus the recorded correction.
Both production runs used the same dedicated `p5motion` sentinel, game 1792431584,
1006.633333-second synthetic H.264 High/AAC 1920×1080 60fps fixture, with video SHA256
`5a286d9ab489d5fe4ec813e1f68c210c9e64a05558cda512649b21510919aaee`.
WebView2 was `155.0.4283.62`, on the same Dell G15/i7-11800H/Intel UHD/RTX 3050 Ti
Windows 10 build 19045 host. No device tuning or decoder switch was used.

The [machine-readable receipt](report.json) retains binary/source/raw-file hashes,
window data, seek witnesses, flow results, source restoration and ordinary-build
identity. Exact raw roots are:

- `build/perf/p5motion/.chronobreak-replay-benchmark/results/drop-corrected-normal-01/`
- `build/perf/p5motion/.chronobreak-replay-benchmark/results/drop-corrected-flow-01/`
- `build/perf/qb-replay-009-correction-20261023/` for frozen subjects, source
  snapshots, bounded helper/analyzer, manifests' preparation context and check logs.

Procedural failures are retained: the first test invocation could not write the
repository command log in the restricted environment and ran no tests; the
authorized retry passed. The first normal preflight used a relative manifest
path and was rejected before launch; its absolute-path retry passed. A stale
nudge-specific test expectation and a test-only timer-count assumption were
corrected without changing product deadlines. No production run failed or was
replaced, and no acceptance criterion was weakened.
