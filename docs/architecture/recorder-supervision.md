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
instance bound resources. Protocol v1 accepts only Status, with 4-KiB frames and
two-second deadlines. Clients verify OS pipe PID, token SID, executable path and
session against the reply, plus protocol and process-instance nonce.

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
Storage owns output path/retention, HEVC its capability bit, login autostart.
Unknown fields survive. Both consumers recheck creation under the lock. Merged
config returns to in-memory callers; root changes use the existing coordinator.
There is no production API for saving a stale full Config snapshot.

## QB-DIST-004 stable path and update contract

Portable staging retains sibling app/recorder and shared `resources/media-runtime`.
The installer must preserve a stable product path; Run cannot point into versioned
staging. Upgrade preserves that path or explicitly migrates the exact owned value.
Lease/pipe names remain version independent. Image/protocol mismatch means
unavailable, never permission to replace an owner.

Status v1 cannot request shutdown. Upgrade asks the user to Quit the tray and waits
for actual owner exit/finalization before replacing the paired binaries/runtime,
unless a later separately approved shutdown integration exists. Never force-kill,
overwrite a live recorder or leave incompatible mixed components. Failure preserves
a usable installed pair. Uninstall removes only exact owned registration and
preserves config/media. Installer payload and clean-machine certification remain
QB-DIST-004.
