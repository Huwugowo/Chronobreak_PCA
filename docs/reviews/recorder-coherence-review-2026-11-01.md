# Recorder coherence review — 2026-11-01

## Conclusion

Chronobreak needs a coherent recording-session contract before more isolated lifecycle corrections. The central defect is an ownership mismatch: the process is documented as the recording lifetime authority, but the native graph and service actually terminate the attempt on selected-window loss. Publication then tries to reconstruct whether that early stop was legitimate from a single process-exit check. Strengthening that check prevents false completion; it cannot make normal window-first shutdown reliable.

The recommended direction is a process-owned session with an attachable window source and a continuous media timeline. Keep the existing encoder, muxer, bounded resources, strict media validation, exact process identity, and safe teardown. Change the boundaries that currently make a window's lifetime equal the session's lifetime. Make gameplay telemetry optional to capture after an explicitly verified startup-policy change. Match History should contain published usable replays; recorder attempts and failures belong in recorder status and recovery tooling.

This is an audit and proposed direction, not a finalized ExecPlan or an implementation/completion claim. Existing feature states and immutable plans remain unchanged. The user explicitly requested stepping back across the recorder and rejects empty/unplayable attempt cards in Match History; that product direction supersedes the previous incomplete-card UX for future work.

## Scope, versions and evidence quality

- GitHub's latest commit was verified as [`a1df7df8dda5c2df50af0d6d968d7b04c19a5ad4`](https://github.com/Huwugowo/Chronobreak_PCA/commit/a1df7df8dda5c2df50af0d6d968d7b04c19a5ad4), committed October 31 at 23:20:29 Europe/Paris. It adds incident evidence and documentation, not product code. Its runtime source baseline is parent `a6a3068`.
- This checkout is at local `8b1c422`, a later process-identity correction. The audit distinguishes its retained-handle protection from the snapshot-based checks used during the incident. The recorded executable hash identifies the inspected image; the incident does not establish reproducible-build equivalence with source.
- Existing uncommitted `feature-list.json`, `recorder/examples/native_nvenc_probe.rs`, audio probes and audio results were preserved. They include ongoing QB-AUDIO-001 feasibility work and are not this audit's implementation.
- Additional concurrent replay/index/UI working-tree changes appeared during the audit and were left untouched. This document does not claim to verify those changes.
- The complete incident report, 125-line sanitized log, manifest, process observations, event summary and media probe were read. Copies retrieved from the commit are under ignored `build/recorder-audit-2026-11-01/`; all four evidence hashes in the manifest match after the manifest's specified LF normalization.
- Product/workflow/verification contracts, the relevant feature definitions, startup checkpoint before its immutable plan, recorder lifecycle architecture, and delegated native/library/data/recovery evidence were reviewed. Independent read-only scouts examined lifecycle, native media, and app/library integration. Root checked critical source ranges and reconciled contradictory scout claims against code.

The committed probe is a container/stream inspection, not a full decode, visual-continuity analysis, A/V synchronization test, performance comparison, or proof that the whole match was recorded. This audit did not open, rename, repair or delete the user's recording.

## What the incident establishes

Times are Europe/Paris; the raw log uses UTC.

| Observation | Evidence and implication |
| --- | --- |
| First process observed 22:59:04; replacement observed 22:59:46 | PID 524 never reached native startup. PID 228 became the recorded generation. The cause of the replacement is unknown. |
| Admission 23:00:51 at gameTime 36.624569 s | Passive readiness had returned unavailable signal, unstable clock and unavailable window. Native GPU initialization had not yet occurred. Opening gameplay was missed; the aggregate decisions cannot apportion that delay precisely. |
| Native initialization about 607 ms; recording announced 23:00:54 | The minute-scale delay was before native initialization. Announcement also waits for source/encode/mux readiness, so it is not an exact first-video-frame timestamp. |
| Terminal output 43,420 frames / 1,098,648,666 bytes | FFprobe found 1920×1080 H.264 at 60/1 FPS, video duration 723.666667 s and AAC/container duration 723.669333 s. A substantial candidate exists. |
| Native cleanup succeeded at 23:12:55.527; target loss handled 23:12:56.152 | Worker success and terminal counters did not constitute semantic game-end evidence. The post-teardown process-exit check failed. |
| Error 23:12:56.168 | No canonical video/metadata publication; partial preserved; post-game job cancelled. There is no later publication reconsideration for that consumed attempt. |
| League absent at approximately 23:13:35 | No exact exit timestamp. Consistent with window-first shutdown, also consistent with other endings; it does not prove the missing tail contained no gameplay. |
| Silent audio selected at service initialization | The AAC stream is expected silence, not evidence that game audio was captured. |
| No watchdog stalls/backend exits before target loss; zero pool recreations | No logged encoder stall or resize rebuild explains this ending. This is not proof of unique/smooth frames or negligible game impact. |

The loading/alt-tab black screen remains causally unresolved. For the first process, the log excludes an already-active WGC/NVENC graph as the immediate mechanism: capture had not been initialized. Passive monitoring still existed, and no absent/passive comparison was performed. Neither recorder innocence nor recorder causation follows.

Sources: [incident report](https://github.com/Huwugowo/Chronobreak_PCA/blob/a1df7df/docs/reviews/recorder-live-incident-2026-10-31.md), [raw log](https://github.com/Huwugowo/Chronobreak_PCA/blob/a1df7df/docs/reviews/evidence/recorder-2026-10-31/recorder.log), [probe](https://github.com/Huwugowo/Chronobreak_PCA/blob/a1df7df/docs/reviews/evidence/recorder-2026-10-31/media-probe.json), [manifest and limits](https://github.com/Huwugowo/Chronobreak_PCA/blob/a1df7df/docs/reviews/evidence/recorder-2026-10-31/manifest.json).

## Code audit

### 1. Process authority is real for discovery but incomplete for ownership

`watcher.rs` discovers the executable/process generation; this is not merely a window-title trigger. Local HEAD additionally retains a query/synchronize handle to the original Windows process and validates its exact creation FILETIME. Only the signaled original handle proves termination. This is a useful correction to snapshot-based absence: enumeration failure, PID reuse, a different selected process, or failed queries must not masquerade as exit. Retain it (`recorder/src/watcher.rs:67,199`). Microsoft documents the process object's signaled state at termination and the validity of retained handles after exit ([process termination](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process)).

However, `NativeSession::run_until_stopped` turns `target_closed` into its own stop boundary (`native/session.rs:355`). The service independently takes `active` on target-validation error, stops/finalizes it, and blocks another attempt for that process (`service.rs:792-813`). Thus the session is gone while its owning process can still be alive.

`NativeRecorderSession::stop_inner` joins the worker, checks worker success and terminal evidence, then invokes an `FnOnce` process check (`native/lifecycle.rs:385-451`). The local handle correction makes that answer authoritative; it remains one instantaneous answer. There is no retained candidate awaiting another observation. A later exit cannot turn this attempt into a replay.

This limitation is documented, not an accidental undocumented timeout. Nevertheless, it fails the desired ordinary-session contract. Current tests correctly enforce that chosen conservative limitation rather than establish that ordinary game endings succeed.

### 2. A delayed exit check alone would preserve the design error

Suppose the window disappears at T1, capture drains, and the process exits at T2. A later successful process wait establishes T2. It cannot prove that meaningful gameplay did not continue between T1 and T2, or that a replacement window was not available during that interval. Any grace length introduces a new success/failure threshold without resolving coverage.

The stronger solution is to keep the session and its timeline alive between T1 and T2. Window loss changes frame acquisition; original-process termination requests final media shutdown. An intentional recorder quit is also a valid explicit stop, with a truthful recording boundary. A real encoder/mux/protocol failure remains a failure. None requires `GameEnd` as a publication prerequisite.

Microsoft explicitly notes that a target window may be seamlessly replaced and raise WGC `Closed` ([WGC Closed](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscaptureitem.closed?view=winrt-26100)). That does not establish that recreation occurred in this incident, but it confirms why HWND closure is an unsuitable process-lifetime authority.

### 3. Most temporary-source continuity machinery already exists

The CFR loop already runs independently of new source frames. If no eligible frame is available it repeats the persistent last-good GPU snapshot (`native/session.rs:331-395,538-612`). Tiny/unusable content is discarded without replacing that image. Hidden/minimized visibility suspends only source freshness expectations; encode/mux progress remains required (`service.rs:424-456,821-839`). There is no current black-frame synthesis policy.

Resize work is already narrow: recreate the WGC pool, then size-dependent converter/input state after a coherent new surface; retain fixed output/NVENC surfaces. There is no reason to replace that with a universal restart system. The October run's one 2×2 rejection and zero recreations do not validate restoration or long-minimize behavior.

The needed source/session ownership split is substantive but bounded. Today `NativeWgcSource` owns the WGC item/pool/session **and** the MTA guard/D3D11 device from which converter and encoder resources are built (`native/source.rs:34-49,121-141,170-188`). Closing that whole source is not a safe way to preserve the encoder. Put worker-affine MTA/device ownership in the long-lived session; make WGC item/pool/callbacks an attachable leaf on that same device. Each attach transaction must confirm that the pinned original process remains unsignaled and validate a fresh HWND snapshot against that active generation before activating callbacks; matching a PID alone is insufficient. Adapter changes remain an explicit unsupported/failure boundary until designed and verified.

Reattachment must reuse callback-safe close, complete quiescence and one-source-at-a-time ownership. If quiescence is unproven, the existing intentional full-stack retention prevents use-after-free (`native/source.rs:144-181`); that is a terminal condition, not permission to pile on new sources. Preserve cancellation, bounded cadence and finite resources.

Finite textures/queues do not bound output storage. A process can remain alive indefinitely after losing its window, and continuous freeze encoding still consumes disk. Before finalizing the design, specify source-unavailable status, bounded reacquisition cadence, user-stop behavior and a resource/storage limit policy. A chosen unavailable-duration or output limit must end as an honest interruption with preserved media, never as proof that the match completed. Ordinary short window transitions must not inherit the current instantaneous terminal rejection.

### 4. Readiness is a product trade-off that lacks its required empirical proof

The passive gate requires three advancing GameStats samples spanning four seconds, gameTime ≥5 seconds, clock consistency and a stable usable window; missing readiness has no timeout admission (`launch_readiness.rs:74-175`). Its approved notes explicitly permit losing opening seconds or the whole recording. This is stronger than requiring a safe, supported capture target.

The safety motivation is legitimate. Its effectiveness against League launch interference is not established: QB-CAP-006's required repeated absent/passive/native Practice Tool comparison is still pending. A stable game clock is not proof that a subsequent GPU/driver call is safe, and temporary telemetry failure is not proof capture is unsafe. The separate active poller already calibrates metadata timing after video readiness.

The preferred end state is capture admission based on exact process identity, supported/stable physical window/device requirements and successful bounded native startup. GameStats then supplies metadata/calibration, independently degradable. This requires an explicit superseding startup design and controlled comparison; do not silently delete the current gate or claim that early capture is already proved safe. At minimum, log the concrete rejection reason, first usable window, first returned frame and first video timestamp so measured opening loss has an explanation.

### 5. Preserve finalization; fix who requests it

The native and common finalizers provide real value: explicit error propagation, terminal capture/mux proof, frame reconciliation, independent media/timeline validation, nonempty output, private staging and canonical publication (`native/lifecycle.rs:399-451`; `finalizer.rs:361-497`; `service.rs:1688-1774`). Do not publish by renaming this incident's partial, accepting HWND closure as completion, or weakening media checks.

The layering to simplify is the native finalizer asking an instantaneous process question after the graph has already ended. The service should own the process observation and stop reason; native code should report source availability or real failure and prove media drain correctness. One normal session-stop decision followed by one joined validation/publication path is easier to reason about than several callers tearing down and subsequently testing whether teardown was legitimate.

Media validity, process termination and semantic match completeness are different claims. A valid recording need not contain a `GameEnd` event or successful EOG data. Likewise a decodable file does not prove complete match coverage. Preserve truthful captured boundaries and stop reasons; do not create a new mandatory gameplay-data gate at the end.

### 6. Match History currently lists attempts as well as replays

`scan_games` pushes a summary for every valid-name game directory (`app/src-tauri/src/library.rs:373-400`). Missing/invalid metadata produces `incomplete_summary`, including when there is no canonical video (`:603-613,670-691`). Consequently this incident directory is eligible for an incomplete row when that root is scanned. The partial filename alone does **not** exclude the directory. Its absence from the reported UI remains unexplained by the committed evidence, which contains no contemporaneous app snapshot/root/refresh trace.

With valid metadata, the summary's video flag only checks nonzero reported length (`:603-667`); replay opening additionally requires a regular nonempty file (`:403-424`). Neither is a fresh decode test. Normal trust comes from recorder publication validation. A metadata-valid directory can therefore still present a broken replay if its media is removed/replaced/corrupted later.

Apply the user's explicit product rule: default Match History membership requires a canonically published replay with strict valid core metadata and a regular nonempty canonical video. Missing League stats/results alone must not hide playable video. Exclude unvalidated, failed and recovery-pending attempts from history while preserving every file and diagnostic. Surface recorder failure/processing in the existing recorder-status/control path. Handle later playback failure there or through a clear replay error, not a fabricated empty match.

Reuse schema-v2 media identity and existing validated publication/recovery semantics. Do not add another per-attempt health file or require FFprobe/full decode on every library refresh. If mutation detection needs strengthening, bind/invalidate a bounded media-validation result using existing media/file identity as a separately scoped design. No lightweight directory scan can guarantee future decodability after arbitrary file mutation.

This is a deliberate change from the older LIB-008/LIB-023 incomplete-card behavior; record it explicitly in the implementation feature rather than quietly changing old completed acceptance history. The storage gap already exists: snapshot and standalone totals sum only canonical `video.mp4` lengths (`library.rs:306-308,484-486,603-606`); this incident's 1.10 GB partial contributes zero video bytes, and automatic deletion skips incomplete entries (`:585-587`). A bounded maintenance inventory must account for retained partials separately from visible history. Filtering cards alone would conceal this existing disk use further. Preserve deletion/recovery safety; do not introduce automatic partial deletion as an incidental fix.

### 7. Optional match data is already mostly separated correctly

Live journal, provisional LCU context and EOG confirmation are independently owned. Optional context installation failure warns and allows media publication; EOG confirmation does not gate it (`service.rs:1600-1647,1713-1755`). Preserve this separation. Optional filesystem work is awaited and can affect end-to-end latency, but missing match data was not this publication veto.

The app and recorder share configuration contracts and a control plane; the app's observer/spawner does not own capture lifetime. Different roots or observing a different executable remain possible integration questions, not established incident causes. Verify one installed end-to-end path rather than assuming them from the standalone executable path.

There is a concrete missing refresh/status connection. `App.tsx:174-198` refreshes on returning to the library/settings close; other paths cover initial load and app mutations, but there is no external-recorder publication subscription or library polling in `libraryController.ts`. The recorder supervisor observes/caches status without emitting a frontend publication event. The recorder UI polls only while its Settings component is mounted (`components/RecorderSettings.tsx:6-19`). The projection also coalesces Starting/Finalizing under Available, although the raw owner status is retained (`recorder_supervisor.rs:88-113`). Thus an already-open library need not show a newly completed recording, and recording failures are not surfaced in the main library view. These are static integration findings, not proof of what this particular app instance displayed.

QB-LIB-024 needs an explicit publication invalidation/refresh contract with initial/reconnection reconciliation, and a compact recorder-status surface outside Settings. Reuse the control owner; do not create another health authority or rescan on every unchanged status sample. A sampled Recording→Idle edge alone can miss a short whole session between polls: current `recorder-control::Status` has no publication generation (`recorder-control/src/lib.rs:55-64`). Choose a bounded publication-change signal or filesystem invalidation in the design rather than assuming the existing cache already supplies it.

### 8. Audio support is incomplete at the acquisition boundary

The current Windows discovery parses FFmpeg DirectShow device names and prefers names resembling Stereo Mix or WASAPI loopback (`encoder.rs:1116-1146`). If none is found, it explicitly selects silent `anullsrc` (`:443-496,1266-1286`). This is not a general Windows render-loopback capability check. A silent AAC track satisfies container shape while missing the user's expected game sound.

The direction is real validated loopback acquisition with explicit availability and timing, plus a visible degraded-audio status. Existing QB-AUDIO-001 work is already investigating native process-tree include/exclude capture and a common playback graph; continue that bounded feature rather than inventing a competing fallback. Its generated feasibility evidence is not yet production capture/synchronization/performance proof. Avoid making video wait for optional advanced audio separation. Microsoft's native [application loopback sample](https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/) is the relevant API model; compatibility must follow actual validated configurations.

### 9. Frame duplication is an unresolved quality warning

25,481 / 43,420 ≈58.7% is a real ratio of recorded counters, not a measured percentage of dropped League frames. `duplicate_ticks` means a tick reused the last snapshot because no newer eligible snapshot had been staged (`native/clock.rs:141-176`; `session.rs:538-612`). The logged `source_frames_surfaced` counts callback/handoff admission, not successful converter snapshot selection. Terminal admission was 41,072; successful stage/copy counts and worker discards are not in this incident log.

The pending-frame reducer preserves eligible captures rather than blindly overwriting them with future frames; relevant tests exist (`native/capture.rs:96-166,1268-1417`). The audit found no demonstrated selection bug there. However, approximately 57 admitted frames/s alongside approximately 35 duplicate ticks/s warrants explanation. It is not justified to declare the 60-FPS file smooth merely because encode/mux counters advance.

Expose existing snapshot-copy/worker-discard/handoff counters, maximum/recent source age and freeze duration in bounded aggregate telemetry. Add meaningful 57/59-Hz and adverse delivery-timing cases; obtain a bounded sanitized timestamp trace if the counters still disagree. Compare decoded output changes with a known changing fixture and, with user authorization, read-only incident media analysis. Frame hashes alone cannot distinguish legitimate static scenes from incorrect selection. A full decode proves decodability, not semantic scene freshness. Keep actual League performance claims under QB-PERF-002.

## Proposed operating contract

One service session owns one exact process generation. A window is a replaceable source within it. The encoder/mux own one monotonic continuous recording timeline. Optional League data describes that recording. The library consumes published replay bundles.

| Event | Intended action |
| --- | --- |
| Process appears, no usable window yet | Wait passively; no pretend recording and no GPU initialization loop. |
| Supported window becomes usable | Validate process/window/adapter; start one bounded native graph; announce recording only on real media progress. |
| Focus loss, occlusion, minimize or transient unusable frames | Keep session/audio/encoder/mux and timeline; repeat last valid image while necessary; continue downstream watchdogs. |
| Selected HWND closes while process remains alive | Detach that WGC leaf safely; keep the session and last-image timeline; look for a valid replacement HWND of the same pinned process with bounded cadence. |
| Replacement HWND on same supported device | Attach one fresh WGC leaf; resume source images without a new recording epoch or overlapping source owners. |
| Original process handle signals exit | Stop once, drain/join once, validate once, publish once, refresh the app. Window-close ordering no longer determines success. |
| Watcher selects another still-live process | Do not reinterpret that selection as old-process exit. Resolve the pinned active owner and allow a new generation only through the explicit session handoff. |
| Recorder deliberately quits | Joined intentional stop with truthful captured boundary; no requirement that League also exit. |
| Real device/encoder/mux/protocol failure or unprovable callback cleanup | Preserve partial, report one actionable failure, suppress unsafe reconstruction; later process exit does not erase the failure. |
| Publication succeeds | Usable replay enters Match History. Optional telemetry/results may arrive or remain unavailable. |
| Capture/publication fails | Keep files/recovery evidence; show recorder status/error outside Match History. |

This does not require a persisted cross-system recording-health state machine, broad automatic restarts, multiple competing polling owners, display-mode manipulation, injection, or a new capture backend. Explicit detached-source availability is enough; avoid naming many states unless each owns a concrete resource or observable transition.

## Engineering direction and release evidence

Use one reviewed lifecycle design for the process/source ownership change, with implementation milestones that share the same end-to-end contract. Do not treat a grace-period patch as the finalization solution and then bolt reattachment onto its aftermath.

1. **Make the desired journey executable before changing production behavior.** Extend a dedicated child-process/window fixture through the actual service, watcher, native graph, finalizer and library path. The parent process must be able to outlive its window deterministically. Assert usable publication and source resumption, not only rejection/partial preservation.
2. **Implement process-owned session/source lifetime.** Retain exact-handle identity, move device/MTA ownership above the attachable WGC leaf, keep fixed encoder/mux/timeline ownership, and centralize stop classification. Cover close/exit ordering and same-process HWND recreation together. Preserve all real failure and integrity checks.
3. **Reassess admission under controlled launches.** Obtain the unchanged absent/passive/current-capture comparison, then compare the proposed window/backend-driven policy in Practice Tool. Measure first usable window, first source frame, first video frame, opening loss and game responsiveness. Finalize an explicit superseding QB-CAP-006 design before replacing its telemetry gate; do not add more unexplained readiness timers.
4. **Apply the usable-history contract and end-to-end status/refresh.** Use existing publication and control surfaces. Retain maintenance visibility/storage accounting without exposing empty match cards. This is a small coordinated integration task, not a new recording-health subsystem.
5. **Close quality gaps on existing tracks.** Explain duplication with actual source-selection evidence and finish real audio acquisition/timing under QB-AUDIO-001. A silent or visibly stuttering video is not a successful user journey merely because its MP4 passed structural validation.

The acceptance matrix should be small, realistic and decisive:

| Scenario | Required outcome |
| --- | --- |
| Ordinary launch, play, normal exit | Early capture under the verified policy; sound availability truthful; one playable replay without an error card. |
| Loading/gameplay alt-tab; minimize longer than 15 seconds; restore | No game interference; same recording/timeline; frozen source only while unavailable; fresh source resumes; downstream stall detection remains active. |
| Window closes, process exits before/during/after current teardown duration | Same successful lifecycle outcome, independent of scheduling order; no missing timeline interval. |
| Window disappears and is recreated while process remains alive | Same session/encoder/mux epoch; only associated same-device source reattached; fresh final footage is actually decoded/checked. |
| Window disappears and process remains alive without replacement | Explicit source-unavailable status, owned session and bounded reacquisition; no declaration of match completion or uncontrolled GPU retry loop. Explicit user stop, real process exit or a declared interruption/resource limit remains decisive. |
| Rapid process restart or observer selection change | No PID/epoch mixing, no unsafe old-session publication, no missed fresh generation after a joined handoff. |
| Live/LCU unavailable, frozen or late | Under the approved replacement admission policy, video works; metadata is honestly unavailable/unmapped; no fabricated match result. |
| Encoder/mux/storage failure; callback quiescence failure | Actionable failure, preserved media, no false success, bounded safe ownership, no broken history row. |
| App open during recording, app closed then reopened | Same configured recorder/library root, publication appears on refresh, failures remain outside history, no dependency on the app staying open. |
| Source capture rate below/near/above 60 and delayed delivery | Truthful snapshot/duplicate/discard accounting and demonstrated output freshness; not just a nominal 60-FPS header. |

Run appropriate static/component checks after implementation. Use generated media and test libraries for destructive/failure scenarios. Then obtain a real supported-hardware Practice Tool run and the project's representative long-session evidence, with early/middle/late picture/sound/timing checks. A generated fixture cannot certify League launch non-interference or negligible overhead; the formal benchmark remains separate.

## Verification performed in this audit

- Verified current GitHub latest commit and inspected its complete added diagnostic evidence.
- Verified the four committed evidence hashes against the manifest.
- Audited exact code paths and test expectations on the local tree, distinguishing `a6a3068` from `8b1c422`; reconciled source-continuity and library-scan claims with direct source reads.
- Consulted primary Microsoft WGC/process/audio documentation for the OS contracts above.
- Recorded the newly identified work as draft/not-started QB-CAP-009 (process/source session ownership) and QB-LIB-024 (usable published history). No existing acceptance criteria, lifecycle statuses or immutable plans were rewritten. The authoritative schema/graph/lifecycle/checkpoint validation passed: 107 roadmap items and 26 plan/checkpoint pairs. Scoped `git diff --check` passed.
- Attempted `cargo test --manifest-path recorder/Cargo.toml --lib native::lifecycle::tests`; the session's command-logging hook could not create its required file under read-only `.codex/logs`, before test execution. A second executable spelling hit the same hook restriction. **No test pass is claimed.** No product code was changed, no full suite was run, and no live capture/full decode/performance check was performed.

Relevant existing work remains CAP-002 (progress/finalization safeguards), CAP-006 (startup), CAP-003/004 (recovery/integrity), PERF-002 (capture-performance proof), AUDIO-001 (real two-group audio), and the existing library/control features. This review supplies design input; their acceptance and completion gates remain authoritative until explicitly superseded through the normal workflow.

## Open-source comparison — 2026-11-01

The user requested an independent check against open-source League recorders before advancing the proposed sequence. Three read-only scouts examined League-facing projects; root checked the mature OBS WGC implementation and critical Segra paths. This is additional design evidence, not runtime verification or a finalized implementation plan. No downloaded project was executed.

The comparison supports separating recording lifetime, capture-source availability and output completion. It does **not** establish that an existing League recorder has already verified our complete proposed contract. Several projects have weaknesses resembling ours. RePlays and Segra are treated as one related family, and their OBS backend is not counted as an independent capture implementation.

### Source evidence and limits

| Project / inspected scope | What the source establishes | What it does not establish |
| --- | --- | --- |
| **OBS Studio 32.0.2**, `c025f210d36ada93c6b9ef2affd0f671b34c9775` | The WGC Closed callback marks its source inactive. Window capture frees/reinitializes that leaf, searches for replacement windows at a one-second cadence, and defers initialization while an existing target is hidden/minimized. These paths do not stop recording output. WGC uses OBS's higher-level graphics device. | OBS is a general capture engine, not a League session controller. Its matching rules are not our process-generation contract; source reacquisition does not prove game non-interference, Vanguard compatibility, or our proposed frozen-image behavior. |
| **windows-record**, revision `2c05d8b7d8431425e5497ba735903cca3feb6ed1` identified by the scout; the public library associated with Ascent/RiftRec | It periodically searches again when a window is lost rather than declaring recording complete. Focus loss produces blank frames. Capture is Desktop Duplication; outer code supplies stop. | This is not the full product application. Its misleadingly named process selector matches window titles, not an exact process generation. Retry logic is not proof of an uninterrupted media clock or safe reattachment to the correct game. |
| **Segra / RePlays family**, retrieved default-branch source snapshots | Segra detects game processes and has separate video fallback and process-audio ownership. Its replay-buffer save waits for mux confirmation before metadata generation and history refresh. Conversely, its Game Capture log handler may request recording stop after window disappearance and a one-second hook check. | This is a useful counterexample to adopting peers wholesale. Hook messages are not process termination. The save path is not a proof that every library entry is playable. |
| **RecorderTool**, `70da48126487da220866969f98daa363319a82d0` | Debounced executable-name polling controls FFmpeg recording; startup looks for a visible PID-owned window. | Window lookup is a one-shot operation: a process observed before its window can miss capture. Title-based GDI capture, video-only output and filename-based history provide little assurance about League/DirectX reliability. |
| **League-Highlights**, `909a486f81f790e1ed45d6046cb0b48aa8afa2ce` | It prefers Desktop Duplication over window GDI, validates candidate window size/process, has a separate WASAPI audio buffer, and probes exported clips. | Video uses rolling video-only segments and fixed initial capture geometry. LCU gameflow participates in recording control. This does not demonstrate WGC rebinding, unified A/V continuity or independence from League telemetry. |

Primary source locations:

- OBS: [`wc_tick` / `wc_render`](https://github.com/obsproject/obs-studio/blob/c025f210d36ada93c6b9ef2affd0f671b34c9775/plugins/win-capture/window-capture.c#L547), [`winrt_capture::on_closed` / device ownership](https://github.com/obsproject/obs-studio/blob/c025f210d36ada93c6b9ef2affd0f671b34c9775/libobs-winrt/winrt-capture.cpp#L109).
- windows-record: [window retry and focus handling](https://github.com/judehek/windows-record/blob/2c05d8b7d8431425e5497ba735903cca3feb6ed1/src/capture/video.rs#L159), [title-based selection](https://github.com/judehek/windows-record/blob/2c05d8b7d8431425e5497ba735903cca3feb6ed1/src/capture/window.rs#L36), [external stop/join](https://github.com/judehek/windows-record/blob/2c05d8b7d8431425e5497ba735903cca3feb6ed1/src/recorder/inner.rs#L486).
- Segra: [`OBSService` source/finalization/log handler](https://github.com/Segergren/Segra/blob/main/Backend/Recorder/OBSService.cs), [`GameDetectionService`](https://github.com/Segergren/Segra/blob/main/Backend/Games/GameDetectionService.cs). RePlays [nightly release history](https://github.com/lulzsun/RePlays/releases/tag/nightly) records the shared maintainer's contributions.
- RecorderTool: [monitor](https://github.com/Razzolate99/RecorderTool/blob/70da48126487da220866969f98daa363319a82d0/src-tauri/src/process/monitor.rs), [start/stop service](https://github.com/Razzolate99/RecorderTool/blob/70da48126487da220866969f98daa363319a82d0/src-tauri/src/recorder/service.rs).
- League-Highlights: [capture backend and segmented video](https://github.com/Kirbyy1/League-Highlights/blob/909a486f81f790e1ed45d6046cb0b48aa8afa2ce/app/services/video_recorder.py), [window selection](https://github.com/Kirbyy1/League-Highlights/blob/909a486f81f790e1ed45d6046cb0b48aa8afa2ce/app/services/league_window.py), [audio acquisition](https://github.com/Kirbyy1/League-Highlights/blob/909a486f81f790e1ed45d6046cb0b48aa8afa2ce/app/services/audio_loopback.py).

Source-retrieval limits matter: Segra's indexed HTML and raw default-branch snapshots differ, and retrieval of its pinned release files failed. The observations above concern the shown code paths, not a claim about the exact behavior of its latest shipped binary. Its visible v1.9.6 release identifies `76002ba`, but that release identity is **not** used as the source pin for these findings. The smaller League projects' narrow implementations offer design clues, not comparable reliability evidence. League Director and other ROFL camera/export tools were excluded as evidence for automatic live-game recording.

### Effect on the proposed direction

1. **Keep the session/source split.** OBS supplies the strongest implementation precedent, including graphics-device ownership above the disposable WGC source. Preserve our strict original-process identity rather than copying executable/title matching. The source may fail or disappear without announcing session completion.
2. **Keep a separate completion/publication boundary.** Segra's save flow reinforces that recording stop and usable-output availability are different events. Chronobreak should retain its stronger media checks and publish/refresh only after success.
3. **Do not derive startup policy from popularity or absence of a clock gate.** These projects provide no controlled evidence that our Live Client gate prevents interference, nor that removing it is safe on the supported League configuration. Keep the planned absent/passive/current/proposed comparison. Minimize/invisibility checks concern source attachability; they do not justify ending the recording.
4. **Use external examples to sharpen our own fixture.** Prove that a process appearing before its window is eventually recorded, that window loss/recreation preserves one session and fresh later footage, and that finalization completes independently of close/exit ordering. A fixed retry delay, nonempty file or nominal FPS header is insufficient evidence.

The accepted sequence remains appropriate: one coherent ownership design, generated end-to-end lifecycle evidence, the ownership/finalization change, measured startup correction, usable-library/status integration, and picture/audio quality validation. No backend rewrite, weaker integrity rule or claim of Vanguard approval follows from this comparison.
