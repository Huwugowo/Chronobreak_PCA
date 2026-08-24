# Recorder remediation Package 2 evidence

Date: 2026-08-18

## Scope

Package 2 reduces recorder control-plane work without changing recording media
or weakening target identity and adapter safety.

The process watcher still takes one all-PID snapshot so a new League process
can be discovered by executable name and dead entries can be removed. It now
requests only the invariant identity fields supplied by `sysinfo` (PID, parent,
name, and start time), with task, CPU, memory, disk, executable, and other
metadata refreshes disabled. Refresh counts, touched records, and cumulative
and maximum durations are exposed in structured control telemetry.

Windows target supervision now issues one combined HWND/PID/visibility query
per active service tick. The cache is populated before recording startup and
retains the last validated visible client rectangle. Monitor and DXGI adapter
enumeration occurs only when a visible rectangle differs from that cache.
Minimization retains the cache and continues to pause the watchdog. Close, PID
ownership change, and movement to another adapter remain terminal.

Two release-only probes preserve reproducible evidence for process refresh and
Win32 target-state behavior. A Package 0 stress helper also received a missing
feature gate so ordinary release probe builds no longer report an unused-code
warning; runtime behavior is unchanged.

## Static verification

The following passed:

```text
cargo fmt --manifest-path recorder/Cargo.toml --all
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
cargo build --manifest-path recorder/Cargo.toml --release --example capture_target_state_probe --example process_watcher_probe
git diff --check
```

Results: 99 library tests and one recorder-binary test passed, including the
minimal-refresh, process-transition, target-cache, and watchdog tests. Three
intentional live tests remain ignored and only the unavailable R11 empirical
fixture is filtered. All five examples compile, and the ordinary release probe
build is warning-free.

## Process refresh comparison

Evidence:

```text
evidence/package2-process-refresh-ab-20260818-140530
```

The Package 0 default `sysinfo` refresh and Package 2 minimal refresh ran in
ABBA order. Each arm executed 1,000 post-warm-up refreshes over the same 234
known processes.

| Implementation | Arm averages | Mean refresh |
| --- | --- | ---: |
| Package 0 default refresh | 3.8852 ms, 3.9466 ms | 3.9159 ms |
| Package 2 minimal refresh | 2.5729 ms, 3.3036 ms | 2.9383 ms |

The measured refresh cost falls 24.96%, or 0.978 ms per two-second service
tick on this machine. Discovery still scans every PID/name and the existing
appear/replace/disappear tests pass; this is not a cached-PID shortcut.

## Combined target-state lifecycle

### Same-adapter resize and minimize/restore

Evidence:

```text
evidence/package2-target-state-same-adapter-20260818-140705
```

A dedicated non-League HWND remained on NVIDIA `DISPLAY5` through two visible
bounds changes and a two-second minimize/restore interval. Across 150 queries:

- 130 reported visible and 20 reported paused;
- only three monitor/DXGI validations occurred: initial plus the two commanded
  visible bounds changes;
- 147 queries reused either the validated bounds or the minimized state;
- restore resumed visible supervision without losing exact HWND/PID identity.

In the actual service the same cache is populated immediately before startup,
so a steady active tick does not repeat the initial DXGI factory/adapter scan.

### Cross-adapter straddling

Evidence:

```text
evidence/package2-target-state-20260818-140604
```

PC B has two screens on different adapters. The fixture began on NVIDIA
`DISPLAY5` at LUID `0000000000013191`. When an enlarged/straddling rectangle's
largest monitor intersection resolved to LUID `0000000000012dcc`, the query
terminated with the explicit cross-adapter rediscovery error.

This is the safe current policy and a known uncommon limitation: a recording
cannot continue when the window's selected monitor moves to the other adapter,
because its WGC/D3D11/NVENC resources are bound to the starting adapter. A
future hysteresis or adapter rebind must be designed with the deferred
multi-segment restart contract; it must not silently introduce cross-adapter
copies in this package.

### Target close and ownership

Evidence:

```text
evidence/package2-target-state-close-20260818-140747
```

Terminating the dedicated fixture caused the next query to fail with `the
selected League HWND was closed`. The combined query still reads the current
window PID on every tick before consulting cached bounds, so an HWND reused by
another process cannot inherit the cache. Existing process replacement and
disappearance tests pass. Deterministic live same-handle reuse was not forced
because Windows controls handle allocation.

## Decision

Keep Package 2. It removes unnecessary periodic process metadata work and
eliminates repeated steady-state DXGI enumeration while preserving the exact
window, minimize, close, process replacement, and cross-adapter safety
contracts. It does not change timestamps, capture admission, codecs, muxing,
polling cadence, or replay semantics. Package 3 may proceed independently.
