# QB-AUDIO-003 — Restore one system-audio track with WASAPI loopback

## Purpose

Record audible Windows playback in the existing recording MP4 without requiring a vendor-specific “Stereo Mix” device. Produce one combined stereo AAC track, synchronized to the existing WGC video epoch. Device changes must either recover within a bounded interval or produce an actionable recording failure; they must never silently select an indefinitely silent substitute.

This is a prerequisite repair separate from `QB-AUDIO-001`, whose two-group League/everything-else requirements remain deferred. Dependencies are the existing automatic recorder (`QB-CAP-001`) and canonical recording timeline (`QB-REPLAY-012`).

## Relevant planning-time architecture

- `recorder/src/encoder.rs`: `AudioSource`, `Ffmpeg::detect_audio_source`, `choose_audio_device`, and `append_windows_audio_arguments` currently discover DirectShow capture-device names. Failure to find Stereo Mix or a name resembling WASAPI loopback selects lavfi silence. This is hardware capture-device discovery, not native endpoint loopback.
- `recorder/src/service.rs`: audio is selected once when the service starts, which also makes device selection stale by the time a recording begins. Diagnostics use the same discovery mechanism.
- `recorder/src/native/session.rs`: `start_with_stop` creates WGC, recovers a usable image, establishes `NativeCfrClock` from that image's physical QPC timestamp, then constructs the mux and NVENC. `finish` drains the encoder and mux before closing WGC. Session ownership survives replaceable WGC windows.
- `recorder/src/native/mux.rs`: `NativeMuxPlan::h264` and `NativeMuxProcess` supervise one FFmpeg child. NVENC Annex-B H.264 enters stdin; FFmpeg copies video, assigns the existing exact CFR grid, encodes AAC, and writes fragmented MP4. Current audio-first input ordering does not establish a WGC/audio clock relationship. The current `-shortest` option is unsafe if a new audio input ends prematurely.
- `replay-time/src/lib.rs` supports one audio stream with coverage relative to the validated video origin. Keep `MediaTimelineV2`, the 48-MHz replay scale, video frame identity, media UUID, and finalization/publication contracts unchanged.
- `recorder/src/poller.rs` owns recording/capture metadata. `app/src-tauri/src/library.rs` strictly parses both metadata levels and preserves them when saving. An additive audio-provenance field must be accepted and preserved there before the recorder emits it.

The video path remains `WGC → D3D11 conversion → NVENC → H.264 stdin`. No audio operation may block its callbacks, GPU conversion, frame admission, or encoder completion.

## Scope and non-goals

Capture the mix on the current Windows default playback endpoint, explicitly `eRender/eConsole`, and follow changes to that default. This means the games/system playback endpoint, not every physical output simultaneously. Audio explicitly routed to another endpoint, a separate communications device, exclusive-mode playback, and protected content are outside this guarantee.

Keep one existing FFmpeg child, one H.264 video stream, one 48-kHz stereo AAC track at the existing 192-kb/s setting, and current MP4 validation/publication. Retain an explicit silent mode for generated fixtures and intentional configuration only. Do not introduce process-loopback separation, microphones, per-app routing, UI mixing controls, a new player, virtual devices, injection, a new video backend, or an FFmpeg rebuild.

## Exploration findings and official API basis

Microsoft's [Loopback Recording](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording) describes endpoint capture using `IMMDeviceEnumerator`, a render endpoint, shared-mode `IAudioClient`, and `AUDCLNT_STREAMFLAGS_LOOPBACK`. It does not require a hardware loopback capture device. Event-driven loopback is supported from Windows 10 1703, earlier than Chronobreak's Windows 10 1903 baseline.

Use the project's existing `windows` 0.62 dependency and Microsoft's [Rust Windows bindings](https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Media/Audio/index.html), adding narrowly required audio, COM, and multimedia-scheduling feature flags. Do not adopt a second general audio framework or hand-written COM ABI.

[GetBuffer](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer) supplies the first sample's QPC position already converted to 100-ns units. Those units match WGC provenance; they are not raw `QueryPerformanceCounter` ticks. Its packet flags distinguish silence, discontinuity, and invalid timestamps. Packet receipt time cannot replace capture time.

The existing untracked `tools/audio` process-loopback investigation is provenance only. Its observed unflagged gaps reinforce the need to account for actual packet clocks; its Windows/build and WebView results do not establish endpoint-loopback or production A/V correctness. Its NVIDIA timing attempt failed before encoding. No new hardware result is assumed by this design.

## Chosen design and rationale

### 1. One owned Windows audio component

Add a narrow native audio module, with a small pure timeline helper if useful. The session owns its cancellation, first error, bounded packet storage, capture worker, PCM writer, pipe, notifications, and final counters. There are at most two added threads: one event-driven WASAPI owner and one normalizer/pipe writer. COM interfaces and packet buffer pointers stay on their owning thread; only owned PCM and checked timestamp/control data cross the queue.

Initialize COM on that worker and pair its lifetime correctly. Wrap event handles, COM-allocated format/endpoint strings, callback registration, and packet acquisition in RAII. Every nonempty successful `GetBuffer` has a matching `ReleaseBuffer` on the same thread, including failures. Copy into reusable bounded blocks and release promptly; no pipe I/O, logging, allocation-heavy processing, or waiting while holding a WASAPI packet.

Use a normal shared-mode audio-engine period, not the smallest available low-latency period. [Initialize](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize) requires zero shared event-mode duration/periodicity; inspect the actual buffer size afterward. Set the event before `Start`, drain packets until empty on each wake, and wait on audio/control/stop events. A modest timeout also handles healthy idle streams and cancellation. Use an MMCSS “Audio” registration for the capture worker, reverted on exit; inability to obtain it is diagnostic, not a reason to alter global priorities or timer resolution.

### 2. Fixed format and a private PCM input

Request interleaved float32, 48,000 Hz, stereo in shared loopback mode, with `EVENTCALLBACK | LOOPBACK | AUTOCONVERTPCM`. Microsoft's [stream flags](https://learn.microsoft.com/en-us/windows/win32/coreaudio/audclnt-streamflags-xxx-constants) provide channel mapping/sample-rate conversion when needed. Validate the requested `WAVEFORMATEX` and checked byte counts; successful `Initialize` accepts that client format. Record `GetMixFormat` as the engine format, not as a getter for the initialized client format. Do not reject conversion solely because a no-conversion format probe returns a closest match. Unsupported initialization produces an explicit error; never reinterpret native-rate or multichannel bytes as this fixed format. Start with the engine's default conversion quality. The optional `SRC_DEFAULT_QUALITY` setting costs more CPU; enable it only if the conversion fixture reveals an audible quality problem and a measured check supports the cost.

Feed normalized `f32le`, 48-kHz, stereo PCM through one private local Windows named pipe into the existing FFmpeg child. Create a unique, first-instance, byte-mode pipe before spawn; restrict it to the current user and reject remote clients. Use overlapped connect/write with cancellation and finite deadlines, not an indefinitely blocked synchronous write. Retain H.264 exclusively on stdin. The existing TCP fixture is an input-ordering precedent, not a reason to expose production audio over TCP.

Open audio before video, specify its format/rate/channels explicitly, keep maps explicit, and prevent raw-audio options from leaking onto the H.264 input. Preserve `-c:v copy`, the existing `setts` video grid, negative-AAC-priming policy, fragment flags and child supervision. Remove `-shortest` for this finite owned PCM path. Do not use wallclock-at-pipe-read or `asetpts=PTS-STARTPTS` to manufacture synchronization.

### 3. Normalize PCM to the existing recording epoch

Raw PCM has no timestamps. Therefore timing must be resolved before bytes enter FFmpeg:

1. Start the endpoint client before WGC capture starts. Retain a rolling bounded pre-roll while no video epoch exists. Publish the chosen first-WGC QPC immediately when `stage_initial_source` establishes the existing CFR clock; never choose a new audio or video zero after graph construction.
2. For each valid packet, compute its target start sample from `(packet_qpc_100ns - video_epoch_qpc_100ns) * 48,000 / 10,000,000`, using checked signed rational arithmetic and nearest-ties-to-even absolute-boundary rounding. Trim pre-epoch samples. Preserve a real late start as counted prefix silence. Keep fractional/rounding state explicit; do not round and accumulate packet durations independently.
3. Place real discontinuities on that sample lattice: insert zeros for positive gaps, discard only the covered portion of an overlap, and count both. A nonmonotonic timestamp, unusable timestamp flag, arithmetic error, queue loss, or implausible clock jump is terminal. Never fall back to packet-arrival time or concatenate unknown coverage.
4. Do not react to each tiny QPC variation. Estimate residual phase and its slope from one-second windows within the same continuous capture generation, with a 1-ms deadband and one full window of sustained same-sign error before correction. Derive a signed correction budget from that averaged slope plus a bounded phase-restoring term, limited to 96 samples/second (2,000 ppm). Carry fractional budget between fixed 10-ms output blocks and apply at most one insertion/removal per output block, independently of input packet size/cadence. Stop phase restoration within 0.5 ms; reset history/budget after a true gap or endpoint generation change. Interpolate adjacent audio for a clock correction rather than insert a zero click. A phase step over 5 ms is a discontinuity, not drift. Record corrections separately from missing capture. Pure tests must prove alternating jitter causes no corrections, no oscillating correction bursts, and simulated ±1,000-ppm drift stays bounded at varied input packet sizes. If a device exceeds these bounds, fail explicitly rather than silently drift or add a new resampler/backend.
5. `SILENT` packets produce zeros for their exact coverage. An initialized, healthy loopback client may produce no packets when nothing is playing: an empty buffer alone is not a time watermark. On idle passes, obtain a valid monotonic [IAudioClock::GetPosition](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclock-getposition) position/QPC correlation, then drain packets again. Respect `GetFrequency` units; the position is not necessarily a 48-kHz sample count. Define a conservative covered-through QPC by subtracting a guard from that clock observation: the larger of 100 ms and the reported [maximum stream latency](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-getstreamlatency) plus actual endpoint-buffer duration plus two engine periods. Bound the guard to 500 ms; validate it with quiet-to-sound and delayed-packet fixtures for each tested endpoint format. Only emit idle zeros through that drained, conservative watermark, mapped to the existing sample lattice. `S_FALSE`, stale/invalid clock evidence or absent clock support cannot advance it; retain the interval within existing bounds, then fail if usable evidence cannot be obtained. A packet arriving behind committed idle coverage is a timing fault, never discarded new sound. Keep separate counters for observed silence, healthy-idle fill, startup gaps and device-recovery gaps.

Never write PCM beyond the exclusive end of committed video ticks: bytes already sent cannot be retracted at stop. Publish this monotonic shared sample credit without blocking video, and clip every pipe write, including a partial 10-ms block, to that credit. Cancellation and terminal errors must wake a writer waiting for credit. Construct/connect the pipe, spawn FFmpeg and construct NVENC without waiting for audio-read or mux-progress readiness; let both producers run before evaluating existing startup-ready gates. Explicit raw-audio format options and bounded input probing must allow the initial committed H.264 ticks to release the corresponding audio while FFmpeg opens its audio-first input. A cold quiet-start fixture must prove this has no probe/producer dependency cycle within the existing startup deadline.

Capture admission stays nonblocking. Reserve at most 8 MiB total for application-owned audio blocks, queues, pre-roll and normalization scratch, including the shared 15-second startup interval; steady queued PCM is limited to 500 ms. Overflow or sustained pipe backpressure fails the attempt. These are combined credit limits, not separate 8-MiB allowances per queue. Do not log or serialize every packet in production.

### 4. Replace only the audio endpoint on device changes

Register `IMMNotificationClient`; callbacks only set a coalesced dirty generation and signal the owner. Microsoft's [notification rules](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient) require nonblocking callbacks and prohibit unregistering or releasing the final MMDevice reference inside them. [Registration does not AddRef the callback](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-registerendpointnotificationcallback): the owner must hold its strong reference through successful unregistration. The callback/control/event state is independently owned and never references GPU/session memory. On failed unregister, disable its work, retain the registration owner/callback/event state in one bounded process-lifetime quarantine, and disable further native-audio sessions until relaunch. Record terminal cleanup failure; never release potentially registered callback memory or claim clean publication. Keep COM-affine cleanup on its owner or retain that owner when necessary; do not introduce unsafe `Send` wrappers to move COM pointers.

Use this bounded state flow: `Opening → Capturing → Reopening → Capturing`, or `Failed`, then `Stopping → Joined`. A matching default-render-role change, current endpoint removal/state/format change, or recoverable WASAPI invalidation schedules reevaluation. Ignore unrelated communications-role changes. Reread the default after registration and after reopening to close notification races; stale generations cannot install a superseded endpoint.

On a real change, stop/drain/release the old client, reacquire the current default, and initialize the same fixed format, following Microsoft's [invalid-device recovery](https://learn.microsoft.com/en-us/windows/win32/coreaudio/recovering-from-an-invalid-device-error). Keep the recording epoch, PCM pipe, AAC encoder, video encoder, and mux alive. Device sample positions reset per client generation and never become a recording clock.

Attempt immediately, then at most once per second within one five-second recovery interval measured from the first failure; repeated notifications do not extend it. Preserve the outage as explicitly missing capture, not healthy-idle silence. Use the new generation's first valid packet or drained clock watermark to bound the gap and resume at its real QPC offset; never advance recovery fill past unverified new-generation coverage. Until that evidence arrives, hold the interval within the five-second budget rather than fabricate a source timestamp from reopen receipt time. Require the device-loss fixture to prove video and mux progress remain bounded during this pause. Emit one transition warning and a recovery summary. No available endpoint, stopped audio service or unsupported format may become an endless silent success. Expiry, stop with unresolved timing, or nonrecoverable errors enter the existing terminal recording-error path and preserve the partial candidate. A later ordinary stop cannot erase that error.

### 5. Joined stop, truthful publication and compatibility

Once video tick admission ends, freeze its final committed frame count. At 60 FPS/48 kHz the audio limit is exactly `frames * 800`, checked for overflow. Request finish-at-that-boundary from the endpoint owner: allow at most one second for valid packets/drained clock coverage to reach it, then stop/drain the client and accepted audio. This covers the conservative delivery guard without inventing an unknown final tail. Emit no sample beyond the final credit, including partial output blocks; an unresolved final interval fails publication rather than receiving guessed padding. AAC priming/padding may extend coded coverage and must remain truthfully described by the existing media timeline.

Signal audio finish before draining NVENC. Keep the audio writer and video drain progressing concurrently; join both finite producers and close both inputs before waiting for FFmpeg exit. Failure/cancellation wakes both credit and pipe waits immediately. The GPU worker must not wait for a writer that is itself waiting for video EOF. Every initialization rollback, cancellation, audio failure, pipe disconnect and normal stop must terminate/join the audio workers and unregister notifications before releasing their shared state, except the explicit bounded registration quarantine on unregistration failure. Preserve first-error precedence and current partial/publication rules. Existing cooperative deadlines do not claim to terminate a stuck Windows/driver call.

Add a versioned optional typed `capture.audio` provenance/summary object in recorder metadata and its strict app reader/save mirror. Record native-system versus explicitly requested silence, fixed format, first/last valid packet QPC (nullable when no packet exists), input/output sample counts, generated silence by reason, overlap/clock corrections, maximum residual, final covered-through watermark, endpoint generation/recovery counts and terminal result. Keep it bounded; no packet ledger or new health sidecar in production. Absent data in older recordings stays legacy/unknown. The app must preserve this object on save; no new replay or export controls are required.

Windows default audio selection becomes a policy resolved per session, not a startup-discovered device name. Retain `LEAGUE_REPLAY_AUDIO_DEVICE=silent/none` as explicit silence; unset/default/system selects WASAPI. Reject old named-DirectShow overrides with a clear migration message instead of silently redirecting them. Leave non-Windows handling unchanged. Diagnostics should activate/test native loopback and distinguish healthy idle from captured packets, explicit silence and unavailable capture. Never claim successful audible capture solely from activation or a present AAC stream.

## Rejected alternatives and planning decisions

- More DirectShow name matching still depends on optional vendor capture devices and does not solve the failure.
- Process-loopback include/exclude is unnecessary for this track and retains the separate two-group capability/timing work.
- Replacing WGC/NVENC, adding an FFmpeg capture backend, moving video through CPU memory, separate audio files/post-recording remux, or a second encoder child adds unrelated ownership and finalization work.
- Timestamped NUT/Matroska transport or a new multimedia library could carry packet clocks, but a bounded native PCM normalizer is the smaller boundary here. Raw PCM without that normalizer is insufficient.
- Capture the engine's changing native format directly only if the approved fixed-format design is superseded; it cannot be silently substituted into a fixed raw-PCM FFmpeg input.

## Milestones and necessary code changes

1. **Audio policy and pure timing contract.** Change `AudioSource`/Windows discovery and argument tests in `encoder.rs`; add the checked packet-to-sample normalizer and bounded storage/control types. Add optional audio metadata types and the strict app reader/save round-trip. Verify deterministic timing and legacy compatibility before emitting new metadata.
2. **Native capture and transport.** Add required Windows feature flags in `recorder/Cargo.toml`, register the module in `native/mod.rs`, and implement thread-affine WASAPI, fixed-format negotiation, notification/recovery, private pipe and cancellable writer. Exercise the production audio module with generated endpoint tones independently of NVENC.
3. **Integrate the existing session/mux.** Wire ownership, early capture/epoch publication, readiness, failure evidence and joined stop in `native/session.rs`, `native/lifecycle.rs`, `native/mux.rs`, and `service.rs`. Keep the video algorithm and finalizer unchanged; add focused assertions preventing audio EOF from shortening video. Run the existing exact-CFR mux fixture.
4. **Focused Windows validation and documentation.** Extend existing generated fixture tools with one system-loopback scenario rather than create a new verification framework. Verify real A/V, recovery, idle and failure cleanup; update recorder documentation and the affected audio paragraphs in `windows-capture.md`, `recorder-lifecycle.md`, and diagnostics verification guidance. Record obtained results in the execution checkpoint, then concise feature evidence.

## Verification design

All fixtures use generated audio/windows and a sentinel-owned temporary library. Never play or modify a user's recordings as test data.

| Acceptance area | Proportionate proof |
| --- | --- |
| Actual sound, one track | A 10–15-second endpoint-only fixture runs the production audio owner and a generated stereo tone. Decode PCM and assert spectral presence/channel mapping; also test silent start followed by sound. Activation or stream existence alone cannot pass. |
| Clock mapping | Pure tests cover pre-roll/late starts, sample-boundary rounding, jitter, ±1,000-ppm drift, flagged/unflagged gaps, overlaps, invalid QPC, 24-hour arithmetic, delayed packets/idle clock watermarks, committed-video write credit and final `frames * 800` clamp. Require injected timestamp/offset errors, unusable final clock coverage and a writer cancelled during a credit wait to fail safely. |
| Real recording A/V | One 60-second generated WGC flash/tone recording with early/middle/late markers through the actual NVENC/mux path. Measure decoded markers using their media PTS and actual source/render QPC evidence, not tone-enqueue time or independent zeroing. Require mapping error ≤20 ms at each marker and ≤10 ms early-to-late change. A deliberate 40-ms audio skew must fail the analyzer. Inspect AAC priming/padding without shifting video zero. |
| Device changes | Switch the real default playback endpoint during a short generated recording when a second endpoint is available; include differing engine formats where available. Require one media identity, continued video/CFR, audible post-switch tone and a counted gap with resumed markers within the same timing limits. Deterministic controls cover event storms, changed-during-open, no device, service loss and five-second expiry. Unavailable hardware scenarios remain explicitly unverified. |
| Failure/lifecycle | Prove cold quiet-start probing and prompt stop after first ticks cannot deadlock or overrun the video audio-limit. Inject timestamp failure, queue overflow, stalled/disconnected audio pipe, cancellation during open/reopen and failed notification cleanup. Require first-error retention, bounded cancellation, joined producers/child and partial preservation; failed unregister retains only its bounded registration guard, prevents another audio session, and never frees callback/event state. Never canonical success after a terminal audio error. Reuse encoder/mux negative and window-recreation fixtures. |
| Media/compatibility | Exactly one H.264 High 1080p60 video stream plus one stereo AAC stream; full decode and unchanged exact scheduled/submitted/completed/muxed video reconciliation. Existing strict MP4 timing/probe/publication checks remain required. Old combined/silent metadata still loads; save preserves new provenance. Smoke replay and one existing export with generated output. |
| Cost and resources | One matched 60-second silent-mode versus WASAPI recording pair with identical generated video and tone playback. Sample recorder+mux CPU/private memory and audio thread/queue/handle peaks. Verify no busy wait, ≤8-MiB audio storage, ≤500-ms steady queue, no continuing handle/thread growth and no unexplained video-accounting regression. An incremental mean CPU cost over one machine-normalized percentage point requires investigation before completion. This bounded check is not a League FPS claim. |

Routine commands from repository root:

```powershell
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo fmt --manifest-path recorder/Cargo.toml -- --check
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run check --prefix app
npm run build --prefix app
python tools/replay_time/run_native_mux_av.py --runtime-root build/media-runtime/windows-x86_64 --pairs 3
cargo build --release --manifest-path recorder/Cargo.toml
npm run desktop:build --prefix app
python build/validate_features.py
git diff --check
```

Run the full canonical validator in `docs/development/VERIFICATION.md` as the handoff/completion authority if the existing helper does not implement its current invariants. The frontend baseline and fresh desktop build cover integration of the small strict-metadata reader change; no frontend feature or playback redesign is planned. Extend the existing native fixture with an explicit loopback option and document its exact command during implementation; do not claim a currently nonexistent command ran. The current native fixtures intentionally use silent audio.

Retain one bounded fixture result summary under ignored `build/` with runtime/hardware/endpoint format, packet/epoch/sample counters, decoded marker errors, failure/cleanup results and resource samples. Include invalid trials and limitations. Missing interactive audio or same-adapter NVENC is environment-blocked, not a timing pass. Use an NVIDIA-connected fixture window; an Intel-selected NVENC startup failure supplies no A/V evidence.

## Performance and reliability gates

No video CPU readback, per-frame audio work on GPU/callback threads, extra media process, runtime rebuild, unbounded queue, periodic device enumeration or per-packet production log is allowed. Endpoint notifications and finite recovery preserve the existing video session. A known audio failure prevents clean publication. Intentional silence and healthy quiet playback remain valid, explicitly distinguished cases.

The existing project video/resource invariants and applicable automated checks remain binding. This repair requires the short generated correctness/recovery/resource checks above; it does not create a new mandatory 30-minute soak, full League match or formal gameplay benchmark. General negligible-impact claims and the unchanged League frame/resource budgets remain owned by `QB-PERF-002`; separate-track capability remains owned by `QB-AUDIO-001`.
