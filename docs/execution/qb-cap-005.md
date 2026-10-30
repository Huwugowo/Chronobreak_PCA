# QB-CAP-005 execution checkpoint

Feature: `QB-CAP-005`
ExecPlan: `docs/exec-plans/qb-cap-005-storage-preflight.md`
Updated: 2026-10-31

## Current milestone

Complete.

## Active unit

None.

## Completed

Bootstrap, bounded scout and design review: existing error route, no new dependency,
8 GiB rate-derived floor, resolved games destination, no active-recording changes.
Implemented storage.rs resolution/available-to-caller query and policy, service
insertion before Starting/bundle allocation, three focused tests and lifecycle docs.
Reviewed final diff and existing terminal startup/tray error route. All feature
acceptance criteria satisfied; canonical completion evidence recorded.

## In flight

None.

## Remaining

None.

## Verification

2026-10-31, Windows:

- `cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features storage_preflight`:
  all three focused tests passed. Controlled 0/below/exactly-at/sufficient capacity
  validates the 8 GiB boundary, actionable messages, resolved query path, unchanged
  sentinel media and no new bundle. Missing/query-error paths fail actionably; a
  real GetDiskFreeSpaceExW call on a temporary destination succeeds without writes.
- `cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features`: passed.
- `cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features`:
  255 passed across library/binary/example targets, 12 existing opt-in tests ignored.
  Initial restricted run had nine fixture failures opening ancestor `C:\Users\Hugo`
  with access denied (239 passed/12 ignored in the library). The same full command
  passed with approved filesystem access; no code/test weakening. Ignored logs:
  `build/qb-cap-005/check.log`, `build/qb-cap-005/test.log` (successful rerun).
- `cargo fmt --manifest-path recorder/Cargo.toml -- --check`: passed.
- `cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings`: passed.
- Canonical schema/invariant/link validation passed (105 items, 26 plan/checkpoint
  pairs); `git diff --check` passed.
- Inspection: service guard is terminal and precedes Starting, bundle allocation
  and native initialization. Existing service failure handling emits one actionable
  ServiceEvent::Error and the tray uses it for its error tooltip. No new UI,
  retention call or active-recording check exists. Interactive low-disk UI and
  hardware capture were not run and are not claimed by this scoped verification.

Initial test invocation hit the shell logging hook's denied write under
`.codex/logs`; the installed Cargo executable ran directly without that wrapper.

## Deviations

Continue from planning into implementation in this session to fulfill the explicit
user request to implement and commit. User requests focused tests only; replace the
generic manual verification item with actionable-message tests and inspection of
the unchanged existing tray error route before implementation, retaining every
acceptance criterion. No live low-disk/UI or hardware campaign is required.

## Decisions

8 GiB is a critical-start floor (~63 minutes at current maximum combined bitrate),
not a full-match guarantee. Low space/query errors are terminal for this process
generation and never trigger cleanup.

## Blockers

None.

## Next action

No further feature action. Retain this checkpoint as verification history.
Unrelated untracked `tools/audio/` files are outside this change.
