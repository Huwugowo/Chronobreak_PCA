# Recorder review remediation Package 0 evidence

Date: 2026-08-18

## Scope

Package 0 adds behavior-neutral observability and test-only stress controls for
the review remediation plan. It does not change capture admission, CFR
scheduling, retry policy, process refresh behavior, target validation,
persistence cadence, codec configuration, or media publication.

The added evidence covers:

- D3D11 video-processor state-configuration bundles;
- exact `1/60` media time base, source QPC age, deadline lateness, catch-up
  pressure, and maximum consecutive catch-up batch;
- feature-gated GPU-worker stalls bounded to five seconds;
- deterministic logical source rates of 60, 144, and 240 Hz;
- process/control-plane call and outcome counts;
- game-log requested/successful/failed writes, revisions, rewritten bytes,
  clone/serialization/write/sync/rename/atomic durations;
- MP4 packet PTS, packet duration, time base, and keyframe cadence.

## Static verification

Commands were run from the repository root with
`<absolute-path-to-cargo.exe>`:

```text
cargo fmt --manifest-path recorder/Cargo.toml --all
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
```

Results:

- all-target/all-feature check: pass;
- strict Clippy: pass;
- library: 96 passed, 3 intentional live tests ignored, R11 filtered;
- recorder binary: 1 passed;
- examples: compiled and passed their empty unit harnesses;
- new deterministic 60/144/240-Hz timing tests: pass;
- new atomic-write revision/cost tests: pass.

## Live non-League timing audit

Evidence root:

```text
evidence/package0-native-timing-audit-20260818-133702
```

The release `native_mp4_probe` used the pinned r6 FFmpeg/ffprobe runtime and a
dedicated animated 1920x1080 exact-HWND fixture. Each arm scheduled 480 ticks
over eight seconds and injected one worker stall after tick 120. All three MP4
files passed ffprobe and complete video/audio decode.

| Injected stall | Maximum lateness | Catch-up batch | Slot tick drops | Submitted/muxed | Output duration | Admitted sources | Snapshot copies | Processor-state bundles |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 ms | 102.376 ms | 7 | 3 | 477 | 7.950 s | 474 | 474 | 477 |
| 250 ms | 253.603 ms | 16 | 12 | 468 | 7.800 s | 468 | 468 | 468 |
| 500 ms | 507.940 ms | 31 | 27 | 453 | 7.550 s | 449 | 449 | 453 |

The 500-ms arm's packet audit records:

- MP4 video time base `1/15360` and nominal rate `60/1`;
- 453 strictly increasing video packet PTS values;
- packet duration 1/60 second;
- duration exactly `453 / 60 = 7.55` seconds;
- keyframes at 0, 2, 4, and 6 seconds, confirming the current two-second GOP;
- audio and video both start at zero and both end at 7.55 seconds.

## Findings established

1. The current converter applies one complete invariant processor-state bundle
   for every converted output frame. R1 is confirmed directly.
2. Every admitted source was copied into the persistent BGRA snapshot in these
   arms. The current pre-CFR source-copy behavior in R2 is confirmed directly.
3. A stalled worker drains consecutive overdue ticks without interleaving
   source reception. Catch-up batch size scales with stall duration.
4. The clock advances before slot admission. Full-ring failures therefore
   remove encoded samples: three, twelve, and twenty-seven dropped samples
   shortened the outputs by exactly 0.05, 0.20, and 0.45 seconds.
5. Raw-Annex-B CFR muxing produces clean monotonic replay PTS and a stable
   two-second keyframe cadence when frame count is correct. This supports
   transactional tick commit, not silent tick skipping or an immediate mux
   rewrite.
6. Package 0 instrumentation itself did not introduce a decode, resource-bound,
   test, or compiler failure.

## Remaining scope

Control-plane and game-log counters are unit/static verified on PC B. Their
representative match values require the later real League environment. Package
1 may proceed because its processor-state baseline and media gates are complete.
