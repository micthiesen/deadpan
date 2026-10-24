# Single-leaf net reuse production qualification

Status: the uninstrumented production candidate passed the listed build, media, timing, and memory checks. Main promotion is pending. The five-pair timing and six-run memory observations are diagnostics, not performance-gate closure.

## Change and scope

The Core change reuses the successful `EditTransaction` produced when applying an eligible `InsertTime` leaf. For ordinary one-leaf replay, it rebases the transaction's revision headers and description rather than rebuilding the same aggregate net diff and inverse. Prefix admission, wire checks, work charges, visitor calls, identity observation, Store authority, final document checks, and history verification remain in the path. Release Core skips the redundant final aggregate validation, diff/inverse construction, and changed-ID traversal, relying on successful leaf application and the revision-only change. The debug whole-edit oracle and Store independently validate the actual result, codec/readback, and inverse. This does not add a cache, schema, or public API.

## Build and real media

The fresh nine-stage release build passed 726 unfiltered Core unit tests, 35 Core integration tests, 12 Store preparation tests, the release app build, and strict checks. The uninstrumented app SHA-256 is `970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2`.

The release CLI media test passed on the physical Mac with the synthetic worker feature. Cached and uncached exports each verified 45 pictures and 72,072 samples with zero measured offset. Canonical PCM, authored limited-gain bits, partition/cold reads, history replay, and register-bank checks matched. The MP4 files are not claimed byte-identical.

## Diagnostic measurements

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

These checks do not establish the 100 ms fallback or 50 ms ordinary-edit target, p95 performance, a memory limit, physical-display latency, or source-decode/PCM performance. The earlier private prototype measurements are a separate batch and are not pooled with this production run; see the [private prototype record](../2026-10-23-insert-split-proof-prototype/README.md).
