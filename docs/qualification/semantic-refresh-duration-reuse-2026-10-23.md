# Semantic refresh duration reuse qualification (2026-10-23)

Semantic selection refresh now shares one proved duration map for its immutable
staged document. The production change is
`3a32c923358c0cb7ca53f821bfb6db3e01ebb466`. Full Core, broader/UI, strict,
actual media/history, native workflow and portable-copy checks pass.

Five final quiet pairs measure control/candidate input-to-commit medians
246.381375 /238.914458 ms and offscreen composition
248.946250 /241.268958 ms. The composition difference is 7.677292 ms, about
3.08%, diagnostic only. The 100 ms fallback and 50 ms ordinary-edit targets
remain open; no requirement or gate closes.

## Change

The only production file changed is
`crates/deadpan-core/src/semantic/planner/selection.rs`. For local command work
on a document without derived clocks, `refresh_children` obtains the current
document's shared durations and borrows that same document immutably through
the existing bounds, child-end and child-index updates. Both anchor indexes,
assignment/error order and partial-update behavior remain. The reference and
derived-clock paths retain their original code. No command application,
provider/allocation order, work charge, host authority, history check, schema
or public API changes. Core 58 /SQLite 87 remain unchanged.

The new standalone `tests/semantic_refresh.rs` adds four differential cases:
wide bound and marked trees with repeated pauses; nested ordinary scopes with
zero groups, marks and untouched siblings; late provider/allocator failures;
and nested FollowSpeed derived-clock fallback. They compare all six plan
fields, exact document/request bytes and event order, immutable inputs and
registers, compound replay, and forward/inverse restoration.

The exact formatted selection source is SHA-256
`f8e16b78ed203dade31c1ff90632371f4d8dc1798db09178b03a3799d93ca526`;
the new test source is
`750a9d78e4db36c9efeab0a68aa7e0ef565d089dfca199ac2a1eadf5649c66bc`.
Both equal the tested [private prototype](semantic-refresh-duration-reuse-prototype-2026-10-23.md).
Qualification pins baseline
`ae5eedd065b851242917b38a428ee40a3588978d` plus those exact two files.
All 2,051 Rust paths and six inputs remain fixed; the new test has no baseline
file. A post-commit source guard confirms the same bytes. Independent full
and sealed-clock follow-up reviews report no findings.

## Verification

| Check | Result |
|---|---|
| All Core tests | 1,526 passed across 70 binaries, zero skipped |
| Broader selected tests | 3,506 passed, 740 skipped |
| UI-feature tests | 1,296 passed, eleven skipped |
| Leak annotations | None in those three runs |
| Core strict checks and formatting | Passed |
| Explicit UI Clippy and formatting | Passed |
| Debug/release neutral media | Each verifies 15 pictures and 24,024 canonical samples |
| Release aligned media | Verifies 19 pictures and 30,430 final canonical samples |
| Native workflows | 1,340 workflow checks plus six Kestrel checks |
| Portable copies | Two nine-file extractions validate and doctor unchanged |

All four new tests pass in the full archived Core suite. All media offsets
are zero, with complete canonical PCM and history controls. Independent-DSP,
listening and ordinary Move PCM-translation claims remain false. The aligned
release test takes 224.988 seconds and retains its slow annotations; it passes
without a retry or assertion change.

The six native scenarios cover sound-group endpoints, macros, named registers,
dot repeat, range-set selectors and transcript-search selectors. They retain
twelve layout/retry warnings and their scope exclusions. Each of the final
ten pause trials also passes seven pause checks and one Kestrel check. No
source/test correction or failed run occurs during production qualification.
The prototype's initial E0624 test-helper compile failure and equivalent
public-field correction remain separately retained in its evidence; no test
ran in that failed prototype build.

## Native timing comparison

Qualified control app SHA-256:
`2761f275c70d471b202ffdf2faa9e047724a169c9607775ff4b4ee6208942608`.
Final candidate app SHA-256:
`a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f`.

Five alternating pairs use fresh processes and fresh 10,000-Hold
Background/Silence packages under one-minute load 3. The unchanged native
replay checks durable publication and accepted offscreen Background
composition. Authored input paths and bytes remain unchanged; only the exact
root SQLite sidecars are excluded and WAL must be empty. Source, input and
executed-binary guards pass throughout.

| Median | Control | Candidate |
|---|---:|---:|
| Input to commit | 246.381375 ms | 238.914458 ms |
| Input to offscreen composition | 248.946250 ms | 241.268958 ms |

This batch is separate from the prototype's 5.131625 ms /2.07% diagnostic
difference. Do not pool them, infer a percentile or attribute the final
difference to one individually timed helper. Source decoding, physical display,
PCM performance, peak memory and calibrated profiling overhead are unmeasured.
Current leaf timing and sampled whole-process memory are the next diagnostic
step before considering a cross-crate one-leaf replay cache.

## Evidence

The independently re-read
[package](../../tools/media-qualification/evidence/2026-10-23-semantic-refresh-duration-reuse/)
contains 470 payload files /15,087,083 bytes. Inventory SHA-256:
`aa437121fee062db0e63f0ea285f909a73916736ea59ac5b6f263fca63d7c02d`.
Every path, byte count, hash and compressed stream verifies. The package
retains exact before/after `.rs.txt` sources, the patch and independent review
provenance, frozen source/input maps, retained executed test/helper identities,
complete logs/reports, portable receipts, ten trial maps, and all three emitted
MP4s with decoder/control evidence. Executables, targets, source archives and
generated project databases are excluded.

All eight qualification stages and 26 post-qualification commands pass.
Packaging succeeds on its first execution. Its verifier checks retained
executed copies against their phase hashes rather than substituting mutable
build outputs. Draft review-schema assumptions were corrected against the
actual primary and sealed-clock records before execution.
