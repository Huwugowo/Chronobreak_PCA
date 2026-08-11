# QueueBack ExecPlan contract

ExecPlans are used only for ready features whose `execution.workflow` is `planned`.

The feature entry in `feature-list.json` defines **what must be true**.
The ExecPlan defines **how to make it true**.

An ExecPlan must be self-contained enough that a fresh implementation agent can execute it without the planning conversation.

## Required sections

### Purpose
User-visible outcome and why it matters.

### Relevant current architecture
Only the repository facts needed for this change, with concrete files/symbols.

### Scope / non-goals
What changes and what intentionally does not.

### Exploration findings
Concise facts, reusable mechanisms, constraints, and risks. No raw grep/log dumps.

### Chosen design
The implementation approach and rationale. Include material rejected alternatives when useful.

### Milestones
Concrete implementation steps, preferably with verifiable intermediate states.

### Verification
Map the work to the feature's acceptance criteria and verification entries. Include exact commands when known.

### Performance / reliability
Required when the change can affect capture overhead, recording correctness, media integrity, process lifecycle, storage safety, or recovery.

### Progress
Short implementation checklist.

### Deviations / surprises
Facts discovered during implementation that materially contradict the plan.

### Decision log
Important implementation-time decisions.

### Completion
Final result, evidence, limitations, and follow-up work.

## Plan review gate

Before finalizing the ExecPlan, challenge:
- architecture understanding;
- missing dependencies;
- edge cases and failure modes;
- persistence/migration/backward compatibility;
- capture performance/reliability;
- whether acceptance criteria are precise and testable;
- whether verification actually proves them;
- whether the feature should be split further.

Do not finalize a plan that merely restates the feature description.
