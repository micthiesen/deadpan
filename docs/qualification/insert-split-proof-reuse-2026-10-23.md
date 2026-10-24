# Insert-split proof reuse: production qualification

**Status:** production qualification complete on this Mac; the performance requirement remains partial. This record is separate from the private instrumented prototype in [its own report](insert-split-proof-prototype-2026-10-23.md).

## Change and build

The production change is limited to crates/deadpan-core/src/insert_time.rs. For the narrow local, compact InsertTime case with no live derived clock and no prior exact duration map, it reuses the already-validated entry duration map only around the existing legacy interior Split bridge. This skips one redundant successful validation of the captured working input. Capture, complete Split-result checks, binding and authority checks, byte/work charges, history, replay and publication checks still run. The borrow ends before the changed working document is installed. No public API, persistent cache or schema changed; Core is 58 and SQLite is 87.

The reviewed source map contains 2,053 Rust files and matches the promoted Main source exactly. Its SHA-256 is ee77ff2f40c620cbf4542f41c877295e982adaec32722796f357331b21b7915e. Only insert_time.rs differs from the qualified base; its SHA-256 is e98dda4425f9e6ca2a19243956c103c1af8d975c38f34d1e1515c2b3c40fae39. All six qualification inputs also match. The uninstrumented release App used for the candidate is SHA-256 e6abcd3a521697f504901f156df445e41681cfc824ff97793f62f4e0865802e1; the control App is 0cb2931562108a68b57915c6135f448bc74293ba20623c7484053152566d31ff.

The nine-stage build completed with Rust/Cargo 1.97.1: toolchain and Cargo checks, formatting, Core unit-test build and run, Core integration tests, Store preparation tests, release App build with ui-harness, and strict all-target Clippy. Results were 724 Core unit tests, 22 Core integration tests and 12 Store tests passed. Release helpers were reused from the preceding qualified build; the release App was freshly built. The physical Mac reports Mac17,7, 128 GiB and 18 cores; the hypervisor flag is zero.

## Uninstrumented timing

Five fresh control/candidate pairs ran in alternating order on the 10,000-beat Background/Silence fixture. Each invocation waited for load at most 3 and no conflicting processes. All 10 trials passed seven pause workflow checks and one Kestrel check each. The source, input-byte and binary guards passed, WALs were empty, and the trials had no diagnostic hooks. Twenty retained layout warnings were not failed checks. Timing excludes physical display and GPU readback; file-cache, power and thermal state are not controlled.

| Measure | Control median | Candidate median | Median paired reduction |
| --- | ---: | ---: | ---: |
| Input to commit | 194.497917 ms | 192.496542 ms | 2.380874 ms |
| Input to offscreen Background composition | 197.044959 ms | 194.965208 ms | 2.504043 ms |

Every paired difference was positive. Commit reductions ranged from 1.007084 to 3.183666 ms; composition reductions ranged from 0.929000 to 3.219083 ms. These five pairs are diagnostic observations, not a percentile estimate or a formal latency-target pass. The 100 ms fallback and 50 ms ordinary-edit targets remain open. This does not establish physical display latency, source-decode performance, PCM performance, or a memory result.

## Qualified media and history

The release CLI test group_endpoints_export::one_leaf_replay_qualified_media_commit_export_matches_uncached was selected once and passed once from a freshly built release archive. The selected CLI SHA-256 is 4574787836923e63dce6d4fa8eea33415d4a9d889853f1200bb9ad2f73d48c00, its extracted test binary is dddd87f5b673c394647c339ad25bef76f7be940951b359bba69706b2067a3148, and the archive SHA-256 is b2ae4159642e2b74c830724e9aa5a789eaeea562563f747ad00a669b189d20c5. The resumer created a fresh extraction directory and reused that exact archive without recompiling. Its earlier harness attempt exited 96 because the extraction directory was absent; no test ran in that attempt. Both failure and successful resume remain retained.

The paths with OneLeafReplay supplied (Some) and omitted (None) both checked complete document bytes, commit/undo/redo, register banks, canonical PCM and limiter-gain bits, partition/cold reads, full history replay, undo/redo after reopen, and the pause's silent interior. Both emitted MP4s independently verified 45 pictures and 72,072 presented samples with zero measured audio offset. The encoded movie files are not byte-identical; the equality claim is for the checked canonical content and history. Independent DSP comparison and listening remain outside this test.

## Retained evidence and limits

The [primary evidence package](../../tools/media-qualification/evidence/2026-10-23-insert-split-proof-reuse/inventory.json) retains 238 payload files (4,863,532 bytes), 239 files including inventory; its inventory SHA-256 is 55fc1aaacd7b6f8a806708c5c7f8c9fd3ec4523e5a23204433a638a695b67e8b. The [independent verification supplement](../../tools/media-qualification/evidence/2026-10-23-insert-split-proof-reuse-verification/root-package-verification.json) reread all payload hashes and sizes, compared 224 approved payloads with their original bytes, decoded 30 gzip streams, checked 10 fixture archives with 30 regular members, matched fixture maps to live and packaged before/after states, verified both export hashes, and confirmed empty WALs.

The collector's initial live-stdout mistake was caught before copying; its unused manifest and correction remain retained. The copier passed on its first execution. The independent verifier initially rejected a valid retained .deadpan directory, then passed after correcting that filename assumption. Both verifier scripts and logs are in the separate supplement; the primary package was unchanged.

This evidence qualifies the narrow production change on the exercised Mac and media fixture. It does not close DP-18, DP-24, a delivery gate, or any broader media, hardware, acoustic, percentile, or latency claim. Coverage remains 89%, with eight sections Complete and sixteen Partial.
