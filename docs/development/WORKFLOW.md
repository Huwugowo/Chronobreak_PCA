# Chronobreak workflow

The workflow has one entry point:

> **Work on X.**

Codex resolves X in `feature-list.json` and checks `stage`.

## 1. Draft -> make it ready

If `stage` is `draft`, do not implement product code.

The goal is only to make the work implementable.

A draft is ready when:
- the observable result is clear;
- scope/non-goals are clear;
- acceptance criteria are specific and testable;
- verification can prove them;
- dependencies are known;
- the work is one coherent implementation unit;
- its execution workflow is chosen.

### Broad draft: epic

Some ideas are objectives rather than implementable features.

Examples:
- improve capture reliability;
- achieve negligible capture overhead;
- make replay navigation excellent.

Keep these as `kind: epic`. Decompose them into concrete child features.

An epic never becomes `ready`; its children do.

Example:

`Capture Reliability`
- detect stalled recording output;
- handle capture/encoder initialization failure;
- recover interrupted recordings;
- validate finalized recordings;
- preflight storage availability.

### Concrete draft: feature

A concrete idea may still lack a good Definition of Done.

Example:

`Timeline Hover Thumbnails`

Refine its behavior, acceptance criteria, verification, dependencies, and workflow. Once the readiness gate passes:

`stage: draft -> stage: ready`

## 2. Ready -> execute the workflow

A `ready` item must be `kind: feature`.

Then execute its `execution.workflow`.

### Direct

`inspect narrowly -> implement -> verify -> evidence -> done`

### Adaptive

`bounded inspection -> direct OR promote to planned`

Promote before implementation when the work turns into meaningful architecture, persistence/migration, concurrency/lifecycle, capture performance/reliability, backward compatibility, or cross-subsystem work.

### Planned

When `execution.plan` is null:

`explore -> useful subagents -> synthesize -> candidate design -> plan review -> ExecPlan -> stop`

When `execution.plan` contains a path:

`read ExecPlan -> implement -> verify -> evidence -> done`

Planned implementation starts in a fresh context so exploratory noise does not carry into implementation.

## 3. Definition of Done

### Global rule
`AGENTS.md` contains the completion gate that applies to every feature.

### Feature-specific DoD
`feature-list.json` owns:
- `acceptance_criteria`;
- `verification`;
- `evidence`.

These define what must be true for that feature to be `done`.

### ExecPlan
An ExecPlan explains **how** a planned feature will satisfy its existing DoD. It does not replace the DoD.

## 4. Changing the DoD

During draft refinement or planned discovery, acceptance criteria may be clarified when the repository reveals real constraints.

Once implementation begins, do not weaken criteria merely because the implementation fails them.

A genuine requirement change must be explicit and documented.
