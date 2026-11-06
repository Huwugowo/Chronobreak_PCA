# Two-group audio feasibility probes

These standalone Windows fixtures investigate QB-AUDIO-001. They are not wired
into the recorder, replay, or export. Feature lifecycle and the next readiness
gate live in `feature-list.json`.

Use **64-bit Windows PowerShell** (`powershell.exe`, .NET Framework), from the
repository root. The activation and source-separation checks require no downloaded
dependencies, driver, endpoint rerouting, game hooks, League process, media runtime,
or user media files. Additional playback/timing fixtures have prerequisites below.

```powershell
# Activate both process-loopback modes without initializing or reading audio.
powershell -NoProfile -ExecutionPolicy Bypass -File tools/audio/probe_process_loopback.ps1 -RequireAvailable

# Play two brief, quiet generated tones and verify real source separation.
powershell -NoProfile -ExecutionPolicy Bypass -File tools/audio/probe_process_loopback.ps1 -ControlledProof
```

Each invocation compiles a dedicated hidden diagnostic executable and retains its
stdout, stderr, and successful diagnostic report under a fresh ignored
`build/perf/qb-audio-001/` directory. Activation failures are diagnostic results;
`-RequireAvailable` makes unavailable activation exit nonzero. The controlled
proof always exits nonzero when its separation assertions fail. A compiler,
native API, timeout, or process error also fails and retains available logs.

The wrapper assigns the probe to a kill-on-close Windows Job Object before a
named startup event allows it to spawn renderers. All descendants inherit that
job. Normal return and timeout terminate/join the job and query its active-process
count until zero; `FIXTURE_PROCESS_TREE=EMPTY` records that observed postcondition.
Closing the wrapper also closes its non-inherited job handle. A focused timeout
check is:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/audio/probe_process_loopback.ps1 -ControlledProof -TimeoutMilliseconds 3000
```

This check must exit nonzero with both the generated-tree termination message
and `FIXTURE_PROCESS_TREE=EMPTY`. A nonzero exit from compilation, activation or
tone assertions alone is not a timeout-cleanup pass.

The controlled proof creates a target parent whose child plays 697 Hz and an
independent process that plays 1301 Hz. It simultaneously opens include-target-tree
and exclude-target-tree WASAPI clients at stereo 48 kHz / 16-bit, reads for six
seconds, and saves **only numeric spectral/packet summaries**. Captured PCM is
never written. Only the fixture's own processes are joined or terminated. Existing
system audio can enter the exclusion client, so this is an interactive controlled
fixture, not a hermetic offline unit test. Mute, low device volume, audio effects,
or unrelated sound at the fixture frequencies can make its assertions fail.

Both groups must have at least one second of samples, an expected-tone amplitude
of at least 0.005 full scale, unwanted/wanted amplitude ratio at most 0.05, zero
reported timestamp errors and no post-start discontinuity. The same measurements
must reject swapped group identities. The amplitudes are projections over the
whole capture window; they do not measure the renderers' gain or capture overhead.

## Platform interpretation

Microsoft documents build 20348 as the minimum for the process-loopback activation
parameters, while OBS documents application capture on Windows 10 version 2004+
and Windows 11. These are different claims. An OS build check alone does not
establish whether this capability works. The checked-in sanitized observation at
`docs/performance/results/qb-audio-001-process-loopback.json` records a controlled
success on Windows 10 build 19045.6466; it does not certify every Windows 10 build,
audio device, or actual League session.

The user preference is to support Windows 10 when it can share this straightforward
API path with little overhead. Do not add virtual audio drivers, endpoint routing,
mix subtraction, injection/hooks, or a substantial alternate legacy backend to
obtain Windows 10 support. Runtime activation, initialization and actual capture
behavior must inform any eventual support decision.

## Timing interpretation and the next proof

The activation check proves only activation. The tone proof establishes only
process-tree source membership. Neither proves recording-time A/V synchronization,
independent replay controls, export mixing, long-run reliability, or performance.

Keep packet QPC values distinct from frame/sample counts. On the observed virtual
clients both first and last device-frame positions were zero; the device-position
clock error is therefore **null**, not a valid six-second drift measurement.
`qpc_vs_cumulative_samples_elapsed_ms` compares the first/last packet QPC interval
with the corresponding accumulated sample interval. Its nonzero result is a
diagnostic requiring explanation, not a demonstrated media A/V error and not
permission to add a compensating offset or widen an acceptance tolerance.

The controlled proof also retains a bounded per-packet ledger: QPC, device position,
frame count, cumulative sample start, flags and numeric spectral/peak observations.
Reconcile an existing report into a **new** output file with:

```powershell
python tools/audio/analyze_packet_ledger.py <run>/result.json --output <run>/packet-accounting.json
```

`arithmetic_reconciliation_passed` proves only that timestamp span equals sample
duration plus the sum of packet gaps/overlaps. `contiguous_valid_timestamp_coverage`
is separate, and no arithmetic result proves video alignment. The observed
2026-10-31 run has four positive gaps per group totaling 79.4253/66.1441 ms,
zero overlaps and no reported post-start discontinuity. The virtual clients do
not flag those timestamp gaps. Concatenating the packets would discard real
timestamp coverage; this is a capture-time contract question, not playback drift
to compensate for.

## Actual WebView2 channel/control fixture

Requires the locked media runtime, an installed Playwright Node module and the
pre-existing release desktop executable. The fixture records that executable's
hash and explicitly does **not** establish a current-tree desktop build.

```powershell
# Set this to an installed Playwright module directory before running.
$env:CHRONOBREAK_PLAYWRIGHT_MODULE = '<absolute-module-path>'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/audio/probe_webview_groups.ps1 -PlaywrightModule $env:CHRONOBREAK_PLAYWRIGHT_MODULE
```

The runner generates a 20-second H.264 MP4 with one quad AAC stream containing
697/941/1301/1709 Hz on its four channels. The existing strict corpus builder
creates an isolated schema-v2 bundle/library. The launched app has a dedicated
configuration, data directory and WebView profile, retention disabled and recorder
supervision disabled. A gated launcher and kill-on-close job contain its complete
process tree. The local diagnostic connection must allow access to the app's
WebView; connection failure is an environment result, not unsupported multichannel
audio. The runner checks the connected app's exact isolated library before UI work.

Inside the actual WebView, one test video feeds a MediaElementAudioSourceNode,
four-channel splitter, per-channel gains and stereo merger. Numeric observations
assert channel identity, group mute, 0.5 gain, pause/seek, presented-frame rate,
native loop, same-element reload and persistent-element layout movement. Swapping
the two group identities must fail. No capability URL or captured audio samples
are written to the result. Nodes and the test element are disposed; the existing
app primary is paused for the test. This is a mechanism fixture, not product UI,
acoustic output-latency, fullscreen, controller recovery or production performance
verification.

The observed WebView2 156.0.4314.56 exposed all four channels, with independent
mute/gain and pause/seek passing. The latest matched run measured the same native
video before creating the audio-source node, then with the control graph. Both
passed presented-frame advancement at 0.25x, 0.5x, 1x, 2x, 4x and 8x. All four
channels retained signal with the graph; loop/reload/layout also passed. The
connected-library identity guard was exercised in this successful run.

An earlier extended run failed at 4x/8x, advancing at about 2.44x/2.62x without a
matched baseline. That failure remains recorded and its cause is unresolved. One
bounded matched success establishes mechanism feasibility, not sustained rate
stability or production performance. Rate assertions measure presented frames;
successful property assignments or channel presence alone do not establish a pass.
At non-unit rates native pitch processing modulates phase; channel presence uses
RMS plus dominant frequency. The exact-frequency projection remains the 1x gain
measurement, not an amplitude invariant across pitch-preserved rate changes.

Sanitized observations, the successful 1x mechanism proof, the earlier failed
extended run and the successful matched comparison are retained in
`docs/performance/results/qb-audio-001-webview-groups.json`.

## Native timing observation collector (incomplete acceptance fixture)

```powershell
cargo build --manifest-path recorder/Cargo.toml --example native_nvenc_probe
powershell -NoProfile -ExecutionPolicy Bypass -File tools/audio/probe_process_loopback.ps1 -ControlledTimingProof -TimeoutMilliseconds 30000
```

This opens a generated window for eight seconds and requires its capture target to
belong to an NVIDIA-connected display supported by the existing native recorder.
It does not change graphics settings or select an alternate capture backend.
The window child renders visual markers and 697-Hz pulses at nominal seconds
1/3/5; a sibling renders 1301-Hz pulses. The window's process tree is the include
target, so the sibling is outside it. The child publishes its own marker epoch;
the coordinator's gate-release epoch is recorded separately. Wave submission is
not an audio-device presentation timestamp.

The extended native example can write `--qpc-ledger <new-path>` beside its raw
generated-window H.264. Each entry binds an accepted encoded frame ordinal to its
original WGC QPC. Raw H.264 cadence must not be substituted for that ledger or the
canonical first-WGC-anchored CFR grid. Audio remains numeric packet/spectral data;
captured PCM is never saved. `fixture_completed` means collection only, with
`analysis_required: true`, `marker_membership_asserted: false` and timing flags
false. The whole-window tone threshold is not a per-marker assertion. Paired
marker analysis, skew-rejecting coverage/mux checks and device-output latency
evidence remain to be implemented/verified before this can satisfy readiness.

The latest user-requested single retry still selected Intel UHD Graphics and
failed native encoder startup with `NO_ENCODE_DEVICE`. No marker/timing pass exists.
The actual capture target must first be confirmed on an NVIDIA-owned display.
The collector's full successful path and the child-epoch handoff are unverified;
its current compile/activation checks and bounded failure cleanup are separate.

Before selecting the persistence/playback design, a bounded fixture must:

1. Reconcile packet QPC and sample coverage against the existing WGC recording
   epoch and exact video grid. Establish silence, gap, overlap, late-start and
   timestamp-error handling at capture time. Prove early/middle/late alignment
   and reject a deliberately skewed marker. Do not concatenate fixed-rate PCM
   from independently started clients and assume synchronization.
2. Demonstrate the preferred single-pipeline two-group controls in the actual
   WebView2 runtime. If that cannot expose both groups, measure a narrowly scoped
   audio renderer slaved to primary-video RVFC through play/pause, seek, rate,
   clip loop, recovery, layout change and disposal. Assigned equal `currentTime`
   values do not prove audible synchronization.
3. Choose and validate explicit group identity, per-group media coverage and a
   truthful representation of existing combined/silent media. Current schema-v2
   parsing permits at most one audio stream; a second MP4 audio track is not an
   additive change to the current reader. Do not infer League membership from
   stream order or call the system mix “everything else.”
4. Decode exports of distinguishable low-headroom source fixtures to prove the
   selected gains/mutes in one publishable AAC mix, preserving optional music,
   source hashes, exact video ranges and existing preset/atomic-publication gates.
5. Measure the eventual production capture path under the existing capture
   benchmark protocol. This probe's CPU, timings and pass flag are not production
   overhead or negligible-impact evidence.

The source and timing proofs are readiness prerequisites. Keep QB-AUDIO-001 draft
and do not finalize an implementation ExecPlan from activation/spectra alone.

## Primary references

- [Microsoft activation parameters](https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ns-audioclientactivationparams-audioclient_activation_params)
- [Microsoft process-tree membership](https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ns-audioclientactivationparams-audioclient_process_loopback_params)
- [Microsoft application-loopback sample](https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/)
- [Microsoft capture packet/clock API](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer)
- [CLR marshaling behavior](https://learn.microsoft.com/en-us/dotnet/api/system.runtime.interopservices.standardolemarshalobject)
- [OBS application audio capture support](https://obsproject.com/kb/application-audio-capture-guide)
- [OBS process-loopback implementation](https://github.com/obsproject/obs-studio/blob/master/plugins/win-wasapi/win-wasapi.cpp)
