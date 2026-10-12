# Sound clocks across existing Repeat Splits, 2026-10-12

RepeatSelection can now repeat a partial range inside an existing sound-bearing
Repeat when Split retains the complete processing input. The surviving fragments
keep their full sound recipes and exact old play clocks. The original implementation
refused every Split owner with preexisting external Repeat ancestry because it
tracked only owner aliases.

## Correspondence

The structural reducer already clones the complete input behind disjoint delivery
Partitions. The sound transport now records Repeat aliases from the same exact
Split mapping, composing successive clones back to their pre-edit identity.
Before restoring an owner, it checks the complete ordered external Repeat ancestry.
Only the newly introduced wrapper may be absent from the old ancestry.

Every prior journal is composed through these aliases. A retained introduced-play
cohort is retargeted to its cloned live Repeat without extending its saved stable
IterationIds. Shared Repeat placements map through the previous historical proof.
Initial plays therefore keep their historical input; later births and shrink/regrow
remain independent. The existing proof still checks full processing structure,
exact owner provenance, eligible concrete paths, chronological timing and work
bounds. Alias count is bounded by the existing document-node limit. No serialized
schema, processing threshold or resource cap changes.

Root-owned sounds and changed processing scopes still refuse. Repeat-owned sounds
cannot change their complete processing extent through count edits. Pause insertion
inside Repeat retains its separate structural refusal; this increment does not
remove that restriction. Unsupported partial composite captures remain guarded.

## Verification

The original core regression fails against parent `1e8baec9` at the exact ancestry
guard. A retained trace identifies two successive clone mappings for both owner
and external Repeat. The final code contains no trace output. The old release
also fails replay of the real fixture's newly supported command during project
open. The updated binary opens the same history; before/after snapshots prove
that the refused read did not change the authored document.

Core regressions verify exact inverse/JSON roundtrip, complete recipes, old
cohorts, preexisting later-born plays and post-Split shrink/regrow identities.
Four PCM regressions cover dry and equalized Sources and nested Preserve, with
both event and owner EQ. They compare initial fragments, retained plays, new-play
phase, silent gaps, whole-Repeat copy/recopy and copies after deleting the original
Repeat. Dry comparisons remain exact; treated comparisons retain the existing
1e-6 bound. Each moved play uses its own origin-based sample boundaries.

Store regressions force a history-write failure and require unchanged snapshots
and row counts. Successful preview/commit, writable/read-only reopen and Undo/Redo
with fresh revision IDs preserve the complete journal and recipes.

All 2,880 core, planning, audio, playback and storage tests pass, with none
failed or ignored. Strict workspace Clippy includes all targets and `ui-harness`.
The four new PCM regressions, both atomic-store Split cases, new 17-picture
export and prior 13-picture export also pass in release. The app/CLI release
build and formatting pass.

The public CLI fixture uses qualified Original MP4 and registered stereo WAV,
independent attached sound, event/owner EQ and historical copies. It repeats
Edit 1..3 inside an existing two-play Repeat, adds a third play, inserts silent
gaps, then performs durable Undo/Redo. Independent nonzero PCM windows keep the
old and newly born plays through the gap edit. Debug and release exports verify
all 17 pictures and 44 audio blocks, zero signal offset, minimum block SNR
18.3042629508 dB and maximum level difference 0.1521896472 dB.

After all Cargo work ends, the final release executable reproduces five nonzero
256-pair authored windows bit for bit against the retained qualified debug
executable, renders the same committed revision, and passes emitted-file
verification without changing its authored document. Final CLI/app identities
are retained alongside source, fixture and test binary hashes.

## Retained failures and scope

The first PCM copy oracle compared a concatenated multi-play interval. At NTSC,
moving a play may change its sample allocation by one, so later comparisons used
unrelated coordinates. Per-play origin boundaries correct the oracle without
loosening any sample tolerance. The first store fixture requested the separately
unsupported InsertTime path and was corrected to the intended RepeatSelection.
An initial old-binary comparison incorrectly expected it to open history containing
the newly supported command; the final record identifies the historical-replay
refusal explicitly. These attempts and the iterator compile error are retained.

The primary agent implemented and self-reviewed this change alone. No live GUI,
device output or listening ran. This is qualification on the current M5 Max with
the pinned FFmpeg build. No DP requirement or release gate is newly complete.

[Evidence](../../tools/audio-qualification/evidence/2026-10-12-repeat-split-clocks/README.md)
retains successful and failed checks, commands, source/binary identities, complete
projects, media and reports. Its checker validates hashes and exact relationships;
`--source` additionally checks the current source files.
