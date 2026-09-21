# Exact boundary resolver verification

Historical report: the later [persistent mark implementation](MARK_VERIFICATION.md)
adds schema 3, mark ownership, and atomic edit transforms. The evidence below
describes the earlier query-only revision.

This report covers the source change titled `Resolve exact revision-aware edit boundaries`, based on `b2c8314b44f2a2de53f61a1b158029ba5ce36f6e`. It implements a read-only query surface. Persistent marks, attachment ownership, insertion/deletion transforms, and commands that edit a resolved range remain required work. Schema 2 is unchanged.

## Implemented contract

[`AnchorIndex`](../crates/deadpan-core/src/anchor.rs) borrows one validated document revision. It indexes authored parent links and Sequence prefixes without expanding Repeat plays. A query resolves Local, Occurrence, Sequence, or original Source coordinates. Repeated Local and all Source targets require explicit complete occurrence scope. Retired identities, missing ancestors, ambiguous scope, cropped Retime regions, absent streams, and out-of-bounds positions fail explicitly.

Local frame positions and all intermediate mappings are `ExactRatio`. Source timestamps and original audio samples retain signed origins and explicit clocks. Positive `SourceNode.audio_offset` delays audio by the corresponding project-frame fraction of the 48 kHz offset. The result preserves the exact position and rounds once with ties-to-even to a project-frame boundary. End boundaries are legal; reversed, empty, or rounded-away ranges are rejected. Media role is explicit selection intent, not inferred from anchor provenance.

The CLI `resolve-selection` reads a bounded protocol-1 request and a read-only snapshot. It checks project/revision before resolution, emits structured errors, and can coexist with the package writer. It does not mutate SQLite, open a window, decode media, or substitute a different occurrence. [`HEADLESS.md`](HEADLESS.md#exact-boundary-selection) contains a runnable request shape.

## Automated evidence

On 2026-09-20, the repository gate passed on Apple M5 Max, 128 GiB RAM,
macOS 26.5.2 (25F84), using Rust 1.97.1 and locked dependencies:
`cargo fmt --all -- --check`, workspace Clippy with warnings denied, workspace
tests, workspace build, and `deadpan-cli doctor`. There were **97 passing tests**
and none ignored: 46 core, 23 store, 16 plan, 10 CLI, and 2 native headless tests.
Clippy initially rejected the large resolved-range enum variant; boxing its two
boundary payloads fixed that without changing JSON output.

Independent core and CLI reviews found no actionable defects. The core reviewer
also reran all ten anchor tests. Review explicitly excluded persistence and edit
transforms because they are not implemented by this change.

[`anchors.rs`](../crates/deadpan-core/tests/anchors.rs) adds ten tests, including two property tests:

- Nested Retimes preserve exact fractions and reject boundaries removed by either mapping.
- Stable occurrences follow play reorder and grouping; removed identities stay invalid after growth. An older immutable index still resolves its original revision.
- Complete ordered nested Repeat ancestry is required, with explicit depth and extra-scope rejection.
- The final boundary of `u32::MAX` plays resolves from compact runs. A one-play Repeat tolerates an unused gap whose period exceeds `i64`.
- Negative video PTS and 44.1 kHz original audio samples resolve equivalently across exact clocks. Zero sample rates and out-of-source boundaries fail.
- Signed 48 kHz audio offsets map exactly at 30000/1001 fps; out-of-host audio is not clamped.
- Project/revision guards, empty-project endpoints, ties-to-even, reversed ranges, and collapsed ranges are checked.
- Strict wire shapes reject unknown coordinate fields. Bias survives serialization, without claiming edit transformation.
- Random nested duration ratios equal an independent cancelled-ratio expectation; random Sequence prefixes equal an expanded boundary sum.

The CLI process test `selection_resolution_is_revision_checked_exact_and_read_only` checks exact/quantized output, requested audio role, stale revision details, range errors, future protocol rejection, and an unchanged snapshot while the writer remains open.

## Limits and next work

The native UI is still the welcome shell. No GUI testing is warranted by this pure query change; GUI aesthetics, native focus/IME, accessibility, and natural keyboard navigation remain explicit full-product acceptance obligations.

This index builds from the whole authored document and scans compact runs for iteration identity. It is not incremental and has no published latency qualification. Exact arithmetic may report representational overflow instead of silently approximating a mathematically valid coordinate. A held source picture has many possible project positions and is rejected by this reverse query.

The source-sample query takes an explicit caller-supplied sample clock. Asset
records do not yet carry a decoder-measured original sample rate, so this query
proves coordinate conversion, not the accuracy of media metadata supplied by a
future import adapter. No source is decoded by this implementation.

The next persistence work must add marks and their transformations atomically to forward/inverse patches, then migrate both schema 1 and schema 2 histories by replay on a consistent candidate. Boundary resolution alone does not satisfy DP-04 or Gate B.
