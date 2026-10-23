# Store preparation detail profile (2026-10-23)

An isolated release profile passed nine rotated fresh-process trials: three
qualified controls, three profiling-off and three profiling-on. Every trial
passed seven first-pause checks and one Kestrel check, preserved authored input
paths and bytes, left an empty WAL, and passed source, input and binary guards.
The three enabled traces each passed the strict 33-phase count/order parser
and ten containment equations. The six disabled/control logs contain no trace.

This adds diagnostic evidence only. No production source, Core 58 or SQLite 87
changed, and no requirement, gate or latency target closes.

## Workload and identities

The existing `large-first-pause` replay uses a fresh `perf make-large --beats
10000` package: 10,000 Background/Silence Holds, 24 fps, no source media and an
empty register bank. It inserts `hold 12f` into the settled edit and checks the
new revision through offscreen GPU composition. The input database SHA-256 is
`40e5750e454cdc1c2e3aceb556f0fb1b12996cdc5780518ee03227a12787515c`.

The private source archive is based on
`a0ff0555e6614752086ddfea030f0b664e78cb7a`, containing the qualified size-count
implementation `ca40995b64fc4acdc0a5e046660a36a1faa7cee5`. Four app/store
files and one new temporary store module contain the instrumentation. Core,
CLI and every other Rust file remain byte-identical; the final map contains
2,051 Rust files. All six qualified input hashes remained unchanged.

The instrumented app SHA-256 is
`396b661014e334d78fe186a249d28dee56bcfeacdb24597dbaaa4c4e3089209e`.
The qualified control is
`c36b894466eb4f16fe976924b05c02ada87b0e8872f7f9a7fc6028a60a342037`.
Both run on this Apple M5 Max with macOS 26.5.2. Pre-run one-minute load ranged
from 1.7212 to 2.5767, below the unchanged gate of 3. OS file caches, thermal
state and power state were uncontrolled.

## Fresh first-pause measurements

Each condition has three samples. These medians are diagnostic; they establish
neither percentiles nor a calibrated instrumentation overhead bound.

| Condition | Input to commit | Input to offscreen composition |
|---|---:|---:|
| Qualified control | 256.183958 ms | 258.869500 ms |
| Profile build, off | 255.854584 ms | 258.387042 ms |
| Profile build, on | 256.061084 ms | 258.544042 ms |

These runs do not replace the separate five-pair
[production qualification](compact-json-size-2026-10-23.md) or establish another
production improvement. The 100 ms fallback and 50 ms ordinary-edit targets
remain open.

## Preparation costs

Profile-on medians, in milliseconds:

| Phase | Median | Relationship |
|---|---:|---|
| App service | 249.611917 | Inclusive outer interval |
| Semantic planning | 103.020292 | Disjoint app child |
| Store prepared commit | 123.673833 | Nested inclusive interval |
| Store command preparation | 110.396833 | Disjoint store child |
| Compound execution | 82.987125 | Nested preparation interval |
| Core replay | 76.287833 | Includes host visitor |
| Core host visitor | 5.053292 | Nested in replay |
| Host after-state size | 4.801875 | Visitor child |
| Entry size | 1.986208 | Execution child |
| Final size guard | 4.712584 | Execution child |
| Final validation | 2.931250 | Preparation child |
| History command plan | 24.085750 | Nested preparation interval |
| Stored edit serialization/bound | 3.129333 | Command-plan child |
| Stored edit historical decode/equality | 19.017083 | Command-plan child |
| Inverse restoration check | 1.852958 | Command-plan child |
| Write plan or bank | 7.349459 | Disjoint store child |
| Durable SQL commit | 6.370958 | Disjoint store child |
| Service refresh | 4.141958 | Disjoint app child |
| Shared publication | 3.990042 | Disjoint service child |

Totals overlap their descendants and must not be summed. The median of each
individual replay-minus-visitor residual is 71.226624 ms; this includes Core
work, callback dispatch and instrumentation overhead. It does not attribute
that time to any individual Core operation. The separate semantic-planning
interval also needs internal attribution before choosing an algorithmic change.
The 33 raw phases and all ten residual distributions are retained.

## Controls and limitations

The gate is `DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE=1`. Only the user-lane
Macro dispatch enables the fixed-size thread-local buffer. Store calls do not
self-enable. Trace output occurs after shared publication and the outer timer
have finished. Failed or refresh-failed paths discard their buffered trace.
Output contains constant labels and numeric measurements, without document
content, IDs or paths. An independent static review found no behavior or
timing-boundary defects.

Each enabled release log contains exactly one 35-line block. All ordinary
phase counts are one; the refresh marker has zero duration, and debug-only
verification has zero count and duration. The parser's valid synthetic trace
passes and all 15 malformed-trace cases reject. Parsing alone does not qualify
the workload. Packaging reparses the captured logs and independently recomputes
the reported medians from the raw trials.

Every replay retains two layout warnings: two frames requiring a second retry,
and a run of three retrying frames with distinct causes. It also retains three
scope-exclusion notes. No failed assertion or skipped unit test is concealed.
Physical presentation, source decoding, PCM performance, queue wait, peak
memory and calibrated timer overhead remain outside this measurement. The
empty-bank callback is not representative of copied-content preparation.

## Evidence

The independently re-read
[evidence package](../../tools/media-qualification/evidence/2026-10-23-store-preparation-detail-profile/)
contains 141 payload files totaling 2,997,706 bytes. Inventory SHA-256:
`e5ec3de077a87bd85be45ee772bba3521fe34ae8485a4cdcb4fc92b0837904e4`.
All paths, byte counts, hashes and compressed streams verify. The package
includes exact baseline/formatted source snapshots as `.rs.txt`, both patches,
source/input/binary identities, scripts, review, parser tests, build receipts,
all nine reports and before/after maps, raw traces and derived summaries.
Executables, Cargo targets, the full source archive and fixture databases are
excluded. Root corrected the draft packager's zero-count expectation and source
snapshot labeling before execution; the final packager completed successfully.
