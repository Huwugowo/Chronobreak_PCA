# QB-CAP-006 — capture-window startup admission

## Purpose

Start video when League has a stable usable capture window, independently of gameplay telemetry. The October 31 trial waited until gameTime 36.6 seconds for Live GameStats; native initialization itself took about 0.6 seconds.

## Relevant planning-time architecture

`launch_readiness.rs` gates bundle/native allocation. `service.rs` retains an exact-process `OwnedHandle`, cancels and joins old startup on process exit, and checks target visibility/identity immediately before native allocation. The native worker already enforces a shared 15-second startup deadline, five-second initial-source inactivity, suitable first-frame sizing and strict callback-safe cleanup. CAP-009 separates an established recording from its replaceable WGC source.

## Scope and non-goals

Replace mandatory Live-clock readiness with physical-window stability and remove fixture-only fake GameStats admission. Preserve process ownership, native construction, active capture, publication and failure suppression. Do not attribute or claim to fix the black screen, change audio/library behavior, or introduce an overlapping-process selection policy. The reported sequence was Alt+F4 followed by relaunch.

## Exploration findings

The Live gate can delay or prevent otherwise available capture. Window snapshots already include PID, HWND, physical client dimensions, DPI and monitor without allocating a graphics device. No evidence establishes gameplay-clock progression as a necessary capture safety condition. A stable window is useful admission evidence, not proof of driver safety or absence of interference.

## Chosen design and rationale

Require two usable, identical window observations spanning at least the existing two-second watcher cadence. Missing/hidden/minimized/tiny windows clear evidence; changed identity/geometry or observation gaps over five seconds restart it. Resolve DXGI only after stability, then recheck the exact snapshot and cancellation; resolution/revalidation must remain fresh within 500 ms. Expected window absence defers; real observation/adapter failures propagate rather than silently retrying graphics queries. No Live request occurs in admission. The existing post-video poller remains optional and unchanged.

Keep native startup deadlines, first-frame checks, cleanup ownership and one native attempt per process generation unchanged. A restarted process earns fresh window evidence. A HWND change during native construction may still fail that startup safely; established-source replacement remains CAP-009 behavior.

## Rejected alternatives and planning decisions

Reject instant admission from one HWND, gameplay-clock requirements, timeout fallbacks, GPU retry loops and speculative owner-handoff changes. Two observations provide positive stability evidence without retaining an unsupported four-second minimum. Preserve immutable predecessor plans; the user's explicit startup correction and workflow override authorize implementing this successor directly.

## Milestones

1. Replace the readiness reducer and its service/fixture consumers; update the two architecture passages.
2. Run focused readiness regressions and targeted Rust checks, record actual results and commit.

## Verification design

Use deterministic paused-time tests for stable-window admission without a Live source, unavailable/tiny/replaced windows, geometry/monitor changes, final-resolution races, stale observations, cancellation and real adapter/query errors. Retain existing process exit/restart and native failure/cleanup regressions. Run locked recorder all-target/all-feature check/tests, fmt, strict Clippy and canonical state validation. No new framework or long hardware campaign.

## Performance and reliability gates

No new worker, polling cadence, GPU attempt or per-frame work. DXGI remains admission-only. Exact process identity, guarded device/MTA lifetime, callback quiescence, failure precedence and publication validation remain unchanged. Report real League launch behavior and unavailable hardware verification honestly; long performance investigations do not block this focused implementation.
