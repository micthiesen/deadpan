# One-leaf replay result reuse qualification (2026-10-23)

Prepared semantic commits now borrow one Core-proved leaf result instead of
applying that same leaf again. Five final alternating pairs measure first-pause
offscreen composition medians of 243.137083 /197.389583 ms, a 45.747500 ms
/about 18.82% diagnostic difference. The cached and uncached real-media paths
publish equivalent edits, exact canonical PCM and durable history, and both
emitted files verify. No percentile, latency target, memory ceiling,
requirement or gate closes.

## Source, boundary and execution identity

Implementation commit `5fdae748ac2fdc67e1a11793c5ab69f6b61da833` is based on
qualified `3a32c923358c0cb7ca53f821bfb6db3e01ebb466`. The retained final map has
2,053 Rust paths against a 2,051-path base: seventeen changed paths, including
two new files. Six pinned inputs are unchanged; Core 58 /SQLite 87 are unchanged.
No format or migration work accompanies this change.

The original fourteen-path private overlay remains the implementation basis.
Thirteen paths are byte-identical. The fourteenth moves an unchanged
7,421-byte private test module to EOF, preserving production text, item order,
documentation and assertions. The three additional paths are the two-file
positive-media test and one existing Store test-call signature correction.
Seven independent source reviews pin these changes.

`OneLeafReplay` owns private immutable entry/request/next values. It has no
public constructor, Clone, serializer, deserializer or mutator; Debug is
redacted. A plan retains at most one capability, with no history, register,
worker or queue persistence. This bounds cardinality and lifetime, not heap
bytes or peak memory.

Core owns its original successful application and seals the result only after
the unchanged post-leaf serialization and charge succeed. Eligibility requires
one ordinary InsertTime Edit, no inputs/writes/isolation, local and compact
modes at creation and use, and no live derived clocks or provisional timings.
Lookup follows the original prefix revision/transaction/ledger admission,
pre-leaf size traversal and charge. It compares complete entry and leaf request
values, then retains revision admission and EXACT_EDIT/sealing entry/exit
behavior. Post-size/charge, ledger observation, host visitor, final net result
and history work still execute. Other shapes and mismatches fall back.

The Host prepared path passes a borrowed capability into the existing shared
Store Immediate transaction. Current source/media, analysis/correction,
register/allocation, relevance and publication authority remains independent.
The bounded host preparation callback runs before publication. Old public
APIs and history replay supply None. The reused pure leaf validation/application
does not run a second time; this is not a claim that every check reruns.

| Executed release app | SHA-256 |
|---|---|
| Qualified control | `a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f` |
| Final candidate | `0cb2931562108a68b57915c6135f448bc74293ba20623c7484053152566d31ff` |

The compiler and report metadata identify pinned Rust 1.97.1. Hardware is
Apple M5 Max, macOS 26.5.2. Media/transcription/tracking helpers are verified
qualified copies, not freshly built helpers. Exact executed test/helper
identities, source maps, commands and results are retained.

## Checks and the test-only source amendment

| Run | Passed | Matching binaries | Skipped |
|---|---:|---:|---:|
| Core | 1,543 | 71 | 0 |
| Broader Store/CLI/audio/analysis/app selection | 3,513 | 110 | 741 |
| UI | 1,296 | 2 | 11 |

Strict Core/Store/CLI all-target Clippy and formatting pass, as do strict
UI all-target Clippy and formatting. The initial CLI-wide test-context
typecheck also passes. These totals are per run; overlapping app tests are
not counted as distinct tests across runs.

Core/broader/UI and UI lint ran before one subsequent test-helper adjustment.
All production source and the other 2,052 Rust files are byte-identical.
The original test maps and executed binary hashes are preserved rather than
rewritten. The amended reader passes strict checks and repeated debug/release
neutral, aligned and positive-media controls afterward.

The new plain-Original fixture reaches the legitimate authored-bus fast path,
whose stage tag is `edge_faded_pcm_before_voice_effects` when the plan has no
independent sounds, beat sounds or authored treatments. The one-leaf reader
requires that exact tag and proves all three predicates on the actual opened
session. The legacy wrapper still requires exactly
`authored_bus_pcm_before_mastering`. No allowlist, added effect, changed PCM
API or weakened stage assertion substitutes for these contracts. All prior
62 media assertions, PCM/gain/cold comparisons and the Some-capability
assertion remain, with three additional same-session plan assertions.

The 24 new Core/Store cases include exact value/mode/TLS binding, real 64 MiB
wire boundaries and genuine Store admission/correction/history refutations.
The 512 MiB cumulative refusal is unreachable for the eligible one-leaf
Compound under four capped 64 MiB traversals; no limit was lowered.
The additional positive release case passes in 6.378 seconds.

The final broader run contains one nextest LEAK annotation for the native mark
address test. Its exact phase/binary/test passes once without LEAK in isolation
using the same retained executable. Two earlier warning cases, Core retained
copy ownership and Store typed references, also pass clean isolated checks on
the final binaries. Those are not reexecutions of the older executable hashes;
both old and executed hashes are recorded. Original annotations remain in
history. No memory-leak cause is inferred.

## Actual media and durable history

| Emitted file case | Verified pictures | Complete final samples | Offset |
|---|---:|---:|---:|
| Debug neutral group/Ungroup | 15 | 24,024 | 0 |
| Release neutral group/Ungroup | 15 | 24,024 | 0 |
| Release aligned sound scope | 19 | 30,430 | 0 |
| Release one-leaf cached | 45 | 72,072 | 0 |
| Release one-leaf uncached | 45 | 72,072 | 0 |

The final aligned run passes in 232.393 seconds; its slow notices are retained.
The earlier passing aligned run took 235.908 seconds and is separately retained.

The positive case uses qualified cfr-bframes Original [12,42), with a fifteen-
frame silent freeze at Edit frame 15. It asserts a genuine capability, uses
Host prepared commit versus the same request with explicit None, and requires
both preparation callbacks exactly once. Transactions, documents, banks,
complete authored/limited PCM and every limiter-gain bit match through
different partitions, cold seams, durable Undo/Redo and reopened full history
replay without a receipt shortcut. Nonzero source audio and an exactly silent
pause interior keep the comparison meaningful.

Every output picture has a literal oracle: before frame 15, Original 12+f;
frames [15,30), Original 26; after frame 30, Original f-3. Both parity exports
verify every picture and all [0,72072) samples, with zero picture failures,
nonzero offsets, uncovered samples or low-SNR blocks and empty audio flags.
Compressed bytes and decoded AAC bits are not asserted identical.

All five actual movies, full decoder/numerical controls and five fixture
archives are retained. Two release control portable copies contain nine
regular files each; validate and doctor leave their paths and bytes unchanged.
The parity fixtures separately retain their package snapshots and full
history evidence. No additional owner-portable validation is claimed for those
snapshots.

## Native workflow and timing

Six release replay reports pass 1,340 workflow and six Kestrel checks. Their
twelve layout/retry warnings remain. Ten final timing reports add seventy
passing pause checks and ten Kestrel checks, with twenty further layout/retry
warnings. No error-severity finding is hidden.

Each timing trial uses a fresh app process and fresh 10,000-Hold
Background/Silence package. Five pairs alternate control/candidate order,
with quiet-load, source/input/executable, fixture and empty-WAL guards.
Only the exact root SQLite sidecars are excluded from authored input equality.
OS cache state is uncontrolled; the first pause follows a settled Background.

| Final n=5 medians | Control | Candidate |
|---|---:|---:|
| Input to commit | 240.560333 ms | 195.108542 ms |
| Input to offscreen Background composition | 243.137083 ms | 197.389583 ms |

The composition difference is 45.747500 ms /18.815517%. This batch is separate
from the [private prototype](one-leaf-replay-reuse-prototype-2026-10-23.md)
and its 18.44% result. These are workload-specific diagnostic medians, not p95,
physical display latency, source decode performance or a 100 ms/50 ms target
pass. No production peak/retention memory measurement accompanies this change.
The earlier private observer results remain sampled whole-app lower bounds,
not exact/per-leaf peaks, retention, a ceiling or proven savings.

## Retained failures and evidence

Five actual qualification failures are preserved:
an existing Store test omitted the new optional argument; the first export
test was placed in shared recipes used by other CLI test roots; strict Clippy
refused the test module before production items; reused UI extraction retained
an old supplemental CLI helper; and the new PCM fixture initially used the
processed-stage reader assumption. Corrections preserve assertions and
production authority. The stale helper refusal precedes test execution; fresh
extraction resumes UI without repeating completed source-identical checks.
Static producer escaping, validation and patch corrections are recorded
separately and are not presented as runtime failures.

All eight qualification stages and 26 post commands pass. The independently
reviewed packager verifies actual reports, source amendments, binary hashes,
every required rerun and raw timing summaries before writing. A separate Root
reread verifies all 995 files /23,149,781 bytes, payload sizes/SHA-256 values,
178 gzip streams, five emitted movies and five safe fixture archives.
Retained Rust has text suffixes and introduces no compile inputs.

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-23-one-leaf-replay-reuse/inventory.json)
SHA-256 is
`6e81c229c306b3ef9163e974ee025cdd083826f38b24238fa3a382d02ec806e9`.
The package excludes executable bytes, build/source archives and raw project
directories. It retains failed histories, reviews and actual emitted evidence.

No independent DSP, listening, physical-display, general format corpus,
model-generation or release qualification follows from these checks.
The full requirements and latency targets remain open.
