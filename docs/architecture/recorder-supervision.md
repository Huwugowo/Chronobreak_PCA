# Recorder supervision and login startup

The desktop observes an independent sibling `recorder.exe`. Closing the library
cancels its observer, never capture. One initial ensure and explicit Retry use
direct console-free spawning; there is no automatic restart loop after tray Quit.
A missing recorder leaves browsing/playback/export usable. Observations run every
five seconds; readiness is the actual initialized service event, not child survival.

A successful launch request projects Starting until readiness is observed. Explicit
launch failure projects Unavailable with Retry. An occupied lease without status
is uncertain ownership: Retry only rechecks ownership and cannot replace that owner.
A known ServiceFailed owner offers no Retry until it exits; the user must Quit its
tray and wait for exit. No Retry action terminates a recorder.

`recorder-control/` owns a lifetime, non-inherited Windows file lease at local
app-data `LeagueReplay/recorder-owner.lock`. It excludes competing writers and
deletion across sessions for the same user, independently of media/config/install
paths. Exit/crash releases ownership; a leftover file is not an owner. Ambiguous
ownership never permits force termination or takeover.

The SID-named pipe permits that user and SYSTEM and rejects remote clients. First
instance creation reserves the namespace; one active request and one waiting
instance bound resources. The v1 Status request stays compatible, with 4-KiB
frames and two-second deadlines. The additional settings request is bound to the
reported recorder instance. The recorder verifies the requesting Windows user,
session and sibling desktop executable before returning settings or queuing apply.

Service availability and capture activity are independent. Initialized Idle means
Available. Capture Starting/Recording/Finalizing/Idle and bounded capture-problem
codes never imply service failure. Retryable startup failures and terminal failure
for one process generation retain Available. Actual initialization failure,
service exit/panic or control failure can fail availability. Intentional shutdown
retains ownership through existing capture/finalizer cleanup. Headless observes
early completion as well as Ctrl+C.

Per-user exclusion does **not** grant cross-session capture access. An owner in
another Windows session always projects OwnerInOtherSession, with no local
recording capability or restart action, even for Idle/Recording. The user must
Quit that session's tray before starting here. No implicit takeover is offered.

Benchmark builds/launches and `CHRONOBREAK_DISABLE_RECORDER_SUPERVISION=1` skip
all supervision and Run-key operations. The synthetic preprod launcher sets the
opt-out. Recorder media/config overrides cannot create another production owner.
Diagnostics and tray smoke skip production ownership.

Login registration is exactly HKCU
`Software\Microsoft\Windows\CurrentVersion\Run`, value `Chronobreak Recorder`,
REG_SZ with only the quoted absolute sibling path, bounded to 260 UTF-16 characters.
Reads never register/repair. Exact matching is On, absent is Off, foreign/unreadable
is unavailable. Dormant TOML autostart=true plus absent remains Off. Desired
preference and effective Windows execution are separate; the latter is unknown.
Explicit enable/disable touches only the exact owned value and `[app].autostart`.
Foreign values remain intact. Persistence failure conditionally rolls back only
this mutation's value; crash between registry/TOML can leave a visible mismatch.

Every config writer holds the same adjacent `config.toml.lock`, reads/validates
latest persisted TOML under it, patches only owned fields and atomically publishes.
Storage owns output path/retention, HEVC its capability bit, and login autostart.
Recording preferences use shared native validation for the supported choices.
Unknown fields survive. Both consumers recheck creation under the lock. Merged
config returns to in-memory callers; root changes use the existing coordinator.
There is no production API for saving a stale full Config snapshot.

The desktop library opens a newly saved folder immediately. The recorder
publishes its own effective destination and capture mode through authenticated
settings queries. Apply coalesces requests and reads the shared configuration
once capture admission, startup and finalization are idle. A validated successor
generation reuses the existing process, tray and ownership lease.

Post-game workers belong to the recorder process and keep each library's existing
ledgers. A bounded registry beside the shared config records the path and
directory identity before admitting recordings. A recorder restart reopens those
identities. Prior libraries can retire after their result and history workers
have durably settled. Unavailable or replaced libraries remain registered.

## QB-DIST-004 stable path and update contract

Portable staging retains sibling app/recorder and shared `resources/media-runtime`.
The per-user setup publishes the pair at `%LOCALAPPDATA%\League Replay`. The
owned Run value points at the recorder in that stable folder and stays unchanged
during upgrade.
Lease/pipe names remain version independent. Image/protocol mismatch means
unavailable, never permission to replace an owner.

Setup takes the exclusive installation guard and the recorder ownership lease.
The desktop and recorder hold shared installed use through actual process exit.
An occupied lease asks the user to close the desktop and Quit the recorder from
its tray menu before retrying. This covers capture, finalization and worker
shutdown. The supported historical beta predates these leases. Setup verifies
its files and registration, checks for existing processes, and denies new
image opens by NTFS file ID through transaction settlement.

Setup stages the complete pair and media runtime on the same volume. A
flushed transaction record in `%LOCALAPPDATA%\.Chronobreak-setup` identifies the
candidate, previous installation and product registration. Setup moves the
verified old tree aside, then moves the complete candidate into the stable path.
Rerunning setup repairs an interruption before proceeding. Setup restores the
verified prior generation before commit and retains a verified new generation
after commit. An unrecognized generation or registration stops recovery.

Uninstall removes verified application files, the owned shortcut and Windows
registration, and the exact recorder Run value. It leaves other files in place,
including clips or captures beneath the product folder. Setup uses the same
configuration lock as app settings writers when reading or removing recorder
startup registration.
