# QB-REPLAY-009 controlled production decoder evidence

Date: 2026-09-09. Acceptance design:
[`qb-replay-009-playback-boundary-and-speed-ladder-v3.md`](../../exec-plans/qb-replay-009-playback-boundary-and-speed-ladder-v3.md).
Execution authority: [`qb-replay-009.md`](../../execution/qb-replay-009.md).

## Result and boundary

**Passed: the ordinary canonical H.264 production playback path selected hardware
decoding in this controlled load.** The isolated player reported
`D3D11VideoDecoder` and `kIsPlatformVideoDecoder=true`, with real advancing RVFC
presentation. The inference depends on independently established single-load
isolation, not on the relative arrival order of CDP properties and load events.

This establishes the hardware-normal acceptance gate for the recorded fixture,
device, runtime and observed interval. It does not identify arbitrary later
reload/recovery generations, certify other devices, or establish a new performance
baseline. Runtime current-load diagnostics remained `unknown`, with decoder name
and platform flag absent. Earlier withdrawn decoder confirmations remain withdrawn.
Human audible output and A/V synchronization are not established by this capture.

## Controlled procedure

The existing benchmark-feature optimized Tauri build ran its ordinary `play_pause`
scenario: 2-second warmup, 10-second interval, pause, 500 ms wait, play for 1 second,
pause and disposal. No media fault or decoder/rate override was injected.

The existing sentinel runner created a fresh app process in its Job Object and a
new WebView profile at `app-data-controlled-decoder-20260909-r2/webview2-user-data`.
The test profile was newly created, with only the existing unsupported-HEVC marker
and empty Data Dragon cache prepared; the marker reused the accepted capability
disposition and prevented startup probe media. It is not a new HEVC result.
The normal primary controller, adapter, loopback server and decoder selection were
used. Existing Media subscriptions completed before source assignment.

A temporary tap retained existing native queue messages through the existing
benchmark writer; two frontend boundary observations and the adapter source-open
observation supplied singleton/load facts. No subscription, poll, trace, ETW,
per-frame observer, remote debug port or decoder-selection mechanism was added.
The retained tap was limited to 256 records/256 KiB, with the existing 16-event/
64 KiB ingress limit unchanged. The tap was detached before the ordinary build;
all three touched source files were restored to exact session-start bytes.

| Attribution prerequisite | Obtained evidence |
| --- | --- |
| Fresh context | Runner process PID 16624; one WebView-created event in the new profile; one document navigation, started/finished; no later navigation. |
| Subscription before load | `Capture.enabled` has `enabled=true`, five subscriptions; the frontend's existing ready promise resolves before the only `controller.open`/adapter source assignment. Before-open source is null and generation is 0. |
| One video and source | Adapter sees exactly one video, exactly one primary marker, no prior `src` or `currentSrc`; exactly one controller open, adapter open and CDP `kLoad`; generation remains 1. |
| Exact player association | One player (`0F07D98D35A20819AF24335317962BDE`); its sole `kLoad.url` equals the marked primary adapter URL including the unique session token. Final source/session and associated runtime snapshot match. |
| DOM evidence limit | This runtime's `playerCreated` supplied no DOM node ID. No native `DOM.describeNode` result is claimed. The independent marked-singleton/source observation plus exact unique load URL establishes the association. |
| Hardware selection | Sole decoder declaration is `D3D11VideoDecoder`; sole platform declaration is `true`; video track is `h264 high`, 1920x1080. No contradictory video-decoder observation. `FFmpegAudioDecoder` refers to AAC audio, not a software video fallback. |
| Actual presentation | Authoritative first RVFC at media time 0; three valid existing rate windows measured 0.999966668x, 0.999955914x and 1.000000000x. Last presented tick is 623199984 (12.983333 seconds). |
| Uninterrupted attribution | Zero seek requests/dispatches, recovery, media errors, source replacement or second load during playback. One player creation/destruction. |
| Capture completeness | Ten contiguous native records, 4233 UTF-8 payload bytes; captures start, enable, player creation, load/lifecycle, decoder properties, playback transitions, destruction and close. Zero loss/incomplete markers; close has zero pending bytes/events. All raw JSON and embedded event JSON parse. |
| Reconciled run | Terminal complete in 15006.5691 ms; exit 0, no timeout/forced termination; 67 accepted records (66 events + one server request), zero drops. Source/config/cache hashes preserved; runner and existing analyzer accepted. |

Final video-quality counters were 784 total/2 dropped frames; one range request
served 7973362 bytes, with zero cancellation. These support playback continuity;
this short diagnostic capture is not an observer-cost or regression campaign.

Cross-source monotonic estimates differ slightly between native and frontend
events. Their numerical ordering is not a load-provenance proof. Subscription
completion is established by the existing awaited control flow and independent
singleton/source/count invariants; properties need no per-load ordering inference.

## Identity and production configuration

- Source: `qb-replay-009-wip`, `78da82b53b1e604a79a95a7554e7b60913f5fe9c` plus
  the existing audited working tree and retained temporary acceptance patch.
- Acceptance binary SHA256:
  `1b40b01bcd68a9cea9acda4533cce09811af0feaf32df1167caa3efb66403096`.
- Ordinary rebuilt executable SHA256:
  `8c7e6f72dc500f675d72f9fd8f7f7fca532ea86f2a5f4384d99d3ad4a4482840`.
- Fixture: `native-current-v2`, game `1787904000`, schema v2, H.264 High
  1920x1080 yuv420p 60 FPS/AAC, 240 seconds; video SHA256
  `131a1a096bed8f3e8042059926a65cccc22733639d2bb63d3a2d705a6d7da5fe`.
- WebView2 `152.0.4191.66`, CDP 1.3; Windows 10 build 19045; Dell G15 5511,
  i7-11800H, Intel UHD driver `32.0.101.7077`, RTX 3050 Ti Laptop driver
  `32.0.15.8132`. D3D11 evidence does not identify which physical GPU decoded.
- Packaged media runtime r6 and both tool hashes were verified by the runner.
  The ordinary Tauri window configuration and builder have no GPU-disable or
  additional-browser-argument override; the three WebView environment overrides
  (additional browser arguments, user-data folder, browser executable folder)
  were unset. The test builder supplies only its isolated profile directory.
  Optional GPU CIM collection remains disabled. No generic GPU counter is used
  as decoder proof.

The one-time WMI attempt to retain actual process command lines occurred after
the app had exited and returned no rows. This is not command-line evidence;
configuration provenance and observed hardware selection are recorded above.

## Reproduction and retained artifacts

Raw root, relative to the repository:
`build/perf/qb-replay-012-webview/.chronobreak-replay-benchmark/results/qb-replay-009-controlled-decoder-20260909-r2/`.
It includes `events.jsonl`, `manifest.json`, `runner-metadata.json`,
`collection-result.json`, `terminal.app.json`, `terminal.json`, `post-hashes.json`,
`server_requests.jsonl`, process samples, and accepted analyzer reports.

Ignored `build/perf/qb-replay-009/decoder-acceptance/` retains
`acceptance-only.patch`, exact attached/source-start files and hashes,
`verified-summary.json` (including artifact hashes), and launch-environment facts.
`build/perf/qb-replay-009/verify_controlled_capture.py` asserts the isolation,
decoder, presentation, loss and terminal conditions against those raw artifacts.

Commands actually used:

```powershell
npm.cmd run desktop:build:benchmark --prefix app
cargo test --manifest-path app/src-tauri/Cargo.toml --features replay-benchmark playback_diagnostics
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <sentinel>/manifests/qb-replay-009/controlled-decoder-r2.json
python build/perf/qb-replay-009/verify_controlled_capture.py
npm.cmd run desktop:build --prefix app
```

Focused diagnostic tests passed (eight); both builds passed their frontend
typecheck/Vite and optimized Tauri compilation. The ordinary build excludes the
tap. Previously passed frontend/Rust/format/Clippy and M1-M5 checks remain valid
for the exactly restored product source. No benchmark tool or analyzer changed.

The first preflight rejected an incorrect new-profile cache path before launch;
correcting it passed preflight. The first sandboxed run, `...-r1`, created no
WebView/frontend, hit the 30-second startup timeout and was terminated. Its
artifacts are preserved and provide no playback/decoder evidence. R2 changed only
the required Windows execution access and fresh result/profile paths. It is the
only completed decoder capture. This is separate from the already settled
`qb-replay-009-audit-fixes-20260909-r2` corrective probe, which was not repeated.
