# Live Client acquisition CPU over a simulated hour

The byte-identical event response fast path cut median match CPU from **5,742.2 to 1,835.9 ms**, a **3.13× speedup** (68.0% less CPU). Event decode, processing, queue charging and normalization together fell from 9,280.4 to 753.4 million measured thread cycles (12.32×). The measurements use the same request schedule and response bytes.

## Match workload

The existing paused-clock scheduler drives one synthetic 60-minute match. The fixture grows from a GameStart event to 324 cumulative rows, with champion kills, FirstBlood, turrets, dragons and Baron kills. One older event is corrected and later reverts. PlayerList uses the existing anonymized ten-player Swiftplay benchmark roster with changing CS. GameStats advances the calibration clock. ActivePlayer admission closes at 30.5 simulated seconds.

Only the transport and writer scheduling are simulated. The fixture returns bounded API bodies to the real scheduler. The driver uses production queue charging and batching limits with virtual deadlines, then production normalization, checksummed framing, file sync, checkpoint projection and atomic sidecar replacement on a temporary directory. It omits HTTP/TLS, the blocking worker channel, and game capture.

The release executables use Rust 1.97.1 on Windows, with baseline product code from `2cead14` and identical profiling instrumentation in both builds. After compilation, two runs of each build were interleaved in the order baseline, optimized, optimized, baseline. The tables use the median of two results per build, including the median of each run’s percentiles. Windows process CPU is user plus kernel time and has coarse increments over a simulated minute. Thread cycles are measured separately and do not estimate elapsed time. Allocation bytes sum the sizes requested by successful allocation and reallocation calls. Fixture setup and journal readback are outside the timed window.

## CPU by simulated minute

Each row covers the work during that minute. The last column sums the four Events processing stages listed above and excludes the fixture body copy.

| Minute | Events by end of minute | Process CPU ms before → after | Events processing Mcycles before → after |
| ---: | ---: | ---: | ---: |
| 5 | 23 | 0.0 → 7.8 | 9.4 → 1.1 |
| 15 | 77 | 7.8 → 7.8 | 34.2 → 2.9 |
| 30 | 159 | 93.8 → 15.6 | 139.3 → 7.6 |
| 45 | 241 | 140.6 → 39.1 | 204.3 → 22.4 |
| 60 | 324 | 156.2 → 31.2 | 308.5 → 28.3 |

Total process CPU in the two baseline runs was 5,375.0 and 6,109.4 ms. Optimized runs used 1,890.6 and 1,781.2 ms. Total allocation calls fell from 16,583,667 to 6,712,246. Cumulative allocated bytes fell from 2,268,705,968 to 810,508,523.


These short runs show host variation. Adjacent baseline/candidate pairs improved
by 2.84× and 3.43×. The median ratio describes this fixture series and does
not establish a statistical bound. A reported zero for one minute means the
process CPU counter did not advance during that interval. Stage cycle
measurements show that work still ran. Both executables include the same
profiling bookkeeping in their measured process CPU. Per-stage elapsed
measurements include cycle-counter calls.

## Stage costs

Cycle sums and allocation totals are per simulated match. P95 and P99 are per stage invocation. Mixed-endpoint persistence costs are listed separately.

| Stage | CPU Mcycles before → after | Wall µs P95 / P99 before → after | Allocations, thousands before → after | Allocated MB before → after |
| --- | ---: | ---: | ---: | ---: |
| ActivePlayer/decode | 0.0 → 0.0 | 1.9 / 1.9 → 1.8 / 1.8 | 0.0 → 0.0 | 0.00 → 0.00 |
| ActivePlayer/fixture_body_copy | 0.0 → 0.0 | 1.2 / 1.2 → 0.9 / 0.9 | 0.0 → 0.0 | 0.00 → 0.00 |
| ActivePlayer/process | 0.0 → 0.0 | 4.5 / 4.5 → 5.8 / 5.8 | 0.0 → 0.0 | 0.01 → 0.01 |
| ActivePlayer/queue_charge | 0.0 → 0.0 | 1.8 / 1.8 → 0.6 / 0.6 | 0.0 → 0.0 | 0.00 → 0.00 |
| Events/decode | 4,986.9 → 409.9 | 1,476.5 / 1,648.8 → 245.6 / 1,193.5 | 7,961.3 → 732.2 | 834.69 → 76.77 |
| Events/fixture_body_copy | 202.4 → 94.8 | 80.8 / 88.8 → 56.9 / 68.5 | 3.6 → 3.6 | 94.50 → 94.50 |
| Events/normalize | 3,186.0 → 248.8 | 922.4 / 1,010.7 → 144.1 / 854.1 | 746.1 → 69.8 | 562.62 → 52.58 |
| Events/process | 869.8 → 74.4 | 248.3 / 279.6 → 43.9 / 227.7 | 2,138.4 → 219.6 | 219.46 → 25.67 |
| Events/queue_charge | 237.7 → 20.3 | 72.3 / 80.5 → 10.6 / 67.2 | 0.0 → 0.0 | 0.00 → 0.00 |
| GameStats/decode | 106.6 → 46.6 | 30.2 / 84.5 → 16.4 / 22.2 | 32.4 → 32.4 | 2.50 → 2.50 |
| GameStats/fixture_body_copy | 15.2 → 8.3 | 3.8 / 16.9 → 2.6 / 4.7 | 3.6 → 3.6 | 0.34 → 0.34 |
| GameStats/process | 33.3 → 19.7 | 7.6 / 11.1 → 6.0 / 9.1 | 36.0 → 36.0 | 6.11 → 6.11 |
| GameStats/queue_charge | 15.1 → 8.2 | 3.8 / 6.2 → 3.5 / 4.2 | 0.0 → 0.0 | 0.00 → 0.00 |
| PlayerList/decode | 1,668.1 → 1,395.7 | 705.0 / 776.0 → 739.6 / 861.1 | 2,809.8 → 2,809.8 | 273.01 → 273.01 |
| PlayerList/fixture_body_copy | 17.3 → 13.6 | 7.8 / 11.5 → 6.9 / 11.4 | 1.8 → 1.8 | 42.74 → 42.74 |
| PlayerList/normalize | 782.9 → 568.7 | 321.9 / 387.4 → 301.2 / 350.3 | 863.1 → 863.1 | 63.44 → 63.44 |
| PlayerList/process | 173.6 → 65.3 | 73.8 / 87.7 → 35.8 / 49.6 | 111.6 → 111.6 | 6.11 → 6.11 |
| PlayerList/queue_charge | 156.4 → 71.8 | 57.7 / 78.0 → 39.2 / 52.8 | 0.0 → 0.0 | 0.00 → 0.00 |
| checkpoint_and_manifest | 511.8 → 409.3 | 8,726.0 / 9,278.5 → 8,757.9 / 46,774.7 | 2.8 → 2.8 | 4.15 → 4.15 |
| checkpoint_projection | 725.4 → 542.3 | 216.6 / 258.6 → 238.7 / 285.0 | 1,676.2 → 1,676.2 | 111.58 → 111.58 |
| encode_and_checksum | 435.7 → 222.9 | 70.2 / 127.8 → 105.3 / 182.7 | 72.5 → 34.7 | 29.39 → 33.26 |
| initialize | 4.0 → 3.6 | 7,288.2 / 7,288.2 → 6,280.0 / 6,280.0 | 0.1 → 0.1 | 0.00 → 0.00 |
| queue_charge | 0.0 → 0.0 | 3.0 / 3.0 → 2.9 / 2.9 | 0.0 → 0.0 | 0.00 → 0.00 |
| write_and_sync | 3,354.6 → 1,040.0 | 2,027.2 / 2,302.1 → 3,199.3 / 12,982.4 | 0.0 → 0.0 | 0.00 → 0.00 |

Late event polls still include changed responses that take the full decoder and revision path.

| Minute 60 stage | CPU kcycles P95 / P99 before → after | Wall µs P95 / P99 before → after |
| --- | ---: | ---: |
| Events/decode | 3,760.7 / 3,911.7 → 1,769.5 / 3,792.7 | 1,686.3 / 1,754.3 → 770.8 / 1,653.0 |
| Events/normalize | 2,327.0 / 2,414.6 → 1,446.7 / 2,395.6 | 1,012.2 / 1,080.4 → 629.0 / 1,074.2 |

## Retained work

All four runs issued 3,600 Events, 3,603 GameStats, 1,800 PlayerList and four ActivePlayer requests with identical response byte totals and zero skipped or failed requests. Recovery validated 9,021 committed records, including 3,600 event observations and 326 revisions, with zero tail bytes. The digest of recovered event evidence and revisions matched (2c005530fbd92dfa) after excluding the wall-clock receipt string.

Commit counts changed from 4,859 to 2,072. Smaller queue records reach the byte threshold less often, so more records share a commit within the existing deadline. Each commit still writes the journal checksum and syncs the file.

The 10× whole-match target would require CPU below 574.2 ms. The optimized run still spends 2,101.6 million measured cycles on roster decode, processing, queue charging and normalization, plus the durable write path for every endpoint. The exact-repeat optimization addresses the event-history bottleneck while preserving the full path for changed bodies. This experiment did not measure in-game CPU or frame pacing.

Raw benchmark output, including per-stage cycle percentiles for each selected minute and exact request/byte totals, is under `build/perf/live-acquisition/`. The reproduction command is in [the verification guide](../development/VERIFICATION.md#simulated-long-match-live-client-profile).

The four final comparison runs are retained in [results.json](evidence/live-acquisition-20261109/results.json).

Baseline executable SHA-256 is `327c1c40137664fe2dd494f6193cc166d4c57fee1c6f4ffc349d41c97a557470`.
Candidate executable SHA-256 is `65eb89aabd72da31bb65eecb92aebc278ae2e3321ef5396a3ea9c8ea73e4d915`.

## Implementation and verification

The collector retains one admitted Events response body and observation ID.
Every request still fetches the full body. Equal bytes can use the transient
reference only after strict decoding established IDs for every event.
Changed JSON, even formatting-only changes, takes the full decoder and
normalizer. Bodies with unidentified events always take that path because
those observations must be emitted again.
The one cached transport allocation is capped at 4 MiB, including its spare
capacity. Buffers exceeding that allocation bound take the full path.

The writer stages the reference result alongside its pending batch and publishes
it only after commit. It rejects stale, missing or cross-epoch references.
The journal stores ordinary event observations and gaps, so existing stored
files and readers keep the same wire representation. A reference cannot be
serialized directly. An exact repeat cannot add newly resolved actors because
the ordinary tracker already suppresses identified events with equal source
fingerprints regardless of their derived actors.

Shared League-data tests passed, including 19 journal and 15 observation tests
and both existing Criterion smoke cases. The repeat regression checks equality
across commits, capacity gaps, rejected speculative batches, unidentified rows,
source resets and rejection of direct serialization. Recorder all-target,
all-feature checks and tests passed, as did formatting and strict Clippy for
both changed crates. The app backend all-target check passed. Nine recorder
publication tests initially failed under restricted parent-directory access.
The complete rerun with access passed. No live-game or hardware run was needed
for this synthetic CPU comparison.
The focused acquisition test also confirms that a valid body with excessive
spare allocation capacity takes the full path on every response.
