# Package 8: Native Backend Audit Follow-up

Date: 2026-08-18

Scope: immediate production-safe changes from the native-backend-only audit.
This package does not change the FFmpeg reference backend, pinned r6 media
runtime, fragmented-MP4/CFR contract, queue capacities, or match/session model.

## Decision

Keep all four changes. They remove avoidable work from live native recording
and startup without changing encoded media behavior:

| Change | Implementation | Preserved contract |
| --- | --- | --- |
| Periodic mux telemetry | Live snapshots read FFmpeg's atomically published `total_size`; only finalization reads filesystem metadata and replaces the estimate with the authoritative file length | Final output validation remains authoritative; no filesystem operation remains on the periodic GPU-worker evidence path |
| Callback quiescence | A waiter registers only during resize or shutdown; normal callback/owner exit skips the mutex and condition variable when no waiter exists | The callback stays nonblocking during normal capture, while the shared mutex plus sequentially consistent handshake prevents a lost final notification |
| NVENC capability validation | Production opens its real D3D11 NVENC session once, validates H.264/1080p60/async caps on it, then initializes the same session | Capability errors retain their detail and close the same owned session; the standalone diagnostic probe keeps its explicit temporary session |
| Tokio control runtime | Tray and headless service modes share a named two-worker runtime; one-shot diagnostics use a current-thread runtime | Blocking native-worker joins remain on Tokio's blocking pool; async service behavior is unchanged |

## Correctness notes

The callback waiter flag and the active/owner counters form one cross-atomic
handshake. Sequential consistency is deliberate: if the waiter observes a
pre-decrement active count, the final callback must observe the registered
waiter, and vice versa. The waiter also holds the same mutex across predicate
inspection and condition-variable wait. Unit tests cover both the cold
notification path and the absence of notifications on normal callback exit.

`NativeNvencEncoder::new` has one production `open_session` call. Capability
validation happens before preset/configuration work; failure closes the real
session and uses the existing initialization-error combiner. The separate
`probe_h264_on_source` temporary session remains reachable only from the
standalone source diagnostic example.

Live `output_file_bytes` is now a nonblocking progress estimate. The terminal
snapshot continues to use `std::fs::metadata` after FFmpeg exits, and the mux
finish checks still reconcile encoded frames, progress termination, reader
errors, and final file size.

## Static acceptance

The following commands passed from the repository root against the final
source:

```powershell
cargo fmt --manifest-path recorder/Cargo.toml --all -- --check
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
cargo build --manifest-path recorder/Cargo.toml --release --bin recorder
cargo build --manifest-path recorder/Cargo.toml --release --example native_mp4_probe --features native-failure-injection
git diff --check
```

Results: 114 library tests and three binary tests passed. Five explicit
environment/profile tests were ignored. Only the absent empirical R11 fixture
was filtered; no R11 result is claimed. Strict Clippy and both optimized builds
passed.

## Final native hardware smoke

Ignored evidence root:
`evidence/native-audit-followup-final-20260818-175850`.

The optimized native probe captured the animated 1280x720 exact-HWND fixture
through WGC, D3D11 conversion, the production NVENC session, and the pinned r6
mux for exactly six seconds:

- 360 due, submitted, completed, and muxed frames;
- media time base `1/60` and terminal output time `6,000,000 us`;
- `1,087,671` progress bytes and the same authoritative final file length;
- maximum one encoder frame in flight, zero slot-admission failures, and a
  one-frame pending high-water mark;
- full-file decode with the pinned FFmpeg completed without an error;
- MP4 SHA-256
  `5053FC37CAA25FB215BBFDE8A1A7CADB27650D369AA0D50744143F8F9F15136E`.

This is a functional native-path acceptance arm, not a League workload or a
new performance-winner claim.

## Quick revision A/B

A matched pre-Package-8/current comparison subsequently proved the large
deterministic reductions and checked for short-run regression:

- stable tray/service process threads fell from 25 to 11 in every arm;
- mean native session startup fell from 240.44 ms to 224.13 ms across four
  arms per revision, with no overlap between the observed ranges;
- all four ten-second native arms and all eight one-second startup arms kept
  exact 60 Hz accounting and fully decoded;
- short native CPU, wall-time, and private-memory aggregates favored current,
  but the per-arm ranges overlap and do not establish a steady-recording CPU
  win.

Method, raw roots, executable hashes, results, and interpretation:
`docs/archive/pc-b-recorder-development/PACKAGE8_QUICK_REVISION_AB.md`.

## Deferred boundaries

- Active-session restart still needs a product-level segment and replay-offset
  contract; retrying into unrelated zero-based files remains prohibited.
- Removing FFprobe or replacing the runtime remains outside this package while
  the pinned r6/reference-backend contract is retained.
- A GPU/driver call that never returns cannot be made safely killable with an
  in-process timeout. Hard containment requires a separately designed and
  supervised helper process with explicit resource and partial-output rules.
