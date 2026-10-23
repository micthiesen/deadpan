# Compound runtime preparation qualification (2026-10-23)

## Result

The Compound runtime preparation change passes focused Store tests, the broader archive, the corrected UI archive, strict checking, UI Clippy and formatting, debug/release media qualification, six release-native scenarios and ten final large-pause trials. The Store prepares the Compound once inside its existing Immediate transaction, then invokes fallible host runtime-bank preparation before any write. It returns that private value only after durable commit. Existing head, bank, allocation, capture and consumed-analysis checks and old commit APIs use the same guarded helper. Core 58 /SQLite 87 is unchanged.

The candidate's five-trial median is 273.119542 ms from input to commit and 275.697709 ms through offscreen Background composition. Control medians are 390.680250 ms and 393.261916 ms. These are diagnostics from one fixture and machine, not a p95 or target PASS; the 100 ms fallback and 50 ms ordinary-edit targets stay open. The comparison does not subdivide the saving into internal phases.

## Change and retained test evidence

The implementation builds on the store compound API and SQLite transaction work already present at commit `8c6e`; this checkpoint adds preparation and commit integration across store, CLI and app. The source identity map records base `09063d3a4169cd2b9fe4f567def147b20417c7b2` plus six changed Rust files. Those six final source copies match the qualified release source map.

The focused store target passed all five tests. The first focused native invocation used `--lib`, selected zero tests, and is retained as a failed selection in `focused-zero-selection.json`; it is not counted as a pass. The new native case was subsequently selected and passed in the corrected full UI archive. The broad archive passed 3,505 tests with 740 skipped; the corrected full UI archive passed 1,296 with 11 skipped. Strict `cargo xtask check --since 0418cd49274ed0ac4094a3993ac29cc15477a852 --no-test`, strict formatting, corrected UI Clippy with warnings denied, and UI formatting all exited 0. Per-command receipts, logs, binary/source manifests, and frozen inputs are retained.

Two independent reviews reported no defects before regression runs. An extra native replay exposed a stale assertion that Split clears semantic repeat state. It fails identically on the qualified baseline (`3bf2d47c6fbc7eade3a259fc8e9ffc275043af2bd62e698a0b98c0937dcfd9bd`): Split is documented as repeatable. The harness now tests unsupported Edit-domain whole-Original insertion. The broader tests precede this harness-only change; all 1,296 UI tests, strict UI checks and release replays were rerun afterward, with the six callback files unchanged. Final workflow counts are sound-group endpoints 197, macros 401, named registers 171, dot-repeat 383, interval selectors 85 and transcript-search selectors 103: 1,340 total, plus one Kestrel check per scenario run. The separate debug visual replay passes 383 dot checks plus Kestrel. Debug performance mode was first explicitly refused before any replay; that launch failure is retained. A review concern about preserving an Original-domain edit does not apply to this Edit capture, whose original context is None; the code and passing replay establish that distinction.

The final release app hash was `69978abd09ffc52dcd281066bc615d7b3e1d5cb11653f5d888d0738015ba1496`. After all 25 corrected post-run steps passed, two inaccurate harness comment lines were changed and formatting was checked. The retained release-built and final comment-only `semantic.rs` copies have separate hashes; the comment-only edit is not represented as a re-tested binary change. The initial wrapper parse error is retained; its body did not run.

## Media qualification

Debug and release verify all 15 neutral-scope pictures and 24,024 audio samples, and release verifies all 19 aligned-scope pictures and 30,430 samples, at zero offset. Complete authored/limited PCM and limiter-gain bits, Undo/Redo and reopening controls pass; the aligned group/Split comparison also retains 24,024 samples. Both nine-file portable extractions pass full validation and doctor with identical paths and bytes. The aligned test's 223-second passing run retains its informational SLOW markers. Both reports keep `independent_DSP_claim: false` and `listening_claim: false`; the aligned report also keeps `move_PCM_translation_claim: false`. No general Move translation, independent DSP or listening qualification follows.

## Large-pause comparison

Each fresh package contains 10,000 direct 12-frame Background/Silence Holds, 1920×1080 at 24 fps /SDR Rec.709, two setup commits and no media. The same `hold 12f` at frame 17,933 inserts time into a private portable copy. Pair order alternates, every run starts below load 3, all seven checks pass, and source/binary guards pass. Original authored paths and bytes remain unchanged; only a 32,768-byte SQLite shared-memory sidecar and an empty WAL appear. Each report retains two layout/retry warnings and three skipped-scope notes, not skipped test failures. Warnings describe second-retry layout settlement and an egui discard-retry sequence.

| Build | Input to commit median | Input to offscreen composition median | n |
|---|---:|---:|---:|
| Qualified control | 390.680250 ms | 393.261916 ms | 5 |
| Candidate | 273.119542 ms | 275.697709 ms | 5 |

The medians fall by 117.560708 ms to commit and 117.564207 ms through composition (about 29.9%). No p95, confidence interval, formal target PASS, physical-display timing, source decoding, PCM performance or finer Store-time attribution is established. The earlier direct 93.633 ms measurement has different boundaries. These samples are not a product-level latency guarantee.

## Evidence package

The companion package is [2026-10-23-compound-runtime-preparation](../../tools/media-qualification/evidence/2026-10-23-compound-runtime-preparation/). It retains source snapshots, qualification receipts, wrappers and logs, runtime binary manifests, reports and before/after input maps, compact actual media outputs and verified portable tars, plus initial failures and corrections. Executable binaries, Cargo archives/targets, and generated SQLite package trees are not retained. The independently re-read inventory lists 430 files totaling 10,600,909 bytes; `inventory.json` SHA-256 is `e0fff4ebe073aa803ea3b3cd79b827342ec12558d0c242a91968c3e1aa98d3c0`.
