# QB-LIB-019: Foreground search admission during imports

## Purpose

An outstanding current search can admit its result while background recording
imports continue. Imports still refresh results, with one bounded query lane.

## Relevant planning-time architecture

`app/src/historyController.ts` owns one query and one import at a time. Catalog
admission gates querying; current context/root/token/navigation identity rejects
stale work. Explicit input increments `queryGeneration`, keeps one latest pending
query and observes scheduled/dispatched/completed/admitted/superseded events.
`App.tsx` passes those events to `benchmarkSearch.ts`; generation also determines
whether an observed result is still current at a later paint opportunity.

## Scope and non-goals

Change only import-triggered query scheduling and focused controller tests. Keep
backend SQL, snapshot/root authority, query inputs/pagination, worker counts and
benchmark event shapes. Stable pending-source retry budgets belong to QB-LIB-020.

## Exploration findings

`pumpImport` calls `requestQuery()` after every recording import, including a
pending response requeued after four milliseconds. `requestQuery` increments the
response-admission generation even when a user query is active. Repeating import
completion before each query response can reject every response indefinitely.
Existing tests hold an import while queries succeed; they do not resolve imports
before the held foreground query. Catalog pagination already preserves queued
input by pumping instead of replacing it.

## Chosen design and rationale

Add one context-local refresh-dirty bit. Import completions call a dedicated
refresh request: while a query is active, mark dirty without changing generation
or pending user action; while a query is pending, pump that existing intent;
otherwise schedule the normal automatic query. Multiple import completions
coalesce into the same dirty bit. A query dispatch consumes dirtiness because it
observes all commits completed before its dispatch.

Use this dedicated refresh path for every successful catalog or recording
completion. Direct query requests remain for typed input and pagination; all such
pending foreground requests outrank automatic refresh equally.

On query settlement, release the active lane and dispatch pending explicit input
first. If none is pending and imports made the query dirty, schedule exactly one
automatic refresh. A subsequent import can make that refresh dirty but cannot
invalidate its response. An explicit newer input still immediately advances
generation and supersedes the old response. Context changes/disposal clear dirty
state. Existing catalog gating, membership validation and error handling remain.
Settlement releases the global lane, then decides against the current context:
an old A attempt cannot revive A dirtiness after A-to-B/A, but must release B's
already pending work. With dirty state after an error, the one automatic refresh
uses existing request error-clearing behavior and may retry once for that data
change; it is not an independent timer/retry loop.

Retain the existing request generation as admission/attempt identity. Automatic
refresh advances it only after the prior response settles; this preserves the
existing benchmark paint-currentness semantics without a new telemetry schema.
The guarantee is admission of a current outstanding result, not an assertion of
physical display before a fresher result or new user input.

## Rejected alternatives and planning decisions

Reject pausing all imports behind queries or cancelling backend work: independent
bounded lanes already exist and productive indexing should continue. Reject
one queued query per import, debounce delays, or ignoring import refresh entirely.
A single dirty bit proves boundedness without changing worker architecture.

## Milestones

1. Add a deterministic held-query/repeated-import regression and demonstrate it
   fails before changing scheduling. Cover import-first and query-first ordering.
2. Implement coalesced refresh and explicit-user priority. Verify replacement,
   context changes/A-B-A, catalog pagination, errors and disposal.
3. Run frontend checks, review the scheduling boundary, document and commit.

## Verification design

Use deferred promises and fake timers in `historyController.test.ts`. Admit setup
catalog/history, hold a user query, complete at least three pending recording
imports before releasing its result and require its action to be admitted exactly
once. Before the correction the same fixture must report superseded instead.
After release assert one coalesced automatic query, not one per import, and never
more than one query/import active. Reverse completion ordering and prove imports
still request an update. While dirty, replace user input and require only the
latest pending intent; stale context/disposal must neither publish nor restart.
Resolve each held import and advance the four-millisecond yield before the next;
finish the final import or close. Assert exactly one admission and no superseded
event for the original action. An import during the coalesced automatic query
must cause exactly one later refresh. Cover error-plus-dirty and a B-context query
waiting behind a stale A attempt, including A/B/A.

Commands: `npm run test --prefix app -- src/historyController.test.ts`, full
`npm run test --prefix app`, `npm run check --prefix app`,
`npm run build --prefix app`, canonical validation and `git diff --check`.

## Performance and reliability gates

One active query and import plus one pending intent/dirty bit; no new timer,
unbounded queue, cancelled IPC, backend task or per-import fan-out. Deterministic
call/admission counts establish scheduling behavior; no whole-app CPU claim.
