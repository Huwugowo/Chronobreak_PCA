# Owned and bounded clip export

## Purpose

QB-CLIP-008 and QB-CLIP-010 jointly ensure export errors/cancellation cannot leave an encoder writing after library ownership is released, and synchronous export filesystem work cannot block async executor workers.

## Relevant planning-time architecture

The Tauri `export_clip` command admits a `MutationCompletion` via `LibraryCoordinator::admit_export`. Its permit serializes mutations and its drop invalidates the captured root. Today that guard lives in the request future. `clip_export::export` performs synchronous strict bundle reads, metadata, path allocation, staging, rename and cleanup interleaved with async encoding/probing. Encoders can fall back to software. One to three preset outputs are staged and atomically published as a batch. Progress and result schemas are already consumed by the UI and replay benchmark.

`run_ffmpeg` uses unbounded progress lines and stderr collection; thumbnail generation uses unbounded `Command::output`. Validation uses the shared bounded media-runtime runner with a five-second deadline and one MiB limit. Paths and source/frame validation are already strict and must remain so.

## Scope and non-goals

Change the ownership boundary and export-local child supervision. Keep codecs, presets, fallback for ordinary encoder exit failure, exact frame validation, public payloads, and atomic publication semantics. Do not alter recorder capture, global process infrastructure, or real user media. The coupled filesystem isolation is QB-CLIP-010 and uses the same owner; separate concurrent owners would weaken cancellation safety.

## Exploration findings

Read-only design review confirmed an admitted blocking owner avoids per-filesystem worker hops and preserves the existing one-mutation bound. Tokio `Handle::block_on` may drive the async pipeline on a blocking thread. Dropping a JoinHandle does not cancel its worker. Progress records, not just stderr, need bounded retained memory. Thumbnail subprocesses need the same supervision. A caller cancellation cannot interrupt an indivisible filesystem call; ownership must survive until it returns.

## Chosen design and rationale

Move the completion guard, owned paths/request/channel and captured Tokio handle into one `spawn_blocking` closure. A caller-drop guard sets a shared cancellation flag; the worker continues cleanup and holds admission. The closure drives the export pipeline with the captured handle, so synchronous filesystem/JSON work runs only on the blocking thread. Evaluate response admission and drop completion in that closure after export returns.

Use an export-local process supervisor with cancellation checks at most 50 milliseconds apart, concurrent stdout/stderr reads, an 8 KiB maximum progress record and a 64 KiB diagnostic tail. Only advancing parsed `out_time_us` resets the 120-second encoding stall clock. A separate absolute encoding deadline is derived from duration (20 times clip duration plus two minutes, at least five minutes and at most six hours); thumbnail work has a 30-second cap. Both are explicit product resource limits, not capture-performance claims. Progress/error parsing and cancellation return typed fatal failures; ordinary nonzero encoder exits retain fallback. On failure, terminate and wait for the child before releasing pipes and returning. No detached pipe-reader tasks are needed. A kill-on-drop fallback covers unexpected unwinding.

Check cancellation before child launch, between stages and before publication. The finite five-second bounded validation probe finishes under worker ownership before cancellation is acted upon. Cancellation before publication cleans all staged outputs. Publication itself is indivisible with respect to caller cancellation: complete or roll back under the same owner.

## Rejected alternatives and planning decisions

Kill-on-drop alone cannot prove reap-before-admission-release. A detached async task retaining the guard still executes synchronous filesystem operations on executor workers. Spawning a blocking job for every filesystem call adds cancellation and ownership gaps. Reusing the shared bounded-output runner cannot support long incremental progress streams without retaining or rejecting legitimate output. Do not retry cancellation, stalls or malformed progress as encoder incompatibility.

## Milestones

1. Implement independently testable owned-worker and bounded child supervision, with finite generated child fixtures.
2. Move export completion and the full pipeline into that worker; thread cancellation through encoding, thumbnails and publication. Preserve result/progress compatibility and cleanup.
3. Validate process failure/cancellation/resource bounds, filesystem-worker isolation and mutation admission; run app tests, formatting, Clippy, benchmark-feature compilation, and generated export publication/media checks.

## Verification design

Use the test executable itself as a controlled child so fixtures do not depend on machine PowerShell execution policy. Exercise invalid UTF-8 followed by a delayed marker, oversized progress records, silent stalls, valid progress, stderr floods, ordinary failure, absolute deadline and cancellation. Assert the child is reaped and no delayed write occurs after return. On a single-thread Tokio runtime, hold a blocking worker and its real mutation permit; abort its waiter and prove timers advance while the slot stays busy until release/cleanup. Existing publication rollback and strict frame tests remain required.

Run `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check`, `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings`, and benchmark-feature compilation. Use only a generated temporary clip fixture for encode/probe/decode/source-preservation checks. The known unrelated PowerShell-script fixture may require a child-only execution-policy environment override; record it honestly.

## Performance and reliability gates

No library admission release before worker completion; no detached drains; bounded retained output; finite normal error/cancel/stall fixtures; async timer progress during held blocking work; unchanged source data and all-or-nothing staged publication. A permanently stuck filesystem or OS termination call cannot safely be force-cancelled; retain ownership rather than permit concurrent mutation. No League or hardware-performance claim is made.
