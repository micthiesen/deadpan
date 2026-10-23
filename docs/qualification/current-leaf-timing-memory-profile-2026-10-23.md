# Current leaf timing and owned-memory diagnostic (2026-10-23)

The current native first-pause route spends about 44 ms in each of its two
ordinary Core leaf applications. Eighteen fresh native trials pass: nine
unobserved timing trials and nine separate owned-memory trials. This supports
testing narrowly scoped replay reuse; it does not establish a cache saving,
calibrated instrumentation overhead, percentile, latency target or memory
ceiling. Production code is unchanged by this diagnostic.

## Source and execution identity

The fresh private source is commit
`3a32c923358c0cb7ca53f821bfb6db3e01ebb466`, including the qualified semantic
refresh change. All 2,051 baseline Rust files and six inputs equal the frozen
production source. A temporary six-file overlay changes five existing files
and adds `leaf_profile.rs`; all 2,046 nonowned Rust files remain identical.
No earlier instrumented source or Cargo cache is reused. Core 58 /SQLite 87
remain unchanged.

The app alone enables numeric recording for an exact
`DEADPAN_CURRENT_LEAF_PROFILE=1` native Macro request. Core has four fixed TLS
slots, checked duration/count accumulation and lazy clock acquisition. It
records no content or identifiers. Guard lifetimes preserve the original
fallible calls, authority, charges, mutations and error branches. Output is
best effort on the service thread after successful refresh, shared publication
and the outer timer's drop. Gate-disabled runs retain branch/TLS overhead,
without activating a buffer, reading a clock or emitting a block.

Independent source review reports no findings. Formatting, a locked fresh-target
release build, ten strict parser tests, five observer lifecycle smokes and
eight receipt-mismatch checks pass. The app build takes 274.310758 seconds;
that is compilation time, not edit latency. The three helper executables are
verified copies from the qualified production build, not freshly built helpers.

| Executed binary | SHA-256 |
|---|---|
| Qualified control app | `a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f` |
| Private diagnostic app | `14ef183594c33011c7557c8affd0bf516d24a55a9994d42a8d0648deb01e6ede` |
| Owned-memory observer | `d1166aef7e1b30c787bb39fa536c18aee9af9da4f2327120d321432205fc0ed2` |

Hardware is Apple M5 Max, macOS 26.5.2, with a 1280×820 offscreen viewport.
Power and thermal observations are retained; no controlled thermal experiment
is claimed. The fresh Cargo compiler cache identifies Rust 1.97.1, matching
the pinned source-directory toolchain. All report `rust` fields instead show
Rust 1.99.1: they invoke `rustc --version` from the launch directory, which
resolves the default toolchain. This runtime field is not compiler provenance.
The untouched reports and a separately inventoried
[post-build toolchain witness](../../tools/media-qualification/evidence/2026-10-23-current-leaf-timing-memory-toolchain/)
retain both facts.

## Timing lane

Three triplets rotate control/off/on, off/on/control and on/control/off.
Each uses a fresh process and fresh 10,000-Hold Background/Silence package.
The unchanged native first-pause replay passes seven workflow checks and one
Kestrel check per trial. Source/input/app/helper/Kestrel guards pass before
and after execution. Authored paths and bytes remain unchanged; only the exact
root SQLite sidecars are excluded and WAL must be empty. Each run enters under
one-minute load 3 with no conflicting Cargo, rustc or Deadpan process.
All 18 reports retain two layout/retry warnings each, 36 total, with no
error-severity findings. Their full causes remain in the raw reports.

| Median, n=3 per kind | Control | Private off | Private on |
|---|---:|---:|---:|
| Input to commit | 236.850291 ms | 240.814792 ms | 239.753209 ms |
| Input to offscreen composition | 239.352541 ms | 243.290584 ms | 242.288042 ms |

The private executable differs from control even with recording disabled.
These nine samples do not calibrate timer/TLS/output overhead or demonstrate
zero overhead. They are separate from the prior five-pair production batch.

All three enabled traces pass the exact six-line/four-row schema, with every
count exactly one and the refresh marker's duration zero. Control/off traces
contain no profile block.

| Enabled phase | Median |
|---|---:|
| Inclusive native service route | 233.354042 ms |
| Semantic leaf `apply_with_result`, including request argument setup | 43.780792 ms |
| Replay leaf `apply_with_result` | 43.559500 ms |
| Other service work, cleanup and overhead | 146.001626 ms |

The two leaf calls are disjoint and inside the outer span. The residual is
computed separately for each trial before summarizing, not by subtracting
overlapping values or medians. It includes other service work and cannot be
attributed to a particular algorithm. Leaf spans include full entry,
reduction, net/result work; they are not reducer-only measurements. The outer
span excludes queue wait, caller receipt processing, UI consumption, later GPU
work and physical scanout. No old Core24 subphase cost is reused as a current
measurement.

## Separate memory lane

Another three rotated triplets use the same fresh workload through an external
observer. It launches the app directly through the existing guarded process
adapter, retains sole Child ownership, samples only that unreaped child, then
confirms group teardown and reaps once. No runtime adapter changes or arbitrary
PID sampling occur. Every trial has a valid terminal receipt, successful app
exit, confirmed cleanup and recomputable raw samples.

There are 16,303 samples, 1,790–1,855 per run. The requested interval is 5 ms;
actual maximum start gaps range 7.526958–7.818667 ms. Observation windows range
13.417–13.884 seconds. Both raw clocks/query times and their receipt aggregates
are retained and independently recomputed.

| Median of each run's observed maximum, n=3 | Control | Private off | Private on |
|---|---:|---:|---:|
| Physical footprint | 699.798088 MiB | 658.688622 MiB | 696.032417 MiB |
| Resident size | 669.953125 MiB | 615.031250 MiB | 652.062500 MiB |

These are sampled lower bounds over the main app's whole replay lifetime.
They include startup, opening, other app threads and later rendering; brief
peaks, workers and external GPU/service memory can be missed. They do not
measure pause-only or per-leaf allocation, retained cache cost, an exact peak
or a ceiling. Variability is not a demonstrated memory saving. Observer-lane
timings remain separately labeled and are never pooled into the timing lane.

## Retained failures and limits

The first standalone observer link fails because Apple's LLVM reader cannot
consume the release libraries' LLVM bitcode. The same source links with the
workspace's ThinLTO setting; the original command, error and source remain.
The initial smoke helper contains a literal escaped newline and fails to
compile before any observer launch. A correction guard first expects four
literals where there are three; it writes nothing, and the following
missing-script invocation launches nothing. A distinct corrected smoke driver
retains the originals and passes all five cases.

The smoke cases cover natural exit, pre-spawn output setup failure, missing
executable, timeout and injected sample refusal. Timeout/refusal use separate
one-substitution fault binaries, not the measurement observer. Group-failure/
leader-fallback and reap-error paths have source review only; no real libproc
permission refusal is claimed. Synchronous libproc calls are cooperative, not
preemptively bounded. Failed cleanup would invalidate a trial and remain
explicit; no such failure occurs in the measured lane.

The replay qualifies Background presentation and durable first-pause
publication. Source decoding, PCM performance, physical display, listening,
lower hardware tiers and full product acceptance are outside this diagnostic.
No requirement or gate closes.

## Evidence and next experiment

The independently re-read
[package](../../tools/media-qualification/evidence/2026-10-23-current-leaf-timing-memory-profile/)
contains 376 payload files /4,211,716 bytes. Inventory SHA-256:
`f2a703fcad7c4710a752de883afe4d938ea99f01134eae7f6d0a859cdc8c1613`.
Every path, byte count, hash and compressed stream verifies. It retains exact
reviewed/formatted `.rs.txt` sources, patches, reviews, source/input/binary
maps, all build and failure records, raw trial reports, clocks, samples,
profile/parser outputs and fixture guards. Executables, targets, source
archives and generated packages are excluded. All 54 measurement commands
pass; packaging succeeds on its first execution after correcting draft schema
assumptions against actual terminal records.

The toolchain supplement contains two payloads /7,497 bytes, inventory SHA-256
`74346bd582f1b55b830b64522a8a2c1b44da463e64ef9d2345282ac4cb9a0078`.
It was captured after terminal trials and leaves the original package intact.

A private opaque one-leaf replay-reuse experiment is now authorized. It must
bind exact immutable entry/request/result values, restrict eligibility and
execution modes, preserve TLS entry/exit effects, and retain every independent
Core/Store authority and cumulative charge. Its correctness, equality/retention
cost, actual speed and memory effect remain unverified. No cache API ships
from this diagnostic.
