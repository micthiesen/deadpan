# Occurrence sound preparation, 2026-10-03

This increment adds a borrowed plan and PCM preparation API for one independent
catalog sound in a concrete current structural occurrence. It advances DP-04
and DP-09 groundwork. Persisted sounds remain root-owned; beat attachments,
their editing/copy lifecycle and `ib`/`ab` remain open.

## Behavior and review

The constructor resolves stable Repeat paths and effective play/gap branches,
then applies enclosing Sequence and Retime maps. A voice on a Retime output
bypasses that stage. A voice below it gets an independent Preserve processor
with full selected input, including neutral padding around its occurrence.
The Original's retained bindings and endpoints cannot become the sound's clocks.

Intermediate preparation retains PointCeil coordinates. Final root allocation
uses RoundEven on exact mapped boundaries. Current silent Holds gate the final
output after processing, so a Hold with no intermediate allocated sample can
still suppress its actual root sample. The source remains a live dependency
even when a requested block precedes the audible recipe.

Independent reviews covered occurrence ancestry, clock composition, budgets,
policy, source admission and the PCM adapter. They found no concrete runtime
defect. The planner review identified missing combined nested Repeat/Preserve
coverage; a dedicated case was added. The adapter review noted that the PCM
oracle uses the existing `CanonicalStretch`. These tests establish structural
adapter equivalence and phase, not independent stretch-algorithm qualification.

The first compile rejected an ambiguous sample-grid type before tests ran.
The root grid now explicitly names `AudioSample`. Test calls were corrected to
pass owned `InstancePath` values, and the temporary provider enum boxes its
larger source variant.

The next run passed all nine new plan cases but failed five PCM fixtures during
document construction. Their shared Source had neither video nor audio, which
the validator correctly rejects. It now uses a valid picture-only Source so the
independent sound can be tested against absent primary audio. The NTSC PCM case
and the unrelated selected occurrence tests passed in that run.

## Verification

The corrected filtered workspace run passes 79 tests, including ten new plan
cases and six real-PCM cases. The full workspace run passes 3,523 tests with
zero failures and zero ignored. Counts overlap;
the focused run is not an additional full suite.

The same source passes `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets --locked -- -D warnings`.
The full test command is
`cargo test --workspace --locked --no-fail-fast`. Source inventories stayed
unchanged during all checks. The final source inventory SHA-256 is
`d9fe588eb5c0c648f05fd4f649aaa8062f56e7cf83b98e50ddc5577f946110ab`.

The plan cases cover a million-play Repeat without expansion, stable overrides,
active explicit gaps, invalid/invisible owners, nested Repeat scopes across
Preserve, source phase through crops, recipe containment and work limits.

PCM cases compare cold and shuffled reads against explicit source resampling
and canonical processing references. They cover a stage-owned voice versus its
descendants, two nested Preserve stages, NTSC phase with a signed sample offset,
44.1 kHz mono resampling, absent primary audio, final Hold suppression, foreign
plan/range/cancellation rejection and source re-admission for silent reads.

## Limits

This API does not persist nested sound ownership, transform sound clocks through
edits, capture sounds into registers, schedule every active occurrence, apply
event gain/edges/allowances or populate the final bus. Those remain required
before native beat-owned sound placement and the full beat text objects.

No UI behavior changed. Native GUI replay, optional app-feature tests, acoustic
delivery and performance measurements were not repeated for this backend
increment. Full-product requirements and Gates A through G remain open or partial.

## Environment and evidence

Base: `8be96ad81e260684becde17f5ac43d3fbd73a291`.
Apple M5 Max, 128 GiB, macOS 26.5.2, Rust/Cargo 1.97.1, locked dependencies and
FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

[Evidence metadata](../../tools/media-qualification/evidence/2026-10-03-owned-sound-voices/metadata.json)
retains exact commands, exits, counts, source identities, environment and review
limits. The same directory contains compressed logs for successful and failed
attempts and their source inventories.
[SHA256SUMS](../../tools/media-qualification/evidence/2026-10-03-owned-sound-voices/SHA256SUMS)
covers every retained evidence file.
