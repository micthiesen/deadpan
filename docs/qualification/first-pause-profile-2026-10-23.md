# First-pause diagnostic profile, 2026-10-23

The 10,000-Hold first-pause delay remains reproducible. Two short, quiet
diagnostic batches place its uninstrumented median at 128.497 and 131.432 ms.
They do not establish a p95 or satisfy a Section 25 target. No production
optimization is included in this record.

## Source and method

The source is immutable commit
`a18ff65443aa8fd1aa5ab63fa33678c9afd88220`, on the same Mac17,7 / M5 Max,
128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and qualified FFmpeg prefix as the
[preceding qualification](neutral-ungroup-2026-10-23.md). All 2,045 Rust source
hashes and four Cargo/toolchain inputs match the archived baseline.

The existing release `perf` example creates each fresh package with
`make-large PACKAGE --beats 10000`, then runs
`edit PACKAGE --cycles 2 --kinds pause`. Each package starts with 10,000
Background/Silence Holds, 10,001 nodes and 120,000 project frames at 24 fps.
The first pause is at frame 17,933, with 8,506 shifted suffix owners; the second
has 1,259. Both insert twelve frames. These positions differ, so the first-minus-
second interval is not a pure measurement of binding creation alone.

Five fresh-package pairs alternate baseline and instrumented executables.
The second batch also alternates which executable runs first. Edit-start load
is 2.366–2.485 in the first controlled batch, and every edit in both batches
starts below load 3. Each edit commits successfully, refreshes the plan, and
the benchmark reopens and compares its head. Expected phase markers occur
exactly once per measured edit, with no fallback/duplicate phase anomaly.

The baseline uses the current checkout's target directory; each archived
instrumented variant uses its own target directory. Executed binaries are
copied and hashed before measurements:

| Executable | SHA-256 |
| --- | --- |
| Baseline | `af725eba81e35a2a005a1b1e6c855684ff1a5e7328c7c093e86c3a5dc6ce6f28` |
| Initial phase timers | `10505d4dae5ecdad2e5bfa7a7c794954df9bd4dbd2f3cfc61a0be3e2475eacc4` |
| Finer Store timers | `054cd772c0902427928306cc29b844e302e75af01a0eb1588a00666157c8ad48` |

Timers use monotonic `Instant` values and diagnostic stderr in disposable
source copies. They preserve every branch, validation, history round-trip,
inverse check and durability operation. Parent timers include diagnostic
overhead and child work; the rows below overlap and must not be summed.
Source and executed-binary guards pass after both controlled batches.

## Results

Total command preparation, commit and workspace refresh, in milliseconds:

| Batch and binary | First pause, min / median / max | Second pause, min / median / max |
| --- | --- | --- |
| Initial controlled, baseline | 128.311 / 128.497 / 130.021 | 31.853 / 32.379 / 32.892 |
| Initial controlled, timers | 128.286 / 129.365 / 139.373 | 33.045 / 33.896 / 35.969 |
| Finer controlled, baseline | 127.786 / 131.432 / 135.256 | 32.277 / 33.669 / 34.724 |
| Finer controlled, timers | 128.990 / 129.840 / 131.238 | 32.231 / 33.008 / 35.779 |

Each row contains five first and five second pauses. The finer timer medians
for the first pause are:

| Phase | ms | Scope |
| --- | ---: | --- |
| Core apply | 55.328 | Includes reducer, setup and final net validation |
| Reducer | 37.333 | Includes capture, Split and insertion work |
| Capture for insertion | 18.793 | Initial physical-owner lattice capture and layout work |
| Shifted-suffix owner loop | 4.019 | 8,506 owners, not all 10,000 Holds |
| Explicit new-binding validation | 3.408 | One validation call, separate from final net validation |
| Net patch and result validation | 6.324 | Exact reversible patch and final validation |
| Store prepare | 89.385 | Includes Core and all following preparation checks |
| Transition admission | 0.282 | Source, generation and transition admission |
| Request wire check | 0.041 | Serialize, bound, decode and equality |
| Edit serialization | 5.037 | 6,416,116 encoded bytes |
| Edit round-trip | 26.656 | Decode, equality and temporary destruction |
| Exact inverse restoration | 1.711 | Existing no-copy restoration check |
| Store write plan | 18.422 | Includes SQLite writes and receipt extension |
| SQLite history write | 14.981 | History row insertion and page-I/O accounting |
| Receipt extension | 2.872 | Hash-chained receipt work |
| SQLite transaction commit | 16.824 | Includes durability sync |
| Workspace refresh | 3.927 | Retained head and RenderPlan compilation |

The second edit's history payload is 144,254 bytes; its edit round-trip median
is 0.641 ms and its history-write median is 0.077 ms. The first payload's audio
patch contains 2,840,426 bytes of timing tables and 3,294,213 bytes of owner
bindings. This identifies wire size as a measured contributor, without proving
that any particular representation change will improve latency.

## Retained failures and next experiment

The first archive copier ignored every directory named `target`, including a
real playback source module. The complete source-hash guard stopped setup
before compilation. The corrected copier excludes a build target only at the
archive root. The first finer-timer patch also stopped before compilation
because an anchor appeared twice; its corrected patch scopes writer anchors
to `write_command_plan`. Both original failures remain retained.

An initial one-edit batch ran above load 3, with load captured before package
creation. It is retained separately and is not the controlled evidence above.
All these runs are short diagnostics, including the quiet ones.

Inspection found 10,001 all-Automatic edge-policy records in the frozen layout.
Omitting just those redundant values would reduce this JSON payload by
1,900,190 bytes, from 6,416,116 to 4,515,926. That is a counterfactual byte count,
not a runtime result.

## Disposable compact-edge prototype results

A disposable Core56/SQLite85 prototype omitted all-Automatic policies from
frozen layout JSON and updated the exact wire-size counter. Five fresh-package
pairs produced first-pause medians (n=5) of 127.513 ms for baseline and
111.362 ms for the prototype. The baseline/prototype document JSON sizes were 15,611,962 /
12,319,659 bytes; SQLite database sizes were 17,404,896 / 13,592,672 bytes.
These are prototype measurements, not production qualification. The first-pause
median remains above the 100 ms Hold-insertion bound; that requirement stays open.

Both measured binaries were uninstrumented and phase parsing was disabled.
Therefore `complete_phase_multiplicity_check: true` in the copied prototype
summary is NOT APPLICABLE; the attribution sidecar records why. The prototype
is disposable, does not change production, and is not production qualification.
Keep strict decoding, bounds, round-trip and inverse checks, full lattice capture
and durable commit semantics.

## Evidence

[Retained records](../../tools/media-qualification/evidence/2026-10-23-first-pause-profile/)
include raw command output, source overlays, full source hash sets, executed
binary identities, load and phase records, package file hashes, setup failures
and scripts. The compact-edge prototype's build receipts, scripts, source
identities, overlay, controlled logs and package command receipts are retained;
its binaries, source tree and SQLite packages are excluded. The attribution
sidecar corrects the prototype summary's phase-multiplicity field. No media
decoding, listening or native input/display claim follows from this headless
Background/Silence workload. DP-24 remains partial.
