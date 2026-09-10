# QB-REPLAY-011 execution checkpoint

Feature: `QB-REPLAY-011`
ExecPlan: `docs/exec-plans/qb-replay-011-local-playback-server-hardening.md`
Updated: 2026-09-10

## Current milestone

M1-M4 complete: implementation, review, app checks and production verification.

## Active unit

None. All acceptance gates are satisfied with the explicit aggregate I/O
disposition below. The baseline was built from `e030161`; after baseline capture,
the staged executable was replaced with the verified hardened production build.

## Completed

- Product/workflow/feature/dependency/architecture/verification bootstrap read.
- Read-only server, consumer and benchmark scouts resolved the implementation
  surface. Plan review accepted the design after specifying consistent opened
  directory/file identities for Windows containment.
- Finalized ExecPlan and initialized this checkpoint. The user's explicit request
  authorizes both planning and execution; continue after the planning handoff.
- Before/after immutable specs created beneath the existing dedicated generated
  v2 sentinel. Before plan generated and first launch preflight passed.
- Baseline r2 completed all ten runs; aggregate report is valid with ten trials.
- Capability/Host/Origin/CSP admission, HTTP connection/header limits, opened-file
  containment, imported-preview revocation and bounded cache downloads implemented.
  Durable threat model: `docs/architecture/local-playback-security.md`.
- Implementation review resolved the debug capability log and clarified the
  approved-location boundary. Benchmark-enabled app tests passed 67/67, frontend
  tests 86/86, analyzer/runner/planner fixtures 82/82 and frontend build passed.
- Normal Rust tests, both Clippy configurations, formatting, production route
  probe and the twenty-trial comparison completed. One valid aggregate I/O flag
  accepted with its attribution limit and measured evidence preserved.
- Sanitized reports, exact route observations, range extraction and disposition:
  `docs/performance/evidence/qb-replay-011-20260910/README.md`.

## In flight

None. All benchmark processes ended cleanly; the unrelated sibling debug app was
left alone. The probe sentinel is
`build/perf/qb-replay-011-delivery-probe/.chronobreak-replay-benchmark`; its JPEG
support asset has a separate SHA-256 receipt alongside prepared bundle/clip/cache
identities.

## Remaining

None for QB-REPLAY-011. QB-REPLAY-009 audible/A-V observation remains intentionally
deferred; range/scrub tail sample limits and optional GPU absence remain explicit.

## Verification

- 2026-09-10: `npm.cmd run desktop:build:benchmark --prefix app` passed against
  pre-change tree; optimized executable copied to `build/release/queueback` for the
  prepared template. Only normal linker-output warning. Earlier Sep9 build is
  superseded for baseline identity by this current-tree build.
- Matrix planner initially rejected relative template/spec paths; no plan or run
  was created. Supplying resolved absolute paths passed. No product defect.
- `run.ps1 -Manifest <first-before-launch> -PreflightOnly` passed: sentinel safety,
  fresh result root, packaged runtime, app/analyzer and six fixture-file identities.
  This initial preflight preceded the later valid live trials.
- Sentinel: `build/perf/qb-replay-012-webview/.chronobreak-replay-benchmark`.
  Specs: `matrix-inputs/qb-replay-011-{before,after}-20260910-r2.json`.
  Before plan: `matrix-plans/qb-replay-011-before-20260910-r2/matrix-plan.json`.
  Generated native H.264/AAC fixture `native-current-v2`, 240 seconds; no user media.
- First r1 seek trial exposed an analyzer assumption contradicted by the existing
  controller contract: RVFC may precede seeked. Seek-139 presented at 61040.0513 ms,
  seeked at 61041.7513 ms, both after dispatch 60916.6513 ms; 18/160 actions had
  this order. Analyzer now requires both completions after dispatch without
  ordering the completions against each other. Added positive/negative fixtures;
  38 analyzer tests passed. Preserved r1 reanalysis is valid under
  `reports/qb-replay-011-before-first-reanalysis`. The original runner stopped
  before its completion receipt, so a fresh r2 matrix was necessary; no receipt
  was reconstructed. All ten r2 receipts and the valid aggregate are retained at
  `reports/qb-replay-011-before-20260910-r2/report.json`.
- Initial Rust invocation was blocked by the configured command logger's sandbox
  write; escalated execution compiled and exposed unsupported BitOr on Windows
  path flags (both requested flags are zero). Using FILE_NAME_NORMALIZED alone
  preserves normalized DOS semantics. Focused tests then passed 19/19 before the
  newly added wire/file tests. No product tolerance or retry was added.
- New wire tests initially returned 404 for authorized routes. Axum Router layers
  run after matching, so rewriting a URI there was too late. Admission now wraps
  an outer fallback before the inner media router matches; hostile-origin and
  resource tests then passed. The remaining HEAD assertion used reqwest's body
  size hint, documented as distinct from Content-Length; changed it to assert
  the actual wire header. Full benchmark-enabled app suite then passed 67/67,
  including every route, Windows junction/race checks, and ten-second header bound.
- 2026-09-10 `npm.cmd run test/check/build --prefix app` passed (86 tests),
  `python -m unittest discover -s tools/replay_benchmark/tests -v` passed 82 tests.
  npm test/build logging required escalation after denied `.codex/logs` writes.
  Probe preparation first rejected relative command paths before copying any
  fixture; corrected to absolute paths as required by the runner contract. Its
  result leaf then failed the run_id contract, also before copying. Corrected
  that spec field; preparation passed for both explicit video fixtures with
  matching source hashes. JPEG remains a separately fingerprinted support asset,
  not a falsely classified negative video fixture.
- Normal app tests passed 55/55; `cargo fmt ... -- --check` and both required
  `cargo clippy ... --all-targets [--features replay-benchmark] -- -D warnings`
  configurations passed. Baseline matrix verifier confirmed two arms, ten trials,
  ten complete results with order/cooldown/integrity receipts.
- Hardened production Tauri build passed (optimized, 1m51s); staged executable
  SHA-256 `e150e2394b0d048eb0da2fe0fc2e1b41a7767d64cd4b7aa9f4fa2994c4d59d9f`.
- Packaged `delivery-route-probe` run passed: clean exit, analyzer accepted,
  prepared source hashes and separately recorded JPEG support hash unchanged.
  Actual origin was `http://tauri.localhost`; all seven variants passed HEAD 200
  and range GET 206. Native loads passed except the unavailable HEVC probe.
  Artifacts: probe sentinel `results/qb-replay-011-delivery-probe-r1`.
- No capability URL matches in JSON/JSONL/log artifacts from the eleven
  post-change production bundles. No new decoder or audible observation claim.
- Canonical schema/invariants/links validated: 60 items and seven plan/checkpoint
  pairs. `git diff --check` passed.
- Post matrix verified two arms/ten trials/ten results; aggregate is valid with
  identical comparison fingerprints and twenty total preserved matched trials.
  Sole analyzer flag is seek `io_write_bytes_delta`; no user seek latency,
  cancellation, CPU/memory or new sustained-growth flag requires disposition.
  Report: main sentinel `reports/qb-replay-011-after-20260910-r2/report.json`.
- Existing game GET 206 records independently show first-body-read median
  0.2852 -> 0.47695 ms for seek and 0.34515 -> 0.40065 ms for scrub; both within
  the unchanged 5 ms floor. Game-range cancellation medians 4 -> 4 and 2 -> 1.
  Range tails lack forty samples and are explicitly ineligible. Extraction is
  retained in `docs/performance/evidence/qb-replay-011-20260910/range-delivery.json`.
- Accepted sole comparison flag: seek job I/O write bytes 28,897,294 -> 38,468,533
  (+9,571,239, 33.12%; band 3,171,576). Collector inspection establishes aggregate
  Job IO_COUNTERS WriteTransferCount, including exited children; retained samples
  cannot attribute native app versus WebView or disk versus IPC/network. All seek
  trials retain ten requests/18,179,774 declared bytes; delivered median rises
  only 339,968 bytes (3.08%), write-operation median 159,272 -> 160,591 (0.83%).
  With all latency, cancellation, CPU/memory, source-integrity and new-growth gates
  passing, accept this observed aggregate byte increase and attribution limitation
  for foundation hardening. Do not claim its cause is known or that the analyzer
  had zero flags. Details and all ten trial counters are in `io-disposition.json`.

## Deviations

Source edits overlap capture/diagnosis of the immutable copied baseline executable.
No build or CPU-heavy check ran during measurement. Paused-control seek and playing
scrub retain the planned measurement boundary.

## Decisions

- Preserve unrelated local skill changes and `.codex/`. QB-REPLAY-009 changes from
  the earlier turn are now committed in current repository state.
- One 64-connection HTTP/1 bound also bounds active request bodies/streams; no
  duplicate stream semaphore or per-file registry for ordinary library media.
- Five-second warmup/cooldown, seed 20260910 and same immutable scenario definitions
  apply to both arms. Do not compile or run CPU-heavy checks during measurement.
- QB-REPLAY-009 audible/A-V observation is explicitly deferred and non-blocking.
- Review found a debug log of the new capability base URL; removed it. Proposed
  file-ID/directory-handle pinning against same-location filesystem replacement
  exceeds the approved local boundary: imports revoke on a new registered
  selection, and containment approves resolved locations. Clarified the durable
  threat model without adding an owner-level filesystem sandbox.
- The versioned comparison permits explicit accepted limitations. Preserve the
  aggregate I/O flag and unresolved attribution; adding destination/process
  tracing or speculative delivery changes is disproportionate to this feature.
  Short scrub thread/working-set growth flags existed before and remain unchanged;
  no new sustained-growth signal appeared. No soak or long-recording claim.

## Blockers

None. The earlier analyzer rejection is resolved from controller evidence; the
valid r2 baseline is preserved. The live debug app in sibling Chronobreak-ui is
unrelated and must not be stopped or modified.

## Next action

No implementation action remains. Resume another feature through the normal
workflow; retain these reports and the immutable plan for provenance.
