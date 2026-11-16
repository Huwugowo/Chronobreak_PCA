# Production mux resource interference

Feature: `QB-PERF-006`. Measured on Windows on 2026-11-10.

## Decision

No production optimization is justified by this investigation. The stream-copy
mux has low CPU cost, bounded private memory and handles, and no observed output
rewrite amplification. The PCM/AAC path accounts for most of the measured CPU
delta. There are isolated video-pipe stalls, including five steady calls above a
60-FPS frame interval in the first high-rate PCM run. They do not establish a
periodic or growing bottleneck or a demonstrated 5–10× correction.

The production mux arguments, buffering, scheduling, audio transport, finalizer
and recovery behavior remain unchanged. The additions are an opt-in fixture,
measurement tools and evidence. There is no before/after improvement claim.

## Workload and coverage

`recorder/examples/mux_resource_probe.rs` drives the actual
`NativeMuxPlan::h264`, `NativeMuxProcess` and buffered video writer in a release
build with `replay-time-fixture`. Each recording is 120 seconds, 7,200 H.264 High
1920×1080 access units, paced against absolute 60-Hz deadlines. A four-second,
240-packet prerecorded Annex-B source repeats without re-encoding. The two
source-generation targets are nominal 12 and 18 Mbps; retained source sizes and
hashes describe the actual byte workloads. Generation and output validation are
outside the resource measurement interval. No League, WGC or NVENC is required.

PCM mode replaces WASAPI endpoint acquisition with precomputed stereo f32
48-kHz tone packets at 10-ms cadence. It uses production packet admission,
copies/pool, audio assembly, credit/lead bounds, named pipe, pump, stop and join.
The ordinary mux encodes AAC at its unchanged 192-kbps setting. Audio summaries
retain the production default `requested_source: system`; acquisition in these
runs is synthetic, as established by the command mode and fixture source.
Video mode uses the actual no-audio production plan.

The recorder measurements include the video feed, writer/flow accounting,
progress and stderr reader threads, audio feeding/transport, and production mux
telemetry sampled at 4 Hz. The wider service watcher, League discovery,
WGC/NVENC telemetry and logging sink are not instantiated. Code inspection of
the session's 4-Hz evidence publication and the service's 2-second watcher /
10-second progress logging found bounded snapshots, not a growing media scan.
This is a mux/transport measurement, not a whole-recorder gameplay benchmark.

The source bytes and fixed trace arrays are resident before measurement, so the
recorder's private memory includes fixture overhead. At 120 seconds, the frame
and progress arrays occupy 245,760 bytes; the prerecorded source occupies about
6.4 or 9.7 MB. Production ownership includes the 1-MiB video buffer and bounded
audio packet pool. These costs must not be interpreted as a leak.

The measured sources contain 6,413,575 and 9,734,383 bytes respectively
(12.827 and 19.469 Mbps over four seconds). All admitted recordings use packaged runtime
`queueback-ffmpeg-8.1.2-windows-x86_64-r6`, resolved through its normal lock check,
and the same release probe executable. The retained machine has 16 logical CPUs
and Windows 10 build 19045. Detailed identities, commands and results are in
`evidence/qb-perf-006-mux-20261110/results.json`.

## Measurement definitions

`tools/native_backend/measure_mux.py` observes recorder and child separately at
0.5-second intervals using `GetProcessTimes`, `QueryProcessCycleTime`,
`GetProcessMemoryInfo`, `GetProcessHandleCount` and `GetProcessIoCounters`.
Every video `write_all` call and its deadline lateness is timed in memory.
Production telemetry observation is separately timed. Percentiles use linear
interpolation over sorted calls (type 7).

CPU is milliseconds of accumulated process CPU per wall second; 1,000 ms/s is
one fully occupied logical CPU. Windows CPU-time ticks are coarse (15.625 ms
observed), particularly for short recorder bursts. Recorder values below
1 ms/s are not precise estimates of total CPU work and must not be read as zero.
Cycle rates are retained as additional relative evidence; they are not converted
to time using an assumed clock frequency.

Startup `[0,6)` is reported separately. PCM preserves the production common
audio/video epoch and startup catch-up, including approximately 340–350 ms of
preparation. Moving video zero after bootstrap changed the fixture's lead/queue
behavior and failed audio-health checks, so that experiment is excluded.
Writer windows use the original epoch; resource windows use receipt of `ACTIVE`,
after mux construction. Their phase offset is bounded by the reported
preparation time, rather than being exactly aligned. Steady results are
`[6,120)`, with additional early `[6,40)`, middle `[40,80)` and late `[80,120)`
windows. Each steady latency distribution contains 6,840 calls.

Process I/O counters include pipes. During steady capture the recorder reads
only child progress/stderr and writes the encoded-video/PCM input pipes.
Estimated logical MP4 writes are child write bytes minus recorder read bytes.
This residual is reconciled against observed file-length growth. The input pipe
write/read counters also reconcile. These are logical process/file I/O counts,
not physical storage-device activity, cache writeback or durability measurements.
An open-file length sample is not the final size; terminal metadata supplies the
final byte count after drain.

## Results

The five admitted arms below use exactly the same executable and runtime. They
are controls and repeats of unchanged production behavior, not an optimization
comparison. CPU is ms/s; memory is peak private MiB during the steady window.

| Run | FFmpeg CPU | Recorder CPU¹ | FFmpeg / recorder memory | Write P95 / P99 / max ms | Writes >16.67 ms | Final MB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| PCM 12, repeat 1 | 225.03 | 0.28 | 18.26 / 11.41 | 0.764 / 1.144 / 7.855 | 0 / 6,840 | 194.644 |
| PCM 12, repeat 2 | 237.27 | 0.55 | 18.02 / 11.42 | 0.804 / 1.182 / 8.915 | 0 / 6,840 | 194.644 |
| Video 18, control | 7.72 | 0.83 | 17.04 / 12.46 | 0.895 / 1.136 / 1.712 | 0 / 6,840 | 292.084 |
| PCM 18, repeat 1 | 230.05 | 0.83 | 20.12 / 14.62 | 0.861 / 1.251 / 30.906 | 5 / 6,840 | 294.269 |
| PCM 18, repeat 2 | 215.78 | 1.10 | 20.16 / 14.59 | 0.890 / 1.220 / 15.047 | 0 / 6,840 | 294.269 |

¹ Coarse CPU-time counters undercount short bursts. The corresponding recorder
cycle rates are 26.37, 26.31, 11.16, 30.71 and 27.94 million cycles/s. FFmpeg
cycle rates are 531.15, 550.49, 28.60, 548.73 and 525.51 million cycles/s.
The PCM FFmpeg measurement is about 0.22–0.24 of one logical CPU, not 22–24%
of this entire 16-logical-CPU machine.

There is no consistent worsening with duration or file size. The following
early / middle / late triplets show the full window results rather than fitting
a growth trend to a small sample.

| Run | FFmpeg CPU ms/s | FFmpeg private peak MiB | Writer P99 ms |
| --- | --- | --- | --- |
| PCM 12, repeat 1 | 206.4 / 232.6 / 233.2 | 18.26 / 18.16 / 18.16 | 1.147 / 1.153 / 1.139 |
| PCM 12, repeat 2 | 239.8 / 246.4 / 225.1 | 18.02 / 17.92 / 17.92 | 1.151 / 1.172 / 1.204 |
| Video 18 | 9.3 / 8.3 / 4.8 | 17.04 / 16.96 / 16.96 | 1.182 / 1.122 / 1.123 |
| PCM 18, repeat 1 | 229.1 / 237.9 / 223.6 | 20.12 / 20.07 / 20.07 | 1.365 / 1.275 / 1.214 |
| PCM 18, repeat 2 | 231.9 / 244.3 / 173.4 | 20.16 / 20.07 / 20.07 | 1.227 / 1.233 / 1.169 |

Recorder private memory ends slightly below its steady starting value in every
arm. FFmpeg handles are constant at 154 with PCM or 134 without audio. Recorder
handles stay at 128–129 with PCM or 107–108 without audio. No ownership growth
was observed. PCM maximum lead remains within its 24,000-frame bound; all four
PCM arms account for exactly 5,760,000 valid, non-silent written sample frames
with no loss/fill/late/error counters and no tail overhang.

Estimated logical MP4 writes are 1.6318 / 1.6316 MB/s for the two low-rate arms,
2.4042 MB/s for video 18, and 2.4661 / 2.4652 MB/s for PCM 18. In every startup,
steady and third window, the estimated file bytes equal observed file growth
exactly, and recorder input writes equal child input reads exactly. There is no
observed logical write amplification or extra recorder file writing during
capture. Progress/stderr traffic is approximately 784–790 bytes/s in steady
state. Fragment output is naturally bursty; counters are averaged over each
window, not evidence of smooth physical storage traffic.

Mux telemetry observation P99 is 0.0021–0.0138 ms at 4 Hz. Video-only drain is
11.1 ms, and PCM drains are 43.0–86.1 ms. The per-frame feed and supervision
show no growing workload. Inspection confirms live telemetry reads atomic
progress counters; filesystem metadata is obtained at termination rather than
repeatedly rescanning the growing MP4. Stderr has a bounded 64-line tail and
bounded input lines. No buffering or scheduling change is supported by these
measurements.

The first high-rate PCM run's five steady over-budget writes occur at 14.035,
28.884, 31.784, 91.434 and 92.884 seconds. Their spacing does not establish a
periodic stall. The repeat has no over-budget steady call, but a 15.047-ms
maximum, so the tail is not dismissed. Steady feed lateness P99 remains
1.150–1.219 ms across the five arms; the first high-rate run has a 14.408-ms
maximum after its longest write. Startup maxima of 27.0–33.8 ms and PCM
preparation/catch-up are reported separately, not hidden in steady percentiles.
This observation does not justify a speculative queue, larger buffer or deferred
write. These counters cannot distinguish an isolated scheduler pause from
encoder/mux backpressure; there is no function-level stall attribution claim.

Two read-only 10-second thread snapshots in the matched 18-Mbps controls show
three FFmpeg threads without audio versus 25 with PCM. In the PCM snapshot one
thread accounts for 239.1 ms/s and 547.1 million cycles/s; the many additional
idle threads do not explain the cost. Windows supplied no thread descriptions,
so these samples do not identify a specific function. An additional unpaced
120-second similar-tone AAC-to-null diagnostic consumes 13.016 seconds user CPU
plus 0.578 seconds kernel CPU (11.970 seconds wall time). It establishes that
ordinary AAC encoding is substantial work. It is not a matched production
benchmark and does not attribute the entire paced audio delta. The packaged
build uses normal release optimization and NASM; inspection found no disabled
SIMD/debug-build defect or audio-affecting local FFmpeg patch. No redundant
audio operation or quality-preserving 5–10× correction has been demonstrated.

## Correctness and limits

Every admitted output must pass the existing native finalizer, including its
independent fragmented-MP4 `trun` CFR grid check, packaged ffprobe decoded frame
count and packaged FFmpeg full single-thread decode with `-xerror`. PCM evidence
additionally requires captured AAC, no audio error/discard/late/fill/idle counters,
exact valid/non-silent sample accounting and bounded lead/tail. There is no
relaxed validation or change to production error handling.

All five admitted outputs passed those checks: exactly 7,200 decoded video
frames, zero-start exact 60-FPS grid, expected stream presence, successful full
decode with empty error logs, terminal progress and zero reader errors. All
four PCM audio-health checks passed. Because persistence behavior is unchanged,
there is no new recovery behavior to qualify and no destructive recovery test
against user data.

Generated output stays under `build/perf/qb-perf-006`, and the probe refuses
replacement of output/trace files. No real user recording is test data. This
bounded experiment establishes behavior through 120 seconds and roughly
194–294 MB; it is not proof of hour-long leak freedom or performance on another
machine or storage device. Capture acquisition and whole-service costs remain
outside its measured scope.

Earlier fixture-development artifacts are not admitted evidence. `smoke-*`
includes short setup checks and failures from a future epoch conflicting with
the production bootstrap deadline or restricted directory pinning.
`baseline-video-01` used an invalid finalizer suffix. `paced-pcm-12-01` decoded
but failed audio-health accounting after an experimental epoch rebase, which
was reverted. `pcm-12-01` predates cycle collection, and `video-12-02` uses an
earlier probe binary. Their results are not mixed into the matched table. None
of those fixture failures was treated as a product defect or bypassed by
weakening a production invariant.

## Verification

- Release example build passed before the five matched runs. Retained probe
  SHA-256: `f321b5931b3afd501a1eb4677b88001081c84cde76f25ec978e39b90142f7306`.
- Recorder all-target/all-feature check passed.
- Recorder all-target/all-feature tests passed: 299 tests across the library,
  binary and examples; 17 hardware/explicit integration tests remained ignored.
- Recorder formatting check and strict all-target/all-feature Clippy passed.
  Clippy initially found a collapsible `if` in the new example. Its final
  let-chain cleanup is semantically identical to the measured branch and does
  not invalidate the retained workload results.
- Python tool syntax, canonical schema/lifecycle/graph invariants and whitespace
  checks are recorded with the completed feature evidence.

## Reproduction

From the repository root, with the locked runtime staged:

```powershell
cargo build --release --manifest-path recorder/Cargo.toml --features replay-time-fixture --example mux_resource_probe
python tools/native_backend/measure_mux.py --name new-pcm-12-01 --seconds 120 --mode pcm --bitrate 12
python tools/native_backend/measure_mux.py --name new-pcm-12-02 --seconds 120 --mode pcm --bitrate 12
python tools/native_backend/measure_mux.py --name new-video-18-01 --seconds 120 --mode video --bitrate 18
python tools/native_backend/measure_mux.py --name new-pcm-18-01 --seconds 120 --mode pcm --bitrate 18
python tools/native_backend/measure_mux.py --name new-pcm-18-02 --seconds 120 --mode pcm --bitrate 18
```

Run arms sequentially and do not compile or decode another arm during capture.
Use unique names. The runner performs media validation and analysis after each
capture. Normal Windows filesystem access is needed by the existing finalizer's
directory pinning. Raw generated media, per-frame traces, process samples and
decode logs remain under the ignored benchmark root; compact durable results
are retained alongside this report.
