# One-leaf replay reuse prototype: timing and memory evidence

All ten timing trials and ten separate observer-memory trials pass. The private prototype reduces the first-pause offscreen composition median by 44.666709 ms, about 18.44%, in five alternating pairs. This is diagnostic evidence for production qualification, not a latency-target pass or production adoption.

## Change and source identity

This private prototype reuses an already-successful pure leaf validation/application result during replay. An opaque borrowed Core capability binds the complete immutable entry and request by value, with matching local/compact modes and no provisional clocks. It applies only to one ordinary InsertTime leaf without inputs, writes or isolation. Core still owns application; the original revision checks, traversal charges, wire limits, ledger, host visitor, final net/history and Store admission remain authoritative. It does not rerun the reused pure leaf checks. It changes 14 Rust paths against baseline commit 3a32c923358c0cb7ca53f821bfb6db3e01ebb466: twelve existing files and two new files, bringing the source map from 2,051 to 2,053 Rust paths. The six qualified inputs are unchanged. Core schema 58 and SQLite schema 87 are unchanged.

The release candidate app SHA-256 is 6d1eaa132978c7c796ae1f56bb6d417bb19627511ce48cdb2e2b48724ccde73b. It is byte-identical to the initial successful release app build. The qualified control app is a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f. Qualified media/transcription/tracking helpers were reused. The owned-process memory observer is d1166aef7e1b30c787bb39fa536c18aee9af9da4f2327120d321432205fc0ed2; the fresh-fixture helper is fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166.

The initial focused build passed all 39 tests and emitted four test-only compiler warnings: one unused Result warning and three unreachable else warnings. Two exact test-only source corrections followed. Independent review found no weakened assertions; production sources, six inputs, and app bytes stayed unchanged. The completed corrected build passed the same 39 tests with zero warnings. A separate initial resume wrapper stopped during preflight because of an existing-target assertion after zero commands; the corrected wrapper removed only that redundant creation. Both histories are retained separately.

## Timing lane

Five alternating control/candidate pairs used fresh 10,000-Hold Background/Silence fixtures and release app processes. Each report contains seven passing pause checks and one passing Kestrel check. The reports record the first pause after the initial Background had settled, with OS file-cache state uncontrolled.

| Median across five runs | Qualified control | Private prototype |
|---|---:|---:|
| Input to commit | 239.682292 ms | 194.917708 ms |
| Input to offscreen Background composition | 242.163542 ms | 197.496833 ms |

These are diagnostic medians for the reported workload. The sample does not establish a percentile, close a latency target, isolate one component's cost, or show performance on a physical display. GPU readback was not in the timing run. The disabled DEADPAN_CURRENT_LEAF_PROFILE marker was absent from every run.

## Separate owned-observer memory lane

A second set of five alternating control/prototype pairs ran the same replay under the owned-child memory observer. This lane is kept separate from the unobserved timing lane.

| Median of each run's sampled whole-app maximum | Qualified control | Private prototype |
|---|---:|---:|
| Physical footprint | 731,940,088 bytes | 686,851,224 bytes |
| Resident size | 723,484,672 bytes | 648,101,888 bytes |

The raw streams retain 18,296 samples in total, with a maximum inter-sample gap of 12.329791 ms. These are sampled lower bounds over the main app's lifetime. Startup and later app work are included, and short peaks can be missed. They are not exact peaks, per-leaf memory, a ceiling, or worker/GPU memory. Observer-lane latency values are preserved per run and are not pooled into the timing comparison.

## Reports, warnings, and retained checks

The twenty reports contain 140 passing pause checks and 20 passing Kestrel checks. Each pause report also has two warning findings: a frame settling layout on a second retry and an egui discard warning. All 40 warnings and their complete report details are retained. They are report warnings, not failed checks.

The corrected release build ran six focused suites: core-refutations, core-boundary-unit, store-atomic, core-reference, core-refresh, and core-trim. Together they passed 39 tests with zero ignored tests. Root's measurement authorization records 24 new checks and no production adoption. The initial test binary copies were verified through their relocation receipts; the package retains the receipts and hashes, not executable bytes.

## Limits and final package status

This is a private prototype result, not a production adoption or product qualification. It makes no source-decode, independent DSP, listening, PCM, physical-display, exact-memory-peak, retention, or formal latency-target claim. The timing comparison does not qualify instrumentation overhead. The original project requirements and latency targets remain open.

The [retained evidence](../../tools/media-qualification/evidence/2026-10-23-one-leaf-replay-reuse-prototype/) contains 394 files and 5,809,852 bytes. Its inventory SHA-256 is `f4989f93e792915eec9470939a7ce9d0a7c987ae653ad6c84c426ba06935479b`. The packager independently recomputes all twenty reports and raw sample summaries, checks source/input/binary identities and both build histories, and verifies every payload after writing. A separate Root reread verifies the complete inventory, byte counts, SHA-256 values and gzip streams. Executables, source archives, build targets, generated databases and fixture packages are excluded. Retained Rust copies have text suffixes.

Next production qualification must include a successful real-media cached commit, exact canonical PCM and durable history against uncached replay, and emitted-file verification. The focused Store tests already cover genuine admission refusal, but do not establish that complete positive path.
