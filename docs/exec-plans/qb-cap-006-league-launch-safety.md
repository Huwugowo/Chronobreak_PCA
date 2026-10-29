# QB-CAP-006 — League launch safety

## Purpose

Defer automatic Windows recording until local game signals and window stability positively indicate readiness. League stability takes priority over opening footage; unavailable readiness can suppress an entire recording.

## Relevant planning-time architecture

`service::start_recording` owns cancellable startup and currently creates the bundle before `NativeRecordingSession::start` initializes WGC/D3D11/NVENC. The process watcher cancels startup on process changes. `HttpSource::local` already bounds local Live Client requests to two seconds and GameStats bodies to 64 KiB; `decode::live` validates gameTime. Window discovery and target validation use HWND/PID, visibility, dimensions and adapter identity without starting capture. Descriptor generation increments on every discovery and cannot serve as stable identity. Active tiny-frame handling and native cleanup remain unchanged.

## Scope and non-goals

Add only a pre-native readiness gate, process creation identity, focused tests and fixture compatibility. Do not redesign active capture, association, recovery, backends or performance measurement. This preventive change does not establish causality for the historical loading incident.

## Exploration findings

Live polling currently starts after native initialization. A visible 320×200 window alone can therefore trigger native startup while loading. The existing generated service fixture intentionally has no Live API and needs an explicit constructor-level fixture source to retain its active-capture coverage. No production configuration or environment bypass is appropriate.

## Chosen design and rationale

Inside the existing cancellable startup task, before bundle creation or native initialization, poll existing local GameStats every two seconds. Require three consecutive valid samples spanning at least four monotonic seconds, gameTime at least five seconds, stable map/mode, and positive game-clock deltas matching monotonic request midpoints within 250 ms. Reject requests taking more than 500 ms and gaps above five seconds. These conservative thresholds intentionally lose opening footage and can defer on overloaded machines.

Rediscover and validate the visible target before and after each request. Bind evidence to PID, HWND, physical dimensions, DPI and adapter/output; ignore the per-discovery descriptor generation. Any unavailable/malformed signal, hidden/closed/tiny window, identity change, clock freeze/regression/jump or stale observation resets accumulated readiness. Return the final verified target, never the old descriptor. Waiting has constant memory, never creates a capture graph and remains immediately cancellable during requests and sleeps. Announce capture Starting only after readiness.

Include sysinfo process start time in `LeagueProcess` equality so observed PID reuse resets startup/suppression. This is a practical second-resolution process identity, combined with immediate HWND validation, not atomic containment of process replacement. A genuine native initialization failure is terminal for that process identity after existing cleanup; no repeated graphics initialization. A new process can try after fresh readiness. Same-process window restoration/reconnect before capture must earn readiness again; existing active-session terminal behavior remains conservative.

Log sanitized decision transitions with monotonic elapsed time, PID/process start time and semantic window identity. Correlate admission with descriptor generation and existing native item/frame/startup/cleanup diagnostics; never log response bodies or credentials. Reuse the existing fixture-only build feature for a generated advancing GameStats source injected through a dedicated service constructor, unavailable through normal CLI/configuration.

## Rejected alternatives and planning decisions

Elapsed launch time, HWND size alone, one LCU phase and failure-open timeouts do not provide positive readiness. Do not initialize capture to measure readiness. Do not retry native startup indefinitely. Do not add another HTTP client, background supervisor, dependency, backend or active-recording restart mechanism.

## Milestones

1. Implement the bounded readiness reducer/waiter and deterministic progression, reset, freshness and cancellation tests.
2. Integrate before native startup, process identity and terminal failure policy; preserve generated fixture coverage through explicit injection.
3. Run recorder component checks, inspect ordering/diff, and retain concise implementation evidence and remaining live-validation limits.

## Verification design

Run `cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features`, `cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features`, `cargo fmt --manifest-path recorder/Cargo.toml -- --check`, and `cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings`. Test stable advancing samples, missing/invalid/unavailable data, latency/gaps/frozen or regressing time, map/window/dimension/adapter changes, cancellation in a pending request, process reuse and native terminal suppression. Validate canonical feature artifacts and `git diff --check`.

The existing acceptance requirement for repeated controlled Practice Tool absent/passive/native comparisons remains a separate live verification gate. Do not claim it passed without execution; retain non-done state if unavailable. No benchmark campaign is required for this implementation task.

## Performance and reliability gates

Before admission there are zero WGC/D3D11/NVENC starts and no recording bundle. One bounded request per two-second cadence and constant-size state; cancellation does not wait for request completion. All uncertainty defers capture. Native failures cannot cause repeated initialization for the same process. No real user media is modified by verification.
