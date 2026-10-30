# QB-CAP-003 — Interrupted Recording Recovery v2

## Purpose

Finish interrupted publication of a validated recording in its original bundle,
without overwriting another artifact or inventing successful capture evidence.
Retain and explicitly report all ambiguous and unvalidated output.

## Relevant planning-time architecture

- `recorder/src/service.rs::stop_recording_inner` accepts only a clean backend
  candidate, validates it, and constructs schema-v2 `RecordingMetadata`.
- `finalizer.rs::publish_validated_candidate` syncs `metadata.pending.json`,
  renames `video.partial.mp4` to `video.mp4`, then publishes `metadata.json`.
  Ordinary error rollback cannot run after process death.
- `validate_candidate` combines a five-second/one-MiB packaged ffprobe summary
  with the existing bounded fragmented sample-table reader and strict replay-time
  producer reconciliation. It does not decode footage.
- MP4 does not contain the recording UUID. The parser receives that UUID from
  expectations; matching stream facts alone cannot establish file identity.
- Production `main.rs` holds the recorder-control owner lease through service
  shutdown. `service.rs::run_inner` resolves media tools before starting capture.
- The app discovers one entry per safe timestamp/collision-suffix directory.
  Only canonical metadata and video authorize playback; incomplete entries are
  protected from automatic retention. Refresh does not reconcile recorder media.
- Windows sidecar and recorder-control code already demonstrate pinned directory
  handles, reparse rejection, file IDs and handle-relative no-replace renames.

## Scope and non-goals

Windows recorder publication and one bounded startup reconciliation pass. No new
UI/status framework, database, periodic scan, remux, decoding or recorder redesign.
Bare interrupted capture has no durable terminal producer counts: retain it for
possible future salvage rather than manufacture a clean schema-v2 timeline.
League compatibility/FPS/GPU measurements remain pending. CFR selection,
dimensions/profile/per-sample validation, partial WGC construction, audio and
epoch alignment remain separate work.

## Exploration findings

The recoverable states are pending metadata beside either private or canonical
video, after successful validation. Existing canonical metadata is authoritative
and must never be replaced. Both video names, missing video, malformed metadata,
reparse paths and mismatched identity are ambiguous. Older pending metadata lacks
independent file identity; preserve it rather than assume association.
QB-CAP-002's broad health model is unimplemented and not needed for this unit:
existing terminal producer evidence and exclusive recorder ownership suffice.

## Chosen design and rationale

1. Add an additive versioned `publication` receipt to staged metadata, binding
   its media UUID to the bundle directory ID and closed media file ID, volume,
   length and last-write time. Keep it in canonical metadata. The app strict
   `library.rs::MetadataDocument` must explicitly accept the optional object and
   preserve it through saved-state writes; all other unknown fields still fail.
   Existing canonical files without the receipt remain readable. The app does not
   use this recorder-only recovery proof to authorize playback. No full-file
   hashing or separate transaction database.
2. Pin ordinary directory ancestry while opening/publishing; reject reparse
   points. Read metadata with a 64-KiB ceiling. Hold file handles against writes
   and deletion during inspection. Reopen/recheck identity after probing before
   acquiring rename access. Use the established Windows handle-relative rename
   pattern with replacement disabled for forward publication and rollback.
3. Recover only when canonical metadata is absent, exactly one video name exists,
   pending metadata/receipt/schema/UUID/timeline agree, and the file and directory
   match the receipt. Re-run the existing strict validator with persisted producer
   expectations and require exact timeline equality. Never infer frame count or
   producer success. Publish private video if necessary, then the original pending
   metadata bytes. A crash at either transition remains recoverable.
4. Startup runs under the existing owner before capture admission. Inspect at
   most 512 immediate entries, admit at most four media probes, and stop admitting
   work after ten seconds. Each admitted probe retains its five-second bound;
   check shutdown between candidates and retain owned work until it joins.
   No recursive walk, parallel probes, periodic rescans or per-frame work.
5. Emit structured per-bundle dispositions for recovered, canonical-untouched,
   ambiguous/conflicting, missing evidence, invalid media, inaccessible and
   budget-deferred cases, plus an aggregate startup result. Preserve every byte
   on failed classification; create no new game directory or library entry.
   Ordinary completed bundles require no probe. No persistent error sidecar is
   needed: recorder daily logs retain reasons and the app retains incomplete rows.

## Rejected alternatives and planning decisions

- Trusting matching UUID fields or ffprobe alone cannot bind an MP4 to metadata.
- Full hashing/decoding/remuxing at startup would violate bounded work and expand
  scope. File generation receipts deliberately do not authenticate hostile edits
  that restore file IDs, size and timestamps.
- Promoting bare fragments by deriving producer counts from media would weaken
  the existing integrity contract. Preserve those bytes instead.
- Recovery on every library refresh risks capture ownership and repeated scans.
- Shared generic storage/health frameworks are unnecessary for this narrow unit.

## Milestones

1. Implement receipt and safe publication primitives; preserve normal two-step
   publication, rollback and finalizer validation behavior.
2. Implement bounded classification/revalidation/reconciliation and wire it into
   service startup with cooperative shutdown and explicit diagnostics.
3. Update the strict app reader with the optional receipt field and a regression
   proving existing/recovered bundles, descriptor admission, saved-state receipt
   preservation and unrelated unknown-field rejection.
4. Add focused deterministic state/collision/idempotence/budget tests and a generated
   media child-process interruption fixture. Run focused and recorder baseline
   checks, inspect recovered output, update canonical evidence and commit.

## Verification design

- Unit tests exercise both pending/video states, completed no-op, foreign and
  canonical collisions, changed file/metadata identity, malformed/oversized or
  missing evidence, duplicate video names, reparse paths and scan bounds.
- A self-executing test child publishes generated fragmented H.264/AAC fixture
  media and stops at pending sync, video rename and metadata rename; the parent
  kills/reaps it, runs production reconciliation twice and verifies one canonical
  pair and exact source bytes. A generated muxer killed before graceful close
  demonstrates retained bare partial disposition. No real recordings are used.
- Packaged-runtime opt-in fixture: `cargo test --manifest-path recorder/Cargo.toml
  recovery::tests::packaged -- --ignored --nocapture` with
  `QUEUEBACK_TEST_MEDIA_RUNTIME=build/media-runtime/windows-x86_64`.
- Recorder `cargo check/test --all-targets --all-features`, `cargo fmt -- --check`
  and `cargo clippy --all-targets --all-features -- -D warnings`.
- Open and seek recovered generated output where the environment permits;
  strict fixture decode/seek checks complement, not silently replace, that gate.
- App backend tests, formatting and strict Clippy; frontend check/build (no frontend
  behavior change). Canonical feature schema/graph/artifact validation and
  `git diff --check`.

## Performance and reliability gates

One finite pass, bounded metadata memory, at most one media child and one fragment
reader, no decoding or hashing in production. Record actual fixture startup timing,
probe counts, idempotence and preservation results. Shutdown waits for owned work;
no dropped background recovery task or competing capture writer. Filesystem calls
can still block in the OS, as in existing storage. Receipt-less old or incomplete
captures remain explicit limitations, not successful recovery claims.

## Plan review

Challenged the UUID association assumption, both rename crash windows, canonical
collisions, reparse/replacement races, shutdown ownership, retention and library
duplication. The receipt addresses the missing association without weakening media
validation. Keeping uncertain fragments and old pairs is intentional. The design
needs no QB-CAP-002 health framework and does not change QB-CAP-004/QB-CAP-007.

This version supersedes the first plan because the strict app metadata reader
rejects unknown fields. Its narrow optional receipt admission is required for
normal and recovered publication to remain playable. Review preserves legacy
receipt absence, unknown-field rejection, saved-state roundtrip and the existing
media-only authorization boundary. No other design or requirement changes.
