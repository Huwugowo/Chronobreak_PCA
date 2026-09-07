# Chronobreak agent instructions

Chronobreak is a League of Legends record, replay, search, and clipping application.

## Mandatory bootstrap

Before planning, coding, editing canonical project state, or delegating work in a new session:

1. Read `docs/product/PRODUCT.md` **in full**.
2. Read `docs/development/WORKFLOW.md` **in full**.
3. Resolve the requested item in `feature-list.json` and read its complete entry, including dependencies, stage/status, execution paths, acceptance criteria, verification, evidence, and notes.
4. When `execution.progress` contains a path, read that execution checkpoint **in full before the ExecPlan**. It owns the current milestone, partial work, obtained results, blockers, and next action.
5. When `execution.plan` contains a path, read that immutable ExecPlan **in full after the checkpoint**. It owns the approved implementation design, not current repository state.

Do not substitute summaries, chat memory, git history, predecessor plans, or broad repository rediscovery for those reads.

Then apply these task-dependent reads before acting on the relevant phase:

- For planned work with no finalized ExecPlan, read `PLANS.md` in full before creating one.
- Before implementing or verifying product code, read `docs/development/VERIFICATION.md` in full.
- Read only the applicable documents under `docs/architecture/` when the feature, ExecPlan, or current unit depends on durable architecture facts.

When a canonical document changes materially during the session, reread the affected section before relying on it.

## Authorities

Use repository state, not chat memory, for durable facts:

- `docs/product/PRODUCT.md` — product scope, priorities, and boundaries.
- `feature-list.json` — feature definition, lifecycle status, Definition of Done, execution artifact paths, and concise canonical evidence.
- `docs/execution/` — mutable per-feature implementation and verification checkpoints.
- `docs/exec-plans/` — immutable approved designs for planned features.
- `docs/development/WORKFLOW.md` — lifecycle, routing, resume, checkpoint, delegation, and completion process.
- `docs/development/VERIFICATION.md` — project-wide verification commands and invariants.
- `docs/architecture/` — durable architecture that is true now.
- `feature-list.schema.json` and `PLANS.md` — structural and ExecPlan artifact contracts.

Do not create competing sources of truth.

## Universal guardrails

- Follow `docs/development/WORKFLOW.md`; the user does not need to choose or name a workflow.
- Work on one concrete feature at a time unless its approved design requires coordinated changes.
- Do not opportunistically implement unrelated work. Record genuine discovered work in `feature-list.json`.
- Prefer existing architecture over parallel mechanisms. Record substantial durable architecture facts under `docs/architecture/`.
- Keep canonical lifecycle state and execution paths truthful throughout the work.
- Preserve unrelated working-tree changes; never reset, discard, or silently overwrite them.
For substantial implementation or active ExecPlan execution, apply the
`forward-engineering` skill. It governs forward progress, focused invalidation,
resume behavior, root-cause handling, and execution-state maintenance.

## Delegation and context acquisition

The root agent owns the task: reasoning, design decisions, implementation, edits, integration, canonical project state, and final verification.

Use subagents primarily to keep substantial read-heavy or noisy investigation out of the root context.

### Delegate exploration

Delegate to a read-only `scout` whenever answering the current question requires substantial repository exploration, including:

* searching across multiple files or modules;
* locating implementations, consumers, tests, or related symbols whose complete relevant set is not already known;
* understanding interactions across several components;
* reconstructing existing behavior from code;
* analysing a large amount of repository evidence or output.

Do not delegate a trivial lookup or a small read of a known file when the root can obtain the needed fact directly with little context cost.

When several independent exploration questions are known at once, batch them into one `task` call and run the scouts in parallel.

A scout gathers evidence only. It does not implement changes and does not delegate further. Its result should contain concise findings, relevant file/symbol/range references, important constraints, and unresolved uncertainty — not its raw search transcript.

The root should use the returned evidence to reason and implement. It may directly inspect a small number of critical source ranges before editing when exact code is required.

### Tool discipline

Prefer targeted searches and bounded reads over broad repository dumps.

When direct root-side investigation is necessary, batch independent reads/searches in the same turn when possible rather than making one model turn per tiny lookup.

Do not repeatedly poll running agents. Use their completion result.

Avoid injecting large command, search, test, or log outputs into the root context when a targeted excerpt or summarized result is sufficient.


## Completion gate

Never mark a feature `done` merely because code was written.

A feature is `done` only when:

- every acceptance criterion in `feature-list.json` is satisfied;
- required feature-specific verification was actually run and passed;
- applicable project-wide verification from `docs/development/VERIFICATION.md` passes;
- required performance/reliability checks pass;
- no known unresolved issue contradicts the claimed behavior;
- relevant durable documentation and canonical state are updated;
- detailed implementation and verification history is retained in the execution record when one exists;
- concise concrete evidence supporting the completion claim is recorded in `feature-list.json`.

If verification was not run, do not claim it passed. If a required check cannot run, keep the feature non-done and record the reason truthfully.

Do not weaken acceptance criteria after implementation merely to make a feature pass. Genuine requirement changes must be explicit and recorded in canonical feature state.

## Safety

- Never use real user recordings as destructive test data.
- Use dedicated fixtures or temporary/test libraries for destructive storage, migration, cleanup, recovery, and corruption tests.
- Do not delete or overwrite recordings, clip libraries, user-selected media directories, credentials, or other user data unless explicitly required and confirmed safe.
- Do not weaken validation, integrity checks, security, or error handling just to make tests pass.
