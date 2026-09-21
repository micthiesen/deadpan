# Video admission qualification, 2026-09-21

This slice adds video container and codec-allocation admission before the native
source decoder. [Source admission](../SOURCE_ADMISSION.md) records the admitted
grammar, deliberate limits and remaining format work. Authored import, playback,
export and full product qualification remain open.

The run used Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1 and the pinned LGPL
FFmpeg 8.0.3 prefix at `/tmp/deadpan-media-compatible-xyhilms4/prefix`.
The [manifest](../../tools/media-qualification/evidence/2026-09-21-video-admission/source-manifest.json)
identifies the base commit, changed implementation and exact committed fixtures.

## Actual verification

- The repository gate passed formatting, workspace Clippy with warnings denied,
  **486 Rust tests**, workspace build and headless doctor. No tests failed or were
  ignored. [Commands and logs](../../tools/media-qualification/evidence/2026-09-21-video-admission/gate/report.json).
- **93 source/media tests passed under ASan/UBSan** with no failed or ignored
  tests. The instrumentation covers the selected C adapters and target C
  dependencies, not Rust or the separately built FFmpeg libraries.
  [Report](../../tools/media-qualification/evidence/2026-09-21-video-admission/sanitizer/report.json).
- All ten committed MP4 fixtures and seven Matroska fixtures pass container
  allocation admission. Semantic negatives still fail native interpretation
  checks for missing color tags, HDR, unsupported depth or interlace.
- Real native tests preserve exact RGB pixels, visible frame identities, B-frame
  ordering, VFR/nonzero PTS, reverse seeks, rotation, sample aspect and YUV range.
  Tests cover padded H.264 coded geometry, 4,097 NAL-unit packets, escaping NAL
  lengths, packet bytes, retained first-frame counting and seek replacement.
  Audio regressions and independent shared-opening-budget tests also pass.
- Six [headless source probes](../../tools/media-qualification/evidence/2026-09-21-video-admission/source-probes/results.json)
  reopened earlier software/hardware-encoded H.264 fixtures and the actual
  accepted generated FFV1 master. Each built a measured index, performed random
  seeks and retained pixel hashes after decoder destruction. These are small
  qualification assets, not full-size playback performance measurements.

FFV1 tests cover all committed configurations, all prefix truncations and
single-bit corruptions, coder types 0/1/2, stored initial states, CRC-repaired
malformed syntax, excessive slices/context state and bounded range-decoder work.
An independent public FFmpeg API harness successfully opened synthesized legal
22-byte and 24-byte configurations which the guard rejects for excessive
allocation expansion. [Inputs, harness and observations](../../tools/media-qualification/evidence/2026-09-21-video-admission/ffv1/deadpan-ffv1-admission-evidence.md)
are retained. A legal configuration can exceed this admission policy; rejection
does not imply a malformed FFV1 file.

Matroska tests cover late metadata, finite parent extents, sparse oversized
blocks, cue/seek pointers into payload bytes, duplicate/unqualified elements,
cancellation and read/list bounds. A 12,000-block sparse file stays below 1 MiB
of actual admission reads.

## Review findings and iterations

The independent general review found no additional actionable defect after
reading the admission/decoder paths and callers, and independently running the
source and media test packages. Matroska magic detection rereads some bytes;
both reads are real work and deliberately count toward the shared I/O allowance.

Independent codec review checked FFV1 field order/allocation formulas and H.264
callback ordering against pinned FFmpeg. It found no remaining codec-boundary
defect in the reviewed scope.

Independent Matroska review found metadata expansion missed by physical byte and
element limits. Nested default-language tags make FFmpeg recursively visit each
child twice. Small generated files produced 3, 255 and 1,023 dictionary entries
for depths 1, 7 and 9. The guard now requires flat SimpleTags and caps their
aggregate count at 1,024. The regression failed before the fix and passes after
it. [Reproduction files](../../tools/media-qualification/evidence/2026-09-21-video-admission/matroska-tags/)
and [iteration logs](../../tools/media-qualification/evidence/2026-09-21-video-admission/iterations/)
are retained.

A separate native batch with depths 1, 10 and 13 was terminated after roughly
34 seconds without completing. Its buffered output does not identify which
depth was active; no individual-depth timing is inferred. An automated cyber-risk
filter later stopped that review agent during an additional demux-resynchronization
investigation. That hypothesis was not established or counted as a passed check.
The agent had already confirmed the flat-tag fix. Review dispositions are recorded
in [review.json](../../tools/media-qualification/evidence/2026-09-21-video-admission/review.json).

The first native test run retained two expected assertion failures because
playlist and packet-budget failures now occur before/within controlled opening.
The first full gate also caught a changed Matroska-audio error code introduced by
the shared guard. The implementation now preserves `unsupported_container`; its
existing test was kept. Final gate and sanitizer logs follow these corrections
and the tag-expansion fix.

## Limits

No native controls, startup or focus behavior changed, so GUI aesthetics,
keyboard navigation and startup were not repeated for this decoder-only slice.
Earlier [native preview evidence](source-preview-2026-09-21.md) retains its original
scope. Python qualification harnesses were not rerun because their implementation
did not change; the Rust gate executes the affected actual decoder paths.

This is a closed, pinned admission profile with cooperative deadlines. It is not
an OS-enforced memory limit, a preemptive decoder timeout, complete malicious-file
coverage or qualification of the separate conversion worker's every input path.
Other legal container variants/codecs and the full required format matrix remain
open, along with project import, native dialogs and measured large-media behavior.
