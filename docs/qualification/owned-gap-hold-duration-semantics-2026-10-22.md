# Owned-gap Hold duration semantics, 2026-10-22

Verified on this Mac. No requirement or gate is newly complete.

## Authority and implementation

The existing mixed and selected duration authorities now admit explicit owned
gap subtrees. An explicit Play edge follows either its effective body or its
exact owned-gap root. A Default edge still follows the literal body. The gap
belongs to the preceding stable iteration, and its current position determines
whether it renders. The final Play's owned gap is a valid dormant definition.
Reordering can change this status without changing the iteration ID.

One bounded parent index validates every selected target before allocation,
including unchanged members. The complete path must contain only ordinary
Sequence and Repeat ancestors. Retime, Partition, placed sounds, forged or
missing ancestry and fake node targets for implicit default gaps still refuse.
An owned gap can occur below an outer Default choice or above an inner Default
body. No play expansion is introduced.

Actual isolation proofs map every target and the separate primary presentation
after each staged leaf. Every changed member gets its exact identity pool and
distinct revision; the final Compound commits once. A dormant primary keeps
the ordinary cursor even when another selected live gap ripples. Its missing
presentation never becomes an invented Anchor. The occurrence reducer skips
root-clock capture only after positively proving dormant ancestry, retaining
ordinary recipe, provider, mark and history reduction.

The selected all-Play path now also rejects excess allocator node or mark IDs
before reservation. Its body-only regression isolates this repair from gap
admission. Saved instructions and their wire format remain unchanged.

Native inspector hints use the retained compact parent and Repeat indexes.
Selected gaps retain distinct Play authority, and an owned final gap can be a
counterpart of a live primary. A single dormant gap with This play at every
level retains the standalone live-boundary refusal. Implicit default-gap
recipes are not independently editable node targets. These limits remain open
work, alongside the remaining temporal scopes and selectors.

## Valid old-code witness

Base: `9b0c9e620f5a56af37ed5b7d7535acdae7e168a6`. Only the new native
test and registration were installed. The final public hint assertion fails
with `None` instead of `Some("hold-duration 3f")`, in 0.013s with 1,212 tests
filtered. The retained and executed test binary SHA-256 is
`c316982109bd3bb32d6c937f32a23b4d97dc03035e95c23d64e837e99bf465ca`.
The 1,927 Rust source files were frozen before execution.

Before that failure, the test validates a silent Background-Hold tree with an
outer two-play Repeat and an inner two-play Repeat. Existing public all-Play
duration control uses exactly three node IDs and no marks, grows output 14 to
15 frames, retains the primary at 2 and moves the other outer gap to 10. Raw
definition growth 3 to 4 yields 16 frames while retaining the separate 1f
default gap. Inverse and JSON checks pass. A final owned 5f definition has no
Anchor; raw growth to 6f leaves output and bindings unchanged. This is a
structural entry witness, not real-media qualification.

## Review and retained corrections

Independent general and temporal reviews found no actionable implementation
issue. Verification review found that equal-length selected results could hide
a swapped primary/counterpart pair in the encoded fixture. It now checks
ordered forward and inverse isolation mapping, the exact owned-gap root under
each final Play edge and the mapped primary presentation.

Initial checks found CLI closure borrowing, a double reference and a tuple
mistaken for a Result in new tests. UI-feature checking then found helper
visibility/import mistakes and cursor type mismatches in the new replay.
These compile attempts do not establish behavior. The unused cached gap-root
set was removed after the new full path checker replaced it.

The first runtime run passes 11 of 18 tests in 6.023s, with seven failures and
one captured-output marker. Two native unit fixtures navigated one Repeat level
too far. A rewritten outer-gap-only core fixture retained a cursor outside its
new Sequence. Audio assertions treated an internal cut between fragments of
the same Source as a terminal endpoint; they now require continuation there,
while real Source endpoints still require silence. The store expected fallback
artifact controls although the accepted artifacts retained exact requests;
the corrected assertion checks the accepted artifact and matching request ID
and options. The CLI independently rounded a relative frame at the moved NTSC
origin, comparing offsets 3,156 and 3,155 samples; it now moves the old
entry-relative offset to the independently derived new entry. An independent
follow-up review confirmed these corrections retain the strong oracles.
The second focused run passes 16 of 18 in 30.044s, with only those two
split-fragment PCM assertions still failing. Direct checks independently pass
both native setup fixes and the provider/history test. At a historically
rounded split, the next fragment's first sample is not the former fragment's
continuation either. The final oracle retains the complete captured 25-frame
pre-split Source document, first validates the 29-frame fixture against it,
then checks every moved fragment and extra NTSC sample against its own
original entry and complete Source endpoint. Both samples and suppression
flags are checked without dropping a sample. No production clock change was
made in response to those failures.

The marked dormant all-Play core test passes alone from its exact retained
binary in 0.007869s, with stdout and stderr reaching EOF before 30s. Its binary
SHA-256 is `86a3462e8d2844beb4d32f2ee686b38c95b4f1dc3ba9c8ed17b8bb5a1470c3cf`.
The concurrent marker is retained; its cause is not established.
The command-reference rewrite intentionally fails its rewrite assertion before
the ordinary generated-reference check.

## Verification

All 18 focused tests pass in 30.532s, with 5,216 filtered and no captured-output
marker. Eleven core cases cover both owned-gap directions, ordered three-gap
mapping with marks and final-primary dormancy, no-op/live/dormant members,
current-position changes after reordering, the dormant raw all-Play path versus
the strict standalone live-boundary API, compact million-play use, atomic
failures, exact allocator pools, partial materialized-gap lattices and counted
named calls. Three native model tests include the original failing entry
witness and independent outer-gap/default-body and live/final gap selections.

Two canonical PCM tests retain partial historical 44.1 kHz Source clocks,
materialized RoomTone/default-gap origins, silent suppression and chunk
independence through both outer Default and outer Play choices, reversed live
primaries and dormant-primary/live-counterpart changes. Dormant-only edits
preserve rendered samples and bindings. The store test retains the shortened
accepted provider and exact live/dormant extension preparations through one
history entry, Undo, Redo, reopen and full validation. The real-media test
executes all six live/dormant grow/shrink export cases.

All 3,465 broader tests pass in 149.448s, with four slow markers on concurrently
executed encoded-media tests and no captured-output marker. All 201 UI-feature
tests pass in 15.283s. The ordinary generated command-reference check passes.
Initial strict lint requests three equivalent Option question-mark expressions
in the cached path helper. After that cleanup, changed-package strict checks
and UI-harness Clippy pass with no warnings; all 199 affected normal app tests
pass in 15.556s. All 201 final UI-feature tests pass in 15.398s.
Formatting passes. The broad run precedes only that Option-syntax cleanup.
Both first-attempt native release replays pass all 27 owned-gap checks plus the
Kestrel compatibility check, in 141 visual and 405 performance frames. The
workflow selects real measured Freeze/RoomTone gaps through routed keys,
resizes two live gaps under an outer Default choice, isolates both under outer
Play 2 while preserving the other outer play, then edits a live gap beside its
final dormant counterpart. One Undo and Redo restore the whole transaction,
and the original private-package baseline is restored. No frame bound changes
or ignored inputs were needed.

The native primary is live: selecting plays sorts their positions. Explicit
dormant-primary continuation is established by the core and real-media CLI
contexts rather than by inventing a native selection route. Two final 960×640
captures were inspected: the two selected live gaps and the mapped outer-Play
result. Picture aspect, both clocks, focus, selected scope, Enter and the
duration control remain visible.

Quiet input-to-state p99 is 1.733ms (45 samples); input-to-commit p99/max is
82.083791ms (4 samples), and picture completion max is 8.377917ms (18 samples).
No timing sample fails or times out. Both replays retain a warning for ten
frames settling on a second layout retry; the reports retain their reasons.
These small scenario samples do not establish general performance acceptance.
The executed release app SHA-256 is
1cc26fb7638836ebe18f8eaf2291475e2398fecf1e8836ae36b05478c6cc098e.
The release export test passes in 28.278s. Six literal layouts contain
74/62/80/56/69/70 pictures: individual/shared grow and shrink, then dormant
primary plus live counterpart for each outer scope. Every emitted picture,
complete presented audio window and immutable provenance is verified:
411 pictures, 658,257 samples, 13 offset-observable audio windows, offset zero,
no uncovered samples, flags, low-SNR blocks or picture failures. The minimum
block SNR is 27.047203dB. All six independently retained nonzero Source windows
per fixture compare within 1e-6 at analytic moved entries, retaining each old
entry-relative sample offset. Owned and default outer-gap silence is checked.

There are 516 observable picture-index comparisons and 294 explicitly
unobservable comparisons for identical neighboring pictures. These remain
reported rather than being counted as evidence of unique picture identity.
Package, movie, report, helper and executed test-binary hashes are retained.

## Evidence and limits

Repository evidence:
[retained records](../../tools/media-qualification/evidence/2026-10-22-owned-gap-hold-duration-semantics).
The inventory hashes every retained file. Source snapshots include the baseline,
failed/ corrected stages and final 24 changed Rust files, with per-file hashes;
the baseline also records its base commit and isolated witness patch. Complete
reports preserve warnings, slow markers and actual emitted-file results.
Executed binaries and private packages remain in
/tmp/deadpan-owned-gap-hold-duration-next-2026-10-21, with their hashes and paths
in the retained records. Publication receipts and owner sign-in links are
excluded from repository evidence. The next fixed-Retime scope is a proposal
only; it is not included in this qualification's passing claims.

The host is Mac17,7, Apple M5 Max with 128 GiB, macOS 26.5.2 (25F84), Rust
1.97.1 and locked dependencies. Development tests use optimization level 2;
encoded and native qualification uses retained release executables. Tests
requiring this Mac or the synthetic artifact worker are required, not skipped.
The accepted-provider tests use an actually admitted synthetic artifact,
without claiming a new inference run. This qualification does not prove clean
machine behavior, physical input layouts, listening, VoiceOver speech, other
hardware or the remaining full-workflow and release requirements.
