# Core planning and replay profile (2026-10-23)

Nine rotated fresh-process trials passed: three qualified controls, three
profiling-off and three profiling-on. Every trial passed seven first-pause
checks and one Kestrel check, source/input/binary guards, unchanged authored
paths/bytes and empty WAL. All three enabled traces passed 24 exact phase
counts and nine containment equations; all six other logs contain no trace.

This is isolated diagnostic instrumentation. Production source, Core 58,
SQLite 87, requirement status and the 100 ms/50 ms targets are unchanged.

## Workload and provenance

The unchanged `large-first-pause` replay inserts `hold 12f` into a fresh
10,000-Hold Background/Silence package at frame 17,933 and verifies the new
revision through offscreen composition. There is no source media or register
content. Database SHA-256:
`40e5750e454cdc1c2e3aceb556f0fb1b12996cdc5780518ee03227a12787515c`.

The separate source archive is based on
`90850b096d87d3e8142b76cda4d0c95591df73d3`, with qualified implementation
`ca40995b64fc4acdc0a5e046660a36a1faa7cee5`. Eight existing files and a new
temporary Core module contain timing hooks. All other Rust files, CLI and six
qualified inputs remain byte-identical. The final map has 2,051 Rust files.
The module has only numeric thread-local measurements and clocks. The app
owns the environment gate, labels and diagnostic output; dependency direction
is preserved. The preceding Store-profile overlay was not reused.

The instrumented release app SHA-256 is
`d6cfd57bbb4b5a622353e16e31bb3bc45b6d48021c5ccbbdff8746aeb13524ac`.
Control SHA-256 is
`c36b894466eb4f16fe976924b05c02ada87b0e8872f7f9a7fc6028a60a342037`.
Hardware is Apple M5 Max, macOS 26.5.2. Pre-run one-minute load ranges from
1.7451 to 2.9741, below the unchanged gate of 3. File caches, power and thermal
state are uncontrolled.

## Fresh first-pause measurements

Three samples per condition, diagnostic only:

| Condition | Input to commit | Input to offscreen composition |
|---|---:|---:|
| Qualified control | 257.323792 ms | 259.902375 ms |
| Profile build, off | 256.846709 ms | 259.529792 ms |
| Profile build, on | 255.832083 ms | 258.317250 ms |

These establish no calibrated instrumentation-overhead bound, percentile,
production improvement or target PASS. Do not pool them with the earlier
[five-pair production comparison](compact-json-size-2026-10-23.md) or
[Store detail profile](store-preparation-detail-profile-2026-10-23.md).

## Core work

Profile-on medians below are inclusive where indicated. Repeated-call rows
sum the disjoint invocations within each trace, then take the three-run median.

| Phase | Median | Calls per trace |
|---|---:|---:|
| App semantic call | 103.196292 ms | 1 |
| Core semantic total | 103.181250 ms | 1 |
| Semantic entry/setup | 13.082292 ms | 1 |
| Semantic execution | 84.772000 ms | 1 |
| Semantic leaf apply call | 49.150125 ms | 1 |
| Semantic after-leaf size/charge | 4.971500 ms | 1 |
| Semantic outer finish | 4.865500 ms | 1 |
| Core replay | 75.157458 ms | 1 |
| Replay size checks/charges | 13.194459 ms | 4 |
| Replay leaf apply call | 48.381125 ms | 1 |
| Replay host visitor | 5.030959 ms | 1 |
| Replay aggregate net call | 7.800334 ms | 1 |
| Shared leaf apply total | 96.856333 ms | 2 |
| Leaf preflight | 0.005541 ms | 2 |
| Leaf reduction | 64.732208 ms | 2 |
| Leaf postprocessing | 17.760625 ms | 2 |
| Leaf net calls | 14.363041 ms | 2 |
| Shared net total | 22.131668 ms | 3 |
| Net before-duration and patch | 10.021166 ms | 3 |
| Net validation | 7.899125 ms | 3 |
| Net finish/inverse construction | 3.930126 ms | 3 |

Semantic execution minus its leaf call and after-leaf size/charge has a
30.641416 ms median residual. It includes resolution, provider/allocation
work, pre-leaf accounting, context refresh and timer overhead. Leaf reduction
and postprocessing likewise contain multiple operations. These intervals
identify where to investigate; they do not identify an individual expensive
helper or authorize removing validation, accounting or historical checks.

Totals must not be added to their descendants. The parser also checks two
unions: semantic/replay leaf calls contain the sum of both leaf applications;
the two leaf net calls plus final replay aggregate contain all three net
transactions. Medians are calculated separately, so arithmetic on rounded
table medians is not the residual calculation. All raw phases, counts and
per-trace residual distributions are retained.

## Verification and limits

Only native user-lane Macro dispatch enables `DEADPAN_CORE_WORK_PROFILE=1`.
Output follows shared publication and outer timer drop, with exactly one
successful-refresh marker; otherwise the buffer is discarded. Output uses
constant labels and numeric values. Diagnostic I/O failures cannot change the
completed save. An independent static review found no behavior, evaluation
order, dependency or timing-boundary defect.

Each enabled trace has one exact 26-line block. Its 24 ordered rows require
ordinary counts of one, leaf counts of two, net counts of three and four replay
size checks. The refresh marker has zero duration. The parser accepts its
valid synthetic trace and rejects all 17 malformed cases, including wrong
multi-call counts and negative union containment. The packager reparses raw
stderr and recomputes all summaries from the trials.

Every native replay retains two layout/retry warnings and three scope notes.
No assertion failed. Queue wait, physical presentation, source decoding,
PCM performance, peak memory and calibrated timer overhead remain outside
this workload. No model, audio-device or source-video performance follows.

The first packager passed trial validation but failed on an optional outline
that Root incorrectly made mandatory. Its script/error and untouched partial
staging are retained. Removing that nonexistent payload allowed a fresh
packaging run to finish; no build, source or runtime result changed.

## Evidence

The independently verified
[package](../../tools/media-qualification/evidence/2026-10-23-core-planning-detail-profile/)
contains 152 payload files /3,434,107 bytes. Inventory SHA-256:
`e18d6c4cd8fe64665af5e21cf0fecb5a90a4411adf214fe80ed85c4197a7fc78`.
Every path, byte count, hash and compressed stream verifies. Exact baseline
and formatted sources are retained as `.rs.txt`, with both patches, source
and executable identities, parser tests, reviews, scripts, build receipts,
all nine replay reports, traces and before/after authored-file maps.
Executables, source archive, full source/target trees and project databases
are excluded.
