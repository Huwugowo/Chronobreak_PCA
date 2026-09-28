# Reconcile export retry admission

## Purpose

QB-CLIP-009 makes Retry and another preset export usable without leaving the editor after an admitted attempt.

## Relevant planning-time architecture

App captures a `LibraryOrigin` into clip-export navigation. LibraryController owns root epoch, request, snapshot token and navigation epoch; `mayPublish` admits current views. Export completion invalidates the captured destination and refreshes, but navigation keeps its old token. The exporter probe resource is keyed only by game ID. Game summaries lack media identity; strict probe and backend metadata bind draft media ID and frame range.

## Scope and non-goals

Reconcile only the still-current editor after its own successful same-root refresh. Preserve draft, options, exact source identity and backend admission. No token replacement across root A/B/A, navigation away/back, missing source, failed scan or a superseding refresh. Settings recovery remains QB-LIB-012.

## Exploration findings

Read-only scout and review fixtures confirmed both success and failure refresh the library while retaining the stale token. A new token alone does not establish source identity; the probe must reload and reject replacement media without adapting the draft.

## Chosen design and rationale

Make controller refresh return a promise for that specific refresh generation, resolving to a current origin only on successful publication, otherwise null. Resolve superseded/disposed waiters; keep one bounded waiter for the current generation. Existing fire-and-forget calls remain valid. App captures the editor object and export origin before awaiting destination reconciliation, then rebinds only if object identity, root epoch/navigation, source membership and current admission all agree.

Key the exporter resource by game ID and snapshot token. Explicitly represent invalid draft/source mapping as a visible error. Disable dispatch while revalidation is pending or invalid. Capture the export origin/token at dispatch for completion reconciliation, preserving UI-owned options and draft.

## Rejected alternatives and planning decisions

Do not blindly borrow controller.origin after any refresh or rewrite the draft to a replacement media ID. Do not remount the editor just to get a token because that loses options. Do not weaken backend stale-token checks.

## Milestones

1. Add generation-specific refresh settlement and App editor reconciliation.
2. Revalidate exporter source on token changes and preserve dispatch origin.
3. Exercise success then repeat, error then retry, root/navigation/source changes, refresh failure/supersession, and unchanged option/draft behavior.

## Verification design

Extend LibraryController and App integration tests plus the actual exporter boundary. Verify the second export command receives the refreshed token; hold probe revalidation and assert dispatch cannot start; reject changed media identity. Run frontend tests, `npm run check --prefix app`, and `npm run build --prefix app`.

## Performance and reliability gates

One refresh-generation waiter, no polling or duplicated scans, no stale completion publishing into a newer editor, and no source identity fallback. Existing backend/source/media delivery gates remain authoritative.
