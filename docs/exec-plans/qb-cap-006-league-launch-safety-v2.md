# QB-CAP-006 — launch-safety review corrections

## Purpose

Correct initial tiny WGC sizing, timing uncertainty, repeated passive DXGI work and partial-startup cleanup while retaining conservative admission and one native graph per process.

## Relevant planning-time architecture

The existing cancellable readiness task samples local GameStats before bundle/native creation. Its target probes currently resolve DXGI twice per sample. Native source creation allocates its frame pool from capture-item Size, and converter creation uses that pool size. Active frame handling already discards unusable NV12 crops. Explicit `NativeWgcSource::close` retains the complete source/device/MTA if callback shutdown cannot prove quiescence; implicit partial-constructor Drop does not provide that guarantee. Worker and lifecycle first-progress deadlines currently treat sustained tiny frames as failed startup.

## Scope and non-goals

Fix only the four reviewed paths, their regression tests and concise canonical evidence. Keep active recording/media integrity, native failure suppression, callback shutdown order, cancellation, the sole backend and storage contracts. Hardware/League compatibility verification remains pending; no benchmark or hardware fixture campaign.

## Exploration findings

A 1×1 item creates a 1×1 pool and can fail converter initialization before existing frame rejection runs. Item Size is initialization metadata; real frame ContentSize drives restoration. Request-midpoint clock differences omit up to half each request's duration. Passive discovery can use physical HWND/PID/size/DPI/monitor snapshots without a DXGI factory. Partial registration needs the full source owner in place before callbacks are installed.

## Chosen design and rationale

- Split lightweight window snapshots from full adapter resolution. Sample PID/HWND, physical dimensions, DPI and monitor before/after requests. Resolve the full target only after readiness, compare fresh snapshots around resolution, reject stale/changed admission and preserve full adapter checks at the native boundary and during active capture.
- Reuse the existing two-second HTTP deadline as maximum RTT. Preserve 250 ms combined fixed clock jitter, but intersect possible game-clock offsets across all samples using each request interval plus half that jitter on each observation. Require positive consecutive progress, stable game/window identity, three samples/four seconds and gameTime ≥5 seconds; retain five-second gap and 500 ms post-response freshness limits. No time-only fallback.
- Select a usable initial pool size using the shared NV12 crop predicate: prefer item dimensions, otherwise the validated target dimensions; reject if neither is usable. Keep the same device/pool/session through transient tiny frames. Check content usability even when frame and pool dimensions agree. While only unusable frames continue arriving before the first copied frame, refresh startup deadlines rather than fail and reinitialize. Missing frames, real errors and cancellation keep their existing bounded stop paths.
- Prepare capture objects without callbacks, construct the complete source owner, then register/start. On any later failure explicitly close that owner after partial dependent converter/mux/encoder locals have dropped. Reuse existing shutdown and full-stack retention on failed quiescence. Log stage, primary error chain and cleanup result, including failures before a complete session exists.

## Rejected alternatives and planning decisions

Do not retry native graph creation for a tiny window, guess a larger clock tolerance, wait on item Size updates, cache an unchecked adapter identity, loosen callback shutdown proof or add a framework. Keep the original plan as immutable history; the user's instruction authorizes this correction and continuous implementation.

## Milestones

1. Correct passive snapshots and interval-aware readiness with focused tests.
2. Correct initial sizing, sustained tiny-frame startup and explicit partial cleanup/diagnostics.
3. Run the recorder component checks once, validate the final diff/artifacts, record hardware limits and commit.

## Verification design

Use deterministic regression tests for request timing/clock drift, frozen/jumped/stale data, resolver call counts and admission replacement, initial invalid/item/target dimensions, tiny-size restoration/deadlines, partial initialization cleanup/error composition, and existing cancellation/resource ownership. Run locked recorder all-target/all-feature check and tests, formatting check and strict Clippy, plus canonical feature validation and diff hygiene. Do not run physical capture fixtures or claim real League compatibility.

## Performance and reliability gates

Zero DXGI resolutions during collecting/unavailable readiness; full resolution only at admission. One native graph retained during an actively progressing tiny-size transition; no native retry loop. Constant bounded readiness state and existing callback/source capacities. Cleanup cannot release the source/device/MTA underneath an unquiesced callback. No user recordings are modified.
