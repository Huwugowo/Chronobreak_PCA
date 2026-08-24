# Chronobreak canonical repository reconciliation

Date: 2026-08-24

Status: point-in-time audit and integration plan; not a competing backlog

Canonical work state after reconciliation remains `feature-list.json`.

## Executive decision

The canonical repository should use the whole-application repository as its chassis and semantically integrate the useful recorder divergence into it. The recorder-development repository is not a replacement application repository, and the August 21 replay handoff is not a greenfield rewrite specification.

The three inputs resolve as follows:

| Input | Audited identity | Authority after reconciliation |
| --- | --- | --- |
| Whole application | `Huwugowo/Chronobreak_PCA`, `main` at `0c0f274` | Canonical product chassis: Tauri/Solid app, recorder integration, filesystem contracts, roadmap, build and verification tooling |
| Recorder development | `Huwugowo/Chronobreak`, `pc-b/qb-perf-005-native` at local `c86bc6f`, five commits ahead of its remote | Source of recorder changes and non-League evidence to transplant or adapt, not a whole-product replacement |
| Replay/review handoff | `chronobreak-replay-canonical-codex-handoff-final-v2.md`, dated 2026-08-21 | Latest replay/review requirements, decisions, defaults, experiments, deferrals, and explicitly open specifications |

The intended first reconciled baseline is:

1. the current whole application and its stable filesystem/app behavior;
2. the existing optimized FFmpeg/WGC path retained as the default, vendor-neutral reference path;
3. the provisional native WGC/D3D11/direct-NVENC path integrated behind the developer selector;
4. the r6 media runtime contract integrated through the existing shared runtime mechanism;
5. both recorder backends retained until valid League evidence supports a backend decision;
6. replay evolution kept WebView-first and driven by real-media measurements;
7. active documentation rewritten to describe Chronobreak without PC-A/PC-B language or stale QueueBack/League Replay prose.

No broad implementation should begin from the current repository state. The tree first needs a controlled recorder integration and state/documentation reconciliation. In particular, current code, active plans, feature state, and architecture prose disagree materially:

- whole-app code already implements most of the optimized WGC/FFmpeg milestones, but the active `QB-PERF-002` ExecPlan still marks Milestones 1-6 unimplemented;
- the whole-app runtime lock is r5, active architecture prose says r3, and recorder development uses r6;
- `feature-list.json` truthfully leaves `QB-PERF-002` in progress, but its evidence and linked plan have not absorbed the newer recorder work;
- the recorder allocates same-second collision directories such as `<timestamp>-1`, while the app accepts only all-digit game IDs and therefore does not list those bundles;
- public/product prose alternates among Chronobreak, QueueBack, and League Replay;
- active architecture documents still point readers to files under `Old_spec/` as if they were current specifications.

This report records repository truth before those conflicts are edited.

## Audit boundary and evidence rules

The audit inspected the tracked whole-app tree, the recorder-development working tree including unpushed commits and untracked files, the active and historical documentation, `feature-list.json`, relevant Rust/TypeScript source, and both user-supplied documents.

Repository state and actual source outrank prose about already-implemented behavior. A current recording produced by the reconciled recorder will outrank historical media assumptions, but no such full recording is tracked in the whole-app repository. The only tracked MP4 is the small HEVC capability probe, so container/GOP/B-frame/audio conclusions that need a real current recording remain measurement work.

Existing evidence documents are treated as claims with a stated scope. This audit does not relabel synthetic or non-League evidence as League evidence and does not claim that unrun broad test suites passed.

## A. Current-state architecture map

### A.1 Repository and canonical state

The whole-app snapshot has 140 tracked files:

| Area | Tracked files | Current role |
| --- | ---: | --- |
| `app/` | 57 | SolidJS frontend and Tauri Rust host |
| `recorder/` | 19 | Separate automatic recorder process |
| `media-runtime/` | 11 | Shared immutable FFmpeg/ffprobe resolver and lock |
| `tools/` | 17 | Capture benchmarking and media-runtime maintenance |
| `docs/` | 20 | Product, architecture, verification, performance, and plans |
| `Old_spec/` | 6 | Historical specifications that are currently too easy to mistake for active requirements |
| root | 10 | Roadmap/schema, agent workflow, progress pointer, empirical Live Client fixture, and redundant archives |

The application stack is Tauri 2 with a Rust 2024 host, SolidJS 1.9.14, Vite 8.2.0, and TypeScript 7.0.2. The audited `main` working tree was clean at `0c0f274`; that commit only adjusts generated-artifact ignores. Its immediately preceding substantive snapshot, `7eac1f4`, is explicitly described by its own commit message as an everything-push whose stability was uncertain. Treat the tree as the required whole-product chassis, not as a verified stable release tag.

`feature-list.json` currently contains 50 items: 9 epics and 41 concrete features. Among concrete features, 13 are `done`, one (`QB-PERF-002`) is `in-progress`, 21 are ready/not-started, and 6 are draft/not-started. Six epics are marked in progress and three are not started. This is useful canonical work structure, but its recorder evidence and plan links need reconciliation before it can represent the newly integrated baseline.

### A.2 Runtime/process boundaries

```text
League of Legends process
        |
        | PID/HWND lifecycle + Live Client HTTP data
        v
separate Rust recorder process
        |  exact-HWND video + one current audio source
        |  atomic JSON persistence + fragmented MP4 finalization
        v
user-selected local media library
  games/<timestamp[-suffix]>/
    video.mp4
    game_log.json
    metadata.json
  clips/
    <game_timestamp>_<clip_timestamp>.mp4
    <game_timestamp>_<clip_timestamp>.jpg
        ^
        | filesystem scan; no database or recorder/app IPC
        |
Tauri Rust host
  config and retention
  library/probe readers
  ephemeral loopback byte-range server
  Data Dragon cache
  FFmpeg/ffprobe-backed export and thumbnails
        ^
        | Tauri commands + local HTTP media URLs
        |
SolidJS/system-WebView frontend
  match library
  persistent <video> replay surface
  timeline/event filters and clip range UI
  export workflow
```

The absence of a database or long-lived recorder-to-app IPC is deliberate and coherent with the local-first product. The recording bundle is the process boundary.

### A.3 Recorder in the whole-app snapshot

The live Windows path is already the optimized external-FFmpeg design, not the older GDI design described by stale plans:

```text
exact League HWND
  -> WGC/gfxcapture on explicit D3D11 adapter
  -> D3D11 BGRA frames
  -> scale_d3d11 to NV12
  -> same-adapter NVENC, AMF, or QSV candidate
  -> 60 fps CFR H.264/HEVC + AAC
  -> fragmented MP4
```

Important current properties:

- League process presence is the video lifecycle authority. `GameEnd` and Live Client readiness do not stop or start capture.
- Startup waits for a visible League HWND, maps it to the DXGI adapter containing the largest window intersection, and declares readiness only after first-frame/QPC and advancing encode/mux evidence.
- Candidate selection is same-adapter and bounded. Live startup explicitly fails with “no display/GDI fallback was attempted” when candidates fail.
- The code still contains a `DesktopRegion`/`gdigrab` branch, but it is used by the diagnostics/profile path rather than as an automatic production recording fallback. It should not be confused with the live capture path.
- Full video frames remain GPU-resident in the optimized path; the code and metadata label `host_readback=false`.
- The recorder resolves the pinned media runtime once. Production does not trust PATH FFmpeg.
- The first WGC frame QPC timestamp is converted into the Rust monotonic/wall-clock epoch and passed to the Live Client poller.
- Windows audio is one DirectShow source selected from an explicit override or detected “Stereo Mix”/WASAPI-loopback-like device. If none exists, the recorder writes a silent stereo AAC track. This is system audio, not a League/Other two-stem contract.
- Polling and video are failure-isolated. Video can remain valid when Live Client data degrades.
- `game_log.json` is atomically rewritten during capture. `metadata.json` is written after stop. Existing app behavior treats these release-era files, rather than a separate recording-state file, as the bundle contract.
- Unsuccessful capture output is preserved as partial media; successful private output becomes canonical `video.mp4`.

The current local synchronization model is internally coherent but incomplete as a cross-system specification. After five advancing, clock-consistent `/gamestats` samples, the poller derives `game_start_video_offset_ms`. Events receive precomputed `video_time_ms`; replay snapshots use `game_start_video_offset_ms + game_time_ms`. This answers today’s app navigation needs, but it does not formally map encoded PTS, replay-index time, WebView time, and export time.

### A.4 Recorder in the intended reconciled baseline

The intended baseline adds a second implementation behind the same service/storage/poller contract:

| Path | Scope | Baseline disposition |
| --- | --- | --- |
| FFmpeg/WGC/D3D11 | Vendor-neutral external FFmpeg path with NVENC/AMF/QSV capability planning | Keep; default and reference path until valid comparison |
| Native WGC/D3D11/NVENC | In-process WGC, conversion, and direct NVENC; FFmpeg retained for audio encoding and fragmented-MP4 muxing | Integrate as provisional, developer-selectable NVIDIA path |

The native path is not GPU-agnostic. It is currently fixed to High/H.264, 1920x1080, 60 fps, direct NVENC. AMD/AMF, Intel/QSV, and native HEVC are not implemented by it. The whole-app FFmpeg path therefore remains required even apart from the missing League decision.

### A.5 Recording bundle and media library

The app scans `games/` and `clips/` on demand. It validates timestamp and clip names before joining paths. It does not use a ReplayIndex.

There is a concrete producer/consumer mismatch that must be fixed during reconciliation. `create_game_directory` creates `<timestamp>`, then `<timestamp>-1`, `<timestamp>-2`, and so on to avoid same-second collisions. The app’s `valid_timestamp` accepts only ASCII digits, so every suffixed bundle is skipped by the library and cannot participate in clip filenames. This is a latent restart/collision data-visibility defect, not an intended format difference. Choose and test one canonical bundle-ID grammar, update recorder/app/clip validators together, and preserve compatibility with already-created suffixed directories.

Current library behavior is tolerant but not a complete degraded-artifact contract:

- a directory without parseable metadata is listed as `incomplete` with unknown game fields;
- a nonempty `video.mp4` can still be marked available;
- opening a replay requires video, parseable metadata with nonzero FPS, and parseable/defaultable game log data;
- age retention skips saved games and incomplete games;
- clip deletion removes the MP4 and optional JPEG;
- there is no versioned cleanup/rebuild policy for future replay indexes, thumbnail atlases, waveforms, or representative images.

### A.6 Tauri host and local media delivery

The Tauri host binds an Axum server to a random port on `127.0.0.1`. It exposes fixed routes for game video, clips, built-in/imported music preview, the HEVC probe, and Data Dragon assets. It supports GET/HEAD and single browser byte ranges, streams only the requested span, and records request/range/byte/completion/cancellation counters.

The current route-name validation prevents path-component traversal for games and clips. Imported music is registered behind a short-lived in-memory token and must have an approved extension. These are sensible prototype safeguards.

The production security contract remains open because:

- the loopback server has no per-run bearer secret or origin check;
- it returns `Access-Control-Allow-Origin: *`;
- CSP allows any `http://127.0.0.1:*` origin for connect/media/image;
- routes open files constructed below configured roots but do not establish a documented canonical-root/handle-based least-privilege contract;
- no threat model defines whether another local process/page reaching the port is in scope.

### A.7 SolidJS replay path

The viewer already follows several important handoff principles:

- one persistent `<video>` element owns decode, buffering, playback, and authoritative seeking;
- `requestVideoFrameCallback` is the preferred visible-frame clock, with animation-frame/timeupdate fallback;
- presented media time is kept local to the viewer rather than pushed into global application state;
- seek requests use a latest-wins queue with at most one active seek and a 100 ms dispatch interval;
- a 1.5 second seek timeout enters bounded recovery; two recovery attempts inside ten seconds lead to a degraded preview with manual retry;
- dev diagnostics expose presented FPS, dropped/total frames, seek latency/state, JS heap, and loopback-server metrics;
- the timeline sorts normalized events by `video_time_ms`, supports player filters and event selection, and drives clip-range creation;
- clip endpoints are aligned to recording frames and enforce a five-second minimum.

The current viewer does not implement:

- the required `0.25x / 0.5x / 1x / 2x / 4x / 8x` speed ladder;
- a narrow reusable logical playback boundary;
- thumbnail-first scrubbing or a uniform thumbnail atlas;
- ReplayIndex loading or validation;
- independent League/Other audio controls or a synchronized two-stem playback layer;
- a committed reproducible open/seek/scrub/resource benchmark harness.

The current 100 ms seek interval is an implementation choice, not a product constant. It should be measured once thumbnails exist rather than copied into a new architecture as an invariant.

### A.8 Clip/export path

Clip/export is correctly owned by Rust/FFmpeg rather than the WebView. Current exports use private staging names, sequential preset production, progress reporting, thumbnail generation, atomic-ish final renames, and hardware encoder preference with libx264 fallback.

Every shipped export currently performs a full H.264/AAC re-encode. The command always builds a video filter, trims/volumes or mixes audio, maps filtered streams, and selects NVENC/AMF/QSV/VideoToolbox/libx264. There is no video stream-copy or video-copy/audio-reencode hybrid fast path.

The current H.264 High + AAC stereo + `avc1` + faststart behavior is a useful implemented compatibility baseline, not the final exported-media contract left open by the handoff.

## B. Recorder divergence reconciliation

### B.1 Divergence identity and preservation requirement

The recorder-development branch is:

```text
repository: Huwugowo/Chronobreak
branch:     pc-b/qb-perf-005-native
HEAD:       c86bc6f Close full-ring redesign gate
remote:     origin/pc-b/qb-perf-005-native at 73c2838
state:      ahead 5; untracked recorder/src.zip
```

The five local-only commits are material and must be preserved before any cleanup or destructive Git operation:

1. `21ffa82` Complete native audit follow-up
2. `2bf8000` Plan post-audit recorder follow-up
3. `5ba8c36` Instrument native mux output stalls
4. `bc39591` Make native startup cleanup abort-safe
5. `c86bc6f` Close full-ring redesign gate

Together they change 23 files by approximately +1,935/-508 lines. They add or revise Packages 8-11 evidence, native lifecycle/mux/capture/NVENC behavior, service/runtime behavior, probes, and the active follow-up plan. The untracked `recorder/src.zip` is a byte-for-byte duplicate of the current recorder source and must not be used as an integration input.

Before integration, create a durable backup reference for `c86bc6f`—preferably a pushed branch or an external Git bundle—and record its full hash in the canonical integration ExecPlan. Do not rely on the current machine’s unpushed branch as the only copy.

### B.2 Architectural divergence

The recorder-development tree contains a native module root plus eleven focused implementation modules:

```text
capture -> clock -> convert -> encode -> mux
   |          |         |         |       |
  WGC       exact      D3D11     NVENC   FFmpeg audio + fMP4
              CFR      BGRA/NV12

lifecycle / session / source / d3d11 / nvenc / winrt
```

It also contains five focused probe examples, native-backend runners, NVENC ABI layout probes, and a developer selector:

```text
QUEUEBACK_WINDOWS_RECORDER_BACKEND=ffmpeg  (default)
QUEUEBACK_WINDOWS_RECORDER_BACKEND=native
```

The selector is deliberately developer-only, with FFmpeg as the default until the real gate is passed.

Native bounded-flow invariants are explicit:

| Resource/boundary | Capacity/behavior |
| --- | --- |
| WGC frame pool | 2 |
| Callback-to-worker handoff | 1, nonblocking `try_send`; intentional drop on full |
| Worker pending source | 1 latest frame |
| NV12/NVENC slots | 4 |
| Maximum encoder in flight | 4 |
| GPU immediate-context owner | one worker thread |
| Full-ring retry | 1 ms; Package 11 gate did not justify replacing it |

The callback never waits for the worker, encoder, mux, disk, or async runtime. The media clock is transactional: a CFR tick is committed only after the encoder accepts the corresponding frame.

### B.3 Integration disposition by component

| Component/change | Disposition | Required adaptation or protection |
| --- | --- | --- |
| Whole-app process watcher, config, tray, storage, metadata, app compatibility | Preserve as canonical integration behavior | Merge recorder changes into these contracts; do not overwrite the directory wholesale; fix the suffixed-bundle-ID reader mismatch |
| Whole-app FFmpeg/WGC path | Preserve | Remains default, vendor-neutral reference and AMF/QSV implementation |
| Native `capture/clock/convert/d3d11/encode/lifecycle/mux/nvenc/session/source/winrt` | Transplant and adapt | Put behind the shared service/session interface; retain exact bounds and failure semantics |
| Developer backend selector | Transplant | Default to FFmpeg; no automatic winner selection and no user-facing support claim yet |
| Package 0 observability | Keep | Reconcile counters with existing `RecordingEvidence` and additive capture metadata |
| Package 1 invariant D3D11 state | Keep | Preserve GPU-only conversion and validate against whole-app device ownership |
| Package 2 control-plane caching | Keep | Semantically merge with current HWND/PID/adapter validation rather than duplicating it |
| Package 3 startup recovery | Keep with scope | Retry only pre-publication dynamic startup failures; active-session restart stays deferred |
| Package 4 source coalescing | Keep | Preserve first-frame tick-zero authority and capacity-one latest-frame behavior |
| Package 5 transactional ticks | Keep | Preserve bounded catch-up and exact duration behavior |
| Package 6 poller persistence coalescing | Keep, then measure | Merge into canonical poller; residual rewrite amplification needs real-match evidence |
| Package 7 non-League acceptance | Evidence to retain, not code package | Consolidate conclusions and raw-root references; do not copy every handoff/evidence document |
| Package 8 audit follow-up | Keep | Nonblocking live mux size, callback waiter only during quiescence, one real NVENC session for capability validation, Tokio workers 16 to 2 |
| Package 9 mux observability/failure seam | Keep | Retain bounded writer/flush timing and feature-gated deterministic stall injection |
| Package 10 lifecycle remediation | Keep | Native startup cancellation must cooperatively join; document hard driver-call containment gap |
| Package 11 full-ring gate | Keep as evidence-only “not triggered” | Do not add notification complexity; production retry remains unchanged |
| Package 12 redundant steady-state `eventdata` request | Still pending | Implement after semantic integration; preserve initial events and make the one-second event loop sole steady-state event authority |
| Native probes/fixture runners | Selectively transplant | Retain the smallest reproducible fixture/failure/A-B set and integrate commands into canonical verification |
| NVENC header/layout reference | Retain only if reproducibility/licensing is explicit | The native code uses manually defined ABI layouts verified by these probes; keep exact pinned header, license/provenance, and runnable probe if that design remains |
| PC-B handoffs, worktree notes, per-package transitional plans | Do not copy wholesale | Reconcile surviving facts into canonical architecture, evidence, and one integration plan |

### B.4 Media runtime reconciliation

The whole application embeds r5; recorder development embeds r6. r6 is not merely a renamed lock:

- runtime ID and diagnostics ABI move from QueueBack 5 to 6;
- compiler/toolchain inputs move from GCC 14.2-era MSYS2 packages to GCC 16.1-era packages;
- `--extra-libs=-lstdc++` and `--enable-ffnvcodec` become locked requirements;
- FFmpeg/ffprobe hashes and sizes change;
- provenance describes an isolated repository-owned MSYS2 root and updated x264/oneVPL inputs.

The integration must update the whole shared `media-runtime/` contract, preparation/build/staging scripts, app bundle staging, recorder and app embedded-lock tests, and verification together. Copying only `runtime-lock.json` would create an invalid mixed runtime.

The runtime should continue to be one shared mechanism used by both app and recorder. Native video still needs the pinned FFmpeg process for current audio acquisition/encoding and fragmented-MP4 muxing, so native integration is not a reason to delete the runtime.

### B.5 Existing non-League evidence and its limits

The strongest existing recorder-development evidence is useful but not a backend-selection result:

- two-order 240-second synthetic A/B: native averaged 4.281 CPU-seconds versus 11.344 for FFmpeg/WGC (62.3% lower) and 159.91 MiB versus 235.62 MiB peak combined private memory (32.1% lower);
- two-order matched 60-second Package 7 A/B: native was 0.328765 and 0.431116 machine-CPU percentage points lower and used about 161 MiB rather than about 235-240 MiB combined private memory;
- 240-second native soak: 14,400 scheduled/submitted/completed/muxed frames, exact 240-second A/V duration, stable handles/threads/memory, and full decode;
- lifecycle/failure fixtures cover resize, minimize/restore, occlusion, target close, encoder stall/failure, mux death, and partial-output outcomes;
- Package 8 proves a deterministic 14-thread reduction and a small startup improvement; its short recording CPU result is explicitly not conclusive;
- Package 9 exposes mux backpressure truthfully;
- Package 11 found no normal full-ring failure and therefore made no production synchronization change.

All of this used non-League fixtures. It neither proves negligible League impact nor settles image quality, actual game-motion pacing, audio behavior, or a final backend choice. The native files were about 19% lower bitrate in the preliminary fixture without an objective/subjective quality comparison.

The native mux currently gives raw Annex-B H.264 to FFmpeg, which stream-copies video and synthesizes CFR timestamps. Accepted files decode with exact counts and fixed 120-frame keyframe cadence, but the pinned mux logs a deprecation warning about unset raw-H.264 packet timestamps. A timestamp-aware mux experiment remains separate future work.

### B.6 Previously blocked empirical fixture

The recorder-development tree’s root fixture was a three-byte `{}` placeholder, but the whole application contains the real 236,168-byte `live-client-capture-20260730-110603.json` with 25 samples.

This audit ran the focused parser test against that real fixture in both code lines:

| Tree | Command | Result |
| --- | --- | --- |
| Whole-app chassis | `cargo test --manifest-path recorder/Cargo.toml focused_snapshot_parts_remain_parseable_from_empirical_capture -- --nocapture` | 1 passed, 0 failed, 49 filtered |
| Temporary clean recorder-development clone at `c86bc6f`, with the real fixture copied in | same focused filter | 1 passed, 0 failed, 127 filtered |

An earlier exact-name attempt selected zero tests and is not counted as evidence. The corrected runs resolve the R11 parser-fixture gap. They do not replace the full recorder suite, Package 12 verification, real League lifecycle, or performance gates.

### B.7 Dual-path decision boundary

Both backends must remain until all of the following are true:

1. the reconciled code passes static, unit, fixture, media, and lifecycle verification;
2. both backends produce current comparable media through the canonical service/storage path;
3. a valid League baseline/capture matrix passes the unchanged `QB-PERF-002` gates;
4. a matched League FFmpeg-versus-native comparison is run in both orders or under an equally controlled design;
5. media quality, pacing, audio, lifecycle, output compatibility, and app replay/export behavior are acceptable;
6. NVIDIA-only native support versus vendor-neutral FFmpeg support is an explicit product/support decision;
7. the losing path has no unique recovery or hardware-support role that remains required.

Synthetic results may prioritize the native path for validation, but they do not authorize backend removal.

## C. Replay handoff reconciliation table

Classification vocabulary follows the handoff and reconciliation prompt.

| Requirement/decision | Current repository | Classification | Canonical action |
| --- | --- | --- | --- |
| Separate recorder and post-game desktop app | Separate Rust recorder and Tauri/Solid app communicate through local files | Implemented and correct | Preserve |
| WebView-first playback | Persistent HTML video in the system WebView | Implemented and correct | Keep as default |
| Narrow logical playback boundary | Viewer manipulates the concrete video element directly; no reusable `open/play/pause/seek/set_speed/audio` boundary | Partial | Introduce a small conceptual/service boundary without abstracting away browser readiness/errors |
| Efficient local random reads | Ephemeral loopback server supports GET/HEAD and single byte ranges with counters | Implemented and sensible | Retain, measure, then harden security |
| Browser presentation callbacks | `requestVideoFrameCallback` preferred; rAF/timeupdate fallback | Implemented and correct | Preserve |
| Keep Solid out of the per-frame hot path | Time state is viewer-local, but still a Solid signal consumed by visible UI | Acceptable current difference | Measure before further optimization; do not globalize per-frame state |
| Coalesced authoritative seeks | Latest-wins, one in flight, at most 10 Hz, dedupe, timeout/recovery | Implemented with a tunable value | Preserve behavior; measure interval after thumbnail feedback exists |
| Thumbnail-first scrub feedback | No replay thumbnails/atlas; only exported-clip JPEGs exist | Missing but specified | Implement uniform basic previews before semantic density |
| Playback-rate ladder | No replay rate controls found | Missing but specified | Add `0.25/0.5/1/2/4/8` through the playback boundary and measure real behavior |
| League-native timeline and navigation | Event markers, player filters, KDA/CS/level state, keyboard jumps, clip anchoring | Implemented v0 and correct | Preserve; later add lanes/LOD only as justified |
| ReplayIndex purpose/content | No ReplayIndex | Missing; lifecycle also open | Design ownership/version/rebuild contract before durable implementation; a read-only scaffold may remain bounded |
| Fast staged opening | Direct metadata/log reads and `preload=auto`; no index/representative preview stages | Partial | Establish open timing, then stage index/preview work |
| Two logical audio stems | One system-loopback-or-silent recording track; normal video volume plus export music mixing | Conflict with current product requirement | Audit/prototype League versus Other capture and synchronized WebView mixing early; do not infer representation |
| Playback health controls background work | Viewer observes media health, but no cross-task scheduler for preview/index/export work | Partial/open | Define after active-recording priority policy; begin with simple bounded yielding |
| Export independent from playback backend | Native Rust/FFmpeg export service | Implemented and correct | Preserve |
| Copy encoded components where valid | All current outputs filter and re-encode video/audio | Missing but specified as priority | Add measured stream-copy/hybrid paths after time and compatibility contracts exist |
| Hardware-assisted re-encode fallback | NVENC/AMF/QSV/VideoToolbox preference with libx264 fallback | Implemented and correct baseline | Preserve behind explicit compatibility/output rules |
| Smart-cut/edge-only re-encode | Absent | Correctly deferred | Do not implement without measured need |
| Playback instrumentation | FPS/drop/seek/heap/server counters exist in dev UI | Partial | Build reproducible scenarios/results and open timing; retain lightweight metrics |
| Production local-media security | Input validation and loopback binding exist; wildcard CSP/CORS/no request secret | Partial/open | Threat-model and define least privilege before production-final claim |
| Degraded/corrupt replay behavior | Incomplete library entries and bounded video reload/degraded UI exist | Partial/open | Define a complete artifact/media/finalization behavior matrix |
| Current-recorder media is truth | No representative current recording is tracked; code and synthetic outputs differ by branch | Requires measurement | Generate fresh reconciled outputs and inspect them before media-policy work |
| Recorder/replay media co-design | Current fMP4/CFR/GOP choices are recorder-driven; replay has no comparative evidence | Requires measurement | Keep current media unchanged until replay measurements justify experiments |
| MSE/libmpv/native/WebCodecs escalation | None in production | Implemented as a decision/deferral | Keep absent until a measured failure selects the relevant experiment |
| Future macOS boundary | Tauri/WebView and some AVFoundation/VideoToolbox branches exist; native recorder path and runtime are Windows-specific | Partial and unverified | Keep platform code narrow; validate actual macOS support separately rather than claiming it |

## D. Known missing specifications

The handoff’s open specifications remain explicit. Existing code may provide a useful current answer, but it does not silently close the broader contract.

| Missing specification | Existing repository answer | Assessment | What it blocks |
| --- | --- | --- | --- |
| Canonical replay time/synchronization | First-WGC-frame epoch, calibrated `game_start_video_offset_ms`, precomputed event `video_time_ms`, direct WebView seconds, clip milliseconds | Sound local v0, incomplete cross-system contract; native raw-H.264 timestamp warning makes the gap concrete | Authoritative semantic synchronization, segmented media, ReplayIndex timestamps, exact export semantics |
| Recorder to app lifecycle/finalization | League process owns start/stop; successful private video becomes `video.mp4`; metadata is written after stop; app uses incomplete flag when metadata is absent | Coherent release-era baseline, but partial/failure/finalizing states are implicit and no active-recording app contract exists | Recording-in-progress UX, recovery matrix, active-session restart/segmentation, trustworthy partial classification |
| Recording bundle identity grammar | Recorder supports all-digit timestamp plus `-<collision suffix>`; app and clip validators accept digits only | Existing implementation conflict, not merely unspecified | Visibility and clipping of same-second collision/restart bundles; must be resolved during integration |
| ReplayIndex ownership/lifecycle | None | Open | Persistent ReplayIndex implementation, rebuild/finalization authority |
| Derived replay-artifact lifecycle | Clip JPEG deletion only | Open | Thumbnail atlases, representative images, waveforms, regenerated index cleanup/versioning |
| Behavior while recording is active | Separate recorder; no system-wide task-priority policy | Open; recorder reliability remains higher priority | Background preview/index generation, concurrent export, resource scheduling and tests |
| Two-stem audio representation/playback/mix/export | One mixed system audio track or silence; separate export music input | Does not satisfy requirement | League/Other controls, independent gains/mutes, two-stem export, seek/rate sync tests |
| Exported-media compatibility | Current H.264 High/AAC stereo/faststart presets and Discord size cap | Useful shipped behavior but not a declared final envelope | Correct stream-copy/hybrid eligibility, normalization, compatibility gates |
| Supported media/hardware envelope | Windows WGC path; compiled NVENC/AMF/QSV; physical RTX/NVENC fixtures; small HEVC WebView probe | Partial and support-label-aware | User-visible hardware claims, codec defaults, macOS promises, failure policy |
| Production local-media access/security | Random loopback port, route validation, wildcard localhost CSP/CORS, no auth token | Sensible prototype, not a complete least-privilege contract | Production hardening/final security acceptance |
| Replay failure/degraded artifacts | Incomplete listing, open errors, two reload attempts, degraded preview/retry | Useful partial behavior, not a complete matrix | Final UX for partial/corrupt media, missing stems, stale indexes/previews, rebuild failures |
| Wider Chronobreak behavior | `PRODUCT.md` and 50-item feature list define more than the replay handoff; active prose has naming/state drift | Partially answered by repository, requires documentation reconciliation | A single trustworthy product surface and roadmap |

These gaps should be converted into bounded feature refinement or explicit architecture decisions only when the next work needs them. They must not all become one blocking “design everything” project.

## E. Measurement gaps

### E.1 Evidence tiers

| Tier | What exists | What it can support |
| --- | --- | --- |
| Source/static/unit | Code inspection; existing tests; two focused real-fixture parser passes from this audit | Architecture and parser claims only |
| Generated non-League media | Existing WGC/FFmpeg and native fixtures, failure matrices, 240-second soak, both-order A/B | Bounded flow, lifecycle, media validity, preliminary CPU/memory direction |
| Fresh reconciled real output | Absent | Needed for actual codec/container/stream/GOP/B-frame/fragment/audio truth after integration |
| Real League evidence | Pre-change whole-app performance data is invalid-but-diagnostic; no native-vs-reference League result | Required for negligible-impact and backend selection claims |
| Reproducible replay baseline | Dev counters exist, committed scenario/result corpus does not | Required before replay optimization or backend escalation |

### E.2 Required fresh recorder/media measurements

After integration, generate new sentinel-owned recordings from both backends through the canonical service path and retain their exact revision/runtime identity. Inspect with ffprobe and full decode:

- container and fragmentation structure;
- video/audio codec, profile, pixel format, dimensions, nominal and measured FPS;
- track count and current audio mapping;
- stream time bases, first/last PTS/DTS, start offsets, duration/skew/drift;
- GOP/keyframe cadence, B-frame presence, lookahead/multipass settings where observable;
- output growth and finalization behavior;
- decoded-frame change/freeze characteristics;
- compatibility with the current app viewer and exporter.

The HEVC probe asset and historical synthetic outputs are not substitutes for these current integrated outputs.

### E.3 Required League-only measurements

League access is needed later for:

- automatic discovery/start/stop against the real League process and HWND;
- focus, Alt-Tab, borderless/fullscreen, minimize/restore, resolution/DPI, and client/game transitions;
- one valid capped baseline/capture pair and one independent uncapped pair under `QB-PERF-002`;
- real game frame pacing, system/League CPU, GPU 3D/encode, memory, drops, counters, output progress, and media validity;
- matched FFmpeg/native comparison sufficient for the backend decision;
- actual game-motion quality and pacing review;
- real audio-source correctness and later League/Other isolation behavior;
- representative long-match poller write shape and Live Client durability.

Lack of League access does not block integration, static verification, fixture work, real local media inspection, replay harness work, or documentation cleanup.

### E.4 Required replay measurements

Use at least one short, one representative, and one long current recording. Record cold and warm cases. The initial baseline should capture:

- app open/no replay idle CPU, RAM, GPU, and disk activity;
- replay open to useful UI, representative visual, first authoritative frame, and playback-ready;
- play/pause reliability and presented/dropped frames at every required speed;
- seek request-to-`seeked` and request-to-presented-frame latency distributions for near and far seeks;
- rapid scrub request count, actual seek count, byte-range count/bytes/cancellations, and settle latency;
- repeated event jumps and clip endpoint edits;
- memory and resource stability across repeated open/close and torture loops;
- behavior at `0.25x`, rapid `0.25x <-> 8x` transitions, and high-speed traversal;
- whether thumbnail feedback makes remaining authoritative-seek latency unobtrusive;
- export startup/throughput/output validation for copy, hybrid, and re-encode candidates;
- two-stem drift, seek, rate-transition, mute/gain, and export behavior once a prototype exists;
- concurrent replay/export/background work, and separately the same scenarios while the recorder is active after policy is specified.

Do not invent product gates before this baseline. Recorder performance gates already defined in `QB-PERF-002` remain unchanged.

### E.5 Verification performed during this audit

This audit performed read-only repository/source comparisons plus the two focused parser tests described in B.6. It also verified:

- `docs.zip` duplicates the tracked `docs/` tree after line-ending normalization;
- `tools.zip` duplicates tracked `tools/` after line-ending normalization and additionally contains generated `__pycache__/*.pyc` files;
- `app/src.7z` duplicates tracked `app/src/` after line-ending normalization;
- recorder-development `recorder/src.zip` duplicates that working tree’s current `recorder/src/` content;
- the whole-app working tree remained clean before this report was added.

No broad Cargo/npm/release suite or League run was performed for this report, so none is claimed.

## F. Repository and documentation cleanup plan

### F.1 Target authoritative document set

Keep the active set small and purpose-specific:

```text
README.md                         canonical project entry and quick start
AGENTS.md                         work process, using Chronobreak language
feature-list.json                 only work-state/backlog authority
feature-list.schema.json          structural contract
PLANS.md                          ExecPlan contract
progress.md                       short restart pointer only

docs/product/PRODUCT.md           product boundaries
docs/architecture/README.md       map of durable architecture documents
docs/architecture/overview.md
docs/architecture/recording-bundle.md
docs/architecture/recorder.md
docs/architecture/replay.md
docs/architecture/media-library.md
docs/architecture/media-runtime.md
docs/architecture/local-media-security.md   once specified

docs/development/WORKFLOW.md
docs/development/VERIFICATION.md
docs/performance/...              protocols and accepted result summaries
docs/exec-plans/active/...        only genuinely active plans
docs/exec-plans/completed/...     only plans still linked as durable evidence
docs/reconciliation/...           this point-in-time report; not a backlog
```

Names may be consolidated differently, but each fact should have one active home.

### F.2 Immediate removals after information migration

The following archives are verified redundant and should be deleted from the canonical product tree:

| Artifact | Disposition | Reason |
| --- | --- | --- |
| `docs.zip` | Delete | Duplicates tracked docs |
| `tools.zip` | Delete | Duplicates tracked tools and contains generated bytecode |
| `app/src.7z` | Delete | Duplicates tracked frontend source |
| recorder-development `recorder/src.zip` | Delete/never transplant | Untracked duplicate of current recorder source |

Git history and normal release artifacts are the recovery mechanism; source snapshots do not belong inside the source tree.

### F.3 `Old_spec/` migration and deletion

Do not delete `Old_spec/` until surviving facts have moved, but do not leave it active afterward.

| Historical file | Surviving content to reconcile | Obsolete/conflicting content to discard |
| --- | --- | --- |
| `SPEC.md` | Local-first product, no database, descriptive-not-coaching boundary | Historical roadmap/status and stale naming |
| `RECORDER.md` | League process lifecycle authority, optional `GameEnd`, Live Client calibration pitfalls, bundle semantics | GDI/Desktop Duplication/primary-monitor fallback, PATH FFmpeg, obsolete audio assumptions, historical snapshot-recovery model |
| `APP-LIBRARY.md` | Filesystem discovery and user-owned media behavior still present in code | Stale screens/roadmap details |
| `APP-VIEWER.md` | Persistent video/timeline interaction details still current | “No thumbnails” as a durable direction and other superseded constraints |
| `APP-CLIP.md` | Clip-range and export UX rationale still current | Full re-encode as the final future strategy |
| `PHASES.md` | No unique work-state authority should survive | Entire historical phase roadmap; replace references with `feature-list.json` |

After migration:

- update `docs/architecture/README.md`, `recorder/README.md`, feature evidence, and comments so they do not point to historical files;
- delete `Old_spec/` in one reviewable cleanup commit;
- rely on Git history for archaeology.

### F.4 Plans, evidence, handoffs, and archive policy

- Keep an active ExecPlan only when its progress matches current code and remaining work. Rewrite or supersede the stale `QB-PERF-002` plan after recorder integration is scoped.
- Do not copy the recorder-development handoff/worktree files into the product repository.
- Consolidate Packages 0-11 into one canonical native-recorder non-League acceptance/evidence summary, retaining exact commits, commands, run roots, limitations, and links to any raw evidence that must remain external/ignored.
- Keep focused raw evidence documents only when they prove a distinct invariant that the consolidated report cannot adequately preserve.
- Keep completed plans while `feature-list.json` points to them as evidence. If they are later consolidated, update all canonical references before removal.
- Delete `docs/archive/feature-list.seed.json` if a final comparison confirms all unique decisions live in the current feature list/product docs. Git already preserves the seed.
- Treat the August 21 handoff as an input to the new replay architecture and feature refinement. Once reconciled, the external handoff filename must not be required to understand the repository.
- This reconciliation report is a dated audit. It must not become a second status file; subsequent status changes belong in `feature-list.json` and active ExecPlans.

### F.5 Naming cleanup

Active product prose, UI, comments, and plans should say Chronobreak. Remove PC-A/PC-B language after integration. `progress.md`, `AGENTS.md`, and active architecture currently use QueueBack; Tauri/package metadata uses League Replay.

Do not bulk-rename compatibility-sensitive identifiers during recorder integration:

- installed app identifier `com.leaguereplay.desktop` may control config/data identity;
- environment variables may be used by developer tooling;
- crate names, runtime IDs, diagnostics ABI strings, metadata values, and existing recording fields may be compatibility/provenance identifiers.

First classify each as public name, internal codename, stable compatibility identifier, or disposable test label. Public prose/UI can move to Chronobreak immediately in a dedicated change; stable identifiers need an explicit migration/alias plan. Historical QueueBack strings inside an immutable runtime identity may remain as provenance rather than be falsified.

### F.6 Generated/reference artifact policy

- Generated build, media, benchmark, evidence, target, cache, and bytecode directories remain ignored and out of Git.
- Dedicated small test fixtures may be tracked when they are deterministic, licensed, and necessary.
- Third-party header snapshots may be tracked only with exact source identity, license, and an active reproducibility purpose. The NVENC layout header/probes meet a potential purpose because native Rust currently hand-defines ABI layouts; confirm and document this before transplanting them.
- Never place user recordings or credentials in the repository.
- Keep the empirical Live Client fixture because it now closes a real parser verification gap; document its sanitization/provenance.

### F.7 Cleanup verification gate

Before deleting any historical material:

1. `rg` finds no active links to the files being removed;
2. every unique surviving fact has one canonical destination;
3. feature evidence paths remain valid;
4. repository-wide verification and Markdown/link checks pass;
5. `git diff --check` passes;
6. a clean clone contains every required source, fixture, script, and instruction without relying on Downloads, machine-specific paths, or the old recorder repository.

## G. Ordered next-work plan

### G.1 Work sequence

| Order | Work | Exit gate | Review point |
| ---: | --- | --- | --- |
| 1 | Preserve the recorder divergence at full `c86bc6f`; create a canonical integration feature and self-contained ExecPlan | Durable backup exists; exact source revisions and scope are recorded | Confirm no unpushed work can be lost and no broad replay work is mixed in |
| 2 | Reconcile current canonical state before code merge: update plan assumptions, runtime r3/r5/r6 truth, bundle-ID grammar, and exact shared contracts | Integration plan reflects current code rather than stale Milestones 1-6 | Challenge merge boundaries, especially service/storage/poller/runtime |
| 3 | Semantically integrate r6 and native modules behind a developer selector while retaining FFmpeg default | Both paths compile through the same canonical lifecycle, metadata, audio, storage, and poller interfaces | Independent implementation review for ownership, cancellation, hard hangs, bounds, unsafe/NVENC ABI, and backward compatibility |
| 4 | Merge Packages 0-10 and the Package 11 evidence decision; implement Package 12 locally; use the real R11 fixture | Focused and broad poller/recorder tests pass; no duplicate event authority or changed final JSON semantics | Review concurrency, durability, failure classification, and runtime integration |
| 5 | Run project-wide static/unit/integration verification from `docs/development/VERIFICATION.md` | Format, Clippy, Cargo tests, frontend type/build, media-runtime tests, benchmark-tool tests, feature schema/invariants all pass | Stop on any app compatibility regression; do not weaken gates |
| 6 | Run canonical non-League media/lifecycle/failure/soak and both-backend A/B fixtures on fresh outputs | Both backends produce valid, changing, app-playable media with exact identity/evidence; current format is inspected | Decide whether implementation is ready for League, not which backend wins |
| 7 | Reconcile active docs, naming, feature evidence, and plans; migrate/delete old specs and redundant archives | A clean clone has one clear Chronobreak story and no required external handoff/archive | Documentation/state review before deletion commit |
| 8 | Run League-only `QB-PERF-002` and matched backend gates on a suitable machine | Valid data passes formal gates and app/media/lifecycle behavior is acceptable | Explicit product decision: default/support matrix and whether either backend can be removed |
| 9 | Establish the real WebView replay baseline using current reconciled recordings | Reproducible open/play/rate/seek/scrub/resource/export results exist | Select only demonstrated replay problems for work |
| 10 | Implement the lowest-risk handoff value: playback boundary, speed ladder, uniform thumbnail preview, and instrumentation refinements | Required speeds and thumbnail-first scrub are usable and measured | Re-evaluate seek interval and whether WebView remains sufficient |
| 11 | Refine/execute the open replay contracts needed next: canonical time, ReplayIndex/artifacts, security, degraded behavior, active-recording policy | Each decision is bounded, documented, and linked to a concrete feature | Do not combine all open specs into one implementation |
| 12 | Prototype and choose the two-stem recorder/storage/WebView/export approach; then add fast copy/hybrid export under an explicit compatibility contract | Real seek/rate drift, gain/mute, export, compatibility, and recorder-impact evidence passes | Architecture decision before product hardening |

### G.2 Risky merge areas

The following require explicit review rather than mechanical copying:

- `recorder/src/service.rs`: two backends, startup cancellation, active target validation, retry suppression, stop/failure paths, telemetry, and metadata ownership;
- `recorder/src/storage.rs`: private/partial/canonical filenames and backward-compatible bundle finalization;
- recorder/app bundle IDs: same-second suffixed directories must remain discoverable and safe without weakening path validation;
- `recorder/src/poller.rs`: epoch calibration, event/snapshot authority, write coalescing, Package 12 request split, and final flush;
- `recorder/src/encoder.rs` and native mux: shared audio input, r6 runtime, external child ownership, CFR/timestamp behavior, and partial recovery;
- `recorder/src/platform/windows.rs`: HWND generation, adapter identity, minimize/restore, cross-adapter failure, and caching;
- native `unsafe`/NVENC ABI: layout versioning, session ownership, async completion, cleanup, and driver-call containment;
- `media-runtime/`: one exact lock/provenance/build/staging/test update across both app and recorder;
- additive `metadata.json` fields: existing recordings and the current app must remain readable/playable;
- naming/config identifiers: do not accidentally fork user config or installed data paths.

### G.3 Verification checkpoints

At minimum, use these checkpoints:

1. **Pre-merge:** clean chassis; divergence backed up; file-by-file semantic map reviewed.
2. **Compile/static:** all Rust targets/examples format, build, Clippy, and test; frontend type/build passes.
3. **Fixture:** WGC/native probes and failure matrix use sentinel-owned temporary output only.
4. **Media:** ffprobe, full decode, decoded-frame-change, timestamps, keyframes, audio, and app open/export checks on both current outputs.
5. **Soak/resources:** bounded memory/handles/threads/queues/output progress with no hidden failures.
6. **Documentation/state:** feature list, plans, architecture, runtime identity, and verification commands agree.
7. **League:** formal post-change matrix before negligible-impact or backend-selection claims.
8. **Replay:** reproducible baseline before changing playback backend or recorder media policy.

### G.4 Explicit deferrals

Do not include these in the first integration unless a newly discovered correctness dependency makes one unavoidable:

- removing either recorder backend;
- presenting native as the user default;
- native AMF/QSV/HEVC support;
- active-session backend/device restart or segmented recording;
- hard GPU-driver-call containment redesign;
- timestamp-aware native mux changes;
- recorder GOP/B-frame/fragment/media-format changes for replay convenience;
- ReplayIndex ownership or derived-artifact lifecycle by implication;
- final League/Other representation without feasibility evidence;
- production-final local-media security without a threat model;
- MSE, libmpv production, native playback, WebCodecs, a custom decoder, or custom frame cache;
- smart-cut/edge-only re-encode;
- semantic/adaptive thumbnail density before uniform thumbnails prove useful;
- montage sophistication, waveforms, or other secondary replay artifacts.

## Final transition condition

Repository reconciliation is complete only when a new engineer can clone one repository and determine, without the old machine split or files in Downloads:

- how Chronobreak records, finalizes, stores, opens, replays, and exports media;
- which recorder backends exist and which one is default/validated;
- the exact shared media runtime identity and how it is built/staged/verified;
- the recording bundle and time/lifecycle contracts, including what remains open;
- which replay capabilities are implemented, missing, measured, or intentionally deferred;
- where canonical work state and verification commands live;
- what still requires a current real recording, League, different hardware, or macOS;
- why historical archives/specifications are no longer required.

Until then, the whole-app repository is the chassis, not yet the fully reconciled canonical baseline.
