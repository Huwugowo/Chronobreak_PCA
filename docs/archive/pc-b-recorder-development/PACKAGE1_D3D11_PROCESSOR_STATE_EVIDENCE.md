# Recorder remediation Package 1 evidence

Date: 2026-08-18

## Scope

Package 1 hoists the invariant D3D11 video-processor configuration out of the
per-frame conversion path. One helper now configures input/output color space,
progressive frame format, disabled automatic processing, center-crop source
rectangle, stream destination rectangle, and output target rectangle:

- once after initial processor creation; and
- once after each accepted input-size reconfiguration.

The per-frame path retains only the input-stream COM lifetime handling and
`VideoProcessorBlt`. The native module-level documentation now reflects the
developer-gated service integration. Capture admission, pool/ring sizes, CFR
scheduling, timestamps, codec/mux configuration, audio, and publication are
unchanged.

The release probe enforces:

```text
processor_state_configurations == processor_recreations + 1
```

## Static verification

The following passed against the changed tree:

```text
cargo fmt --manifest-path recorder/Cargo.toml --all
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
git diff --check
```

Results: 96 library tests and one recorder-binary test passed, three intentional
live tests remained ignored, and only the unavailable R11 empirical fixture was
filtered. All examples compiled.

## Resize and visibility lifecycle

Evidence:

```text
evidence/package1-processor-state-hoist-20260818-134507
```

A release native probe captured a dedicated animated non-League HWND for 20
seconds through three commanded sizes and one minimize/restore cycle. The
pinned r6 FFmpeg/ffprobe runtime reported:

- 1,200 scheduled/submitted/completed/muxed frames and zero tick drops;
- five capture and converter processor recreations;
- six processor-state configurations, exactly `5 + 1`;
- 1920x1080 High H.264 at exact `60/1`, time base `1/15360`;
- video and AAC start at zero and both last exactly 20.000 seconds;
- complete A/V decode with empty error output;
- 1,200 decoded video frames and 1,089 unique hashes. The expected repeated
  hashes cover the deliberate two-second minimize interval while CFR duplicates
  the last valid GPU snapshot; visible animation otherwise continues changing.

An initial harness attempt intentionally failed before recording because the
fixture HWND was hidden. Exact-HWND discovery rejected it as designed. The
passing run explicitly showed only the fixture HWND before capture.

## Matched resource comparison

The Package 0 commit `f885aba` was built as the baseline. Both implementations
used the same release profile, feature set, pinned runtime, 1280x720 animated
fixture, 1920x1080/60 output, and exact-HWND native path.

### CPU, cycles, and memory

Evidence:

```text
evidence/package1-cpu-ab-20260818-135506
```

Each implementation received a fresh animation instance and ran for 60
seconds. Recorder CPU and hardware process cycles were sampled about every 100
ms. Both arms encoded all 3,600 ticks with zero drops.

| Metric | Package 0 baseline | Package 1 | Change |
| --- | ---: | ---: | ---: |
| Processor-state configurations | 3,600 | 2 | -99.94% |
| Recorder CPU time | 1.45312 s | 0.92188 s | -36.56% |
| CPU as percent of one core | 2.4219% | 1.5365% | -0.8854 pp |
| Hardware process cycles | 9,342,080,240 | 8,388,926,528 | -10.20% |
| Peak working set | 64,647,168 B | 63,676,416 B | -1.50% |

Both runs observed one initial WGC size settling/recreation, so Package 1's
two configurations are the initial processor plus that accepted recreation.

### GPU neutrality

Evidence:

```text
evidence/package1-performance-ab-r2-20260818-135047
evidence/package1-performance-ab-r3-20260818-135210
```

A separate short matched sequence sampled Windows GPU-engine counters by
recorder PID. The two baseline arms averaged 54.09 aggregate engine-percent;
the two Package 1 arms averaged 51.48. Mean per-arm peaks were 68.11 and 67.36
respectively. These are noisy utilization sums rather than a claimed GPU
speedup, but they reject a material GPU regression and are consistent with an
unchanged number of video blits and encoded frames.

The first comparison attempt used a two-second inter-arm pause and the next
NVENC open correctly returned `NO_ENCODE_DEVICE`. The completed sequence used
the repository's audited 15-second NVENC cooldown. CPU observations from the
slow GPU-counter loop were excluded; the separate 100-ms comparison above is
the CPU result of record.

## Decision

Keep Package 1. It satisfies the call-count invariant, lifecycle and media
gates, and is neutral-to-better on GPU and memory while materially reducing
recorder CPU work. No timestamp, mux, codec, resource-bound, or replay contract
change is needed. Package 2 may proceed independently.
