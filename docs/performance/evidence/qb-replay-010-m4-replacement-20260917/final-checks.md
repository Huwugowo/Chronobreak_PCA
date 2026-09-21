# Final M4 replacement checks

Recorded 2026-09-17 after the 50-run replacement and canonical updates.

| Check | Result |
| --- | --- |
| `python build/qb010-m4/validate_canonical.py` | Passed: 60 roadmap items, 8 plan/checkpoint pairs |
| `git diff --check` | Passed; only existing Git LF-to-CRLF working-copy warnings |
| Python syntax/import check for replacement retainers and `tools/replay_benchmark/qb010_manual.py` | Passed |
| Replacement receipts/provenance | 50 attempted, 50 valid, 0 invalid; 10/10 matrices passed |
| Required event/request loss | 0 in both arms |
| Source/fixture preservation | Frozen subject identities and all 409 prepared-file hashes match |
| Normal verification | Frontend 107 tests, Rust 86 (one opt-in ignored), replay Python 91 (one opt-in skipped), fmt, Clippy, TypeScript/Vite, normal and benchmark desktop builds passed; receipts in `verification.json` |
| Engineering disposition | Passed: working-set, handle, read/request and cancellation findings have explicit evidence-backed classifications and no production change is warranted; see `engineering-disposition.md` |
| Partial-body child diagnostic | Passed: deterministic 206 partial-body probe reproduced packaged ffprobe exit code 0 and server-side client close/reset; receipt in `cancellation-probe.json` |
| Manual acceptance (completed 2026-09-21) | User passed Games/local failure/retry/playback; save, clip deletion, A/B/A root switching, export submission and cleanup during verified optional work; and final source/export playback. `hold-export2.json` has a verified packaged child and the explicit in-hold export submission; `audit-export-overlap-verified.json` validates four H.264/AAC/full-decode exports. `audit-final-verified.json` has the final one saved A game, retained clips and unchanged B. |
| Manual helper verification (2026-09-21) | Four focused helper tests passed after extending the disposable overlap window to 30 seconds; replay Python 95 ran with one existing opt-in skipped; real standalone packaged-child hold/resume smoke, py_compile/help and git diff --check passed. No production/subject/fixture changes or new benchmark campaign. |

The wrong-filename reference parity invocation is retained in `verification.json`
as an invocation error; the corrected parity command passed 2/2. GPU counters were
unavailable and are not reported as zero. No recorder/media-runtime rerun was needed
for this app-only unit. Focused `clip_duration` lifecycle tests passed, including the
packaged protected-route probe; campaign request telemetry does not retain per-child
exit status, so that limitation remains explicit in the cancellation disposition.
