# Single-leaf net reuse prototype

Status: private diagnostic prototype. Its source is not promoted to Main and the measurements do not qualify a product performance gate.

## Change and scope

The prototype keeps the successful `EditTransaction` produced while applying an eligible `InsertTime` leaf. During ordinary one-leaf replay it rebases that transaction's revision headers and description instead of rebuilding the aggregate net diff and inverse. Prefix admission, wire checks, work charges, visitors, identity observation, Store authority, and final document checks remain active. In the release Core build, the fast path skips the redundant final Core aggregate validation, diff/inverse construction, and changed-ID traversal, relying on the successful leaf and revision-only change. The debug whole-edit oracle and Store independently validate the actual result, codec/readback, and inverse. This is not a general cache.

## Identity and environment

| Item | Identity |
| --- | --- |
| Qualified Main base | commit `9211db7043d14e57e06782342aa7defd1313a8b6`, 2,053 Rust paths, source-map SHA-256 `ee77ff2f40c620cbf4542f41c877295e982adaec32722796f357331b21b7915e` |
| Private candidate | 2,054 Rust paths, source-map SHA-256 `0bf07513646ffdfee5aec03bb1b64b6fff757196512aeea883226c675b15ed02`; one added profile module and 14 changed paths versus qualified Main |
| Qualified control app | SHA-256 `e6abcd3a521697f504901f156df445e41681cfc824ff97793f62f4e0865802e1` |
| Matched profile baseline app | SHA-256 `ffbaf7273f10e1a67041b3645d027be525bce4cc82d04985707ba781c63ee655` |
| Candidate release app | SHA-256 `1a12c2e9f866de1cef3ec64ea3997b26a1baafa194a95e3eef9b4587607d288a` |
| Real-media CLI / test binary | CLI `5a0f376535cb5553843dd1cf5d562cbdc44e44c0202182c27820925ffa3c54fa`; test `d3210a244854be52aaf5bb73ad27bfbf470789cee0a39e973427f92ff8bc9582` |
| Memory observer | SHA-256 `d1166aef7e1b30c787bb39fa536c18aee9af9da4f2327120d321432205fc0ed2` |
| Machine and toolchain | Mac17,7, 18 physical/logical cores, 128 GiB, AC power; Rust 1.97.1 |

The three source maps and six build inputs are retained separately. The candidate sources were copied and built in an isolated tree; qualified Main's immutable source snapshot is retained as the comparison basis.

## Checks completed

The fresh nine-stage build completed with 734 unfiltered Core unit tests, 35 Core integration tests across four retained binaries, 12 release Store preparation tests, a release app build, and strict checks. The earlier private build attempt was intentionally interrupted after an unused `Result` warning; the corrected private test then passed in the fresh build.

One release CLI test passed on the physical Mac with the synthetic worker feature. Cached and uncached exports each verified 45 pictures and 72,072 samples with zero measured offset. Canonical PCM, authored limited-gain bits, partition and cold reads, history replay, and register-bank checks matched. The emitted MP4 files are not claimed byte-identical.

The six-rotation timing lane completed 18 trials and 36 commands, with 126 workflow checks and 18 Kestrel checks passing. The separate owned-observer memory lane completed six runs. It reports sampled whole-app lifetime lower bounds, not exact peaks.

## Diagnostic measurements

| Lane | Input-to-commit median | Input-to-background-composition median |
| --- | ---: | ---: |
| Control | 191.571 ms | 194.008 ms |
| Matched profile baseline | 191.968 ms | 194.591 ms |
| Candidate | 186.371 ms | 188.802 ms |

Across six paired observations, the candidate's median reduction against the matched baseline was 6.048 ms at commit (range 0.782 to 6.678 ms); composition reduction was 6.160 ms. These are diagnostic observations, not a percentile or target pass. The lane retained 36 warnings across 18 trials.

The separate memory lane's median sampled maximum physical footprint was 651.329 MiB for baseline and 668.954 MiB for candidate. Candidate samples ranged from 621.204 to 670.814 MiB. Sampling covers the whole app lifetime at bounded intervals; it does not isolate leaf state or establish an exact peak, ceiling, or retained-memory attribution.

## Retained failures and limits

The first timing invocation failed a harness post-command assertion because its metric tuple was compared directly with a set. That control run was excluded; the corrected retry completed all 18 trials. The first real-media invocation stopped at preflight because the separate media pins lacked required baseline pins; the retry used corrected pins and passed. These histories are retained harness failures, not product failures.

The measurements are local to the retained fixtures and machine. They do not establish the 100 ms fallback or 50 ms ordinary-edit target, a p95 result, production qualification, a memory limit, physical-display latency, or source-decode/PCM performance. The earlier InsertTime split prototype is a separate batch and its measurements are not pooled with this run.
