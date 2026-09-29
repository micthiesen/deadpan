# SDR encoder timing qualification

The pinned FFmpeg 8.0.3/native AAC path does **not** satisfy the current
no-edit-list export timing contract at 60 fps. Disabling edit lists moves all
six measured stereo events and the meaningful stream endpoint 1,024 samples
late, or 21.333 ms. One frame is 800 samples. Default-edit-list references retain
exact event positions and authored stream endpoints. This finding does not
authorize changing the specification, shifting the source, discarding a priming
packet or widening the tolerance to an AAC block. DP-17 remains open.

The [developer harness](../../tools/media-qualification/compatible/README.md#sdr-encoder-timing-experiment)
and [retained evidence](../../tools/media-qualification/evidence/2026-09-28-encoder-timing/README.md)
record the inputs, commands, failures, media, source identities and limits.
There is no Render control or product encoder integration in this increment.

## Measured cases

All pictures are numbered 320×180 limited-range Rec.709 YUV420. Input includes
five color patches and five independently checked neutral OETF patches. The
canonical input audio count uses one origin-based ties-to-even boundary at
48 kHz. Movie timescale is the checked LCM of the rational video numerator and
48,000; output uses fast-start and explicit square-pixel/progressive signaling.
The encoded tolerance is recorded per case as the largest integer strictly
below a video frame. Exact-sample equality is reported separately.

| Case | Actual result |
| --- | --- |
| Hardware, no B, 120 frames at 30000/1001, default edit lists | Scoped reference checks pass; audio events and stream end exact. |
| Hardware or OS software, no B, same input, edit lists disabled | No `edts`/`elst`; all audio events and stream end +1,024 samples. Below one frame at this rate, but insufficient to qualify the path at 60 fps. |
| OS software, two B requested, edit lists disabled | One actual consecutive B-frame; picture PTS and terminal boundary shift by one video frame. Fails exact picture timing. |
| Hardware, two B requested, edit lists disabled | Explicit mux rejection for PTS preceding DTS. No silent fallback. |
| Hardware, 120 frames at 60/1, default edit lists | Exact audio events and stream endpoint. |
| Same 60 fps input, edit lists disabled | All six events +1,024 samples and endpoint +1,024. Fails the declared 799-sample tolerance. |
| Hardware, one frame at 60/1, edge content, default/disabled | Reference stream end exact; disabled end +1,024 and fails. Overlapping marker windows are explicitly unqualified for event timing. |
| Hardware edge content at 30000/1001 and 32 frames at 25/1 | Default references exact; disabled events/end +1,024. Paired AAC payloads are identical, and unshifted opening/closing PCM differs. |

Ordinary and manual-skip decoding agree on the no-edit-list displacement.
At 30000/1001, the reference ordinary decode covers samples `[0,192512)` for
192,192 authored samples; manual decode also retains the leading interval
`[-1024,0)`. The 320 physical trailing samples are distinct from the exact
signaled stream endpoint. Without edit lists, both modes cover `[0,193536)`,
with stream end 193,216. At 60 fps, the disabled stream end is 97,024 for 96,000
authored samples. No observation aligns or crops PCM using detected impulses.

Hardware no-B GOP intervals are 16 frames at 30000/1001 and 30 at 60 fps;
software intervals are 15 at 30000/1001. Key flags, NAL types and IDRs are retained
observations. They do not prove independent closed-GOP decoding.

## Review and corrections

The initial 13-case run retains two overly strict video checks. Hardware decoded
frames report unspecified SAR `[0,1]`, while the MP4 stream explicitly reports
`[1,1]`. The corrected probe retains both; the oracle uses valid stream evidence
only for unspecified frame SAR. Missing, malformed or nonsquare evidence fails
without suppressing independent identity/timing checks.

The MOV muxer converts Annex-B H.264 NAL framing to length prefixes. Raw video
packet hashes therefore need not match across muxing and remain diagnostics.
AAC packet equality remains required. All encoded bytes from the initial run
were preserved; only video observations were decoded again for this correction.

Independent review also found missing corrupt-frame flag checks, library-symlink
admission gaps, source attribution recorded only after execution, and process
faults that could hide in an unselected encoder mode. These are corrected with
regressions. The parent checked the pinned header's actual corrupt/key flag
values, 1 and 2, before running the follow-up tests. Native key flags cannot be
mistaken for corruption. Final independent review reported no further finding.

## Verification and provenance

Base commit: `1c509da1b44415daae98a711961aab2ad0d8252f`.
Host: Apple M5 Max, 128 GiB, macOS 26.5.2; Apple clang 21.0.0, SDK 26.5.
The actual build receipt is the September 26 receipt for
`/tmp/deadpan-ui-ffmpeg/prefix`, SHA-256
`baf7437c0cf4a61c94d75efd92db50b5ed3a96b2fa8f1e891284bdcf7c3d78c7`.
It is not the older September 20 prefix. Seven dylibs, 26 build logs and 9,893
archive source files match their retained evidence. All 143 installed headers
match that source; ffprobe matches the build-tree executable. Header and ffprobe
hashes are fresh observations, not invented historical receipt fields.

- Initial source inventory `e4b1e649bd8c9446824f9ec0a2d8091206f7d6f385bbb9d8d6b3a43a2ffd4368`:
  52 Python tests pass in 2.71 seconds; the 13-case native matrix completes in
  18.05 seconds and exits 1 with the retained failures.
- Final source inventory `223f4fa376615e3afe42110a231a02ffa62903d614d5785e74f7b2fe6d842b74`:
  58 Python tests pass in 3.89 seconds. Video-only re-observation takes 6.56
  seconds, preserving all original MP4/AAC bytes and audio/packet observations.
- The original compatible CFR fixture still passes all 40 assertions after the
  minimal shared C helper hooks, in 2.24 seconds.
- Strict `-Wall -Wextra -Werror` C compilation passes. ASan/UBSan instruments
  the probe and completes all 13 cases in 32.24 seconds, with no sanitizer,
  signal, timeout, launch or unexpected observation failure. Its overall exit
  remains 1 because the selected path fails the encoded timing matrix.
- The corrected and sanitizer reports each retain 1,267 checks: nine cases pass
  their scoped checks, three fail timing, and hardware B encoding is a separate
  negative capability. These overlapping populations are not summed as unique
  tests. The short-clip event ambiguity remains explicitly unqualified.

Inputs, sources, real dylibs and their load-path symlinks are rechecked after
execution. All rechecks pass, and no source changed during a run. Loader
overrides are removed. Full logs survive failures. Sanitizers do not instrument
FFmpeg or Apple frameworks, and leak detection is disabled on this macOS path.
No Rust or GUI code changed, so the completed workspace/UI gates were retained.

## Source explanation and remaining work

The pinned AAC encoder declares 1,024 initial samples; its audio-frame queue
starts the first packet at −1,024. MOV normally represents that through an edit
list. With that list omitted, the measured demux path starts its index at zero.
The muxer's AAC `roll` sample groups do not supply arbitrary leading/trailing
sample counts, and this FFmpeg demuxer does not interpret them as an exact trim.
The retained source note records symbols, line references and source hashes.

The subsequent [AVFoundation comparison](native-audio-2026-09-28.md) reads the
same files with retained raw/output timestamps and trim attachments. It finds
a different no-edit-list failure: missing opening events and later events
1,088 samples early. The default-edit-list reference aligns in both readers.
That later evidence does not replace this original FFmpeg result. On 2026-09-28
the user separately approved permitting edit lists for verified encoder delay,
padding and frame reordering; the [decision record](native-audio-2026-09-28.md#export-policy-decision-and-remaining-scope)
preserves emitted-file timing verification. The approval does not turn the
failed no-edit-list files into passing results.

Fresh-decoder closed GOPs, full boundary-content and acoustic quality, the shared
renderer-to-Rec.709 encoder transform, immutable project rendering, final-file
publication, HDR, full-resolution performance, the supported OS matrix and the
one-action native workflow remain required.
