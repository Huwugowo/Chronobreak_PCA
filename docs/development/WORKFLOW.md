# Chronobreak workflow

The workflow has one entry point:

> **Work on X.**

The agent performs the mandatory `AGENTS.md` bootstrap, resolves X in `feature-list.json`, and follows the feature's canonical `stage`, `status`, and `execution` fields. The user does not need to select a workflow or explain whether this is a first implementation or a resume.

`feature-list.json` is the only lifecycle authority. An ExecPlan is immutable approved design. A per-feature execution record is the only detailed authority for implementation progress, obtained verification, blockers, and the next action. Do not create a second status narrative or active-plan index.

## 1. Draft -> ready

If `stage` is `draft`, do not implement product code. Make the work implementable.

A draft is ready when:

- the observable result is clear;
- scope and non-goals are clear;
- acceptance criteria are specific and testable;
- verification can prove them;
- dependencies are known;
- the work is one coherent implementation unit;
- its execution workflow is chosen.

### Broad draft: epic

Objectives such as improving capture reliability, achieving negligible capture overhead, or making replay navigation excellent remain `kind: epic`. Decompose them into concrete child features. An epic never becomes `ready`; its children do.

### Concrete draft: feature

Refine behavior, acceptance criteria, verification, dependencies, and workflow. When the readiness gate passes, set `stage: ready`. Do not claim implementation evidence during refinement.

## 2. Ready -> route execution

A `ready` item must be `kind: feature`. Execute `execution.workflow`.

### Direct

`inspect narrowly -> implement -> verify -> concise evidence -> done`

Direct work keeps `execution.plan` null. It does not require an ExecPlan or planned-feature checkpoint.

### Adaptive

`bounded inspection -> direct OR promote to planned`

Promote before implementation when the work becomes meaningful architecture, persistence or migration, concurrency or lifecycle, capture performance or reliability, backward compatibility, or cross-subsystem work. Set `execution.workflow` to `planned`, then follow the planning pass below. Do not write an ExecPlan while leaving the feature routed as adaptive.

### Planned with no finalized plan: planning pass

This state is `execution.workflow: planned` with `execution.plan: null`.

1. Read `PLANS.md`.
2. Explore only enough to resolve the design and verification questions.
3. Use bounded delegation when it improves parallelism, context efficiency, or review quality.
4. Synthesize one self-contained candidate design and challenge it at the plan-review gate.
5. Save the finalized ExecPlan at a stable status-neutral path.
6. Initialize its execution checkpoint before implementation starts.
7. Set both `execution.plan` and `execution.progress`, validate canonical state and linked files, then stop.

The planning pass does not implement product code. Once linked, the ExecPlan is immutable.

### First implementation from a finalized plan

A planned feature with a finalized ExecPlan must already have an initialized execution checkpoint, even when no implementation milestone has started.

Read in this order:

1. the complete feature entry;
2. the execution checkpoint, to learn the current handoff and explicit next action;
3. the immutable ExecPlan, for the approved design;
4. applicable verification and durable architecture material.

Confirm that the checkpoint names the same ExecPlan path. Before starting the first substantial unit, set `Current milestone`, `Active unit`, targeted files/symbols when useful, and `Next action` so an abrupt interruption leaves a truthful handoff.

### Resume an in-progress planned implementation

Read the mutable execution checkpoint before treating the ExecPlan as current repository state. Then read the immutable ExecPlan as implementation design.

- Resume the specifically incomplete/current unit.
- Treat recorded completed milestones, units, and checks as restart assumptions unless current repository evidence contradicts them, the current unit depends on rechecking them, or final completion verification requires rerunning them.
- Do not remap completed subsystems, begin a broad “assess existing implementation” pass, or reconstruct completed work from git history by default.
- Do not reread predecessor plans, investigations, reports, or protocols merely because implementation exists. They are provenance; open one only for a concrete unresolved question absent from the active plan and durable architecture.
- If the checkpoint identifies no active partial unit, perform its explicit next action.

If implementation materially invalidates the approved design, stop that unit. Record the contradiction in the checkpoint, create and review a versioned superseding ExecPlan, update the feature and checkpoint to the new path, and preserve the old plan. Never edit or lifecycle-move the old plan.

## 3. Execution checkpoint contract

Every planned feature with a finalized ExecPlan has one stable mutable record at `docs/execution/<lowercase-feature-id>.md`, referenced by `execution.progress`.

The record is a bounded restart checkpoint, not a diary. Use this structure:

```text
# <FEATURE-ID> execution checkpoint

Feature: `<FEATURE-ID>`
ExecPlan: `<exact execution.plan path>`
Updated: YYYY-MM-DD

## Current milestone
## Active unit
## Completed
## In flight
## Remaining
## Verification
## Deviations
## Decisions
## Blockers
## Next action
```

The checkpoint contains, at minimum:

- the associated immutable ExecPlan path/version;
- the current milestone and one active implementation or verification unit;
- concise completed milestones/units;
- any partial work in the active unit;
- targeted files/symbols for that unit when useful;
- remaining milestones and gates;
- actual command/scenario results and artifact references already obtained;
- implementation-time deviations from the approved plan;
- implementation-time decisions future work depends on;
- blockers and their exact effect;
- one explicit next action.

Do not duplicate the feature's acceptance criteria, the complete ExecPlan design, product scope, or durable architecture. Summarize large result sets and link their durable or ignored artifact roots. Remove obsolete transient detail as the checkpoint advances; retain decisions, deviations, failures, limitations, and verification facts future work still needs.

### Checkpoint discipline

Initialize the record when the ExecPlan is finalized, before implementation starts.

Update it:

- immediately after a substantial implementation unit or milestone materially changes state;
- after a meaningful verification pass or failure;
- after a material deviation, implementation decision, or blocker;
- before beginning the next substantial unit, so `Active unit` and `Next action` remain useful if the session dies.

Do not rewrite it after every tiny edit.

### Abrupt-interruption recovery

When the checkpoint shows an active or in-flight unit:

1. inspect only that unit and its targeted working-tree diff first;
2. reconcile which described changes are complete, partial, absent, or contradicted;
3. repair the unit and checkpoint;
4. continue from the recorded next boundary.

An interruption is not justification for broad repository archaeology. Escalate to wider inspection only when concrete evidence contradicts the checkpoint or a required fact is absent from the active plan and architecture.

## 4. Delegation

Delegation is optional and harness-neutral.

- Delegate bounded, independently solvable exploration, implementation, review, or verification when it materially improves parallelism, context efficiency, or quality.
- Give each delegated unit explicit scope, interfaces, ownership, and acceptance evidence.
- Keep integration, cross-cutting design decisions, canonical feature/checkpoint state, and final completion verification with the root agent.
- Do not delegate trivial or tightly coupled work merely to use additional agents.
- Reconcile delegated results into the current unit before updating canonical state.

## 5. Definition of Done and evidence

`feature-list.json` owns the feature-specific Definition of Done:

- `acceptance_criteria`;
- `verification`;
- concise `evidence` supporting the canonical status claim.

An ExecPlan explains how planned work is designed to satisfy that Definition of Done; it never replaces or updates it. The execution record owns detailed actual implementation and verification history.

During draft refinement or planned discovery, requirements may be clarified when repository facts reveal real constraints. Once implementation begins, do not weaken criteria merely because implementation fails them. A genuine requirement change must be explicit and recorded in canonical feature state.

## 6. Completion

Never mark a feature done until the universal `AGENTS.md` gate and every feature-specific criterion pass.

For planned work:

1. run final required verification against the completed tree;
2. update the execution record with final implementation state, exact results, limitations, deviations, decisions, and no remaining active unit;
3. reduce `feature-list.json` evidence to the concise canonical completion claim and set `status: done`;
4. validate schema, invariants, and linked artifacts;
5. leave the immutable ExecPlan unchanged at its stable path.

For direct/adaptive work that remained direct, record concise concrete completion evidence in the feature entry and keep plan/progress absent or null.

If a required check is failed, not run, or environment-blocked, retain truthful non-done status and checkpoint state. Do not convert unavailable evidence into a pass.
