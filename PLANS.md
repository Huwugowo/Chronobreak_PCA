# Chronobreak ExecPlan contract

ExecPlans are used only for ready features whose `execution.workflow` is `planned`.

`feature-list.json` defines **what must be true**. An ExecPlan defines the approved planning-time **how**. The per-feature execution record owns what actually happened during implementation and verification.

## Artifact invariants

An ExecPlan is a self-contained, immutable planning artifact.

- It records the design approved at the end of the planning pass.
- Its normal path is status-neutral: `docs/exec-plans/<feature-id>-<subject>.md`.
- Once finalized and linked from `execution.plan`, implementation must not edit it.
- It contains no lifecycle status such as not started, in progress, blocked, or done.
- It contains no implementation progress, actual command results, implementation-time decisions/deviations, blockers, next action, or completion outcome.
- It summarizes every fact normally required to execute the design. Links to predecessor plans, investigations, reports, protocols, or external specifications are provenance, not mandatory recursive reads.
- An implementation agent opens referenced provenance only when a concrete implementation or verification question remains unresolved by the referenced ExecPlan and current durable architecture.

Do not create an ExecPlan that merely restates the feature description or requires broad repository rediscovery before its first milestone can start.

## Required planning content

### Purpose

The intended user-visible outcome and why it matters.

### Relevant planning-time architecture

Only the repository facts, existing mechanisms, concrete files/symbols, invariants, and ownership boundaries needed for this design.

### Scope and non-goals

What the design changes and what it intentionally leaves to other features.

### Exploration findings

Concise reusable findings, constraints, measurements, and risks. No raw search or log dumps.

### Chosen design and rationale

The selected implementation approach, contracts, state transitions, failure behavior, and rationale at enough detail for normal execution.

### Rejected alternatives and planning decisions

Material alternatives that were considered and why they were rejected. Include planning decisions future implementation must preserve; omit routine choices.

### Milestones

Concrete ordered implementation units with clear boundaries and useful intermediate checks.

### Verification design

Map the milestones to the feature's acceptance criteria and verification specification. Include exact commands, scenarios, fixtures, and evidence shape when known, but never record implementation-time results here.

### Performance and reliability gates

Required when the change can affect capture overhead, recording correctness, media integrity, process lifecycle, storage safety, recovery, or another measurable product invariant.

## Plan review and finalization

Before finalizing an ExecPlan, challenge:

- architecture understanding and subsystem ownership;
- missing dependencies and feature boundaries;
- edge cases, failure modes, and interruption safety;
- persistence, migration, and compatibility consequences;
- capture performance, reliability, and bounded resource use where applicable;
- whether acceptance criteria are precise and testable;
- whether verification would actually prove them;
- whether the feature should be split.

Finalization is one atomic handoff:

1. save the approved ExecPlan at its stable path;
2. initialize the feature's execution checkpoint against that exact plan path;
3. set `execution.plan` and `execution.progress` in `feature-list.json`;
4. validate the feature list and linked files;
5. stop the planning pass without implementing product code.

After this handoff, the ExecPlan is immutable.

## Superseding a plan

If implementation materially invalidates the approved design, do not edit the plan. Stop the affected unit, record the contradiction in the execution checkpoint, create and review a versioned superseding ExecPlan, then update the feature and checkpoint to the new path. Preserve the superseded plan as history.

Minor implementation facts that do not invalidate the design stay only in the execution checkpoint. Never move a plan merely because feature lifecycle status changes.
