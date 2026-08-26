# Chronobreak agent instructions

## Mission

Develop Chronobreak as a League of Legends-specific **record, replay, search, and clipping** application.

Prioritize:
1. recording reliability;
2. negligible measurable impact on League performance;
3. a fast League-native replay experience;
4. fast, simple clipping/export;
5. local-first ownership and recoverability.

Chronobreak is not a coaching platform. Descriptive game data may help users find, navigate, understand, or clip recordings. Prescriptive gameplay judgment, coaching, build/matchup advice, decision grading, and "what you should have done" features are out of scope unless the user explicitly changes the product direction.

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

If `execution.plan` already contains a path, treat that ExecPlan as the implementation map: read it, inspect only what the next implementation step requires, and implement it. Do not repeat broad architecture or subsystem mapping unless a concrete repository contradiction or missing fact blocks the plan. In this state, subagents should handle bounded implementation slices, targeted uncertainties, review, or verification — not generic remapping of the system.

## Delegation and subagents

You are explicitly authorized to proactively use subagents without the user requesting delegation on each task. The main agent decides when delegation is worthwhile and owns orchestration, synthesis, integration, canonical state, verification, completion, and subagent lifecycle.

Use subagents only when they materially improve parallelism, context efficiency, or result quality. Do not delegate for ceremony and do not fill available concurrency slots merely because they exist.

### Good uses

- During planning without an authoritative ExecPlan, mapping genuinely independent subsystems.
- Targeted investigation of an unfamiliar API or concrete uncertainty.
- Isolated test, log, fixture, or benchmark analysis.
- A disjoint implementation slice with a clear write scope.
- Independent review of a plan, implementation, or specific risk.

Do not create subagents for overlapping edits, duplicated investigation, or work the main agent is simultaneously doing itself.

### Assignment discipline

Treat each subagent as a **bounded, single-assignment worker**, not as persistent memory for the project.

When spawning a subagent:

- give it one concrete objective with a clear deliverable;
- name the relevant paths or subsystem and important constraints;
- state whether it may edit files or is read-only;
- keep its scope disjoint from other active agents;
- by default, tell it not to spawn further subagents; allow descendants only when the main agent explicitly decides that a second level of independent decomposition is useful.

Parallelize only genuinely independent work. If tasks depend on each other's findings or touch the same code, run them sequentially or keep them in the main agent.

While subagents run, perform useful non-overlapping work when available. When no useful independent work remains, call `wait_agent` once with a long timeout and rely on its event-driven wakeup. Do not poll with repeated short `wait_agent` or `list_agents` calls; subagent messages and completions wake an active wait immediately.

### Context inheritance

Minimize inherited conversational context.

- Prefer `fork_turns: none` when the assignment can be made self-contained in its spawn message.
- Otherwise pass the smallest recent-turn slice that contains information the worker genuinely needs, typically a small positive `fork_turns` value.
- Use `fork_turns: all` only when the full parent conversation is genuinely necessary to complete that specific assignment.

Do not use full-history forks merely for convenience. Repository files, an authoritative ExecPlan, and a precise task prompt should carry durable context whenever possible.

### Model routing

When model overrides are available, use the lowest-capability model that is comfortably sufficient for the bounded assignment:

- use Luna for narrow, mechanical, high-volume work such as focused searches, enumeration, simple transformations, and straightforward test/log triage;
- use Terra for substantive bounded exploration, implementation, debugging, or review;
- use Sol for a subagent only when that delegated task itself genuinely requires frontier-level reasoning or architectural synthesis.

Do not accidentally inherit the main agent's Sol/max configuration for routine workers when a cheaper model is appropriate. Choose reasoning effort proportionally to the assignment rather than inheriting maximum effort by default.

### Agent lifecycle

A completed assignment ends that worker's lifecycle.

- Use `followup_task` only to clarify, correct, or finish the **same assignment** while its existing context is directly useful.
- Do not reuse a completed mapping/research/review worker for implementation, a new phase, or a different task. Spawn a fresh worker with a concise task instead.
- Do not keep a large-context worker alive because it "already knows the codebase"; preserve useful knowledge in its concise handoff, code changes, tests, or durable repository artifacts.
- After consuming a worker's final result, close it with `close_agent` when that tool is available.
- If the current Codex runtime does not expose `close_agent`, treat the completed worker as retired and never reactivate it for later phases.

A subagent's final response should be a concise handoff: findings or changes, evidence/tests, remaining uncertainty, and paths touched. Do not return large raw command output when a summary or relevant excerpt is sufficient.

## Context economy

Context is a working resource. These rules apply to the main agent and all subagents.

- Inspect narrowly before reading broadly. Prefer targeted `rg` queries and relevant file ranges over raw dumps of large files or directories.
- Keep terminal output bounded. Filter verbose commands, request only relevant ranges, or redirect large output to a temporary file and inspect the useful excerpts.
- Prefer one bounded command that answers a coherent small question over many tiny tool -> model round trips, but do not batch unrelated reads into a huge output dump.
- Do not repeatedly reread unchanged files or rediscover facts already established by the authoritative ExecPlan, repository state, or a concise subagent handoff.
- For verbose tests or benchmarks, preserve the full output when useful but feed only the summary and relevant failures/evidence back into model context.
- If exploration discovers durable information needed later, record it in the appropriate plan, architecture document, feature evidence, or code rather than relying on a long conversational context to remember it.

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
