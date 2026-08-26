# PC-B verification guide

This is the short command-routing guide for the provisional PC-B repository.
Use it when the task requires verification; it is not a substitute for the
task-specific ExecPlan or detailed evidence reports.

## Rust and static checks

Run from the repository root. Use the explicit Cargo path if `cargo` is not on
PATH:

```powershell
$cargo = '<absolute-path-to-cargo.exe>'
& $cargo fmt --manifest-path recorder\Cargo.toml -- --check
& $cargo check --manifest-path recorder\Cargo.toml --all-targets --all-features
& $cargo clippy --manifest-path recorder\Cargo.toml --all-targets --all-features -- -D warnings
& $cargo test --manifest-path recorder\Cargo.toml --all-targets --all-features
```

Run focused tests named by the active plan before the full suite. Do not use a
filtered or skipped suite as proof of a full gate unless the limitation is
recorded in the result.

## Native recorder fixtures

Use the scripts under `tools\native_backend\` and create a new ignored
timestamped evidence root for every run. Never overwrite an earlier evidence
root. Keep League closed on PC B.

The staged runtime must be:

```text
build\media-runtime\windows-x86_64
```

It must validate as the embedded r6 lock. Do not substitute system FFmpeg.
The matched preliminary A/B runner is:

```powershell
$runtime = (Resolve-Path 'build\media-runtime\windows-x86_64').Path
& powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File tools\native_backend\run_preliminary_backend_ab.ps1 `
  -MediaRuntimeRoot $runtime -PreflightOnly
```

Only continue to a 240-second pair after the preflight prints
`CHRONOBREAK_PRELIMINARY_AB_PREFLIGHT=PASS`. The detailed A/B procedure and
results are in `docs/archive/pc-b-recorder-development/PRELIMINARY_BACKEND_AB.md`.

## External gates

The following are not currently satisfied by PC-B and must not be represented
as passed:

- the QB-PERF-002 schema-v2 PerfProc process counter preflight;
- three independent heartbeat-free QB-PERF-002 League captures;
- actual League lifecycle acceptance and the M8 backend comparison;
- the R11 empirical `live-client-capture-20260730-110603.json` fixture.

The diagnostic detail is retained in `docs/archive/pc-b-recorder-development/PC_B_NON_LEAGUE_VERIFICATION.md`.

## Evidence policy

Evidence reports under `docs/` explain completed work; they are optional deep
dives, not default onboarding. New claims require fresh commands run against
this repository and a new immutable evidence root.
