# QB-DIST-002 execution checkpoint

Feature: `QB-DIST-002`
ExecPlan: `docs/exec-plans/qb-dist-002-recorder-supervision-and-login-v2.md`
Updated: 2026-10-28

## Current milestone

M1-M4 product implementation and M5 automated verification/documentation are complete.
Canonical status remains ready/in-progress because manual acceptance is outstanding.

## Active unit

No product-code unit is active. The narrow supervisor follow-up to e79f295 is
implemented and validated. The original Windows manual acceptance gates remain.

## Completed

- Reviewed v2 tightens the approved service/capture, latest-under-lock field mutation,
  other-session capability and stable-path/update contracts. Original v1 is preserved.
- M1: recorder-control crate, profile lifetime lease, restricted bounded local pipe,
  OS peer/image/session validation, versioned independent service/capture status.
  Ten real child contenders are excluded; crash releases ownership without unlink.
- M2: normal tray/headless acquire ownership before config/capture; diagnostics and
  tray smoke avoid production ownership. Typed capture lifecycle leaves retry and
  per-generation terminal errors service Available. Actual service initialization,
  exit/panic/control failure can fail availability. Headless selects early completion
  against Ctrl+C; existing finalizer ordering remains authoritative.
- M3: one serialized desktop ensure/observer, cached typed status/Retry, sibling-only
  console-free launch, no close/timeout kill or restart loop. Other-session status
  never confers local capability. Benchmark/explicit synthetic opt-out gate every
  lifecycle/registration operation; maintained preprod launcher sets the opt-out.
- M4: actual-versus-desired login read/update IPC and Settings control, exact owned
  HKCU Run command, bounded quoting, foreign conflict, verified write and conditional
  rollback. Every config writer (storage/HEVC/login/recorder creation) uses the same
  adjacent lease, fresh read under lock, field-only patch and atomic publication.
  Both parsers preserve unknown fields; merged config/root publication is coordinated.
- Current architecture and verification docs describe implemented boundaries and the
  QB-DIST-004 stable sibling path, exact Run migration and tray Quit/actual-exit handoff.
- Follow-up: successful launch requests alone project Starting; explicit launch errors
  project Unavailable with actionable Retry. An occupied lease with unavailable status
  remains uncertain and cannot be replaced. A known ServiceFailed owner has Retry
  disabled until exit. Launch arbitration, shutdown and benchmark gates are unchanged.

## In flight

No automated check or build remains in flight. No production Run value, user config
or real recording has been changed by verification. Dedicated generated roots only.

## Remaining

- Actual portable desktop repeated launches/app-close during recording/tray Quit
  observation; generated service/observer shutdown evidence is distinct.
- Actual enable/sign-out/sign-in/disable/sign-in under a disposable Windows account.
- Real same-user multi-session observation when a disposable environment is available.
  Injected session projection is automated evidence, not a real second session.
- Do not mark done until required manual gates and unchanged acceptance criteria pass.

## Verification

All results below are 2026-10-28 on Windows; exact commands are in the approved plan
and docs/development/VERIFICATION.md. Detailed artifact root: build/verification/qb-dist-002/.
The result.json artifact records staged normal release sizes/SHA-256 identities,
test logs, resource samples, generated capture and the unrun manual gates.

- Narrow supervisor follow-up: `cargo test --manifest-path app/src-tauri/Cargo.toml
  recorder_supervisor::tests -- --nocapture` passed all 6 tests (173444.log), including
  real Windows spawn failure, held lease with no status, launch readiness, failed-owner
  Retry rejection, other-session/capture separation and disabled/no-runtime construction.
  App all-target Clippy passed (173320.log); app formatting and diff hygiene passed.
  The first test compile caught an incorrect event variant in the new test; corrected
  before the passing run. No benchmark, release rebuild or manual acceptance rerun.
  The remaining results and staged release identities below are the e79f295 baseline.
- Shared control all-target suite: 5 unit + 6 real Windows integration tests passed;
  opt-in soak excluded from the normal suite. Latest log .codex/logs/command-20261028-170615.log.
  Includes ten child contenders/crash release, lease deletion exclusion, image/protocol
  rejection, silent-client deadline, bounded shutdown and disposable HKCU round-trip.
- Five-minute generated control-owner/observer/launch-trigger acceptance passed:
  303.99 seconds, 60 samples, baseline 150 handles/6 threads, peak 153 handles/6
  threads, final 151 handles/3 threads. Log .codex/logs/command-20261028-165847.log.
  This measures the control boundary only, not app-wide resources or capture FPS.
- Recorder all-target/all-feature suite passed: 195 library tests (9 opt-in ignored),
  3 binary tests and 3 example fixture tests; log .codex/logs/command-20261028-165433.log.
  Added actual headless initialization-failure regression passed (170910.log).
  Recorder check all-target/all-features passed (164255.log); final recorder Clippy
  passed (171130.log). Shared final Clippy passed (170449.log).
- Desktop suite passed 157 tests, 9 opt-in ignored (165406.log). Added final config
  tests passed 7, including three real stale child writers and recorder/app parser
  interoperability, creation recheck, unknown nested fields and inline TOML preservation
  (170803.log). Final desktop Clippy passed (171115.log).
- Frontend suite passed 557 tests/40 files (165449.log); added Settings tests passed
  3 for legacy true/absent Off, actual matching/desired false On, and foreign/other-session
  disabled actions (170231.log). Typecheck and production build passed (165642.log).
- Generated production-service native capture passed for ten seconds with isolated
  lease/pipe/output: build/qb-data-003/capture-fixtures/330ca226-2487-411a-a261-7bc9b89e1ac8/.
  Actual status was Available/Recording, survived observer close/reopen, then intentional
  Shutdown reached Stopped with canonical media/metadata. Locked ffmpeg strict full
  decode (-v error -xerror -threads 1) passed. This is not interactive Tauri/tray acceptance.
- Final normal desktop and benchmark desktop builds passed with the inline-TOML
  compatibility change. Release recorder build passed (165931.log); its later delta
  adds only a regression test/formatting. The refreshed normal portable pair was
  staged before the benchmark build replaced the development app executable.
  Paired staging and locked r6 runtime verification passed at build/release/qb-dist-002/.
- All three Rust formatting checks passed. Canonical validator passed 102 items and
  22 plan/checkpoint pairs. git diff --check passed after normalizing recorder/config.rs.

## Deviations

- Pipe reconnection initially failed/hung: recreating a first-instance listener while
  clients retained handles was invalid. Corrected by creating one waiting instance
  before closing each request instance (maximum two). Fixture retries are bounded;
  no product retry/fallback was added.
- Focused review found recorder rejection of nested fields preserved by the app.
  Both parsers now share forward-compatible defaults; cross-consumer fixture passes.
- A compiler failure identified the existing settings persistence safety test's old
  helper signature; updated its invocation while retaining its unchanged invariant.
- Initial approval-review usage-limit and logging access/collision failures were
  resolved and checks subsequently ran. They were not code-test failures.
- Manual acceptance does not block independent implementation or automated checks.

## Decisions

- Recorder owns its independent tray/service; app close releases observation only.
- Capture retry/suppression remains in the recording service; status never parses
  Error text to infer death or restarts capture through the app.
- No uncertain/other-session owner is killed or replaced. Status v1 has no shutdown command.
- Reads never reconcile dormant autostart preferences or Windows registration.
- QB-DIST-004 owns installer payload and clean-machine certification. It must retain
  stable siblings/Run path, await user-mediated tray Quit/actual finalization, preserve
  a usable paired install on update failure, and preserve user config/media on uninstall.

## Blockers

No automated/product-code blocker remains established. Manual login needs a
suitable disposable Windows account; it has not been exercised. Actual portable
UI/tray acceptance and real multi-session evidence remain not run. Current installer
recorder omission remains QB-DIST-004. None of these blocks further independent code work.

## Next action

For later manual acceptance, rebuild and stage the corrected normal app with the
paired recorder (the existing staged app predates this correction). Then use the
normal portable pair in a disposable Windows environment for repeated
launches, app-close during capture and tray Quit, then actual login enable/sign-in/
disable/sign-in. Exercise a real same-user second Windows session when available.
Record observations here; only claim done after the required manual gates pass.
