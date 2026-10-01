# Edited slice interior insertion and replacement, 2026-09-30

`SpliceSliceAt` inserts a captured edited slice inside a named direct child;
`ReplaceSlice` replaces a nonempty range under an ordinary Sequence. Both use
one reversible command. The [contract](../EDITED_SLICES.md) records exact clocks,
ownership, bounded identity pools and historical media admission. Native edited
registers and placement controls, atomic move, role-only placement and temporal
occurrence interiors remain required.

## Core verification

On pinned Rust 1.97.1, 21 slice tests, 59 pause/insertion tests, 18 mark tests,
16 sound-routing tests and 12 sound-allowance tests pass. Strict all-target
Clippy for `deadpan-core` also passes.

The new cases cover chronological interior pastes, equal-length replacement
retaining sample support through old sample 4804, editing already copied nested
Partitions, conditional allocation at the maximum timing ordinal, joint
Split/import pool conflicts, mark-fragment budgets and one root sound/allowance
transform. Existing Source command admission remains unchanged.

The first slice run passed 18 tests; three new fixtures were rejected before
reaching the edit because their Source nodes had neither video nor audio. The
fixtures now use valid asset-backed video Sources. The corrected run passes all
21 tests. No production change was needed for those fixture failures.

## Decoded audio

All 15 edited-slice PCM tests pass against actual decoded fixture audio on the
pinned toolchain. They include the eight earlier seam cases and seven new
placement cases: shorter/equal/longer replacement, Source and RoomTone interiors,
previously edited fragments, nested copied Partitions, complete Repeat/Preserve
contexts and replacement across whole composite children. Comparisons retain
every destination sample and use independent scalar allocation/support oracles.

The first run passed 14 tests. One oracle incorrectly treated geometric Source
extent as complete discrete support. A diagnostic rerun retained literal plan
fields and reproduced the same failure; independent core inspection confirmed
the boundary. The original seven-frame Source owns samples `[0,11211)` on its
retained RootRoundEven grid. Inserting 14 frames at frame 5 allocates 3,204
samples to the surviving suffix `[19,21)`, whose mapping is `8008 + j`. The last
point maps to 11211 and must be silent even though the geometric endpoint is
11211.2. The corrected oracle retains all 3,204 output samples and asserts the
exact suppressed interval `[33633,33634)`. Separate tests still require real
continuation when retained support includes the extra sample, including old
sample 4804 in equal-length replacement. Production code was unchanged.

## Independent review

A fresh read-only review found no concrete defects in core placement, shared
splitting, compact historical renaming, conditional timing ordinals, resource
admission, marks/sounds/allowances, store historical recapture or durable/headless
test coverage. It rechecked the diff after the fixture corrections. The reviewer
did not run tests; execution remains separately recorded. A second read-only
inspection independently confirmed the retained audio support diagnosis above.

## Picture, persistence and command-line checks

Seven picture-plan tests pass. Three new cases compare every surviving picture
across strict interior insertion and shorter/equal/longer replacement, including
Source/Hold endpoints around complete Repeat and Preserve owners. Repeated edits
through copied nested Partitions retain original owner clocks. Independent PTS,
frame-center and Q32 framing oracles complement reverse-order frame comparisons.
These tests inspect the canonical plan and do not claim decoded GPU output.

Thirty-nine store tests pass across edited slices, source registration,
generation bundles and generated admission. Both new commands pass preview,
one-command persistence, exact authored Undo/Redo, stale rejection and reopen
checks. Invalid identity pools and exhausted clocks leave authored rows intact.
Historical Source and accepted Hold captures survive source deletion; forged
receipts/artifacts fail without writes, qualification counts stay unchanged and
detached generation requests remain detached. The generated fixture establishes
synthetic admission behavior, not AI footage or decoded generated pictures.

Two CLI tests pass, including serialized interior/replacement requests,
history-neutral dry runs, joint identity collision rejection, exact commit and
durable fresh-revision Undo/Redo. The store, CLI and picture runs share source
manifest `ab4c1da26bb1dacdc521dfc3f38575aed1e5d70cd2505fe88a3618cd4b0f3d44`.

## Integrated gate

All 2,739 locked workspace tests pass across 182 result groups, with none failed
or ignored. Formatting and strict all-target workspace Clippy pass on Rust
1.97.1. Formatting took 1.4 seconds, Clippy 606.4 seconds, and the complete
workspace test command 1,251.5 seconds including compilation and doctests.
The checks share source manifest
`ab4c1da26bb1dacdc521dfc3f38575aed1e5d70cd2505fe88a3618cd4b0f3d44`.
Every source hash was verified again after the gate.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-edited-placement/README.md)
includes exact commands, exits, timings, source manifests, host/toolchain
identity, compressed full logs, independent review and the initial fixture and
oracle failures. The host was Mac17,7, arm64, macOS 26.5.2, with the qualified
FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`. Core 34/database 43 are unchanged.

This backend checkpoint does not establish native UI, decoded GPU picture,
device/acoustic or emitted-file qualification. Native controls and keyboard
routing did not change, so no unchanged UI-feature run or native replay was
required. The user's separate Deadpan window and desktop were not inspected or
changed. No DP requirement or release gate changes status.
