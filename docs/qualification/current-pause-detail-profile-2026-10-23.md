# Detailed native first-pause costs, 2026-10-23

Pure semantic leaf application takes a median 43.467459 ms, remaining semantic
planning 33.948583 ms, and history encoding/readback 22.490084 ms in this private
diagnostic. Capability snapshot construction takes 0.004750 ms; it is not a
useful optimization target. The three explicitly instrumented document-size
categories together take a same-trial median 38.098416 ms.

These measurements identify existing work, not an optimization or a product
target pass. The full goal remains active at an estimated 89%, eight Complete
and sixteen Partial.

## Baseline and scope

This refines the two dominant regions from the
[coarse route study](current-pause-route-profile-2026-10-23.md). Main source
baseline is f319cfd83d772425c0bb333b4a28f04a21b2163e; qualified code remains
5fdae748ac2fdc67e1a11793c5ab69f6b61da833.

Qualified control App SHA-256:
0cb2931562108a68b57915c6135f448bc74293ba20623c7484053152566d31ff.
Fine diagnostic App SHA-256:
90ba5b33b06a5a27213942c6b636ea18c4df5f266df3149aa7621dba282faaf1.

The fresh private copy/target has nine changed paths relative to the immutable
2,054-file coarse source. All 2,045 nonowned Rust files and six pinned inputs
match. Relative to qualified main, ten existing files differ and one temporary
helper is added. Main contains none of these hooks; Core 58 / SQLite 87 stay
unchanged. Fine source-map SHA-256:
e58e62f56f0859cb184e54c56ac0def09b6ec9315b72838feafe1f9ae6c76351.

The host is the same Apple M5 Max MacBook Pro /128 GB /macOS 26.5.2 with
Rust/Cargo 1.97.1. Hardware metadata is whitelisted, stderr omitted, battery ID
redacted and hostname omitted. Power/thermal observations precede the build;
per-trial load/process observations do not prove every system condition.

## Timing and verification contract

The native gate/lifecycle, current-result checks and publication boundaries
are unchanged from the coarse study. Native Macro Apply(InsertPause) alone can
activate the helper. It retains bounded scalar state, no authored values or
identifiers; inactive hooks read no clock. It measures dispatch through
reconciliation/store_update/wake, excluding input queue delay and later UI/GPU/
physical display work. Formatting/output occur after deactivation.

V2 has exactly 28 lines: BEGIN, 26 ordered PHASE rows, END. The original thirteen
count-one phases now retain exclusive residual time while their children run.
The thirteen added counts are 4/1/1/4/1/1/1/1/3/0/1/1/1. The checkpoint row must
have count zero and elapsed zero. Every enabled trial satisfies these counts,
all three success markers and exact checked sum conservation. Maximum encoded
trace size is 2,774 bytes with expected counts, or 3,268 with arbitrary u64
counts; the 4,096-byte buffer remains sufficient.

Eight tests compile from the exact frozen std-only helper source with opt-level
2, debug assertions off, overflow checks on and unsafe forbidden. All pass:
compile 0.237 seconds, run 0.004 seconds. This avoids compiling the entire Core
unit-test binary only to filter eight scalar tests; it is not a claim that the
full Core suite ran again. Twelve release Store transaction tests, the actual
release App, strict release Core/Store/App all-target Clippy and formatting pass.
Twenty parser tests pass, including a corrected negative case that now reaches
the actual sum-overflow check. Source, parser, drivers and final packager have
independent no-findings reviews.

## Fresh-process trials

The unchanged replay uses hold 12f at frame 17933 in a pristine 10,000-Hold
Background/Silence fixture. It checks the original history baseline, exact
[5,12,7] split, suffix preservation, selected Hold and new same-ticket offscreen
picture. It changes a private portable copy only. All authored input paths and
bytes stay unchanged; the two root SQLite sidecars are excluded with WAL empty.

Three rotated control/off/on triplets pass 63 workflow and nine Kestrel checks,
all eighteen make/app commands, source/input/binary guards and one exact V2
block per enabled invocation. Eighteen layout/retry warnings remain. No native
assertion fails in this batch. No source decoding or device audio is involved.

| Lane, n=3 | Commit median ms | Observed commit range ms | Offscreen composition median ms |
| --- | ---: | ---: | ---: |
| control | 194.051000 | 193.846833 to 197.704500 | 196.676916 |
| off | 194.373417 | 194.333125 to 194.496959 | 196.972459 |
| on | 194.426958 | 193.591708 to 194.493375 | 196.922250 |

The overlapping observations neither resolve profiler overhead nor establish an
overhead bound. These are diagnostic independent invocations, not p95 results
or 100 ms/50 ms target passes. Do not pool them with the coarse or earlier
optimization batches.

## Direct exclusive categories

The semantic_plan and store_prepare rows below are residuals after their new
child intervals, so their individual values must not be compared with the
earlier unpartitioned rows. No inclusive parents are added to their children.

| Category | Minimum ms | Median ms | Maximum ms |
| --- | ---: | ---: | ---: |
| native_admission | 5.542000 | 5.656750 | 5.699042 |
| semantic_plan | 33.815543 | 33.948583 | 34.052751 |
| native_continuation | 7.238584 | 7.289750 | 7.353126 |
| store_entry_misc | 0.019082 | 0.020124 | 0.021250 |
| store_prepare | 3.218081 | 3.232750 | 3.233125 |
| host_runtime_bank | 0.001083 | 0.001083 | 0.001125 |
| store_write | 7.222791 | 7.231833 | 7.249667 |
| sqlite_commit | 5.395542 | 6.503167 | 7.123667 |
| store_cache_publication | 0.001000 | 0.001042 | 0.001167 |
| native_postcommit | 0.301833 | 0.313583 | 0.319958 |
| workspace_refresh | 4.105167 | 4.120500 | 4.136667 |
| native_finish | 0.959041 | 0.972084 | 1.029125 |
| native_publication | 3.918917 | 4.033375 | 4.173625 |
| semantic_document_wire | 13.365749 | 13.410083 | 13.628957 |
| semantic_leaf_apply | 43.331083 | 43.467459 | 43.792500 |
| capability_snapshot | 0.004209 | 0.004750 | 0.004791 |
| replay_document_wire | 12.998792 | 13.060417 | 13.150293 |
| reuse_eligibility_eq | 0.172166 | 0.172833 | 0.187833 |
| reuse_clone_exit | 0.000666 | 0.001000 | 0.001125 |
| replay_net_aggregate | 7.569875 | 7.575750 | 7.911750 |
| replay_misc | 0.719376 | 0.722166 | 0.730041 |
| store_compact_size | 11.470667 | 11.608125 | 11.627959 |
| store_checkpoint_wire | 0.000000 | 0.000000 | 0.000000 |
| store_visitor_authority | 0.192167 | 0.205167 | 0.211750 |
| history_codec_readback | 22.309250 | 22.490084 | 22.509709 |
| history_inverse_restore | 1.850209 | 1.871167 | 1.890375 |

The three worker totals are 188.299917, 187.244834 and 188.133125 ms, median
188.133125 ms. Each run conserves exactly; separate column medians need not sum
to the median total.

The retained [disjoint derivation](../../tools/media-qualification/evidence/2026-10-23-current-pause-detail-profile-verification/derived-disjoint-totals.json)
regroups directly measured rows within each same trial: original semantic
region median 90.830916 ms, Store preparation 60.955167 ms, and the three named
size regions 38.098416 ms. It never subtracts overlapping parents or old runs.

The 4+4+3 counts identify only the instrumented document-size sites, not every
serialization. Their regions include existing charges/limit checks where
present. History codec/readback includes real serialization, decoding and
equality work; its cost is not pure JSON encoding. Pure leaf application and
semantic residual work still need more precise current-code inspection.
Eligibility/full-value matching and reuse clone/exit each execute once in these
targets. These capability-snapshot/reuse-exit costs do not justify optimizing those
calls or prove any memory result.

## Evidence and corrections

The [primary inventory](../../tools/media-qualification/evidence/2026-10-23-current-pause-detail-profile/inventory.json)
contains 195 payload files /4,749,341 bytes, 197 files including inventory and
summary. Inventory SHA-256:
492b02231d9f450427d2e6be3cf22f168d1207cf389090a921c4462dd9b4cf34.

Independent verification rereads every size/hash, decodes 25 gzip streams,
compares sixteen decoded payload hashes with their live source files, and
checks nine safe root-relative authored-fixture archives with 27 regular
members. Every WAL is empty. Shared memory and the empty root WAL are omitted
from archives; the authored member bytes equal the before/after maps.
The [separate verifier supplement](../../tools/media-qualification/evidence/2026-10-23-current-pause-detail-profile-verification/files.json)
retains the passing verifier and reproducible disjoint-sum derivation.

The initial parser overflow test could reject an earlier illegal checkpoint
duration. It now sets that duration to zero and requires the precise overflow
error; original test/log/receipt and review correction remain. Static packager
review corrected an unproduced freeze.py path, standalone artifact provenance
and final-review/provenance bindings before execution. The actual source
freeze was an inline Root command and is documented as such. One handoff hash
suffix was mistyped; actual bytes were independently hashed before review/GO.
These are static evidence corrections, not product/runtime test failures.
Packager and final independent byte verifier both pass on their first execution.

## Continuation

Current-code inspection identifies validation, two scope AnchorIndexes,
occupied timing-ID traversal, real provider-plan construction, refresh, Split,
diff and result checks within those regions. Their individual costs are still
unknown; exact-address validation proofs do not transfer to staged clones.
The next action is one bounded nine-child microprofile of those existing calls,
not a proof-reuse API or VM bypass. Preserve validation, authority/refusal order,
original byte/work charges and exact result/history semantics. Low capability
snapshot cost rules out treating that construction as the main bottleneck.
Keep the qualified 0cb29315 App as control; never ship these temporary hooks.
No optimization, source/PCM/display/memory qualification, requirement or gate
closes from this study.
