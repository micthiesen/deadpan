# Saved independent sound clocks, 2026-10-03

Attached sounds now retain exact historical PCM through supported ordinary
Sequence edits. This advances DP-04 and DP-09. No product requirement or release
gate is complete.

## Behavior and supported scope

Core schema 45/database 57 stores a chronological journal for each owner/local
sound identity. Every entry names a fresh capture of its pre-edit layout. The live
document supplies the final placement. Moving out and back retains both steps,
including intermediate sample clipping. Unchanged origins add no entry.

Commands with an explicit timing identity can transport an otherwise unchanged
complete top-level sound-bearing branch: insertion, Source/slice splice,
replacement, ripple/range/child deletion and MoveRange. Whole-owner deletion
removes its events and journals. Imported unclocked whole owners keep their
events while existing eligible branches move. A changed surviving processing
branch, partial owner capture, retained-clock copy/import, root-owned temporal
transport and unsupported occurrence isolation fail explicitly.

The planner compiles the first retained layout as an independent processing
definition, with only the sound assets selected for that clock. Original audio,
old gains and old edge policies are absent. Complete Repeat, override and Retime
structure remains. Intermediate layouts supply exact translations of the first
actual processed extent, including nested Preserve output. Nominal owner bounds
do not replace that processed extent.

The canonical bus transports old integral PCM labels through every placement.
Current Hold gates, edges and event/ancestor gain apply afterward. Virtual edge
progress survives allocation clipping; at exact shared boundaries a current
Hold owns its newly rounded endpoint. Source, mapping or offset replacement
retires that sound's journal. Label, gain and edge changes retain it.

Current and retained plans share one work budget, deadline, source admission and
PCM residency cap. Prepared-stage cache identity includes the immutable plan.
Clock-only store changes recheck the exact owner/local sound's retained source
receipt and original. Reopen, Undo and Redo preserve journals with fresh revision
IDs. Earlier unused development databases refuse before writes.

## Review and corrections

Independent core/store review found no actionable issues. Audio review identified
unnecessary compilation and retention of every journal layout with all event
assets. The final reader caches only first layouts and groups their selected
assets. Intermediate placements use exact affine deltas, preserving actual
processed bounds. At most 64 first plans and 64 aggregate selected asset records
are retained. The reviewer rechecked the correction and found no further runtime
issues. Tests cover an 80-clock journal, retries and distinct asset groups.

Early focused runs exposed compilation errors in the new code and fixtures:
an unsupported ratio comparison, an ambiguous sample-route type, fixture
metadata dereferences/name shadowing, and a source-mapping duration type. The
final mapping fixture uses a valid natural-rate SelectedPlacement. These failed
attempts are retained in the evidence.

Two audio expectations needed correction. Mixed-bus suppression is the
intersection of its contributions, so an independent route gap does not mark
the entire bus suppressed when the Original has no explicit Hold mask. A later
Automatic-edge check placed a current silent Hold at the sound's exact endpoint;
that Hold correctly owns the newly rounded boundary. Separate regressions now
check retained edge progress and the current-Hold override. At the latter
boundary, the final gain is 1/192 instead of the ungated retained envelope's
3/192. Independent review confirmed the existing policy.

Strict lint also required boxing the larger prepared-input enum variant and
removing an unnecessary singleton-slice clone in the store test. The `doctor`
format-refusal label now includes schema 56.

The full workspace run exposed one more fixture assumption: a short selected
input through Preserve has the complete processor output as its audible extent.
The gate test now attaches its sound to the Retime output, bypassing that
processor and establishing the intended narrower audible interval. Production
code did not change after the broad run. The complete affected planner target
passes in the rerun below.

## Verification

- `cargo test --workspace --locked --no-fail-fast`: 3,588 passed, one new
  fixture assertion failed, zero ignored. All other targets passed.
- `cargo test --workspace --locked --test routed_voice`: all 11 passed after
  the fixture correction, zero failed or ignored. Ten overlap the broad run.
- `cargo fmt --all -- --check`: passed before and after the fixture correction.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed
  before and after the fixture correction.

Only `crates/deadpan-plan/tests/routed_voice.rs` changed after the broad run.
Its source inventory SHA-256 was
`eafa75199b77853fb6cd3a0dde230991735e4c9a05a4f8ac1ea60ed26e970a7c`.
The corrected target, final formatting and final lint used
`8588cea6b70f485b15e280a1f916cc0b8c62d0d477101415f77dc5ffb211279f`.
Every run verifies that its source inventory stayed unchanged during execution.

[Evidence metadata](../../tools/media-qualification/evidence/2026-10-03-sound-clocks/metadata.json)
records each command, exit status, source inventory, environment, review and
verification scope. Compressed complete logs retain failures and passing reruns.
[SHA256SUMS](../../tools/media-qualification/evidence/2026-10-03-sound-clocks/SHA256SUMS)
covers all retained evidence files.

## Environment and untested scope

Base: `53bed8e967b940b825bd92e363b4ed0d19b33998`.
Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust/Cargo 1.97.1,
locked dependencies, FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

Real decoded PCM comparisons exercise NTSC moves and returns, cold shuffled
reads, nested Preserve, current gain, source revocation and shared residency
limits. Preserve references reuse the canonical DSP engine and do not
independently qualify its algorithm. The store lifecycle test uses a retained
real media fixture and forces a history-write failure before checking reopen
and fresh-revision Undo/Redo.

No native UI changed or interactive app was launched. Optional app configurations,
acoustic delivery, performance measurements and emitted-movie equivalence are
outside this increment. Temporal edits inside surviving sound-bearing branches, complete
copy semantics, allowances, tails, native beat-sound placement and release
qualification remain open.
