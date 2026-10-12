# Sound occurrence delivery windows, 2026-10-12

Partial attached-sound edits need to select previously processed output without
restarting its source, Preserve stages or effect history. Whole-placement sound
clocks already preserve rounded sample labels through moves. The sampled route
now also supports a chronological selection and a separately retained rounding
anchor at each placement.

`AudioSoundRoute::with_occurrence_windows` accepts one `AudioOccurrenceWindow`
per retained grid in a checked full identity chain. Selections use the unchanged
complete recipe coordinates; the first must retain that entire recipe. Each
later selection clips physical delivery only. The next read intersects every
intermediate allocation, so a later wider selection cannot resurrect previously
clipped samples. Positive exact selections may contain no physical samples.

Each transition copies old integral sample labels using its declared anchor on
both grids. Frame displacement alone cannot recover that phase at fractional
frame rates. The anchor may differ from the exposed preparation allocation,
as required when an owner exposes hidden effect history behind an exterior crop.
The existing production `with_identity_anchor` path uses this same implementation
with full windows and the same anchor at every step.

Routes keep their complete source/DSP input and its original sample grid.
`AudioRoutedRoot::occurrence` still validates that capture. A window grants no
source admission, Hold permission or ability to substitute another occurrence.
The existing route node, query-work and span limits remain in force. No schema,
resource ceiling or user control changes.

## Evidence

An independent dense sample-label oracle covers three sample spacings, including
30000/1001 timing, five positive/negative phases, two crops, changed anchors,
further moves and an expanded final allocation. Complete and shuffled reads
match. Separate cases cover disjoint and sampleless selections, bounded query
work, malformed windows, incomplete history, nonidentity chains and overflow.

A decoded-media test selects two Repeat occurrences through nested 3/2 then 2/1
Preserve stages. Its reference prepares both complete stages independently, then
selects their output. Moved/cropped output and cold seeks match exactly. Hidden
prefix and suffix input remain available at their original labels. Revoking the
source fails even for a clipped, silent output query.

The first media test requested blocks larger than the 256-frame reader limit
and failed with `Range`. It now reads within that existing limit; production
limits did not change. The successful focused run is retained alongside the
failure. All 1,121 plan/audio/playback tests pass with none ignored. Formatting
and strict workspace/all-target Clippy pass. All four new route regressions,
the decoded-media regression and the app/CLI build pass in release.
[Retained evidence](../../tools/audio-qualification/evidence/2026-10-12-sound-occurrence-windows/README.md)
includes command logs, the implementation patch, source and executed-binary
hashes, failure evidence and the self-review.

## Remaining integration

This implements the sampled transport needed by partial attached-sound edits.
The editor still refuses those copies. `sound_clocks::placements` must derive
every chronological visible interval and delivery anchor, including treatment
input contexts. The gate path must transport the original envelope with the same
sample shifts and apply current Hold policy afterward. Core correspondence must
admit only proven transparent crop ancestors before capture can remove its
guard. Recopy, repeated moves, store reopen and Undo/Redo need command-created
regressions before that editing behavior can be claimed.

The primary agent implemented and self-reviewed this work alone. No independent
review, live GUI, physical audio, listening check or new encoded-export evidence
is claimed. No DP requirement or release gate is newly complete.
