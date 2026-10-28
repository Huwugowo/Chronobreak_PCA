# QB-DIST-002 — Recorder supervision and Windows login startup v2

## Purpose

Opening the ordinary desktop application must ensure that one background recorder
is available. Enabling login startup must launch that same recorder without
opening the library window. Closing the library must leave recording available;
quitting the recorder must finalize active capture through the existing service.
Repeated launches must reuse the owner instead of recording a match twice.

This v2 supersedes the original supervision/login design, preserving its staged implementation while tightening service/capture status separation, latest-under-lock config mutations, session capability limits and the QB-DIST-004 update contract. This design implements QB-DIST-002. Its acceptance criteria remain in
`feature-list.json`; this document specifies implementation and verification.

## Relevant planning-time architecture

- `recorder/src/main.rs` selects diagnostics, headless, smoke or ordinary tray
  operation. Release builds already use the Windows GUI subsystem. `run_headless`
  spawns `service::run` but waits for Ctrl+C before inspecting its JoinHandle,
  allowing initialization failure to leave a live process without a service.
- `recorder/src/tray.rs` owns the native tray/event loop and a service worker.
  Quit sends `ServiceCommand::Shutdown`; events project Idle, Recording, Error and
  ShutdownComplete. Preserve this tray and capture owner, rather than adding a
  second service or embedding capture into Tauri.
- `recorder/src/service.rs::run` initializes storage, validates the native plan,
  resolves media tools and audio before emitting Idle. Its command branch already
  cancels startup, finalizes capture and stops post-game work. Capture startup,
  retries, watchdog, finalizer and native graph remain its authority.
- `app/src-tauri/src/lib.rs::run` initializes the desktop, config, media tools and
  library. It has no recorder lifecycle owner. Benchmark launch resolution occurs
  before setup and already isolates app data/configuration.
- `app/src-tauri/src/config.rs` uses `toml_edit` and atomic replacement to preserve
  unrelated settings. App and recorder share `LeagueReplay` configuration, with
  a dormant `[app].autostart` boolean, defaulting to true, but no OS registration.
  Settings exposed to the UI currently omit autostart.
- `tools/media_runtime/stage_release.ps1` already stages app and recorder as sibling
  executables with `resources/media-runtime` beside them. Both media-tool consumers
  resolve that exact locked runtime relative to their own executable. The current
  Tauri installer resource map includes only media tools, not the recorder.
- Existing architecture authorities are `docs/architecture/recorder-lifecycle.md`,
  `media-runtime.md` and `desktop-replay.md`. The latter two include dated external
  backend wording; the sole supported capture path is native WGC/D3D11/NVENC as
  stated in recorder-lifecycle and the current service.

## Scope and non-goals

Implement recorder ownership, a small local status protocol, app ensure/observe
behavior, early-service-failure propagation, and a user-controlled login setting.
Establish the sibling-executable distribution contract using the existing portable
stager. QB-DIST-004 owns changing installer payloads and clean-machine install,
upgrade and uninstall certification. QB-DIST-003 owns complete profile/codec and
other installed settings. This feature supplies its login-startup contract.

No capture backend, recorder watchdog, bundle schema, recording-health authority,
partial recovery, retention, export, playback or League timing change is intended.
No Windows service, administrator elevation, scheduled task, startup downloader,
parallel tray, or app-close kill is introduced. Automatic recovery after a recorder
process crash is not promised; the app exposes failure and explicit Retry, and a
later normal app/login trigger may start an owner after the old one has exited.
An intentional tray Quit must remain stopped for the current app session until
explicit Retry or a later app/login launch.

## Exploration findings

The missing installed launch path and headless early-failure hang are concrete
code defects/gaps, not capture-performance findings. Existing service shutdown is
the integration point for graceful finalization. A process PID or successful
spawn does not establish service readiness.

Single ownership must cover tray and headless normal modes, concurrent desktop
windows, direct launches and login triggers. It must not be keyed to the media
root: changing libraries cannot create another recorder. Multiple Windows sessions
for the same user share config/output, so session-local ownership alone is too
weak. Conversely, different users must not block one another.

Replay benchmarks and ordinary synthetic UI fixtures currently launch an app
without starting capture. Preserve that isolation explicitly before enabling any
normal-app startup side effect. No tests may target an existing user library or
use a real League recording for destructive shutdown cases.

## Chosen design and rationale

### Shared boundary and process ownership

Add a small `recorder-control/` Rust crate used by app and recorder. It contains
versioned typed wire messages, bounded Windows transport/client primitives, the
ownership lease and scoped configuration-write locking. It must not depend on
Tauri, native capture, league-data, or the recorder service. Keep orchestration in
each consumer; use narrow injectable OS seams for deterministic failure tests.
Add this crate to the existing locked Windows CI check helper and each consumer's
lockfile. Use the supported repository Rust toolchain and Windows APIs directly;
there is no new daemon or general RPC framework.

Use a stable lock file under the current user's local app-data `LeagueReplay`
directory, outside the recording library and independent of app install version.
Hold one non-inheritable Windows handle for the full recorder lifetime, opened
with read/write access and sharing that rejects another writer or deletion.
Validate the profile-owned parent and reject reparse redirection; never unlink a
held or stale lock file to obtain ownership. Kernel handle closure releases the
lease after exit or crash; an empty leftover file is not an owner. This avoids
thread-affine named-mutex ownership across Tokio tasks and also covers the same
user in another Windows session. A contender that cannot acquire the lease must
query the current owner and exit without starting a service. An inaccessible or
ambiguous lease is an explicit error, never permission to start a second owner.

Acquire ownership before normal mode initializes capture, creates tray resources,
or opens a library. Diagnostics and dedicated smoke/fixture modes do not acquire
the production lease or serve the production pipe. Tests use a separately scoped,
sentinel-owned local-app-data directory and pipe namespace; shipping ordinary
launches cannot select a second production namespace through config/media paths.
Non-Windows builds retain current unsupported/platform behavior without invoking
Windows registration or launch operations.

### Status transport, readiness and shutdown

The recorder owner hosts one local named pipe derived from the current user SID
and product identity. Restrict its DACL to that user and SYSTEM, reject remote
clients, make handles non-inheritable, and use first-instance creation. One owner
serves at most four bounded connections, with one request/reply per connection,
4-KiB maximum frames and two-second request deadlines. Do not expose arbitrary
commands, filenames, shell arguments or media mutation through this pipe.

Protocol v1 accepts only Status. Replies carry protocol version, recorder PID,
Windows session ID, random process-instance nonce, and exactly two independent fields: `service_state` (Starting, Available,
Stopping, Failed, Stopped) and `capture_activity` (Idle, Starting, Recording,
Finalizing), plus an optional bounded capture-problem code. Service failure has
a separate bounded safe reason code; capture problems never set it.
This is ephemeral service availability, not QB-CAP-002 recording health or a new
durable media authority. Do not include recording paths, game IDs or credentials.
The client verifies the OS-reported pipe server PID, user and executable identity
against the approved sibling recorder path; mismatched image, session or protocol
is reported as an existing incompatible/unavailable owner, never killed or
replaced. A trusted owner in another session blocks duplicate startup and is
displayed as `OwnerInOtherSession`, with `recording_capable_here=false`, regardless
of whether that other-session service reports Idle or Recording. Per-user exclusion
prevents duplicate writers; it does not grant WGC/window access across sessions.
The current session cannot automatically record while that owner holds the lease.
No implicit takeover, remote-session capture, or local Ready projection is allowed.
The user must intentionally quit the old-session recorder and launch in the current
session; the application explains that limitation without offering force-kill.

Service Starting begins when ownership/transport are established. Service Available
follows the actual service Idle event, not child survival; capture activity starts
Idle. Capture lifecycle signals set Starting/Recording/Finalizing/Idle without
changing service availability. Stopping follows intentional shutdown. Publish terminal failure to pending clients
and propagate service exit promptly. If a client misses the short terminal state,
pipe closure/child exit still produces a failure, never readiness. Tray failures
continue to be visible through the existing tray error projection.

Service availability and capture-attempt outcome are separate fields. The service
state progresses Starting -> Available -> Stopping/Failed/Stopped, with capture
activity separately Idle/Starting/Recording/Finalizing and an optional bounded
capture problem. Only actual service initialization failure, unexpected service
return/panic, or control-owner failure can set service Failed. Existing retryable
capture startup errors and terminal errors for one League process generation keep
service Available; preserve the service retry/suppression policy. An Error event
alone cannot infer service death. Add typed capture lifecycle signals where needed
rather than parsing error text. A terminally unsupported capture attempt may say
available with capture unavailable/problem; it must never claim Recording. Service
readiness means initialized and available in the current session, not proof that
any hardware/target configuration will successfully record.
Tests cover retryable startup error then recovery, terminal per-generation failure,
normal later generation, and actual initialization error/worker exit independently.

Refactor lifecycle integration narrowly so tray and headless feed the same status
snapshot. Fix headless by selecting service completion against Ctrl+C; early error
returns immediately. Ctrl+C and tray Quit send the existing Shutdown command and
await service completion, retaining all existing finalizer ordering and native
cleanup bounds. A UI status timeout does not kill an initializing/finalizing owner.
Once Stopping is observed, no ensure operation starts a replacement until the
ownership handle is released; no shutdown deadline silently becomes force kill.

### App ensure/observe behavior and isolated launches

Add `app/src-tauri/src/recorder_supervisor.rs`. In an ordinary Windows launch,
resolve only `<current-executable-directory>/recorder.exe`, require a regular
non-reparse executable, and use direct argument-vector spawning with no shell,
no console, no inherited standard pipes, and no kill-on-drop/kill-on-close Job.
The sibling's own runtime resolver remains authoritative for media tools. Missing
or invalid recorder returns actionable unavailable status without blocking library
browsing, playback, or export. There is no PATH search or implicit cargo build.

First query an existing owner. Otherwise attempt one launch of the ordinary tray
mode; racing launch attempts are settled by the recorder lease. A duplicate
launcher exit is not Ready: complete the bounded status handshake with the winner.
Allow 30 seconds overall for owner/readiness observation; at expiry show that
startup did not complete and continue observing, without killing or launching
another process. A failed status request is Unknown, not evidence of process death.

The Tauri backend is the only app owner. One task serializes ensure/retry and
observes status no more often than every five seconds while the window is open;
frontend subscriptions consume its cached typed state. Multiple app processes may
observe the same recorder, but all starts remain subject to the one recorder
lease. The UI projects Starting, Available, Recording, CaptureProblem, Stopping,
Unavailable, ServiceFailed or OwnerInOtherSession from the independent fields.
OwnerInOtherSession always has recording_capable_here=false and no start/retry
action while the owner remains alive; other-session Idle/Recording never projects
local Available/Recording. CaptureProblem keeps service availability distinct;
Retry never restarts the service merely to bypass its capture retry/suppression
policy. Other Retry actions are offered only where meaningful. No per-frame callbacks or history queries
drive this task. Closing Tauri cancels its observer only; the recorder continues.
No automatic restart loop competes with tray Quit or service capture retry policy.

Benchmark builds (`replay-benchmark` feature) and recognized benchmark launches
disable the entire supervisor/registration path before setup. Add an explicit
`CHRONOBREAK_DISABLE_RECORDER_SUPERVISION=1` developer/test opt-out for ordinary
synthetic UI runs. In isolated mode return an explicit DisabledForTest state and
perform no pipe connection, spawn, production lock access or Run-key read/write.
The opt-out is not inherited as a production recorder instruction and must not
enable a second recorder. Update synthetic UI launch helpers and verification
instructions that use normal builds to set it. Do not modify archived benchmark
sources or historical result identities.

### Login registration and minimal UI

Use exactly one current-user Windows Run value named `Chronobreak Recorder` under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Its REG_SZ command is the
quoted absolute sibling recorder path in ordinary tray mode, with no library UI,
shell, environment expansion or user-supplied arguments. Reject embedded quotes,
invalid paths, and command lines over the documented 260-character limit before
mutation. The default tray invocation means login has the same visible tray and
service owner as app launch. Windows may delay Run execution; this design does
not promise instant recording availability at sign-in.

Add a separate autostart read/update IPC contract and a minimal Settings control,
`Start recorder when I sign in`, plus actual registration status and safe error
feedback. Show Enabled only for the exact installed command; absent is Off and
another command under the owned value is Needs repair. Do not equate the existing
default-true TOML preference with actual Windows registration or silently register
existing users on upgrade/first app launch. Existing desired preference and actual
registration may differ and must be represented explicitly. Explicit enable or
disable is the only operation that reconciles them in this feature.

The read IPC returns `desired_autostart: bool` and `actual_registration` as
`matching`, `absent`, `foreign`, or `unavailable`, plus a separate optional safe
error and an effective-startup observation (`unknown` unless independently known).
The toggle is checked only for `matching`, unchecked for `absent`, and unavailable
with explicit feedback for `foreign`/`unavailable`. In particular, legacy
`desired_autostart=true` plus `absent` means Off/not registered, not Enabled or
Needs repair. Clicking that unchecked toggle sends an explicit enable request even
though the dormant desired boolean was already true. A matching Run value with
desired false still shows actual registration as On and the mismatch separately;
explicit Off removes it and reconciles the preference. Reading any mismatch never
performs repair. Tests cover absent `[app]`, legacy true and false, with no Run-key
mutation on config load, settings read or ordinary app startup.

Serialize all app config writers, including ordinary settings and HEVC marker
reconciliation, through a short cross-process lock adjacent to config. Recorder
default config creation must honor that lock too. Every mutation acquires the
cross-process lock, reads and validates the latest persisted TOML inside that
critical section, patches only its declared owned fields, validates the resulting
merged document, and atomically publishes it before releasing the lock. Locking
a save of a previously loaded full Config is explicitly prohibited. Ordinary
storage settings patch output_path/auto_delete_days only, HEVC reconciliation
patches hevc_playback_supported only, and login updates patch autostart only.
Initial creation rechecks existence under lock and never overwrites a concurrent
creator. Return the merged Config to callers and reconcile published library/root
state through its existing coordinator; no stale in-memory full snapshot may be
written back. Tests interleave two independent stale callers, including config
creation, and assert both disjoint changes plus unknown fields survive. Do not hold library publication
locks across registry work or await. Autostart mutation reads the latest TOML and
prior Run value, applies/verifies the intended registry change, and atomically
updates only `[app].autostart` using the existing unknown-field-preserving writer.
On a persistence failure, restore the prior owned registry state if it is still
the value this operation wrote. If restoration fails or an external change races,
return explicit inconsistent/repair-needed state with actual readback; never
overwrite a newer foreign value or restore a stale whole config document.
Cross-resource atomicity is not claimed. Crash between the two writes leaves an
observable mismatch; subsequent reads display it and explicit Retry/reselection
can reconcile it. Do not create a second durable settings journal.

Disable removes only the exact expected installed command; absent is idempotent
success. A foreign/tampered command is a conflict requiring deliberate repair,
not authorization to delete an unrelated launch target. Neither toggle stops a
running recorder or touches media. Do not manipulate Windows StartupApproved or
override an OS/user startup-disable choice; report when effective execution cannot
be established. Full settings semantics remain QB-DIST-003.

### Stable path and update handoff to QB-DIST-004

The installed app and sibling `recorder.exe` live at a stable product install path;
the HKCU Run command must not point inside a version-numbered staging directory.
QB-DIST-004 must either preserve that stable path during upgrade or explicitly
migrate the exact owned Run command under the same config/registration rules.
The ownership lease and pipe namespace remain version independent. An updater
must explicitly ask the user to Quit the recorder from its tray (v1 Status IPC
cannot request shutdown), or use a later separately approved authenticated
shutdown integration. It must wait for actual
owner exit and completed finalization before replacing binaries/runtime, and
never overwrite/force-kill a live recorder to complete an upgrade. If shutdown or
replacement cannot safely complete, leave the installed pair usable and report
upgrade failure. Do not mix new app/old incompatible recorder or runtime bytes;
protocol/image mismatch remains explicit unavailable, never automatic takeover.
Version-compatible running owners may be reused only at the approved stable path.
Installer uninstall removes only the exact owned login value, and must preserve
configuration and user media. Clean-machine, upgrade and uninstall evidence stays
QB-DIST-004; this feature's staged-layout tests bind the stable path/identity and
mismatch semantics that integration must preserve.

## Rejected alternatives and planning decisions

- Embedding capture in Tauri or killing its child on close would lose the
  independent automatic recorder. Keep one detached recorder with its own tray.
- Process-name scans/PID files alone cannot prevent simultaneous startup or prove
  readiness; a kernel-held lease plus bounded status handshake can.
- A session-local mutex permits duplicate writers across sessions for one user;
  a profile lease covers that shared config/output boundary and is thread agnostic.
- PATH lookup, downloads, development target-directory guesses and arbitrary
  recorder-path settings weaken packaging identity. Use the existing sibling layout.
- Spawning headless at login would remove the expected tray. Use ordinary tray mode
  and fix headless early failure for its supported CLI/test lifecycle separately.
- A permanent restart daemon, remote control endpoint, durable recording status
  database, or settings transaction journal exceeds this coherent feature. Keep
  failure visible and retry explicit rather than hiding crash loops or stale state.
- Installing the recorder in Tauri resources under a different executable-relative
  runtime root would duplicate runtime discovery. QB-DIST-004 packages the approved
  sibling layout; this feature proves the contract using portable staging.

## Milestones

1. **Ownership and protocol.** Add recorder-control, Windows lease and restricted
   bounded pipe, versioned typed status and fake OS seams. Add CI/lockfile coverage.
   Prove one service owner under contention, release after crash, cross-session
   conflict, rejected peers and protocol/frame/time limits before app integration.
2. **Recorder lifecycle.** Integrate tray/headless ownership and service events;
   fix early service completion; preserve diagnostics/fixture isolation and graceful
   shutdown. Prove immediate init failure, duplicate launch, readiness ordering,
   intentional Quit, Stopping and cleanup through deterministic child fixtures.
3. **Desktop supervision.** Add sibling resolver, single ensure/observer task,
   isolation gates, typed UI status and explicit Retry. Update normal synthetic
   launch helpers. Prove app close leaves the recorder, stale responses cannot
   overwrite a new owner, and all benchmark/test opt-outs perform zero lifecycle
   OS operations. Stage the paired release through the existing script.
4. **Login control.** Add registry adapter, cross-process config mutation lock,
   targeted TOML updates/readback/rollback and minimal Settings control. Prove
   quoting, absent/owned/foreign values, concurrent writers, errors and crash-gap
   mismatch reporting without changing media or silently enabling old defaults.
5. **Windows acceptance and documentation.** Exercise portable installed-layout
   launch/login/repeated triggers/app-close/tray-Quit with generated media. Record
   actual sign-out/sign-in separately. Complete the applicable automated baseline,
   update current architecture and verification docs, and retain detailed evidence
   in the checkpoint before claiming the feature's criteria are satisfied.

## Verification design

Use `docs/development/VERIFICATION.md` as the command authority. Start with focused
new-crate and recorder-control tests, then recorder/app suites after interfaces are
connected. Planned commands include:

```powershell
cargo test --manifest-path recorder-control/Cargo.toml --all-targets
cargo fmt --manifest-path recorder-control/Cargo.toml -- --check
cargo clippy --manifest-path recorder-control/Cargo.toml --all-targets -- -D warnings
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo fmt --manifest-path recorder/Cargo.toml -- --check
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
npm run desktop:build --prefix app
npm run desktop:build:benchmark --prefix app
cargo build --release --manifest-path recorder/Cargo.toml
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/stage_release.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/media_runtime/verify.ps1 -RuntimeRoot build/media-runtime/windows-x86_64 -ReleaseRoot build/release/queueback
python build/validate_features.py
git diff --check
```

**Acceptance 1/2: one owner.** Build a real Windows child-process fixture using the
same lease/transport code, not process-list counting alone. Race at least ten
direct/app/login-equivalent triggers; record entrants and maximum active service
owners, expecting exactly one. Repeat with an owner still Starting, inaccessible
pipe, rejected image/protocol, another session, a crashed owner and a reused PID.
Multiple temporary contender processes may exist; none may initialize another
service or tray. Prove no force-kill/takeover on uncertain status.

For cross-session ownership, inject distinct Windows session IDs while contenders
use the identical per-user local-app-data lease and pipe namespace; assert that
no session component enters ownership and a different-session owner yields the
explicit conflict response. An ordinary child launch does not establish a real
second Windows session. A real same-user multi-session exercise requires a
disposable RDP/Fast User Switching environment and a separately labeled manual
receipt when available; do not substitute the injected check for that observation.

**Acceptance 3: unobtrusive lifecycle.** Use the actual portable release with a
dedicated config, isolated local-app-data/pipe and generated exact-HWND target.
Exercise app launch, repeated app launch, close/reopen library during active
capture, then tray Quit. Require continued frame progress across app close and
playable strictly decoded canonical output after intentional Quit, matching
source/metadata identity and no orphan owned FFmpeg worker. Inject missing runtime
and unsupported profile before startup: readiness must fail without a Ctrl+C
hang and UI must remain usable. Use the existing native fixture machinery and
service injection seams; do not redirect any failure test to user media.

**Acceptance 4: login control.** Unit tests use a registry facade; Windows integration
uses a unique test value in a disposable HKCU subkey and restores only owned test
values. Validate exact production name/key/command construction separately. Actual
sign-out/sign-in requires a disposable Windows test account/profile and must be
recorded independently from manually executing the Run command. Enable, sign in,
observe one ready tray owner, open app without duplication, disable, sign in again,
verify no login-launched recorder; later app launch must still ensure one. Check
that prior generated recordings/config fields remain intact. Do not alter the
current user's actual login preference merely to obtain test evidence.

Map UI tests to independent service/capture states, retryable and terminal capture
errors that leave the service available, actual service exit, and OwnerInOtherSession
with no local capability/start/retry. Also cover actual versus desired login
state, rejected updates and isolation. Map config tests to concurrent writers,
unknown-field preservation, registry/config errors, conditional rollback and crash
between writes. Keep code/command outputs and Windows receipts under ignored
`build/verification/qb-dist-002/`; record binary/runtime identities, scenario,
owner counts, exit/state transitions, source hashes and exact failures. Sanitized
checkpoint evidence must distinguish automated, generated-media, actual-login
and unavailable observations. A portable contract pass is not an installer pass.

## Performance and reliability gates

No extra work runs in capture-frame callbacks or game-data polling. One recorder
lease, one status listener, four bounded pipe clients, one app observer and finite
request frames/timeouts are the resource contract; no task-per-history-entry or
process polling loop. Idle observation cadence is at most once per five seconds.
Five minutes of generated idle/repeated launch/close operation must show stable
owned thread/handle counts after clients settle and no queued task growth. This
does not establish a League FPS or negligible-overhead claim.

Never bypass an ambiguous owner, kill an active recorder for readiness timeout,
replace foreign registration, drop accepted shutdown/finalizer work, or erase
partial output. Intentional shutdown remains observable until owned cleanup ends.
Tests must cover config/registration failure and interrupted startup, as well as
the happy path. Actual sign-in and interactive acceptance may remain unavailable;
if a required gate cannot run, retain non-done status and state the precise gap. Unavailable manual acceptance does not block other implementation units or applicable automated/integration checks; proceed through the staged development and record only the unmet acceptance gate separately. No unrelated verification campaign is authorized.

Planning API references (provenance, not recursive execution prerequisites):
[Windows mutex guidance](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createmutexw)
also recommends a profile locked file for per-user ownership;
[Run keys](https://learn.microsoft.com/windows/win32/setupapi/run-and-runonce-registry-keys)
define current-user login registration, delayed scheduling and the command limit.
