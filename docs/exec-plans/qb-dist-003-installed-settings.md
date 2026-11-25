# QB-DIST-003 Installed settings and safe apply

## Purpose

Let a player configure the supported recording mode and library without interrupting a match. Settings shows what is saved, which library the desktop opened, and what the recorder confirms it is using.

## Relevant planning-time architecture

The app and recorder use the same TOML and adjacent configuration lock. The app owns field-level atomic writes and its library coordinator owns root publication. The independent recorder holds a per-user lease and provides bounded status IPC. It reads capture settings at startup. Capture admission, finalization, and shutdown belong to the recorder service. Match result, history and ranked LP ledgers are owned by coordinators rooted at the original library.

## Scope and non-goals

Cover the native Windows Auto/High and Auto/H.264 preferences, configured output, age retention, and actual Windows login registration. Keep the current NVIDIA capture path, installer maintenance contract, recording formats and media in their existing locations. Preserve the status-only request for existing clients.

## Exploration findings

Changing the destination currently switches desktop browsing immediately. A running recorder keeps the startup folder. A plain restart across folders leaves old-root enrichment ledgers on disk without a consumer. Waiting for history retries to finish can indefinitely delay a folder change.

## Chosen design and rationale

- Share recording preference types and the Windows validation rule between the app and recorder. The desktop keeps persisted preference strings readable so it can offer explicit repair for unsupported legacy strings. Patch only the recording fields requested under the existing configuration lock. Preserve the existing storage and login operations, unknown TOML, and configuration readback. Resolve and validate a selected destination before publishing the settings or desktop root.
- Add a bounded local recorder settings request and effective-settings query through the existing control namespace. Authenticate a mutating request to the installed sibling desktop and same Windows user and session, with recorder instance verification and one coalesced pending request. The request carries no paths. The recorder reads and validates the latest saved preferences. Publish effective destination, configured preference labels in use, actual H.264 High mode and pending/apply-error state only after the service confirms it. Keep the query scoped to the trusted local desktop. The UI distinguishes unavailable from applied and uses existing ensure when the recorder is stopped.
- Use a controlled restart of the capture service generation within the recorder's existing process, tray and lease. Admission and apply are serialized in the recorder. Only a generation with no startup, active recording, finalizer, or retained current League process can release capture resources for apply. A pending request survives desktop close. Validate the replacement configuration and usable library before relinquishing the previous generation, and preserve or restore the old generation on failure. Allow explicit tray Quit to retain its existing semantics. Preserve the installer's requirement for actual process exit.
- Let the process own enrichment coordinators across capture service generations. Register each effective recorder library in a bounded, validated, atomically stored list under the existing per-user config directory before accepting captures there. Reopen outstanding roots after process restart using the existing EOG and history ledgers and guarded directory identity. Lease each root once. Preserve pending admissions, accepted results, retry counts and original deadlines for EOG, history and ranked LP. Retire a previous root only after both workers confirm they have durably settled all work. An inaccessible or ambiguous previous root stays recorded for retry. If the bounded list cannot safely accept the destination, keep the current generation and surface the reason.
- Retention and login remain desktop-owned. The recording settings view shows configured destination and current recorder destination independently. Choosing a folder changes desktop browsing and saving preferences alone does not claim they are applied.

## Rejected alternatives and planning decisions

A live mutation of capture paths would couple active capture, finalization and rooted workers to one mutable configuration. A generation restart lets capture adopt validated startup settings while enrichment continues using its original libraries. An ordinary tray Quit and launch remains useful for existing clients, but cross-root resume requires tracking prior libraries independently of the currently configured destination.

## Milestones

1. Share recording preferences, validate writes and expose explicit legacy repair using the existing config and settings UI.
2. Preserve and resume original-library enrichment across service generations and process restart using fixture libraries.
3. Add trusted effective settings and queued apply, with capture-generation restart and status presentation. Exercise contention, failure and retry.
4. Verify packaged behavior and record hardware-dependent acceptance separately.

## Verification design

- Run relevant recorder-control, recorder, app backend and frontend suites, applicable checks in `docs/development/VERIFICATION.md`, and canonical validation. Cover defaults, old preferences, disjoint writes, validation failures and actual registration.
- Use temporary libraries A and B and controlled fake capture states. Save B during startup, recording and finalization. Verify A is effective through actual finalization and B only after acknowledgment. Cover idle, unavailable/invalid B, other sessions, stale IPC responses, restart, app close, latest-request coalescing, and intentional tray Quit.
- Seed accepted and unconfirmed EOG, match history and ranked LP work in A. Prove pending work can install into A after B becomes effective and after a recorder process restart, with unchanged identities, attempt counts and deadlines. Inject a failure to register or reopen a root and verify safe retention and truthful status.
- When the supported hardware and League are available, repeat destination switching on the installed pair during a real match and finalization. Verify correct folders, ordinary playback, and installed login settings.

## Performance and reliability gates

The desktop observes cached settings. Configuration, rooted ledger registration and restart work run on explicit settings actions or startup. Capture frame processing and normal game polling retain their existing behavior. Apply never interrupts an admitted startup, capture, finalization or accepted enrichment. A fixture must prove no extra capture owner and no wrong-library write.
