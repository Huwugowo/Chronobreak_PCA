# QB-AUDIO-003 — Native system loopback with independent audio failure

## Purpose

Capture ordinary Windows playback as one synchronized system-audio track while protecting gameplay video. **Audio acquisition failures change audio availability and quality; they do not invalidate independently healthy video.** A genuine video capture, encoder, shared mux, storage or invalid-container failure still follows the existing recording failure path.

This design supersedes `qb-audio-003-wasapi-system-loopback.md` following the user's engineering review. Preserve that plan as history. The explicit requirement change also replaces its terminal-audio-error, mandatory idle-clock proof, custom drift-controller and exact PCM/video-credit requirements.

## Relevant planning-time architecture

- `recorder/src/encoder.rs` owns `AudioSource`, `detect_audio_source`, device-name matching and Windows audio arguments. Windows currently lists DirectShow capture devices once at service startup and falls back to `anullsrc`; this is not endpoint loopback.
- `recorder/src/native/session.rs` obtains the first real WGC QPC epoch before starting the mux/NVENC graph. The exact-CFR video clock, GPU conversion, NVENC admission/completion and frame reconciliation already work independently of audio acquisition.
- `recorder/src/native/mux.rs` starts one locked FFmpeg child: native Annex-B H.264 on stdin, selected audio as another input, video stream copy with the existing ordinal-to-CFR `setts` filter, AAC and fragmented MP4. Audio-first opening exists. The current `-shortest` and mandatory audio mapping need explicit finite/absent-audio variants. `validate_mux_only` currently forbids probe overrides as well as video capture/filter/encode work.
- Native startup readiness and the service watchdog use real WGC/encode/mux progress under existing deadlines. `RecordingEvidence.protocol_error` is fatal; audio-local outcomes must not enter it. Native finish currently drains NVENC, finishes FFmpeg, then closes WGC.
- `service.rs` always supplies `expected_audio_codec: Some("aac")` to the common finalizer. In `replay-time/src/lib.rs`, `None` means audio must be absent, not optional. Introduce a distinct optional-AAC expectation; do not reinterpret `None` or relax the video checks. `MediaTimelineV2` already represents no audio and independent finite audio coverage, capped to video for replay/clip authority.
- Recorder `CaptureMetadata` in `poller.rs` and the app's strict `CaptureMetadataDocument` in `app/src-tauri/src/library.rs` must both know any additive audio summary. The existing recorder-control `CaptureError` means the recording failed, so it is unsuitable for audio degradation. This repair reports through bounded warning logs, diagnostics and saved metadata without adding a new UI lifecycle.

Use the existing `windows` 0.62 bindings and exact packaged `MediaTools` pair. No runtime rebuild or alternative tool discovery is needed. Current generated native fixtures use intentional silence; existing TCP PCM/mux fixtures prove neither endpoint capture nor named-pipe behavior.

## Scope and non-goals

One default `eRender/eConsole` endpoint, fixed 48-kHz stereo PCM, one AAC track when available, one recording identity and one FFmpeg mux child. Preserve WGC → D3D11 → NVENC, video timestamps, fragmented MP4, strict validation and no-replace publication.

Separate League/everything-else tracks, process-loopback inclusion/exclusion, multiple endpoints, microphone audio, new replay/export controls, alternate video backends, post-recording audio remux and a custom resampler/PLL are outside this feature. Existing `silent/none` configuration remains explicit intentional silence. Non-Windows capture behavior stays unchanged.

## Exploration findings and principal risks

Microsoft's [endpoint loopback API](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording) captures a shared-mode render endpoint without requiring Stereo Mix. Event-driven loopback is supported on Chronobreak's Windows baseline. [GetBuffer](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer) provides a first-sample QPC value in 100-ns units; an empty buffer supplies no new sample/timestamp evidence. Neither pipe arrival time nor stream existence proves capture or synchronization.

The main integration risk is FFmpeg input opening/probing, not the Windows capture call. An audio-first pipe whose producer waits for committed video could create a circular wait. Remove that producer dependency and prove the actual transport before integrating WASAPI. FFmpeg also documents a ten-second default interleave allowance; an explicit smaller, nonzero limit is needed to bound starvation buffering. Zero does not disable that buffering. See [FFmpeg format options](https://www.ffmpeg.org/ffmpeg-formats.html).

Endpoint quietness, disconnection and timestamp anomalies are normal reliability concerns for a game recorder. Coverage uncertainty must remain visible without becoming a video failure. A short real-device quiet-to-sound test is necessary; deterministic timestamp tests cannot establish actual endpoint behavior.

## Chosen design and rationale

### 1. Separate video validity from audio outcome

| Condition | Audio action | Recording action |
| --- | --- | --- |
| Valid system packets | Timestamp and feed PCM/AAC | Publish strictly validated video and present audio |
| Healthy client with no packets | Supply inferred idle silence; retain uncertain coverage | Continue video; empty buffers alone are not a fault |
| No endpoint, stopped audio service or failed capture initialization | Mark unavailable; run bounded audio recovery while the PCM pump supplies counted silence | Continue video without waiting for endpoint readiness |
| Device change or invalidation | Replace only the endpoint client; fill/account for the missing interval | Keep the epoch, video graph, mux and media identity |
| Bad timestamps, capture queue loss or audio worker failure | Discard unusable samples, record degradation; recover or disable capture | Continue video with counted fill or finite audio |
| Dedicated audio transport setup/open fails before video production | Select video-only; if necessary reap the initial child and restart once without audio | Start video within the same startup deadline |
| Dedicated audio writer/pipe ends after spawn | Close only audio input; retain any valid finite AAC prefix | Continue video if the shared child/video path is healthy |
| Shared FFmpeg exit/failure, video failure, storage failure or invalid MP4 | Existing terminal handling | Preserve partial output; do not claim canonical success |

The normal transport stays open through recoverable endpoint outages, avoiding mux restarts. Silence generated for unavailable/uncertain capture is a delivery fallback, never evidence that sound was captured. Audio recovery expiry, invalid QPC and failed audio cleanup do not become `CaptureError`, process-generation capture suppression, or a fatal `protocol_error`. Preserve actual shared mux failures and video watchdogs; do not suppress them under an audio label.

### 2. One capture owner and a small PCM timeline pump

Add a Windows native audio module under `recorder/src/native/`, with small checked timing and summary types that can be tested independently. One dedicated MTA COM owner creates, uses and releases the enumerator, client and capture service. Select through `IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eConsole)`, initialize `IAudioClient` with shared-mode `LOOPBACK | EVENTCALLBACK | AUTOCONVERTPCM`, set its event and obtain `IAudioCaptureClient`. Use explicit float32 48-kHz stereo and the normal shared engine period. Windows supplies the required matrix/conversion; [conversion flags](https://learn.microsoft.com/en-us/windows/win32/coreaudio/audclnt-streamflags-xxx-constants) document the higher-cost default-quality SRC option. Select its use based on the short audible conversion check, without adding a second native resampling pipeline.

Drain available packets on capture events; copy into reusable bounded storage and promptly release buffers on that same thread. The [Rust binding](https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Media/Audio/struct.IAudioCaptureClient.html) is not `Send`/`Sync`. Keep COM pointers on their owner and use ordinary owned PCM/control data across the boundary. Do not introduce unsafe thread-safety wrappers.

Use a separate pump/writer thread because named-pipe backpressure must not delay packet release or device callbacks. It handles timestamp placement, fill and cancellable pipe I/O. This justifies at most two new threads; do not build a general audio executor. Audio callbacks and the existing GPU worker perform no audio processing or pipe I/O.

Bound all application-owned audio buffers, queue capacities, scratch and in-flight copies to 8 MiB combined, and steady queued PCM to 500 ms. Use fixed small blocks and reusable allocations, coalesced notifications and transition/terminal summaries. Queue overflow discards stale audio with explicit accounting; it cannot block video or grow storage. OS pipe buffers and FFmpeg memory are measured separately rather than hidden inside this application-memory claim.

### 3. Timestamp alignment without a custom clock controller

Share the existing first-WGC epoch as soon as it is established. Convert a valid packet anchor to a signed sample slot with checked rational arithmetic:

`sample_slot = nearest_ties_even((packet_qpc_100ns - wgc_epoch_100ns) * 48_000 / 10_000_000)`.

Trim samples before zero. Anchor a contiguous packet run using QPC, then retain its actual sample sequence and use device-position/sample-count continuity to detect gaps or overlap. Re-anchor after a real discontinuity, endpoint replacement or an idle interval already filled by the pump. Insert counted silence for a gap and trim overlapping/late data against the irrevocably emitted cursor. A resumed packet uses its valid QPC, never a fresh audio zero. Honor `SILENT` without dereferencing nonexistent sample data. A flagged invalid timestamp, impossible range or backward/inconsistent clock is an audio anomaly: discard the unusable packet and recover/degrade.

Do not re-edit every contiguous packet to chase QPC jitter. Record a compact maximum/last QPC-versus-sample residual so real drift is observable. No one-second estimator, ppm budget, interpolated sample slipping, adaptive rate loop or mandatory `IAudioClock` watermark belongs in this repair. If the real A/V proof reveals material drift, first establish its cause and propose a standard resampling correction as a separate design decision; do not invent a controller to make a test pass.

The pump advances in that same 100-ns QPC domain. Share the exact QPC/`Instant` anchor pair established through `instant_from_qpc_100ns` for the native video clock; do not choose a separate process-start `Instant` as audio zero. With that shared anchor, use one small delivery allowance (initially 100 ms) for packet arrival. This allowance delays delivery, not media PTS, and is not a correctness proof or video failure deadline. For due intervals without packets, generate bounded zero blocks classified as intentional silence, inferred idle, unavailable capture or dropped/invalid audio. Empty buffers from an otherwise live client do not trigger reopen merely for being quiet. Count inferred idle separately from known capture loss; uncertainty is explicit.

Critically, the pump never needs video-frame credit, NVENC completion or mux readiness to produce initial PCM. Its pacing bounds how far it can advance, and its buffering bounds how much it can retain. Endpoint readiness is also absent from the video startup gate.

### 4. Prove and retain the simplest mux transport

Keep H.264 on stdin and use one randomly named, current-user-only local Windows named pipe for `f32le -ar 48000 -ac 2` input. Create its secured server before FFmpeg spawn; reject remote clients and use first-instance protection. Connection and writes must support cancellation and finite waits. Preserve explicit input maps, `-c:v copy`, the exact existing video `setts` filter, negative AAC priming semantics and fragment/flush flags.

Open PCM first, with its paced producer able to connect and supply silence independently of video admission. Before starting NVENC/video production, give connection and a complete initial PCM block sufficient for AAC packetization a short setup sub-deadline, capped at one second and the remaining startup budget. A partial write is not handshake success. This handshake requires no endpoint packets or video progress. A setup/connect/first-write failure selects video-only: cancel the pipe, terminate/reap the not-yet-video-producing child if it exists, and spawn the video-only plan once within the same absolute startup deadline. There is at most one live FFmpeg child and one such pre-video retry; do not restart the mux after video production begins. Preserve the original WGC epoch and current source through this choice. The transport fixture must exercise failure after spawn but before connection/first write, not just pipe creation failure.

After handshake, start video production without waiting for FFmpeg input probing to finish. Remove `-shortest`; audio EOF cannot signal video EOF. A video-only plan omits the audio input, map and encoder arguments entirely. An optional map alone cannot make a failed input harmless. Device readiness never gates this handshake; healthy audio capture is not a condition of video startup.

Set an explicit nonzero `max_interleave_delta`, initially 250,000 microseconds, and retain finite FFmpeg/pipe queue capacities. The focused transport proof must verify progress and bounded memory both during quiet startup and after audio EOF; the option alone is not a wall-clock or memory guarantee.

Begin the proof with the existing probe-option bans intact and explicit raw input formats. Do not claim bounded probing from nonexistent flags. If the locked runtime cannot meet the existing startup deadline that way, the proof may justify only a narrow input-scoped probe-option allowlist for these known pipe formats, with exact values, startup/media evidence and guard regression tests. Keep all bans on video work and timestamp-algorithm changes. Do not weaken the entire validator or integrate an unproved transport; an incompatible design requires a revised handoff.

At stop, stop capture admission/retries and request finite PCM drain/EOF while video drains. Use the final video boundary to trim unwritten PCM where available, without a new write-credit protocol. Already written audio and AAC priming/padding retain their actual coverage; the existing timeline caps replay authority to video. An unavailable or late audio tail may end early. Never wait for a real packet or clock proof to authorize video publication. Cancel a stalled audio write and close that input within its bound; close both producers before the existing FFmpeg wait/reap.

### 5. Recover one endpoint safely

Resolve the default endpoint per recording, rather than selecting a DirectShow device at daemon startup. `silent/none` stays intentional silence; unset/default/system requests WASAPI. Reject obsolete named-DirectShow overrides with a migration message instead of quietly remapping them. An unavailable requested system source is a nonfatal audio outcome.

Use one coalesced desired-endpoint generation and one retry scheduler. Relevant default-render-role changes, current-device state/format changes or API invalidation cause release/reacquisition of only the audio client, following [Microsoft's recovery sequence](https://learn.microsoft.com/en-us/windows/win32/coreaudio/recovering-from-an-invalid-device-error). Reread the default after registering/opening so a stale open cannot replace a newer endpoint. Device sample positions restart per client; the recording epoch never does.

Attempt promptly and no faster than once per second during a five-second initial recovery window. After that, report unavailable and use a low-frequency retry, initially every 30 seconds, until stop or a relevant device notification. Coalesce notification storms and rate-limit retries; notifications do not extend a fast-retry window indefinitely. These limits bound audio recovery work and status changes, never video survival. Quiet clients can use a low-frequency health check for service/invalidation errors; packet absence alone is not an error. No healthy-state periodic device enumeration is needed.

Notification callbacks only update bounded state and signal the owner. Microsoft's [callback rules](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient) prohibit blocking/reentrant unregister. The owner retains a strong callback reference through [unregistration](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-registerendpointnotificationcallback). If safe unregister/release cannot be established, retain only that independent registration owner/callback/event state, bounded to one retained owner for the process, and disable subsequent audio capture until relaunch. The retained state owns no GPU, video pipe, mux or recording output. This precise lifetime safeguard is not a general quarantine subsystem and does not disable subsequent video recordings.

Audio cancellation, worker panic and teardown errors are reported separately. Ordinary cleanup is owned and joined with finite waits; existing cooperative Windows-call limitations are not a hard driver-hang guarantee. Audio state must be independently owned so a stalled audio owner cannot hold video finalization hostage. If an owner must outlive its cancellation wait, the same single-owner retention bound applies and new audio is disabled; the pump can still close independently. Do not free potentially live callback state to meet a timeout.

### 6. Strict publication and compact truthful evidence

Replace the finalizer's ambiguous `Option<String>` with one explicit `AudioExpectation` enum: `RequiredCodec(codec)`, `OptionalCodec(codec)` and `Absent`. Required accepts exactly one valid matching stream; optional accepts no audio or one valid matching stream; absent accepts no audio. Extra streams and malformed present streams fail in every mode. Windows system capture uses optional AAC; intentional-silence and existing callers retain their deliberate expectations. Present audio still needs valid codec, sample rate, time base and coverage; malformed audio/container data is not relabeled as harmless capture degradation. Exact nonempty video grid/count, mux success, zero video origin, bounded common probing and no-replace publication remain mandatory. Do not change the persisted `MediaTimelineV2` schema.

Update all expectation constructors, including `recorder/src/finalizer/recovery.rs`. Recovery already has a staged, fully validated terminal timeline and requires exact equality on re-probe. Preserve that stronger contract: reconstruct `RequiredCodec` for its recorded present audio and `Absent` for its recorded absent audio, never optional based on an unvalidated probe or `capture.audio` status. Do not add a second persisted expected-media authority or let optional capture erase a previously validated track during recovery. Add a focused generated pending-publication regression for video-only and finite AAC outcomes; existing receipt, generation and no-overwrite protections remain unchanged.

Keep outer `CaptureMetadata.schema_version: 1` and add `audio: Option<AudioCaptureSummaryV1>` with its own nested version 1 in both recorder and strict app metadata types. Keep source mode, outcome (captured, idle/uncertain, degraded, unavailable or intentional silence), actual input/output counts, fill counts by reason, discarded frames/discontinuities, reopen count, first/last valid QPC and compact residual/last-error summary. A retained finite AAC prefix remains valid media but has a degraded capture outcome. All-fallback output cannot claim successful native sound capture. Missing legacy summaries remain unknown. Preserve the summary unchanged during app save and reject unknown nested fields/versions consistently, including staged recovery metadata; it is provenance, not another media authority.

Emit one warning on loss/degradation and a recovery/final summary, without per-packet logs. Diagnostics distinguish packets observed, activated-but-idle/uncertain, intentional silence and unavailable capture. Do not use a present AAC track or successful activation as the audible proof. UI presentation of that saved status can be added separately; do not repurpose the existing fatal capture-error state.

## Rejected alternatives and planning decisions

- More DirectShow name matching cannot provide ordinary endpoint loopback reliably.
- Perfect audio or failed recording contradicts the user's explicit video-preservation requirement.
- A new audio clock authority, idle watermark proof, exact PCM/video write credits and a custom drift controller add failure modes without endpoint evidence. Keep QPC anchoring, continuity accounting and observable residuals.
- Always aborting or restarting FFmpeg on device loss would discard healthy video or require segmentation/remux. Keep the PCM input alive through capture outages; allow finite audio or video-only when transport cannot continue.
- A second encoder process, native AAC stack, timestamped multimedia transport or alternate video backend is unnecessary for one track. The early proof is the boundary for challenging the chosen raw-PCM transport.

## Milestones and necessary code changes

1. **Transport feasibility first.** Extend the existing generated mux fixture with a private named-pipe PCM producer and the proposed finite/absent-audio argument variants, isolated from production startup. Use the exact shipped runtime. Prove quiet cold start, concurrent H.264/PCM input, audio EOF while video continues, post-spawn connect/first-write failure followed by the single pre-video video-only retry, cancellation and clean exit under the existing startup/finish limits. Retain exact argv, progress, frame count, decode and bounded-buffer observations. No WASAPI or GPU integration until this passes; do not treat the proof as endpoint or A/V evidence.
2. **Native audio owner and small timing boundary.** Add necessary Windows Audio/COM feature flags in `recorder/Cargo.toml` and the module in `native/mod.rs`; implement the thread-affine client, bounded queue/pump, QPC anchor/continuity logic, notification/retry and independent audio outcome. Add focused timing/lifecycle tests, including empty buffers and bad timestamps. Exercise actual tones/quiet-to-sound through the production audio module without NVENC.
3. **Integrate without changing video authority.** Update `encoder.rs` policy, `native/mux.rs`, `native/session.rs`, relevant lifecycle diagnostics and `service.rs` ownership/finalization. Add explicit audio expectations in `replay-time` and all callers including `finalizer/recovery.rs`, compact metadata in `poller.rs` and the strict app mirror/save path. Prove audio-local errors never become fatal video evidence; preserve genuine mux/video errors, finite cleanup and existing exact-CFR assertions.
4. **Focused acceptance and handoff.** Run the five proofs below, relevant release build and applicable integration checks. Update only affected audio/ownership paragraphs in recorder documentation, `windows-capture.md`, `recorder-lifecycle.md` and diagnostics guidance to describe obtained behavior. Keep detailed results/limitations in the execution checkpoint and concise evidence in the feature entry.

## Verification design

Use generated tones/windows and sentinel-owned temporary outputs only. Reuse existing fixture tools; no new benchmark or verification framework.

| Proof | Required evidence |
| --- | --- |
| Actual sound | One short generated stereo tone sequence, including quiet start then sound. Decode captured output and check tone presence and channel assignment. Exercise ordinary shared playback, with conversion if available. Activation/AAC presence is insufficient. |
| A/V alignment | One short actual WGC/NVENC recording with early and late flash/tone markers. Compare decoded media markers with observed WGC/render QPC, not enqueue times or separately zeroed tracks. Require at most 20 ms marker mapping error and at most 10 ms early-to-late change. Reuse the analyzer's deliberately skewed negative control; do not build a drift simulator matrix. |
| Lifecycle and fault isolation | The early transport proof covers cold quiet start/EOF/cancellation. In one generated native recording, inject an audio-local timestamp/capture failure, keep video running beyond the fault, stop normally and require canonical publication, full video frame reconciliation/full decode and degraded audio evidence. Keep existing genuine video/mux negative checks. |
| Device change | One actual default-endpoint switch when a second endpoint exists: same video/mux identity, post-switch sound at its QPC offset, accounted outage. Focused deterministic cases cover unavailable endpoint, service invalidation/retry and bounded callback/stop handling. Mark unavailable hardware scenarios explicitly unverified. |
| Observed cost | A short matched intentional-silence/system-capture fixture pair with identical video and tone playback. Retain recorder+FFmpeg CPU/private memory, audio threads, queue/storage and handle observations. Enforce finite bounds/no busy wait or growth; investigate an obvious regression rather than impose an invented CPU percentage gate. No League FPS claim. |

Add only focused affected-code tests: shared QPC arithmetic/pre-roll, run continuity/gap/overlap, quiet fill, invalid timing and late data; bounded retry/cancellation; required/optional/absent audio with strict malformed-media rejection and staged recovery equality; and metadata legacy-without-audio/current-with-audio, unknown nested-field/version rejection and unchanged save round-trip. Test the policy boundary rather than mirror every helper. The existing packaged-runtime mux analyzer remains reusable:

```powershell
python tools/replay_time/run_native_mux_av.py --runtime-root build/media-runtime/windows-x86_64 --pairs 1
cargo test --manifest-path replay-time/Cargo.toml
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo build --release --manifest-path recorder/Cargo.toml
npm run desktop:build --prefix app
```

During each unit run its targeted checks. Run the applicable routine automated group from `docs/development/VERIFICATION.md`, replay-time checks and canonical validator once at integration/completion, not after every milestone. Record exact new fixture invocations when their interfaces exist. A generated replay/export smoke can reuse the accepted artifact and existing tools; it does not authorize playback/export redesign or a separate broad interactive matrix.

Retain a compact generated result under ignored `build/`: runtime/hardware/endpoint format, QPC/sample/fill/error summary, early/late decoded marker errors, published-video-after-fault result, cleanup and resource observations. Preserve failed attempts. Missing interactive audio or same-adapter NVENC is environment-blocked, not a capture/timing pass.

## Performance and reliability gates

One live shared FFmpeg child, with at most one video-only retry before video production; no audio work on video callbacks/GPU worker; no video CPU readback; normal audio period; at most two added threads; at most 8 MiB application-owned audio storage and 500 ms queued PCM; finite cancelled transport and bounded retry/retention. Audio-device availability never gates video admission/readiness, media identity, intentional stop or publication of an otherwise valid video. Optional transport setup falls through to video-only within the existing startup deadline. Mux/video/container integrity remains mandatory.

This feature requires the focused real-audio, alignment, fault-survival, available device-change and resource evidence above. It adds no long soak, full League match, formal gameplay benchmark or arbitrary CPU-percentage gate. Broader capture-performance acceptance remains with `QB-PERF-002`; separate tracks remain with `QB-AUDIO-001`.
