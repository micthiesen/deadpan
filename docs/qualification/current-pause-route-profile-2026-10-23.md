# Current native first-pause route, 2026-10-23

The remaining cost is concentrated in semantic planning and Store preparation:
their exclusive medians are 91.664125 ms and 60.879542 ms in three enabled
trials. The median worker route is 188.710583 ms. This is a private diagnostic,
not a production optimization or a latency-target qualification.

## Source and execution identity

The baseline is main 40959c5e26f5fc9713eda5a02ccd61377cc65a28, whose qualified
implementation is 5fdae748ac2fdc67e1a11793c5ab69f6b61da833. The production
[one-leaf replay qualification](one-leaf-replay-reuse-2026-10-23.md) remains
separate and unchanged.

Control executable SHA-256:
0cb2931562108a68b57915c6135f448bc74293ba20623c7484053152566d31ff.
Private diagnostic executable SHA-256:
1bac97db7cdb7830592fe8f896e6c54f509b998501a3ae047e60746a7decafcb.
Executed-copy hashes, reused qualified helper identities and build receipts are
retained in the [evidence](../../tools/media-qualification/evidence/2026-10-23-current-pause-route-profile/inventory.json).

A fresh source copy and separate Cargo target contain four changed existing
Rust files and one temporary Core module: App service/macros, Store prepared
commit, Core module export and the scalar timing helper. All 2,049 other Rust
files and six pinned inputs match the baseline; final Rust count is 2,054.
Source-map SHA-256:
12acb33168d093a788cba673ba98c532c6729c6f73564ee75d57d7c473e8ab41.
The temporary timers are not in main. Core 58 / SQLite 87 remain unchanged.

The host is an Apple M5 Max MacBook Pro with 128 GB RAM, macOS 26.5.2, using
Rust/Cargo 1.97.1. A public metadata derivative removes hardware stderr and
battery identifiers; the pinned source observation is preserved privately.
Power/thermal observations precede the build, rather than proving conditions
at every trial. Each trial separately records load and competing compiler/app
process checks.

## Measurement contract

The retained production replay inserts hold 12f at Edit frame 17933 in a fresh
10,000-Hold Background/Silence fixture. It operates on a private portable copy
and checks the exact [5,12,7] split, selected inserted Hold, retained suffix,
history baseline, new revision and same-ticket offscreen picture acceptance.
The original input's authored paths and bytes remain identical; only root
project.sqlite-shm and an empty project.sqlite-wal are excluded from that
comparison. No source decoder or device audio is involved.

DEADPAN_PAUSE_ROUTE_PROFILE=1 is read once by the owned native service worker.
Only native Macro Apply(InsertPause) activates it. The bounded numeric TLS
helper reads no clock when inactive and retains no document, request, ID,
path or closure. It pauses parent accounting while child categories run.
Exactly thirteen count-one phases and three success markers are required;
their checked integer sum equals the independently retained outer interval
using the same boundary timestamps. Reentrancy, cancellation, borrow conflict,
overflow, missing markers and stack/count mismatch discard the diagnostic.

The interval begins immediately before worker dispatch and ends after the same
iteration's reconciliation, store_update and wake return. It excludes input
queue delay, UI processing, later GPU work and physical display latency.
Formatting and one bounded best-effort stderr write occur after deactivation.
Clock/helper overhead belongs to adjacent observed categories and is not
subtracted. The parser requires the exact 15-line V1 block and conservation.

## Verification and trials

All eight helper tests and twelve Store transaction tests pass, with no ignored
cases. Strict release Core/Store/App all-target Clippy and formatting pass.
The fresh release Core unit-test binary took 727.56 seconds to compile/run;
this is build cost, not an edit-time observation. Eighteen parser tests pass.
The source and final drivers have independent no-findings reviews.

Three rotations use control/off/on, off/on/control, on/control/off. All nine
fresh-process trials pass seven workflow checks and one Kestrel check each:
63 workflow and nine Kestrel checks total. All eighteen make/app commands
complete successfully. Each enabled run emits one valid thirteen-phase block;
disabled/control runs emit none. Eighteen layout/retry warnings remain in the
reports. There were no native assertion failures in this batch.

| Lane, n=3 | Commit median ms | Observed commit range ms | Offscreen composition median ms |
| --- | ---: | ---: | ---: |
| Qualified control | 195.230667 | 194.115083 to 197.159916 | 197.802083 |
| Diagnostic, gate off | 194.561208 | 194.480916 to 194.859333 | 197.164875 |
| Diagnostic, gate on | 195.100625 | 193.153667 to 195.840084 | 197.472708 |

The overlapping observations do not resolve an overhead difference or establish
an overhead bound. Do not pool these samples with earlier studies or call them
percentile/100 ms/50 ms target passes.

## Direct exclusive worker categories

| Category | Minimum ms | Median ms | Maximum ms |
| --- | ---: | ---: | ---: |
| native_admission | 5.571167 | 5.581625 | 5.760791 |
| semantic_plan | 91.330500 | 91.664125 | 92.212417 |
| native_continuation | 7.159500 | 7.258251 | 7.308250 |
| store_entry_misc | 0.014874 | 0.023208 | 0.024709 |
| store_prepare | 60.446042 | 60.879542 | 61.075666 |
| host_runtime_bank | 0.000791 | 0.000958 | 0.000959 |
| store_write | 6.961375 | 7.279584 | 7.303041 |
| sqlite_commit | 5.242291 | 6.334875 | 6.739459 |
| store_cache_publication | 0.000834 | 0.001042 | 0.001125 |
| native_postcommit | 0.288750 | 0.305541 | 0.307666 |
| workspace_refresh | 4.123334 | 4.141250 | 4.161166 |
| native_finish | 0.977709 | 0.986000 | 1.003209 |
| native_publication | 4.089791 | 4.107083 | 4.133041 |

The three outer intervals are 186.788500, 188.710583 and 189.302458 ms. Each
trial's phase sum conserves exactly; independently selected column medians need
not sum to the median total. Store entry is exclusive miscellaneous time, not
SQLite time. Planning and preparation remain coarse regions, not attributed
serializer, clone or validator costs.

## Retained evidence and corrections

The immutable primary package has 184 payload files /4,143,448 bytes and 186
files including inventory and summary. Inventory SHA-256:
769b2da713cc347640af179acacf7f0785d5f041f6d254511793569fd46ee63b.
Independent rereading verifies every payload size/hash, decodes all 23 gzip
streams and checks nine safe fixture archives containing 27 regular files.
Archived member bytes equal the authored before/after maps; shared memory and
the verified-empty root WAL are intentionally excluded. No executable, raw
project tree, compile-input .rs file or private raw host observation is packed.

The initial source-copy setup omitted twelve pinned historical Rust fixtures
because an evidence exclusion was too broad. Restoring those exact files fixed
the setup before source edits/builds. Static driver/packager reviews corrected
zero-test acceptance, missing identity/failure bindings, producer-schema and
archive-retention mistakes before execution; all earlier drafts/reviews remain.
These are setup/review corrections, not product test failures.

My independent verifier subsequently stopped three times on assumptions about
archive suffixes, outer directory names and sidecars. The corrected verifier
requires exact authored map equality, safe regular member names, empty WAL and
every byte/hash. Its [separate supplement](../../tools/media-qualification/evidence/2026-10-23-current-pause-route-profile-verification/files.json)
retains those attempts, the final verifier and its passing receipt without
changing the primary package inventory. The packager itself completed on its
first execution.

## Continuation

Refine only the two dominant planning/preparation regions with direct disjoint
measurements of the existing work. Historical Store/Core profiles do not
establish their current internal costs. Keep the qualified 0cb29315 executable
as production control; the private diagnostic binary is not a replacement.
No optimization, source-decode/display/PCM/memory claim, requirement completion
or gate closure follows from this study. Coverage remains 89%, eight Complete
and sixteen Partial; the full development goal remains active.
