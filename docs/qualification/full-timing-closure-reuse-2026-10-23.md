# Complete frozen-layout reuse qualification (2026-10-23)

Complete frozen timing tables now reuse their immutable storage when slicing
keeps every node. The production change is
`ae5eedd065b851242917b38a428ee40a3588978d`. Full Core, broader/UI, strict,
actual media/history, native workflow and portable-copy checks pass.

Five final quiet pairs measure control/candidate input-to-commit medians
257.618792 /248.292125 ms and offscreen composition
260.353458 /250.976250 ms. The composition difference is 9.377208 ms, about
3.60%, diagnostic only. The 100 ms fallback and 50 ms ordinary-edit targets
remain open; no requirement or gate closes.

## Change

Only `crates/deadpan-core/src/audio_reference.rs` changes. After the entire
required-alias/ancestor loop and existing derived-clock fallback,
`FrozenAudioLayout::sliced` checks local mode, nonprovisional admission and
complete node coverage. It retains the exact `wire_bytes()` guard and returns
the existing immutable Arc. Partial/provisional tables and reference mode
retain the rebuild. No public API, schema, new cache, work charge, validation,
provider/capture authority, inverse/history check or publication path changes.
Core 58 /SQLite 87 remain unchanged.

The private fields are immutable after admission. With full closure, the old
rebuild retains every wire field, including Sequence children, overrides, gap
overrides and lineage. Waiting until the complete alias loop preserves late
missing-alias errors. The byte guard preserves canonical wire-limit refusal
even for private admission without canonical-size checking. Excluding
provisional layouts preserves their command-scoped authority.

The exact formatted source is SHA-256
`ed3259f86f811e61adcffdc58972342057b9f3afa1622fd38bc42db5c44dd70d`.
It equals the tested [private prototype](full-timing-closure-reuse-prototype-2026-10-23.md).
The qualification records baseline
`ca40995b64fc4acdc0a5e046660a36a1faa7cee5` plus that one-file overlay. All
2,050 Rust paths and six qualified inputs remain fixed through the checks.
A post-commit guard confirms the same bytes. Independent source review found
no defects; formatter-only changes are retained alongside the reviewed source.

## Verification

| Check | Result |
|---|---|
| All Core tests | 1,522 passed across 69 binaries, zero skipped |
| Broader selected tests | 3,506 passed, 740 skipped |
| UI-feature tests | 1,296 passed, eleven skipped |
| Leak annotations | None in those three runs |
| Core strict checks and formatting | Passed |
| Explicit UI Clippy and formatting | Passed |
| Debug/release neutral media | Each verifies 15 pictures and 24,024 canonical samples |
| Release aligned media | Verifies 19 pictures and 30,430 final canonical samples |
| Native workflows | 1,340 workflow checks plus six Kestrel checks |
| Portable copies | Two nine-file extractions validate and doctor unchanged |

The five new Core cases pass in the full archived suite. They cover exact
local/reference wire and Arc-sharing behavior over nested layouts, partial
and provisional slicing, late missing aliases with derived clocks, the real
canonical size-limit refusal, and a 10,000-Hold interior pause's complete
document/transaction/forward/inverse equivalence. Existing scoped-capture and
randomized command-work reference comparisons also pass.

All media offsets are zero, with complete canonical PCM and history controls.
Independent-DSP, listening and ordinary Move PCM-translation claims remain
false. The aligned release test takes 238.739 seconds and retains its slow
annotations; it passes without a retry or assertion change.

The six native scenarios are sound-group endpoints, macros, named registers,
dot repeat, range-set selectors and transcript-search selectors. They retain
twelve layout/retry warnings and their scope exclusions. These warnings are
not hidden failures. The final ten pause trials additionally pass seven pause
checks and one Kestrel check each. No source or test corrections were needed
during production qualification.

## Native timing comparison

Qualified control app SHA-256:
`c36b894466eb4f16fe976924b05c02ada87b0e8872f7f9a7fc6028a60a342037`.
Final candidate app SHA-256:
`2761f275c70d471b202ffdf2faa9e047724a169c9607775ff4b4ee6208942608`.

Five alternating pairs each start a fresh process and fresh 10,000-Hold
Background/Silence package, under one-minute load 3. The unchanged replay
checks durable revision publication and accepted offscreen Background
composition. Authored input paths and bytes remain unchanged; only exact root
SQLite sidecars are excluded and WAL must be empty. Source/input/executable
guards pass throughout.

| Median | Control | Candidate |
|---|---:|---:|
| Input to commit | 257.618792 ms | 248.292125 ms |
| Input to offscreen composition | 260.353458 ms | 250.976250 ms |

This is separate from the private prototype's 13.445833 ms /5.16% diagnostic
difference. Do not pool the batches, infer a percentile or attribute the final
difference to one individually timed helper. Source decoding, physical display,
PCM performance, peak memory and calibrated profiling overhead are unmeasured.

## Evidence

The independently re-read
[package](../../tools/media-qualification/evidence/2026-10-23-full-timing-closure-reuse/)
contains 455 payload files /14,990,467 bytes. Inventory SHA-256:
`697803f666c8860fc997333a0378bbdca0371399a8612a718ddd0c36067418ad`.
All paths, byte counts, hashes and compressed streams verify. It retains exact
before/after `.rs.txt` sources, patch and review provenance, frozen source and
input maps, executed test/helper identities, complete build/test/native/media
logs and reports, portable verification receipts, all ten trial maps and the
three emitted MP4s with decoder/control evidence. Executables, targets, source
archives and generated project databases are excluded.

All eight qualification stages and 26 post-qualification commands pass. The
final packager verifies retained executed copies against their phase hashes;
later mutable build outputs are not substituted for earlier executables.
Its draft's nonexistent leak-receipt requirement and original-snapshot path
were corrected against actual records before execution. Packaging succeeds
with no failed run or incomplete stage.
