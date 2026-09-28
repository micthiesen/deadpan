# Root sound ripple history qualification

This increment adds core 30/database 36 root sound journals through ordinary
Sequence InsertTime, SpliceSource and Delete, plus neutral non-root Split.
It does not complete sound editing, DP-09, any release gate or the full project.
Native placement, root Split, temporal occurrence edits, nested ownership,
Repeat/Retime sound transformations, allowances, voice effects, listening and
preview/export acceptance remain open.

## Retained contract

The complete source recipe, selected support, original root extent and sample
grid stay independent of later audible fragments. Ordered operations retain
their physical cuts. At 30000/1001 fps, two one-frame insertions resume original
sample 3204 at current sample 6406; a final frame offset would choose 3203.
Source endpoints and envelope boundaries retain integral sample labels alongside
exact semantic provenance. Current silent Holds clip routed output once.

SetSound parameters preserve routing. Explicit ReplaceSound discards it in one
undoable transaction. Migration validates the old command in its old context
before replaying modern code. Frozen core 29 rejects a sound-bearing Split even
when its modern patches and snapshots contain no new routing fields.

## Fixtures and review

The preserved previous CLI was built from clean `d6e931c8c57f5684302ca9bc903d43ea84fed1a8`.
Its SHA-256 is `0bcffddfc0d8383841ecec308ee4bea93c8bae40b36fcdcba0ec5f31e772a922`.
The [producer](../../crates/deadpan-store/tests/fixtures/produce-v35-sound-route-history.py)
authored every database-35/core-29 snapshot and patch through that binary. The
[provenance](../../crates/deadpan-store/tests/fixtures/v35-sound-route-history.provenance.json)
records its commands and hashes. The fixture has 13 revisions, eight history
entries, pending redo, two sounds, an abandoned parameter branch, one measured
source qualification and its managed Original. Reconstructed SQL was reopened
and validated by the same old CLI. Positive migration evidence uses no schema
relabeling or patch rewriting.

Independent read-only review covered core/storage compatibility and the explicit
sample-route task stack. A separate review covered raw/projected providers,
shared budgets, admission and envelope semantics. Review identified repeated
per-block island construction and a physical Hold-overlap counterexample after
four odd sample shifts at 32000 fps. These require indexed immutable envelope
construction and physical clipping/edge selection independent of exact Hard
coincidence. The same drift requires event survival to follow integral retained
support, with a logical fallback only for initially sampleless selections.
An extreme but valid endpoint also requires clipping widened translated bounds
before narrowing to an integral output sample label. Virtual envelope labels
must retain sufficient width independently of bounded output allocations.

## Verification

Focused checks passed nine core routing tests, eight root route/envelope plan
tests, five store migration/durability tests, five audio admission/budget tests
and four real decoded-PCM playback tests. The plan checks include 600 fragmented
cuts under a bounded query budget, physical Hold clipping after repeated odd
shifts, and a virtual envelope endpoint beyond the output integer range. Separate
route suites cover 64 NTSC edits and 512 chronological operations without
recursive history traversal. Playback verifies shuffled warm/cold queries,
reopen, undo/redo, the original 44.1 kHz source phase and the transported endpoint.

The final gate passed with one unchanged source manifest:

- `cargo fmt --all -- --check`: exit 0.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0.
- `cargo test --workspace --locked --no-fail-fast`: exit 0, 1,875 passed,
  zero failed, zero ignored, across 151 reported test groups, including doc tests.
  The same process completed in 680.29 seconds without restart.

The [retained evidence](../../tools/media-qualification/evidence/2026-09-27-root-sound-routing/README.md)
contains command reports, compressed logs, source hashes and earlier failed
attempts. Independent review findings are closed. Checks ran serially on Apple M5 Max, 128 GB,
macOS 26.5.2 (25F84), Rust/Cargo 1.97.1, using the explicitly selected FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`.

No native UI changed in this increment. Computer use, painted UI replay,
listening and physical device qualification are not evidence supplied by these
headless checks. The existing imagegen placement board remains the visual target
for the later native controls.
