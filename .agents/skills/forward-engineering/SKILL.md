---

name: forward-engineering
description: >
Keep implementation moving forward through the active feature and ExecPlan,
prioritizing real product changes and focused validation over bookkeeping,
repeated exploration, defensive symptom patches, and unnecessary replay of
completed work. Use during substantial implementation, ExecPlan execution,
resumed work, migrations, debugging of implementation failures, and
long-running engineering tasks. Especially use when an agent is tempted to
reopen settled decisions, rerun already-valid work, broaden validation
unnecessarily, add fallbacks/tolerances around a failure, or duplicate
exploration already performed by a scout.
-----------------------------------------

# Forward Engineering

Advance the active implementation through real capability, focused evidence,
and minimal durable execution state.

The default loop is:

**implement -> validate -> checkpoint -> advance**

Do not confuse activity, metadata, repeated review, or repeated exploration
with engineering progress.

## Action classification

Before substantial work, classify the proposed action as one of:

1. **Semantic implementation**

   * Builds, removes, changes, or connects product behavior, runtime paths,
     producers, consumers, adapters, schemas, fixtures, or final output.

2. **Focused validation**

   * Establishes whether changed behavior is correct through compilation,
     tests, runtime reproduction, artifact inspection, invariants, benchmarks,
     resource measurement, or end-to-end behavior.

3. **Execution state**

   * Maintains the minimum durable information required for another context,
     compaction, interruption, or resumed session to continue correctly:
     active feature, authoritative ExecPlan, forward cursor, completed work,
     unresolved blockers, validation state, in-flight work, and exact next
     action.

4. **Administrative bookkeeping**

   * Produces metadata that neither changes product behavior, proves
     correctness, nor materially enables correct resumption.

Prefer categories 1 and 2.

Maintain category 3 cheaply at meaningful boundaries.

Avoid category 4 unless explicitly required.

## Forward cursor

ExecPlan execution is forward-only by default.

Do not reopen completed milestones, redo architectural research, reconstruct
settled decisions, or rerun accepted work merely because:

* context was compacted;
* another agent did not personally witness the earlier work;
* a progress marker changed;
* a receipt or administrative record is stale;
* additional confidence would feel safer.

Move backward or replay earlier work only when a substantive condition holds:

* relevant input or implementation meaning changed;
* a dependency or target revision changed;
* new runtime evidence contradicts the previous conclusion;
* previous output is malformed, incomplete, inconsistent, or incompatible;
* the changed dependency cone actually reaches the previous milestone.

Uncertainty caused only by lost conversational context is not evidence that
completed engineering work became invalid. Read the durable execution state and
continue forward.

## Smallest dependency cone

When something changes or fails, invalidate only the implementation,
validation, evidence, and conclusions that actually depend on it.

Do not replay an entire feature, plan, benchmark matrix, test suite, or
architectural investigation when a smaller dependency cone establishes
correctness.

Examples:

* A fixture change invalidates evidence derived from that fixture, not unrelated
  architectural decisions.
* A recorder timestamp change invalidates timestamp-dependent verification, not
  every recorder capability.
* A UI change does not invalidate a previously accepted capture architecture.
* A changed producer requires rechecking affected consumers, not unrelated
  stages.

Broaden the cone only when concrete dependencies or evidence require it.

## Root cause before mitigation

When validation fails, determine which intended invariant was violated and
which producer, consumer, fixture, test, or assumption is responsible.

Do not immediately add:

* tolerances;
* retries;
* fallbacks;
* clamps;
* compatibility branches;
* silent remapping;
* default substitution;
* error swallowing;
* defensive state recovery;
* test weakening.

Such behavior is appropriate only when it is itself part of the intended
product contract or when evidence shows the underlying condition is genuinely
unavoidable.

Prefer correcting the behavior that produces the invalid state.

A downstream symptom becoming harmless is not equivalent to fixing its cause.

## Evidence over markers

A capability is not correct because:

* a file exists;
* a checkbox is checked;
* a milestone says complete;
* a test command was invoked;
* a worker reports success;
* a progress document says it passed.

Use evidence appropriate to the change, such as:

* successful compile or typecheck;
* focused unit/integration tests;
* runtime reproduction;
* artifact or media inspection;
* invariant checks;
* end-to-end verification across changed interfaces;
* benchmark/resource measurements when performance matters.

An execution record is substantive when it identifies what was exercised,
against what expectation, and what result was observed.

Administrative status never grants correctness.

## Validation discipline

Validate the smallest changed dependency cone that can establish correctness.

Start focused.

Broaden validation when:

* an interface boundary changed;
* shared infrastructure changed;
* dependencies make the affected scope uncertain;
* focused evidence reveals a wider problem;
* the ExecPlan explicitly requires broader acceptance evidence.

Do not run an entire verification matrix merely because validation is required.

Performance-sensitive changes require measurement when the relevant claim is
about performance. Static inspection alone cannot prove a runtime performance
claim.

## Execution checkpoint

Execution state is not disposable bookkeeping.

Keep the repository's established execution checkpoint/progress mechanism
truthful enough that work can resume after interruption or compaction without
repeating exploration.

Update it at meaningful boundaries, especially:

* after completing and validating a milestone;
* before intentional compaction;
* before leaving substantial unvalidated edits in flight;
* after discovering a blocker that changes the exact next action;
* when the authoritative ExecPlan or forward cursor changes.

Record only what materially helps resumption:

* active feature and authoritative plan;
* forward cursor/current objective;
* settled decisions that must not be re-litigated;
* completed and validated work;
* in-flight or unvalidated changes;
* literal blockers;
* exact next action.

Do not turn checkpoint maintenance into a second implementation project.

## ExecPlan discipline

The authoritative ExecPlan defines the intended engineering outcome.

While executing it:

* continue from the current forward cursor;
* do not silently broaden scope;
* do not resurrect superseded plans;
* do not rewrite settled architecture because another implementation seems
  locally convenient;
* reconcile the plan only when new evidence materially invalidates an
  assumption or required implementation path.

A newly discovered implementation detail does not automatically require a new
plan.

Plan revision is for materially changed engineering truth, not routine
execution discoveries.

## Review discipline

Review is substantive when it can discover defects or challenge consequential
engineering decisions.

Use review at meaningful risk or validation boundaries.

When a required review has passed and no subsequent change invalidates it, do
not repeat the review merely for additional confidence.

If review finds a defect:

1. identify the underlying issue;
2. fix it;
3. revalidate the affected dependency cone;
4. repeat only the review scope invalidated by that fix.

Do not create endless review -> review-of-review loops.

## Resume behavior

When resuming interrupted work:

1. Read the repository's normal bootstrap instructions.
2. Identify the active feature, authoritative ExecPlan, execution checkpoint,
   and current diff.
3. Determine whether the latest edits are validated or still in flight.
4. Continue from the forward cursor.
5. Validate unfinished work before building further on it.

Do not begin with a broad repository audit unless the durable execution state
is demonstrably inconsistent or new evidence requires one.

Do not redo completed exploration simply because conversational context was
lost.

## Completion

A feature or milestone is complete when its intended behavior exists and the
required acceptance evidence supports it.

Before declaring completion:

* ensure no relevant work remains merely hidden behind bookkeeping;
* ensure in-flight changes are validated;
* ensure required focused evidence exists;
* ensure the checkpoint reflects the final state;
* ensure no unresolved blocker contradicts the completion claim.

Once these conditions are satisfied, finish.

Do not add another implementation, exploration, or review cycle merely because
there is remaining context or time.
