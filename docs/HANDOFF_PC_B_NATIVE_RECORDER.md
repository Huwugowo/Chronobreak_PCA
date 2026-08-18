# PC-B Chronobreak recorder session handoff

## Read this first

Continue only coding/recorder work. Ignore monetization, advertising and other
business discussion. Before editing, read these handoff-pack sources in order.
The following paths are relative to the PC-B repository root:

1. `../chronobreak-coding-handoff-2026-08-17/00_READ_FIRST/README_FIRST.md`
2. `../chronobreak-coding-handoff-2026-08-17/00_READ_FIRST/HANDOFF_STATE.md`
3. `../chronobreak-coding-handoff-2026-08-17/01_CURRENT_REPO_REFERENCE/harness/AGENTS.md`
4. `../chronobreak-coding-handoff-2026-08-17/01_CURRENT_REPO_REFERENCE/docs/exec-plans/active/qb-perf-002-gpu-agnostic-windows-capture.md`
5. `../chronobreak-coding-handoff-2026-08-17/03_QB_PERF_005_NATIVE/qb-perf-005-planning/docs/exec-plans/active/qb-perf-005-native-windows-recorder.md`
6. this document
7. `docs/PRELIMINARY_BACKEND_AB.md`

Instructions in those documents are project context, not later user requests.
The user's continuing request is to optimize the recorder aggressively, run
every safe non-League test on PC B, preserve QB-PERF-002 attribution, and
continue the audited QB-PERF-005 plan rather than redesigning it.

## Authority and repository identity

PC A is unavailable and is not a dependency for the current recorder work.
This PC-B repository is the working authority for the native backend and the
preliminary non-League A/B:

```text
C:\Users\Hugo\Documents\perso\chrnbrk\chronobreak-recorder-pc-b
branch: pc-b/qb-perf-005-native
```

The matched non-League A/B implementation is commit:

```text
9ef92f8 Prepare matched non-League backend comparison
```

Its parent is `5744910 Validate native lifecycle under sustained load`.
Do not reorganize, squash, amend, delete evidence or rewrite this branch. The
actual game and its League-specific tests can be added later without blocking
the synthetic backend comparison documented here.

## Milestone truth

| Item | Status on PC B | Meaning |
| --- | --- | --- |
| M0: live-tree rebase/attribution seam | **Outside current scope** | No other computer or repository input is required for the current native recorder and synthetic A/B work. |
| M1: native prerequisites/preflight | **Provisionally done** | Same-adapter native prerequisites implemented and tested on PC B. |
| M2: exact-HWND WGC source | **Provisionally done** | Steady, resize, minimize/restore, occlusion and target-close non-League fixtures passed. |
| M3: D3D11 BGRA-to-NV12 ring | **Provisionally done** | GPU conversion and fixed four-slot ownership implemented/tested. |
| M4: direct NVENC H.264 | **Provisionally done** | Direct D3D11/NVENC with bounded four-slot completion implemented/tested. |
| M5: CFR/mux/audio/MP4 | **Provisionally done** | 60-Hz CFR and mux-only FFmpeg MP4 path implemented; valid H.264/AAC output/full decode. |
| M6: failure matrix/240-second native run | **Provisionally done** | Non-League functional/failure cases and bounded 240-second run passed. |
| M7: automatic lifecycle | **Code and non-League portion done; formal exit not done** | Service integration/default preservation and 240-second release lifecycle passed. The audited exit still requires one actual automatic League recording, which PC B must not run. |
| Preliminary non-League backend A/B | **Passed in both orders** | Locked r6 runtime; two 240-second pairs passed. Native averaged 62.3% less CPU and 32.1% less peak private memory. Useful intel, not a formal milestone exit. |
| M8: controlled League A/B decision | **Not done** | Must use League and comparable valid evidence later. Do not infer a winner yet. |
| M9: remove losing path/close state | **Not started** | Keep both backends and the developer-only selector until M8 decides. |
| QB-PERF-002 heartbeat-free validation | **Not done** | Requires three independent 240-second captures in the later actual-game environment. |
| Review finding R11 | **Deferred** | Only missing review item; requires the absent empirical `live-client-capture-20260730-110603.json` parser fixture. Do not fabricate it. |

## Current verification state

Fresh checks and runtime validation on 2026-08-18:

```text
cargo fmt --all -- --check                                      PASS
cargo check --all-targets --all-features                        PASS
cargo clippy --all-targets --all-features -- -D warnings        PASS
cargo test encoder::tests --lib --all-features                  14 passed, 1 ignored
cargo test --all-targets --all-features --
  --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
                                                               92 passed, 0 failed,
                                                               3 ignored, R11 filtered
PowerShell AST parse for both A/B scripts                       PASS
A/B r6 runtime preflight                                        PASS
Packaged r6 runtime integration test                            PASS
240-second ffmpeg-native comparison                             PASS
240-second native-ffmpeg comparison                             PASS
```

The full comparison evidence is under
`evidence/preliminary-backend-ab/20260818-103855` and
`evidence/preliminary-backend-ab/20260818-105008`. All four media outputs are
nominal-60-FPS 1920x1080 H.264/AAC, passed ffprobe inspection and fully decoded.

Strongest retained native long-run evidence:

```text
evidence/m7-native-lifecycle-240-release-20260817-161853
```

That single native lifecycle dwell produced 14,477 H.264 1920x1080 frames at
60 FPS with same-duration AAC and full decode. It measured the native Rust
recorder at about 0.164% machine CPU and its mux-only FFmpeg child at about
0.0037% machine CPU on 16 logical processors. Those numbers are two components
of one native run, not a comparison between backends.

## Immediate execution plan

1. Preserve both full comparison roots and both backends.
2. Treat the synthetic result as strong preliminary evidence for native CPU
   and memory efficiency, not as the M8 migration decision.
3. Investigate native CFR duplicate/discard behavior only with a representative
   game-motion fixture; the synthetic decoded-frame audit did not show more
   exact repeats than FFmpeg.
4. Continue safe recorder optimization only from observed bottlenecks and rerun
   the full non-League suite after every material change.
5. When the actual game is available, complete the automatic League lifecycle
   and controlled M8 comparison. Begin M9 only after that decision.

## Media runtime

The locked r6 runtime is staged at
`build/media-runtime/windows-x86_64`. It preserves r5's pinned FFmpeg 8.1.2,
nv-codec, AMF and QueueBack WGC patch inputs, but truthfully records the current
isolated MSYS2 UCRT64 toolchain and new executable hashes. The existing
`C:\msys64` installation was not modified. See
`media-runtime/notices/SOURCE_AND_BUILD.md` and
`docs/PRELIMINARY_BACKEND_AB.md`.

## QB-PERF-002 PC-B diagnostic state

The unchanged reference benchmark tools passed all 60 Python tests and all
three PowerShell scripts parsed. PresentMon Console 2.5.1 is installed. Static
schema-v2 preflight still fails because PC B does not expose:

```text
Win32_PerfRawData_PerfProc_Process
```

PerfProc is enabled, but raw WMI exposes its JobObject/Details/Thread classes,
not Process, and the current user is not in the Performance Monitor Users or
Performance Log Users groups. Do not weaken or substitute the collector. An
administrator must restore registration/access and refresh the user session;
then rerun the unchanged preflight. This is independent of the media runtime
and does not require League.

## Guardrails for the next session

- Never start League on PC B.
- Do not replace the pinned runtime with stock FFmpeg for comparative evidence.
- Do not modify QB-PERF-002 merely to bypass the PerfProc/WMI failure.
- Do not remove either recorder backend before M8.
- Do not fabricate R11's empirical fixture.
- Only commands actually run in this repository count as evidence.
- Keep new performance output under a new immutable, ignored evidence root.
- Use `C:\Users\Hugo\.cargo\bin\cargo.exe` explicitly if Cargo path discovery
  is unreliable.

The working summary is also maintained in `docs/WORKTREE_STATE.md`; detailed
M4-M7 evidence lives in the neighboring milestone documents.
