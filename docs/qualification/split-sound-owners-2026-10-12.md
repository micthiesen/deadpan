# Attached sounds across owner splits, 2026-10-12

Eligible interior pauses, partial deletions and range moves now retain attached
sounds on every surviving fragment. Previously the sound transport detached
the events before Split, restored only the original owner and rejected the
changed crop. Removing that refusal alone would have lost the copied suffix.

The outer command now passes its sound-clock capture through the existing
scoped edit context. Split reports its exact old-to-new node map. The capture
records each sound owner's original identity, including successive copies of
a copy, and restores only aliases still present in the completed tree. It
checks the temporary attached-sound count before expanding the alias set.
No schema, command wire shape or DSP implementation changes.

For these aliases, transport narrows the retained scope to the complete
processing owner and proves correspondence with each historical layout. The
pre-edit placement is retained even for an unmoved left fragment: its new
Partition changes delivery, while processing still needs full input. Old
journals remain chronological. Imported events are merged independently, and
equal-shaped siblings cannot substitute for the recorded owner. Unrecorded
crop changes retain their refusal.

## Verification

Core regressions cover successive pauses after an earlier move, narrowing an
older group scope, removal of the original physical owner during partial
deletion, exact inverse patches, JSON round trips and atomic sound-limit
failure. Partial Repeat wrapping still rejects atomically because external
Repeat occurrence mapping for split owners is not implemented here.

Audio regressions compare independently rendered pre-edit windows after two
pauses, a two-ended deletion and a range move. They exercise dry attachments,
event plus owner EQ, and Original audio mixed with the attachment through
shared EQ. Dry PCM matches exactly; treated PCM uses the existing 1e-6 bound.
Inserted Hold intervals contain zero PCM. The existing nonlinear/EQ automation
test now exercises attached splits as well as its root-event route and compares
arbitrary reads against a complete render.

The first new PCM oracle used frame-position differences for its second pause.
At 30000/1001 fps, the two actual insertions each shift 1,602 samples. Their sum
is 3,204, whereas the selected frame boundaries differ by 3,203. Correcting the
oracle to compose the actual integral shifts resolves both dry and EQ failures;
no tolerance or production audio code changed to accommodate them.

A real-media store test forces history insertion to fail, checks unchanged
snapshot and row counts, then commits the previewed transaction. Both sound
owners, complete recipes and historical owner identities survive reopen,
fresh-revision Undo/Redo and read-only validation.

The CLI fixture begins with copied and recopied stereo WAV attachments, each
with event and owner EQ, after deleting the original owner. Two further pauses
split one copied owner into three. All three pre-edit comparison windows contain
nontrivial PCM. The first fixture attempt selected a window beyond the WAV's
8,197-sample end; the corrected fixture moves the cuts earlier instead of
weakening the nonzero check. Debug and release export verify all nine pictures, 21 audio
blocks and a zero-offset signal window. Its minimum block SNR is 2.9190013602 dB
and maximum block level difference is 1.1074020624 dB under existing thresholds.
This is timing and decoded-output evidence, not listening qualification.

The prior qualified binary at `743b349d` refuses the same interior-pause request
against the retained seven-frame fixture without changing its document. The
new binary accepts that request and retains all three sound owners. Both binary
hashes, request, dumps and responses are retained.

All 2,867 core, plan, audio, playback and store tests pass, with zero failures
or ignored tests across 146 result groups. Strict workspace/all-target Clippy
with `ui-harness`, three release PCM regressions, the release store regression,
release export, app/CLI release build and formatting all pass. The final release
CLI also passes the old/new request comparison.

[Retained evidence](../../tools/audio-qualification/evidence/2026-10-12-split-sound-owners/README.md)
includes command receipts and logs, failures, source and binary hashes, the full
patch, old/new comparisons, exported MP4, project package and self-review.

## Remaining scope

Split owners with external Repeat ancestry, root-owned attachments, changes to
an enclosing processing subtree, and standalone cloning that omits a retained
processing scope still have explicit guards. Range endpoints still require
Source, ordinary Hold or supported fragments. Native attachment placement,
broader attachment policies and full mastering qualification remain open.

The primary agent implemented and self-reviewed this increment alone. No live
GUI, device output or listening check ran. No DP requirement or release gate is
newly complete.
