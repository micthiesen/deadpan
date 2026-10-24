# Single-leaf net reuse production qualification

Status: the uninstrumented change is promoted to Main after the listed build, media, timing and memory checks. Main Rust sources match the qualified snapshot exactly. The five-pair timing and six-run memory observations are diagnostics, not performance-gate closure.

## Change and scope

The Core change reuses the successful `EditTransaction` produced when applying an eligible `InsertTime` leaf. For ordinary one-leaf replay, it changes only the forward destination revision, inverse origin revision and description. Authored timing allocation IDs keep the leaf revision. Prefix admission, wire checks, work charges, visitor calls, identity observation, Store authority, final document checks, and history verification remain in the path. Release Core skips the redundant final aggregate validation, diff/inverse construction, and changed-ID traversal, relying on successful leaf application and the revision-only change. The debug oracle recomputes the original whole edit and asserts equality. Store independently validates the final document and checks the actual outer edit size, decode/readback equality and inverse. The existing private capability retains its successful edit; public APIs and schemas are unchanged.

## Build and real media

The fresh nine-stage release build passed 726 unfiltered Core unit tests, 35 Core integration tests, 12 Store preparation tests, the release app build, and strict checks. The uninstrumented app SHA-256 is `970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2`.

The release CLI media test passed on the physical Mac with the synthetic worker feature. Cached and uncached exports each verified 45 pictures and 72,072 samples with zero measured offset. Authored PCM, limited PCM, limiter-gain bits, partition/cold reads, history replay and register banks matched. The MP4 files are not claimed byte-identical.

## Diagnostic measurements

Each fresh process uses a 10,000-Hold Background/Silence package and a 1280×820-point offscreen viewport. Source decoding and PCM preparation are outside this timing lane; the real-media check is separate.

Five alternating control/candidate pairs completed 10 timing trials and 20 commands. All 70 workflow checks and 10 Kestrel checks passed; the lane retained 20 warnings.

| Lane | Input-to-commit median | Input-to-background-composition median |
| --- | ---: | ---: |
| Control | 193.445 ms | 195.911 ms |
| Candidate | 188.425 ms | 191.147 ms |

The paired median reduction was 5.356 ms at commit (range 4.062 to 5.768 ms) and 5.468 ms at composition. These five pairs do not establish a percentile or target pass.

The separate owned-observer memory lane completed six gate-off runs, three per build. Median sampled maximum physical footprint was 667.423 MiB for baseline and 665.939 MiB for candidate. Median sampled resident size was 653.922 MiB for baseline and 672.938 MiB for candidate. The candidate's resident median was 19.016 MiB higher. These bounded samples cover whole-app lifetime; they do not isolate leaf state, establish an exact peak or ceiling, or attribute retained memory.

## Identity and limits

The immutable qualified base is commit `9211db7043d14e57e06782342aa7defd1313a8b6`, with 2,053 Rust paths and source-map SHA-256 `ee77ff2f40c620cbf4542f41c877295e982adaec32722796f357331b21b7915e`. The production candidate has 2,053 Rust paths and three changed files: `crates/deadpan-core/src/compound.rs`, `crates/deadpan-core/src/one_leaf_replay.rs`, and `crates/deadpan-core/tests/semantic_leaf_replay.rs`. Candidate source-map SHA-256 is `f973b941a3f71b70cd0826cf6ab3f5ceb68395e5407690bea79f8e5a49e53a53`; six build inputs are hash-pinned separately.

| Executable | SHA-256 |
| --- | --- |
| Control and matched baseline app | `e6abcd3a521697f504901f156df445e41681cfc824ff97793f62f4e0865802e1` |
| Candidate release app | `970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2` |
| Real-media CLI | `840a1618074ff3dc283b6d6c3bcde58cf5e9e49820297b1bbac9d6416c6f2a4b` |
| Real-media test binary | `41a0b8613bd49a5a04666e9245e992fc72db6641bf7a2bb58cc94fa8ae513bcb` |
| Owned memory observer | `d1166aef7e1b30c787bb39fa536c18aee9af9da4f2327120d321432205fc0ed2` |

The three qualified helper hashes and six build-input hashes are retained in the evidence manifest. Rust 1.97.1 ran on Mac17,7 with 18 cores and 128 GiB memory, on AC power.

These checks do not establish the 100 ms fallback or 50 ms ordinary-edit target, p95 performance, a memory limit, physical-display latency, or source-decode/PCM performance. The earlier private prototype measurements are a separate batch and are not pooled with this production run; see the [private prototype record](single-leaf-net-prototype-2026-10-24.md).

The source and analysis reviews report no findings. Whole-edit and wire equality tests use the same capability with different outer revision lengths, preserve authored leaf allocations and compare actual hit/fallback behavior. Real 64 MiB document and 512 MiB cumulative-work refusals retain their errors and visitor counts. No new Store aggregate-edit boundary fixture is claimed; the existing actual edit-size/readback/inverse checks remain unchanged.

The owned-memory observer and its five earlier lifecycle smokes are reused with exact binary/source/library pins. It was not rebuilt here. All six runs have valid sample and cleanup receipts. Actual sample counts range from 1,750 to 1,859; the largest sample-start gap is 7.613375 ms. These observations establish no memory saving or retained-edit cost. The retained forward/inverse state adds live memory, including transient retention through admission for an ineligible retained InsertTime.

## Evidence and promotion

The [primary evidence](../../tools/media-qualification/evidence/2026-10-24-single-leaf-net-reuse/) contains 341 payload files /12,369,663 bytes, excluding its inventory. Inventory SHA-256: e0ca61a60782b5c3bb5f7b5b311a2893d0032137d2a3a758fb5785335cada68b. All producer stages and the first copier execution passed.

The [independent post-copy reread](../../tools/media-qualification/evidence/2026-10-24-single-leaf-net-reuse-verification/) verifies every stored path, hash and size, 320 decoded payloads (46 gzip, 274 identity), 16 safe fixture archives with 48 authored files, all 35 media evidence files and both actual MP4 hashes. Every approved live input and fixture map still matches, including empty WALs. Its first execution passed and the primary was unchanged.

Main's 2,053 Rust sources and six pinned inputs match the tested candidate exactly after the three-file promotion. Formatting and the authored diff check pass. Core 58 /SQLite 87 and public APIs are unchanged. Raw compiler/test logs and the pinned commands reference retain their original EOF whitespace.

Coverage remains 89%, eight Complete and sixteen Partial. No gate, requirement, p95 result, physical-display measurement or 100 ms/50 ms latency target closes. The full development goal remains active.
