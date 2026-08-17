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

The real PC-A repository is still untouched from the end of the long Codex CLI
session. It remains the source of truth. None of the later QB-PERF-002 takeover
kit or QB-PERF-005/native work in this PC-B repository should be represented as
already installed on PC A.

This is the canonical PC-B patch repository:

```text
C:\Users\Hugo\Documents\perso\chrnbrk\chronobreak-recorder-pc-b
branch: pc-b/qb-perf-005-native
```

The matched non-League A/B implementation is commit:

```text
9ef92f8 Prepare matched non-League backend comparison
```

Its parent is `5744910 Validate native lifecycle under sustained load`.
All PC-B changes are local commits on this branch. At handoff completion,
`git status --short` must be empty. Do not reorganize, squash, amend, delete
evidence or rewrite this branch before a fresh-PC-A rebase audit.

If the user supplies a fresh whole-repository ZIP or live repository files,
stop making native edits and first rebase/port this commit stack against that
exact state. Inspect the new tree and its instructions, preserve existing user
changes, replay the native commits in dependency order, resolve conflicts by
behavior rather than blind patching, then rerun the complete checks. Preserve
the QB-PERF-002 reference path and its synchronized `frame_seq` fix separately.

## Milestone truth

| Item | Status on PC B | Meaning |
| --- | --- | --- |
| M0: live-tree rebase/attribution seam | **Not authoritative** | The seam and attribution boundary are prepared in the partial PC-B repository, but the exact whole PC-A tree has not been supplied/rebased. |
| M1: native prerequisites/preflight | **Provisionally done** | Same-adapter native prerequisites implemented and tested on PC B. |
| M2: exact-HWND WGC source | **Provisionally done** | Steady, resize, minimize/restore, occlusion and target-close non-League fixtures passed. |
| M3: D3D11 BGRA-to-NV12 ring | **Provisionally done** | GPU conversion and fixed four-slot ownership implemented/tested. |
| M4: direct NVENC H.264 | **Provisionally done** | Direct D3D11/NVENC with bounded four-slot completion implemented/tested. |
| M5: CFR/mux/audio/MP4 | **Provisionally done** | 60-Hz CFR and mux-only FFmpeg MP4 path implemented; valid H.264/AAC output/full decode. Exact r5 rerun remains. |
| M6: failure matrix/240-second native run | **Provisionally done** | Non-League functional/failure cases and bounded 240-second run passed. |
| M7: automatic lifecycle | **Code and non-League portion done; formal exit not done** | Service integration/default preservation and 240-second release lifecycle passed. The audited exit still requires one actual automatic League recording, which PC B must not run. |
| Preliminary non-League backend A/B | **Ready, not run** | Commit `9ef92f8`; blocked only by the absent exact r5 runtime. Useful intel, not a formal milestone exit. |
| M8: controlled League A/B decision | **Not done** | Must use League and comparable valid evidence later. Do not infer a winner yet. |
| M9: remove losing path/close state | **Not started** | Keep both backends and the developer-only selector until M8 decides. |
| QB-PERF-002 heartbeat-free validation | **Not done** | Must run from the untouched/rebased PC-A reference path: three independent 240-second captures. |
| Review finding R11 | **Deferred** | Only missing review item; requires the absent empirical `live-client-capture-20260730-110603.json` parser fixture. Do not fabricate it. |

## Current verification state

Fresh checks after adding the A/B arm on 2026-08-17:

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
A/B missing-runtime fail-closed check                           PASS
```

The three ignored tests are real environment-gated lifecycle tests: the new
FFmpeg arm plus the existing native recording and native startup-cancellation
arms. The actual matched A/B did not run because its exact runtime is absent.

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

1. If a fresh PC-A repository/ZIP arrives, rebase onto it before anything else.
2. Otherwise obtain the exact staged
   `queueback-ffmpeg-8.1.2-windows-x86_64-r5` runtime. Prefer copying a
   known-good exact PC-A runtime. Do not use PC B's stock FFmpeg 9.
3. Run the preflight in `docs/PRELIMINARY_BACKEND_AB.md`; do not continue
   without its explicit PASS marker.
4. Run one 240-second `ffmpeg-native` pair, let the machine return to idle, then
   run a mirrored `native-ffmpeg` pair to reduce order/thermal bias.
5. Analyze both `comparison-summary.json` files together with media, terminal
   evidence and raw process CSVs. Report process-attributed CPU, memory,
   validity and variability; label the result preliminary and keep both paths.
6. Continue safe recorder optimization only from observed bottlenecks and rerun
   the full non-League suite after every material change.
7. When PC A/League becomes available, complete untouched QB-PERF-002 first,
   then the external League portion of M7 and controlled M8. Begin M9 only
   after that decision.

## Exact runtime blocker

This repository contains `media-runtime/runtime-lock.json`, the r5 source patch
and notices, but no locked executables anywhere in the handoff/worktree. The
required hashes are documented in `docs/PRELIMINARY_BACKEND_AB.md`.

PC B has `C:\msys64`, but its current installation does not contain the exact
locked package set; its `msys2-runtime` was observed as 3.6.9-2 while the lock
requires 3.5.4-8. The canonical build script exists only under the supplied
reference tree and hardcodes `C:\msys64`. Do not mutate that installation or
call an approximate rebuild canonical. If the exact runtime cannot be copied,
reproduce the locked packages in an isolated root and validate every final
hash.

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
then rerun the unchanged preflight. This is independent of the missing r5
runtime and does not require League.

## Guardrails for the next session

- Never start League on PC B.
- Never claim PC-B patches are installed on PC A.
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
