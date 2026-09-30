# Independent review of the five-second oracle correction

Reviewed source and retained evidence on 2026-09-30. No blocking finding in the
capacity extension or the complete-file comparison. No repository files were
changed, and no preparation scripts, tests, builds, decoders or native programs
were run for this review.

## Capacity changes

Direct diffs against the three repository files agree with the retained diffs:

- `native_audio_oracle.py`: `MAX_SAMPLES` changes from 192000 to 240000; the
  positive exact asset-duration ceiling changes from four to five seconds; two
  corresponding diagnostics change.
- `project_encode_oracle.py`: only the positive authored reference-count ceiling
  changes from 192000 to 240000, plus its diagnostic.
- `avfoundation_probe.m`: only `MaximumPCMFrames`, the exact positive CMTime
  duration ceiling and its diagnostic change.

The retained originals compare identically with the repository files, and the
repository diff for those files is empty. `edits.json` and `manifest.json` retain
the exact unique replacements and original/modified/diff SHA-256 values.
`prepare.py` validates pinned source hashes, applies each unique replacement,
and refuses to replace differing retained evidence. I reviewed that mechanism
and the recorded hashes; I did not rerun hash validation.

No picture, PCM, PTS, format, attachment, record-count, reader-status or hash
admission rule is relaxed. Picture bounds remain maximum error 48, mean absolute
error 1.5 and mean squared error 16. PCM bounds remain maximum error 0.25 and RMS
error 0.02, with exact silence required for a wholly silent reference. The
five-second limit remains finite and rejects a 240001-sample authored fixture.

## Complete-file comparison

`qualify-decoded.py` places the scratch directory first on the fresh process's
import path. It builds a separately named AVFoundation reader from the scratch
source and retains that binary and the modified source hashes. The original
reader and original failed report remain separate.

The script requires the captured and direct-input contracts to agree, fixes the
case to 128 frames at 30000/1001 and 320x180, and requires the authored sample
interval `[0, 205005)`. It admits exact picture and PCM reference extents, checks
the movie against its manifest SHA-256, reads all 384 picture planes, and checks
EOF on both picture files. Both channels of all 205005 authored sample frames
are compared for ordinary FFmpeg, manual FFmpeg and AVFoundation output.

`compare_pcm` requires contiguous physical offsets beginning at zero, exact
contiguous reported sample PTS, complete authored coverage and final physical
coverage equal to the entire decoded PCM extent. It rejects omitted endpoints,
gaps, overlaps and unaccounted bytes. Physical priming/padding outside the
authored interval remains retained, finite and mapped at its reported PTS; it
is not compared against invented reference samples outside the authored range.
That distinction is unchanged from the original oracle.

The native reader keeps the default full-asset range and writes every returned
PCM buffer. `_adapt` requires integral absolute sample PTS, identical raw/output
timing, exact buffer durations and valid format/attachments. The parent requires
native track start zero and duration 205005. There is no PCM realignment,
timestamp rewriting, truncation, gain adjustment or fabricated decoder result.
Synthetic observations exist only in the explicitly pure unit-test fixtures.

## Evidence limits

The initial `decoded-bound/report.json` records `PCM fixture exceeds four
seconds`, raised by the original `compare_pcm`, with no process faults. It stops
at that exception; it does not establish later manual or AVFoundation results.
The separate five-second run must supply those observations and pass every
comparison and final artifact admission before a full-file success is claimed.

The eight pure test methods cover exact source edits, unchanged two/four-second
results, complete 205005/five-second intervals, over-cap rejection, format/trim/
PTS/status/hash/nonfinite failures, missing or overlapping coverage, unchanged
fidelity/no-alignment behavior and loss of the final authored sample. Their
source is meaningful; this read-only review does not claim test execution or a
new native qualification result.
