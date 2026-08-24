# QB-PERF-005 M5: native CFR and MP4 evidence

## Scope and authority

This is provisional PC-B evidence for the audited QB-PERF-005 M5 design. It
does not modify or supersede the untouched PC-A repository, does not complete
QB-PERF-002 reference validation, and does not claim the missing pinned r5
FFmpeg binaries are present.

## Implemented contract

- Rational first-frame-QPC 60-Hz scheduling without accumulated rounding.
- Explicit source-discard and duplicate-tick accounting.
- One reusable same-device BGRA source snapshot per WGC size generation.
- GPU-only snapshot copy, video-processor conversion and direct four-slot
  NVENC submission; no full-frame host readback or software conversion.
- Two-phase resize handoff: duplicate the last valid GPU snapshot while WGC
  recreates, then drain/reconfigure when the first new-size frame arrives.
- FFmpeg receives only native Annex-B H.264 through stdin and performs video
  stream copy, current AAC audio handling and fragmented-MP4 muxing.
- Bounded mux progress/stderr readers and finite FFmpeg shutdown.

## PC-B live validation

All runs targeted visible non-League fixtures on the NVIDIA-owned display. No
League of Legends test was run.

| Run | Result |
| --- | --- |
| Static 10 s | 600/600 ticks and frames; zero scheduler/ring drops |
| Animated 10 s | 600/600; 1,751,591 bytes; max in flight 4; zero drops/errors |
| Animated + two resizes | 600/600; 1,975,256 bytes; max in flight 3; zero drops/errors |

For the animated run, FFprobe reported H.264 High, 1920x1080, `yuv420p`,
60/1 FPS and 600 decoded video frames. Video and AAC audio both started at
0.000000 and ended at 10.000000 seconds. A full FFmpeg decode passed, and all
600 decoded frame hashes were distinct for the changing fixture.

The two-resize run recorded three real pool recreations (the initial WGC size
settling plus two commanded resizes) and exactly four source-snapshot
allocations: one initial allocation plus one per recreation. All 600 CFR ticks
were preserved across both resize gaps.

Evidence is intentionally ignored by Git and remains under:

- `evidence/m5-native-mp4-static-r3-20260817-141913/`
- `evidence/m5-native-mp4-animated-20260817-142057/`
- `evidence/m5-native-mp4-animated-resize-r2-20260817-142416/`

## Failures found and corrected

The first live run exposed a WGC/QPC anchor assumption: the compositor frame
timestamp was 15.1036 ms ahead of the immediately sampled local QPC. The clock
conversion now supports a bounded 100-ms future compositor skew and rejects
larger implausible offsets. Unit tests cover past, accepted-future and rejected
future anchors.

The first resize run lost two scheduled ticks because the converter discarded
its old source snapshot before the recreated pool delivered a new frame. The
two-phase handoff now retains the old GPU snapshot until the first new-size
surface is ready. The repeated live resize run preserved all 600 ticks.

## Toolchain caveat

The live mux/decode checks used the separately installed stock FFmpeg 9.0 only
for provisional M5 validation. It is not the missing pinned QueueBack r5
runtime and cannot satisfy the authoritative runtime-identity gate.

## Static verification

Formatting, all-target compilation, 30 focused native tests and all-target
Clippy with warnings denied pass. The full Rust suite passes 78 of 79 tests.
The sole failure is the reviewed R11 poller test, whose ignored empirical
fixture is absent and represented locally only by `{}`; M5 does not fabricate
League sample data to make that unrelated test green.
