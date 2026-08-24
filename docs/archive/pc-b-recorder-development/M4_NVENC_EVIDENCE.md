# QB-PERF-005 M4: direct NVENC evidence

## Scope and authority

This is provisional PC-B evidence for the audited QB-PERF-005 M4 design. It
does not modify or supersede the untouched PC-A repository, and it does not
complete the blocked QB-PERF-002 reference validation.

## Implemented contract

- H.264 High profile at 1920x1080 and 60 FPS.
- NVENC P4/HQ, VBR 12 Mbit/s average, 18 Mbit/s maximum and 24 Mbit VBV.
- GOP/IDR period 120, no B-frames, no lookahead and single-pass encoding.
- BT.709 limited-range NV12 input and matching H.264 video signal metadata.
- Four D3D11 NV12 textures registered once with NVENC.
- Four output bitstream buffers and exactly four registered completion events.
- Non-blocking submission through a capacity-four channel.
- Ordered completion thread that waits, copies bitstreams, unmaps resources and
  releases slots without issuing D3D11 immediate-context commands.
- Explicit atomic `free -> converted -> submitted -> free` slot ownership.
- Bounded two-entry WGC input-view cache. A new texture identity replaces an
  old cached view instead of growing the cache or terminating capture.

The NVENC ABI constants, structure sizes, versions and function-list offsets
are pinned to `nv-codec-headers` 12.2.72.0 and checked against the included C
layout probes under `reference/nv-codec-headers-n12.2.72.0/`.

## PC-B live validation

All runs targeted a visible non-League fixture on the NVIDIA-owned display.
No League of Legends test was run.

| Run | Result |
| --- | --- |
| Static Notepad | 12/12 submissions completed; valid SPS/PPS/IDR Annex-B stream |
| Animated 10 s | 600/600; 1,716,962 bytes; max in flight 2; zero slot drops/errors |
| Two resizes | 598/598; max in flight 2; zero errors |
| Target close | 168/168; prompt bounded exit; zero errors |
| Minimize/restore | 420/420; max in flight 2; zero errors |
| Cache-fix 30 s | 1,801/1,801; zero slot drops/errors |
| Stability 240 s, r2 | 14,338/14,338; 42,584,210 bytes; max in flight 2; zero slot drops/errors |

The 240-second output was independently counted as 14,338 decoded frames by
FFprobe and fully decoded by FFmpeg. It reported H.264 High, 1920x1080,
`yuv420p`, limited range and BT.709 colour metadata.

Evidence is intentionally ignored by Git and remains under:

- `evidence/m4-nvenc-steady-20260817-123055/`
- `evidence/m4-nvenc-animated-20260817-124718/`
- `evidence/m4-nvenc-resize-20260817-125052/`
- `evidence/m4-nvenc-target-close-20260817-125124/`
- `evidence/m4-nvenc-minimize-restore-20260817-125151/`
- `evidence/m4-nvenc-cache-replacement-30s-20260817-125412/`
- `evidence/m4-nvenc-stability-240s-r2-20260817-125512/`

## Failure found and corrected

The first long run stopped after about ten seconds when WGC exposed a third
historical texture identity. The converter had incorrectly treated that as a
pool-capacity violation. WGC's current pool depth can remain two while texture
identities change over time. The fix keeps exactly two cached input views and
round-robin replaces an entry when a new identity appears. A regression unit
test verifies that the cache never exceeds two entries.

The failed run remains available as
`evidence/m4-nvenc-stability-240s-20260817-125231/` and is not counted as a
pass.

## Toolchain caveat

The live stream validation used locally installed FFmpeg 9 only as an
independent decoder/prober. It is not the missing pinned QueueBack r5 runtime
and cannot satisfy the authoritative QB-PERF-002 or M5 mux-runtime identity
gate.

## Static verification

The M4 worktree passes formatting, all-target compilation, focused native
converter/encoder tests and all-target Clippy with warnings denied. The
repository-wide undocumented-unsafe lint still reports only 28 pre-existing
findings in `src/platform/windows.rs`; M4 adds none.
