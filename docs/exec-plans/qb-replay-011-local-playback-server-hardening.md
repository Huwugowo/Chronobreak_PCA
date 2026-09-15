# QB-REPLAY-011 — Local playback server hardening

## Purpose

Keep native local replay fast while preventing an unrelated web page or an
unauthorised loopback client from reading media through Chronobreak. All existing
delivery routes must share a small, explicit security policy. No recording or
clip migration is required.

## Relevant planning-time architecture

`app/src-tauri/src/playback_server.rs` owns an Axum router, Tokio file-slice
streaming, single-range parsing, cheap production counters and optional bounded
benchmark request records. Its six route classes are game video, clips (MP4/JPEG),
embedded built-in music, imported MP3/WAV previews, embedded HEVC probe and cached
Data Dragon PNGs. Lexical validators already constrain identifiers. The server
currently binds an ephemeral IPv4 loopback port, accepts requests without
authorization and emits wildcard CORS.

`lib.rs` starts the server in Tauri setup and stores the returned base URL in
AppState. `library.rs`, `music.rs` and commands for HEVC, imports and Data Dragon
append route paths to that base. The primary playback controller appends its own
`qb_playback_session` query for decoder/load association; this query is not an
authorization mechanism and remains unchanged. Diagnostics carry the full URL
only across trusted IPC/native WebView inspection; benchmark records contain
route classes and counters, not URLs.

Tauri clones configuration before setup and may create the normal window before
setup runs. Therefore the listening socket must be reserved before Builder runs
to put its exact origin into CSP before the first document loads. Benchmark mode
already creates one isolated WebView manually. The native video element, decoder,
controller and recording format remain the existing owners of playback.

## Scope and non-goals

Change loopback admission, URL capabilities, delivery file resolution, imported
preview revocation, CSP, bounded transport and their tests/documentation. Preserve
GET, HEAD, single ranges, native cancellation and local/offline playback.

Do not replace Axum, HTML media, the replay controller or the recorder. Do not add
TLS, accounts, cookies, a proxy, a general file API, ReplayIndex, preview generation,
an installer, or a new performance framework. QB-REPLAY-009 audible/A-V observation
is explicitly deferred by the user and is not a dependency of this feature.

## Exploration findings

- Port randomness cannot authorize a request. Browser media/image requests can
  omit Origin; CORS alone cannot prevent their reads.
- Existing base-URL plumbing can carry a capability path without changing every
  consumer or the controller's session query.
- `axum::serve` deliberately hides HTTP connection configuration and enables
  automatic protocol handling. Its existing Hyper dependencies expose the small
  HTTP/1 server builder needed for explicit bounds.
- HTTP/1 serves one response body at a time per connection. A connection permit
  retained through the response therefore also bounds handlers and active file
  streams; an independent stream semaphore is redundant here.
- Canonicalizing an input path then reopening it leaves a race. Windows can report
  the final resolved path of an already-open handle; streaming that handle closes
  the validation/open gap without walking and locking every directory.
- Five identity-matched trials per arm are required by QB-REPLAY-008 comparisons.
  The historical single seek/scrub trials are insufficient as a comparison arm.
  A prepared generated 240-second H.264/AAC v2 fixture is available under
  `build/perf/qb-replay-012-webview/.chronobreak-replay-benchmark`. This focused
  comparison measures delivery overhead, not long-recording scaling or a new
  decoder/capture claim.

## Chosen design and rationale

### Boundary and capability

Protect against hostile browser origins, DNS rebinding, unrelated local clients
without the capability, accidental/malicious links escaping approved roots and
bounded connection/request exhaustion. The OS account, trusted Tauri frontend,
IPC, configured library selection and Riot HTTPS endpoint remain trusted. Do not
claim protection from code with the user's file/memory/debugger privileges, a
compromised frontend, malicious media decoders, changing file contents through
another write handle, hardlinks deliberately placed by a filesystem owner, or
availability against an attacker monopolising the OS/network.

Reserve a nonblocking `127.0.0.1:0` listener before Tauri Builder creation. Generate
32 random bytes using the OS CSPRNG, fail startup if randomness fails, encode as
64 hexadecimal characters, and return a base URL ending in `/cap/<secret>`.
Mount all routes behind one admission function which strips that exact prefix
before dispatch. Missing/wrong capabilities and unregistered paths fail closed.
Generate a new capability for each process/server instance. Do not log the base
URL, request URI, imported capability, or request headers. Return no redirects.

Imported previews retain the existing one-current-preview model with an
independent 256-bit random token and approved canonical exact file path. Registration
validates MP3/WAV and a regular file. Replacement or explicit token-matched release
revokes future opens; a stale release cannot revoke a newer selection. Wire release
to preview disposal/replacement. Already-admitted streams may finish from their
approved handles. App exit ends the listener and all capabilities.

### Request policy and CSP

Require exactly one Host matching the bound `127.0.0.1:<port>`; reject other names,
alternate ports and absolute-form request targets. Reject any present Origin
outside the exact production Windows origin `http://tauri.localhost` and, in a
development build only, the configured `http://localhost:1420`. An absent Origin
is allowed only with the capability. Null/hostile/duplicate origins fail closed.
Allow GET/HEAD and narrowly formed OPTIONS preflight for those methods and Range.
Do not accept request bodies or transfer encoding. CORS reflects only the allowed
origin, varies on Origin, exposes only range/length/type headers, and grants no
credentials. Apply common no-store, nosniff and no-referrer headers to responses;
keep Data Dragon cache policy private if caching is retained.

Replace a reserved localhost port-zero CSP source with the actual reserved origin
in the generated context before Builder construction. Limit loopback sources to
connect/media/image directives; remove unused asset protocol allowances after
consumer inspection. Preserve Tauri's script hashing and the required IPC/style
sources. Development support is local Vite at its configured exact origin; remote
TAURI_DEV_HOST origins are not implicitly trusted. Verify actual Origin/CSP behavior
in a packaged WebView and exercise the allowed development origin in HTTP tests.

### File resolution

Snapshot approved canonical library and Data Dragon roots at explicit startup or
settings registration, not afresh from a potentially replaced root on each request.
Obtain each root identity from an opened directory using the same
`GetFinalPathNameByHandleW` flags and namespace as candidate files. Compare path
components with consistent Windows ordinal case semantics; do not compare a DOS
input path to an unrelated extended/volume namespace or strip prefixes ad hoc.
Requests take one immutable root snapshot. Open the candidate, reject nonregular
files, resolve its final path from that handle and check component containment in
the approved root (or exact equality for imports). Stream the same opened file.
Reject any resolution failure. Never stream an unverified reopened pathname.
The Windows implementation uses `GetFinalPathNameByHandleW`; unsupported platforms
must fail closed unless an equivalent opened-handle implementation is provided.

Root changes affect subsequent admissions. An already-admitted read keeps its
previously approved handle; no forced decoder interruption or per-chunk path
lookup is required. A link resolving within the approved root is harmless; an
escape through a junction/symlink/reparse point is rejected. Directory races which
redirect the open outside the snapshot are rejected by the final-handle check.
The same policy covers cached Data Dragon reads. Existing Riot cache population
is trusted application behavior; it must not create a bypass to deliver an
unvalidated file. Bound icon download bytes so simultaneous cache misses have a
finite allocation bound.

### Resource bounds

Keep the router and range/CountingReader implementation. Serve it with the locked
Hyper HTTP/1 builder: 64 live connections, reject excess sockets without spawning
or queueing; 16 KiB header buffer, 32 headers, ten-second header deadline and no
request bodies. Retain each permit until the connection task drops. A socket idle
deadline may reclaim stalled readers without imposing a total stream lifetime.
Normal long playback must never expire merely because it has played for a long
time. Add cheap active/peak/rejected-connection and rejected-request counters.
Use bounded file chunks; serve embedded slices without per-request whole-asset
copies. No new remote dependency is introduced.

## Rejected alternatives and planning decisions

- Cookie/header authentication cannot directly serve every HTML media use case;
  capability paths reuse the existing URL API with no persistence.
- Strict mandatory Origin breaks legitimate no-CORS media requests. Capability
  authentication plus validation of any supplied Origin provides the boundary.
- TLS, per-file capability registries for all library media and per-request tokens
  add management cost without addressing a threat beyond the chosen boundary.
- Separate connection/request/stream queues duplicate HTTP/1's sequential ownership.
- Lexical checks alone, pre-open canonicalization alone and final-component-only
  reparse flags do not establish opened-file containment.
- Directory-handle sandboxes protecting against a same-user filesystem owner
  exceed this local delivery boundary. Validate the actual opened handle and state
  the limit on subsequent content mutation instead.

## Milestones

1. Finalize this plan and initialize/link the execution checkpoint. Capture the
   current production build and a fresh immutable five-seek/five-scrub baseline
   before modifying delivery. Preserve all failed/invalid trials.
2. Implement the common capability/Host/Origin policy, socket reservation/exact
   CSP, bounded HTTP/1 transport and counters. Verify wire behavior on loopback.
3. Implement opened-file containment, imported random token/release lifecycle,
   bounded icon downloads and route-wide regression fixtures. Write the durable
   threat model at `docs/architecture/local-playback-security.md` and link it from
   desktop-replay architecture.
4. Run focused automated checks, the complete applicable app checks, production
   WebView route compatibility and the matched post-change seek/scrub arm. Review
   defects and disposition actual deltas using QB-REPLAY-008 rules. Update the
   execution record and canonical feature evidence only after all gates pass.

## Verification design

Automated loopback tests cover all six route classes through the common policy:
missing/wrong/rotated capabilities; allowed/hostile/absent/null origins; hostile
Host/rebinding; GET/HEAD/OPTIONS; valid full/open/suffix ranges, 206 and 416
(including empty files); bad identifiers/types; random import replacement and
token-matched release; connection/header/body limits and guard/counter recovery.
Temporary Windows junction fixtures prove outside-root rejection, within-root
success, switched roots and replacement between path selection and opening.
Assert the streamed bytes come from the validated handle after a pathname changes.

Use a bounded opt-in seam in the existing benchmark WebView to fetch route fixtures
and validate status, headers and bytes under the production CSP. Native replay
and seek/scrub prove media playback compatibility; fixture clip/music/image/probe
loads validate the remaining consumers. The seam may only use sentinel-owned
fixtures and must never emit capability URLs in retained events. Do not add a
general-purpose remote evaluation or file-serving interface.

Run `cargo test --manifest-path app/src-tauri/Cargo.toml` both normally and with
`--features replay-benchmark`, `cargo fmt --manifest-path app/src-tauri/Cargo.toml
-- --check`, `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets
--features replay-benchmark -- -D warnings`, `npm.cmd run test --prefix app`,
`npm.cmd run check --prefix app`, `npm.cmd run build --prefix app`, and
`npm.cmd run desktop:build:benchmark --prefix app`. Run benchmark analyzer fixtures
if the harness changes. Validate schema/invariants/links using VERIFICATION.md.
Recorder/runtime suites are unnecessary unless their code or contracts change.

## Performance and reliability gates

Use the existing sentinel runner, matrix planner/verifier and analyzer. Fix the
same fixture, full observer, five-second warmup and scenarios before baseline:
five paused-control seek trials with at least 40 observations per near/far and
forward/backward stratum (160 generated targets, seed 20260910, 160-second pacing
window), and five playing scrub trials with five deterministic bursts at 60 Hz.
Use a five-second minimum cooldown between new process trials. Keep
targets, request rate, layouts, cooldown and runtime identical after the change.
Run `powershell -NoProfile -ExecutionPolicy Bypass -File
tools/replay_benchmark/run.ps1 -Manifest <immutable-manifest>` for each trial,
verify ordered matrix results, aggregate each arm and pass the baseline report to
`tools/replay_benchmark/analyze.py --compare` for the post-change report.

An unfavorable change requires disposition when it exceeds both five percent and
`max(resolution floor, 3 * baseline MAD)` (5 ms latency, 0.1 normalized CPU point,
8 MiB memory, 1% byte/count metrics). Forty observations in the exact stratum are
required for tail comparisons. New errors, timeouts, recovery, required event loss,
source mutation or sustained growth always require disposition. Inspect request
range/first-byte/cancellation outcomes as well as user seek latency and resources.
Preserve invalid runs, identify the violated invariant before a fix or rerun and
do not weaken the feature criteria to obtain a passing report.
