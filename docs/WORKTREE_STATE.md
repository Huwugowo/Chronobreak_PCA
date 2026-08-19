# PC-B repository state

This is the short live-state document for the canonical repository. Update it
when milestone status, external gates, or the immediate implementation focus
changes. Detailed proof belongs in the evidence reports.

## Workspace

- Repository: `C:\Users\Hugo\Documents\perso\chrnbrk\chronobreak-recorder-pc-b`
- Active branch: `pc-b/qb-perf-005-native`
- Product boundary: provisional PC-B recorder worktree; not the complete PC-A
  product repository.
- Runtime source: tracked `media-runtime/`, locked r6.
- Staged runtime: `build/media-runtime/windows-x86_64` (generated and ignored).

## Current working state

- M1-M6 and the non-League portion of M7 are provisionally complete.
- The preliminary synthetic backend A/B passed in both orders and favors native
  on CPU and memory. It is not the M8 League decision.
- The original recorder remediation is implemented and evidenced through
  Package 8 at commit `21ffa82`. The active successor plan is
  `docs/exec-plans/active/qb-perf-005-post-audit-follow-up.md`.
- Package 9 observability is the next implementation unit. Inspect `git status`
  and `git diff` before editing so every package remains independently
  attributable and existing work is never discarded.
- The active plan and fresh Git state determine the exact next implementation
  package. This summary is not a second task backlog.

## Hard constraints

- Never start League on PC B.
- Keep both recorder backends and the developer selector until the real M8
  League comparison decides between them.
- Use only the pinned r6 runtime for comparative evidence; do not substitute
  stock FFmpeg.
- Do not weaken QB-PERF-002 validation or fabricate the missing R11 fixture.
- Preserve existing evidence and user data. Use dedicated fixtures for
  destructive tests.

## External gates

- QB-PERF-002 schema-v2 PerfProc process-counter access is unavailable on PC B.
- Three independent heartbeat-free QB-PERF-002 captures remain for the later
  actual-game environment.
- Actual League lifecycle acceptance and the M8 A/B comparison are not done.
- R11 still requires the empirical
  `live-client-capture-20260730-110603.json` fixture.

See `docs/VERIFICATION.md` for commands and `docs/PC_B_NON_LEAGUE_VERIFICATION.md`
only for the detailed diagnostic history.

## Read routing

Default: read this file and root `AGENTS.md`. When editing recorder code, also
read `recorder/AGENTS.md`. Open the active plan, verification guide, or evidence
reports only when the task requires them. The external handoff pack and the
baseline worktree are historical archives, not coding prerequisites.
