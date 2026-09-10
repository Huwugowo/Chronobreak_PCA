# Local playback delivery boundary

Chronobreak's Windows desktop app serves local media through one ephemeral IPv4
loopback HTTP server. The native WebView media elements continue to own decoding,
buffering, seeking and audio. Delivery never copies a recording through Tauri IPC
or reads it into one application buffer.

## Threat model

The boundary protects media from unrelated web pages, DNS-rebinding requests and
local clients which do not possess the app's capability. It also prevents a
recognized route from streaming a file outside its registered location through
symlinks, junctions, reparse points or a path changing between selection and open.
Connection/header limits bound server-owned resources during local request abuse.

The Windows account, trusted bundled frontend/IPC, explicit library selection and
Riot's HTTPS asset service are trusted. This is not a sandbox against software
that can read the user's recordings, process memory, debugger state or WebView
profile directly. It does not protect against a compromised frontend, decoder
vulnerabilities, deliberate hardlinks made by the filesystem owner, concurrent
changes to an approved file's contents, or an attacker monopolising the OS/network.
The availability guarantee is finite server ownership, not uninterrupted service
while an attacker fills every slot. Non-Windows handle containment is currently
unimplemented and fails closed; no pathname-only fallback is used.

## Admission and URLs

`playback_policy.rs` reserves `127.0.0.1:0` before Tauri builds its first window.
The OS CSPRNG supplies a 256-bit per-instance secret. Failure to bind or obtain
randomness fails startup. Every media/static URL begins
`http://127.0.0.1:<port>/cap/<secret>/`. Port randomness is not authorization.
The server compares the capability before dispatching a route, then strips its
prefix while preserving the controller's `qb_playback_session` query. That query
identifies a playback load; it cannot authorize access.

Exactly one Host must match the reserved numeric loopback authority, including
port. Alternate localhost names, ports and absolute-form request targets are
rejected. An Origin, when present, must be exactly `http://tauri.localhost` in
production, or additionally `http://localhost:1420` in a debug development build.
Null, duplicate, remote and other local origins are rejected. Native media/image
requests can omit Origin, so an absent header is allowed with the capability.
CORS reflects only a permitted origin, varies on Origin, grants no credentials,
and exposes only range/length/type headers. GET and HEAD are supported; a narrowly
formed OPTIONS preflight permits those methods and the Range header. Request
bodies and transfer encoding are rejected before handlers.

These are bearer capabilities: a leaked URL includes the process capability and
may grant access to other recognized library routes to a client able to use it.
Do not log request URLs, headers, capability values or imported tokens, persist
them in library metadata, or send them to remote services. Native decoder binding
uses the URL only in trusted in-memory IPC/CDP state. Benchmark events retain route
labels and counters. Responses use no-store, nosniff and no-referrer and never
redirect. Exiting the process closes the listener and invalidates its URLs; the
next instance generates an independent capability. This is not revocation of a
copy of bytes that a client already obtained.

## CSP and development

The configuration contains a reserved `http://127.0.0.1:0` source in connect-src,
media-src and img-src. Startup replaces it with the bound origin in the generated
Tauri Context before Builder clones it. The listening socket stays reserved, so
there is no close/rebind race. This retains Tauri's script hashes and required IPC
and style sources, without trusting all loopback ports or adding another webview.
Object/frame/form loading and base-URL changes are disabled.

The supported development URL is local Vite at `http://localhost:1420`. A remote
`TAURI_DEV_HOST`, arbitrary localhost port or alternate production scheme is not
implicitly trusted. Any future runtime origin must be proven in that WebView and
explicitly added to the policy. CORS is not the primary authorization boundary.

## Files and root changes

`playback_file.rs` snapshots approved root identities from opened directory handles
using Windows `GetFinalPathNameByHandleW` with normalized DOS names. Candidate
handles use the same extended-path namespace and Windows ordinal case comparison,
component by component. No ad-hoc prefix stripping or string-prefix containment
is used. The candidate must be a regular file. A failing handle query or containment
check denies delivery. The exact validated handle becomes the Tokio streaming
file; its pathname is never reopened after validation.

Library and Data Dragon root path snapshots are made at explicit startup/selection.
Each request uses one immutable snapshot. A library settings change affects later
admissions; an already-approved stream may finish from its old handle. Redirecting
a root or descendant outside the snapshot fails the opened-handle check. An
internal link is allowed when its final target stays inside the approved root.
Renaming/replacing a pathname after validation cannot redirect that open handle
to different bytes. Preventing later writes to the already-approved file is outside
the account-level boundary and would interfere with ordinary media ownership.
These approvals identify filesystem locations, not immutable file IDs. Replacing
a regular file or directory at the same approved resolved location remains inside
that boundary. Pinning directory handles or file IDs against a filesystem owner
would add another sandbox beyond this threat model.

Imported MP3/WAV preview is the explicit external-file exception. Registration
approves one canonical exact file and mints an independent 256-bit token. Only the
current token opens it; registering a replacement selection or token-matched
release revokes future opens.
An old release cannot revoke a newer selection. The frontend serializes preparation
and revokes results that arrive after selection changes or disposal. Choosing
another source or closing the exporter unloads the preview and releases it.

## Routes and resource ownership

All routes, including future additions, pass through the same admission layer:

| Route | Approved source |
| --- | --- |
| games / identifier / video.mp4 | Registered library root; recognized game ID |
| clips / filename | Registered library root; recognized MP4/JPEG filename |
| music / filename | Compiled-in, recognized music bytes |
| music-preview / token | Current explicitly approved exact MP3/WAV file |
| probe / hevc.mp4 | Compiled-in probe bytes |
| ddragon / version / kind / asset | Registered cache root; validated version/kind/key |

Adding a route must not introduce a second listener, unguarded router, pathname
reopen or arbitrary file API. Data Dragon's existing trusted cache population
remains optional; downloads have the existing 12-second deadline and a 2 MiB icon
limit, including chunked responses. All resulting files pass the same opened-file
check before delivery. A cache miss or network failure cannot gate local replay.

The locked Hyper HTTP/1 implementation hosts the Axum router. It admits 64 live
connections, drops excess sockets without task/queue growth, bounds headers to
32 entries and a 16 KiB buffer, and gives headers a ten-second deadline. One active
handler/response body per HTTP/1 connection means that the same permit also bounds
active streams; a separate stream queue is unnecessary. Permits live through the
connection and are released on disconnect/error. A stalled body reader may occupy
one bounded slot until it disconnects. There is no total playback lifetime timeout.

File bodies retain the existing bounded ReaderStream chunks and range slices.
Embedded assets use borrowed static byte slices rather than full-asset copies per
request. Production counters report requests/ranges/bytes/completion/cancellation,
active/peak file streams, active/peak connections and rejected admissions. Parser
rejections before the policy layer are not counted as policy-rejected requests.

## Verification ownership

Wire tests cover each route with authorization, GET/HEAD, single ranges, 206 and
416, unsupported paths/types, origins/Host, imports and connection/header recovery.
Dedicated temporary Windows junction and replacement fixtures exercise containment.
The `delivery-route-probe` cold-open benchmark scenario is an explicit opt-in
acceptance seam: its benchmark-only command returns fixed sentinel fixture URLs,
and the production WebView checks HEAD, range fetch and native video/audio/image
loading. It has no caller-supplied path or evaluation interface and emits only
labels/status/length/type/load results. HEVC load can truthfully report unsupported.
Normal benchmark arms do not invoke it. Actual results and limitations belong in
`docs/execution/qb-replay-011.md`, not in this architecture contract.

The existing QB-REPLAY-008 comparison rules govern the matched pre/post seek/scrub
measurement. No new recording, decoder or long-recording performance claim follows
from this local-delivery hardening.

Windows handle semantics: [GetFinalPathNameByHandleW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew).
Tauri policy integration: [Content Security Policy](https://v2.tauri.app/security/csp/).
