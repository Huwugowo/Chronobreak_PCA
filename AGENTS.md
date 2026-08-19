# QueueBack agent instructions

## Mission

Develop QueueBack as a League of Legends-specific **record, replay, search, and clipping** application.

Prioritize:
1. recording reliability;
2. negligible measurable impact on League performance;
3. a fast League-native replay experience;
4. fast, simple clipping/export;
5. local-first ownership and recoverability.

QueueBack is not a coaching platform. Descriptive game data may help users find, navigate, understand, or clip recordings. Prescriptive gameplay judgment, coaching, build/matchup advice, decision grading, and "what you should have done" features are out of scope unless the user explicitly changes the product direction.

## Sources of truth

Use repository state, not chat memory, for durable project facts.

- `feature-list.json` — canonical work state and feature-specific Definition of Done.
- `feature-list.schema.json` — structural contract for the feature list.
- `docs/product/PRODUCT.md` — durable product boundaries.
- `docs/development/VERIFICATION.md` — authoritative project-wide verification commands.
- `PLANS.md` — ExecPlan contract.
- `docs/architecture/` — durable architecture facts.
- `docs/exec-plans/active/` — active plans.

Do not create competing sources of truth.

## The work rule

Whenever the user asks to work on an item, resolve it in `feature-list.json` and inspect its `stage`.

### `stage: draft` -> refine

Do not implement product code.

Work only on making the item implementable:
- make the observable outcome clear;
- define scope and important non-goals;
- define specific, testable acceptance criteria;
- define verification that can prove those criteria;
- identify dependencies;
- make the work small/coherent enough to implement as one feature;
- choose `execution.workflow`.

If the item is too broad to become one implementable feature, keep it as an epic and decompose it into concrete child features.

A draft feature becomes `ready` only when the readiness gate above passes.

An epic is never implementation-ready; it remains a parent objective and is advanced by refining/decomposing its child features.

### `stage: ready` -> execute

A ready item must be a concrete feature.

Check dependencies, then follow its `execution.workflow`.

The user should not need to choose or mention the workflow.

## Execution workflows

### `direct`

For localized, well-understood work.

`inspect narrowly -> implement -> verify -> evidence -> completion gate`

No ExecPlan or subagents by default.

### `adaptive`

For apparently bounded work that may hide important complexity.

Start with bounded inspection.

- If the work remains local, low-risk, and clear: execute as `direct`.
- If it affects multiple subsystems/processes, persistence/migrations, concurrency/lifecycle, capture performance/reliability, backward compatibility, or has meaningful architectural tradeoffs: change the workflow to `planned` before implementation.

### `planned`

For complex, high-risk, cross-subsystem, performance-sensitive, persistence-sensitive, or architecturally consequential work.

If `execution.plan` is `null`, this pass is planning only:

1. explore the relevant architecture;
2. use subagents where useful to isolate independent read-heavy exploration, tests/logs, benchmarks, or review;
3. synthesize findings in the main agent;
4. produce a candidate design;
5. review/challenge the design for missing dependencies, edge cases, failure modes, migrations/backward compatibility, performance/reliability risk, and verification gaps;
6. resolve material uncertainties and ask the user only for genuine product/architecture decisions;
7. write a self-contained ExecPlan following `PLANS.md`;
8. save its path in `execution.plan`;
9. stop before implementation.

Implementation of planned work starts from a fresh context.

If `execution.plan` already contains a path, read that ExecPlan and implement it. Do not repeat broad exploration unless the repository materially contradicts the plan.

## Subagents

Subagents are a tool used mainly inside planned work when they reduce context pollution or parallelize independent read-heavy investigation.

Good uses:
- mapping separate subsystems;
- unfamiliar API investigation;
- test/log/benchmark analysis;
- independent plan or implementation review.

Do not create subagents for ceremony or parallel overlapping edits.

The main agent owns synthesis, user-facing decisions, canonical state, integration, verification, and completion.

## Scope

Work on one concrete feature at a time unless an active ExecPlan requires coordinated changes.

Do not opportunistically implement unrelated work. Add real newly discovered work to `feature-list.json` instead.

Prefer existing architecture over parallel mechanisms. Record substantial durable architecture decisions under `docs/architecture/`.

## Global completion gate

Never mark a feature `done` merely because code was written.

A feature is `done` only when:
- every acceptance criterion in `feature-list.json` is satisfied;
- required feature verification was actually run and passed;
- applicable project-wide verification passes;
- required performance/reliability checks pass;
- no known unresolved issue contradicts the claimed behavior;
- relevant durable documentation/state is updated;
- concrete evidence is recorded in `feature-list.json`.

If verification was not run, do not claim it passed.

Do not weaken acceptance criteria after implementation merely to make a feature pass. Genuine requirement changes must be explicit.

## Safety

Never use real user recordings as destructive test data.

Use dedicated fixtures or temporary/test libraries for destructive storage, migration, cleanup, recovery, and corruption tests.

Do not delete or overwrite recordings, clip libraries, user-selected media directories, credentials, or other user data unless explicitly required and confirmed safe.

Do not weaken validation, integrity checks, security, or error handling just to make tests pass.

## State updates

Keep `feature-list.json` truthful.

Status:
- `not-started` — no implementation/planning currently active;
- `in-progress` — refinement, planning, or implementation is active;
- `blocked` — progress requires an unresolved external dependency or user decision;
- `done` — completion gate passed.

For planned features:
- `execution.plan: null` — no durable implementation plan exists yet;
- a plan path — that ExecPlan is the authoritative implementation guide.

Keep `progress.md` only as a short restart pointer, never as a second backlog.

