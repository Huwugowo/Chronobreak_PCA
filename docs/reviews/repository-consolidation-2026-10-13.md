# Main checkout consolidation — 2026-10-13

The user explicitly requested reconciliation of the local work and the GitHub
PC handoff into `main`, followed by a commit and push. This supersedes earlier
no-commit/no-push instructions for the preserved feature work.

## Sources and reconciliation

- Local baseline: `a82d81f005823768a21b8aef4a94bd954ce26903`.
- Local replay-opening implementation, reference patch, tests, immutable plan and
  latest campaign checkpoint were preserved in `d201d76` before integration.
- Incoming handoff: `ac98d7397740fe714ae1caf7ce06987f9efa2467`, including GitHub
  main `1aa52446f5ad227379f33a3b8c3d0bb531bb9360`, capture/finalization corrections,
  QB-DATA-003 and the implemented portion of QB-DATA-002.
- Git combined the viewer, viewer tests and roadmap without textual conflicts.
  The integrated viewer retains both opening instrumentation and optional EOG
  facts. The roadmap retains the incoming DATA entries and local REPLAY-010 entry
  exactly; the local replay checkpoint was preserved before this integration note.
- The older September snapshot and historical feature branches were not replayed
  over these current sources. Ignored builds, raw captures, recordings and local
  process state are not transferred through Git.

## Verification

Exact command/exit receipts and logs are under ignored
`build/reconcile-20261013/`. Results on the combined source:

| Check | Result |
| --- | --- |
| Recorder all-target/all-feature check, test, fmt, Clippy | Passed: 172 library, 3 binary and 3 example tests; 6 existing ignored checks |
| Recorder debug build | Passed |
| Recorder release build | Passed |
| App Rust tests, fmt, all-target Clippy | Passed: 99 tests; 1 existing ignored check |
| Shared League-data crate locked test, fmt, all-target Clippy | Passed; standalone crate has no unit tests; producer/consumer tests run in recorder/app |
| Frontend tests | Passed: 231 tests in 25 files |
| Frontend production build, including TypeScript check | Passed |
| Tauri benchmark and normal production builds | Passed; normal build is the final executable |
| Replay Python suite | Passed: 116 run, 115 passed and 1 existing opt-in skip |
| Capture Python suite | Passed: 60 tests |
| Canonical schema/lifecycle/link validation | Passed: 62 items and 10 plan/checkpoint pairs |
| Focused integration review | No actionable merge-specific findings |

The original local reference patch is preserved byte-for-byte, including its
diff-context whitespace. Source whitespace checks recognize Windows CRLF; the
reference patch is data and is excluded from source trailing-whitespace checks.
The existing Vite dynamic-import warning and Windows linker informational warning
remain non-fatal. No product-code correction was needed during reconciliation.

The first PowerShell logging helper treated ordinary native stderr as a terminating
error; it was corrected before rerunning those checks. The standalone shared-crate
test initially could not download its locked dependencies inside the network
sandbox; the permitted network retry passed without dependency changes.

## Remaining feature work

This consolidation is not feature completion. QB-DATA-002 remains in progress for
real primary-local LOSS encoding, explicit delayed EOG and real resource evidence.
QB-REPLAY-010 retains its closed failed campaigns and telemetry diagnosis handoff;
no campaign or native acceptance was rerun. Its frozen subjects describe their
original source states, not this newly combined application. QB-DATA-004 remains
a draft. The canonical feature entries and execution checkpoints own those states.
