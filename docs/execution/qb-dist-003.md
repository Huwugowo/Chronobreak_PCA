# QB-DIST-003 execution checkpoint

Feature: `QB-DIST-003`
ExecPlan: `docs/exec-plans/qb-dist-003-installed-settings.md`
Updated: 2026-11-24

## Current milestone

Milestone 4, verification and installed acceptance.

## Active unit

Exercise the installed settings and login workflow when the installer reaches installed acceptance.

## Completed

Shared Windows validation and field-level persistence, legacy repair UI, authenticated effective settings, idle apply and guarded recovery of original libraries. Local automated verification and the staged-media fixture passed.

## In flight

None. Installed acceptance is pending.

## Remaining

Verify a real active capture and finalization through an installed destination change. Verify old-library enrichment and actual login registration on the installed pair.

## Verification

The Windows desktop suite passed 232 tests, including concurrent recording, storage, login and HEVC writes and legacy preference repair. Frontend type check and all 643 frontend tests passed. The recorder suite passed 307 tests across targets, including 69 League client tests. Those cover original-library result, history and ranked LP recovery and refusal to reopen a replaced library. The staged-runtime integration verified that saving B leaves A effective, explicit apply stays queued while a disposable watched process exists, B becomes effective when that process exits, and invalid settings leave B effective. Recorder-control passed 16 tests, including rejection of settings access from a different executable and acceptance from the sibling desktop. Strict Clippy passed for the desktop, recorder and recorder-control. The full fixture runs require normal Windows permissions to pin publication directories and exercise a disposable registry value.

The five-minute recorder-control reliability check passed. Handle and thread counts remained within the existing bounds across 60 simulated observer and launch cycles. The optimized Windows desktop build with bundling disabled also passed.

## Deviations

None.

## Decisions

Reuse the recorder lifecycle and preserve post-game work at its original root. Start with the shared configuration contract.

## Blockers

The real League/NVIDIA run is an installed acceptance dependency. It does not block deterministic integration work.

## Next action

Once the installed pair is available, exercise outstanding acceptance through a real match and finalization.
