# Encoded project qualification source review

Reviewed `qualify_project_picture.rs`, its new `encoded.rs`, the new project
oracle and tests, `export_probe.c` changes, and the subsequently added
`qualify_project_encode.py`. Source only. No builds, native commands, tests,
formatting, or repository edits performed.

## Findings

The findings below describe the original reviewed source. Both are resolved
in the follow-up source review recorded at the end of this note.

### P2: Conflicting SAR observations can pass the video oracle

Location: `tools/media-qualification/compatible/project_encode_oracle.py:26-34`.

`_effective_frame_sar` returns a positive frame SAR directly. When every frame
reports `[1, 1]`, changing `stream_sample_aspect_ratio` to `[2, 1]` does not
change any check. `codec_parameters_sample_aspect_ratio`, which the new probe
reports, is never checked. This can accept conflicting container and decoded
aspect evidence despite the square-pixel output contract.

Require every explicit valid SAR declaration to agree with the square effective
SAR, while retaining the documented missing-frame fallback to a valid stream
SAR. Add negative mutations of each header declaration. Parent acknowledged
this finding and is implementing the fix; this review did not execute it.

### P2: The new runner reads unbounded report and artifact inputs before admission

Location: `tools/media-qualification/compatible/qualify_project_encode.py:145-152`
and `:30-35` in the reviewed source.

`json.loads(args.picture_report.read_text())` materializes the entire
caller-selected report before any size or regular-file check. The required
case-name set check does not require exactly seven records, so an arbitrarily
large array of duplicate named cases passes that admission and triggers an
unbounded number of qualification commands. Candidate/reference paths are
also opened and completely hashed through inherited `artifact()` before the
later expected-length checks; this accepts FIFOs/devices into an unbounded
read and scans arbitrarily large regular inputs.

Admit the report as a bounded regular file before reading it, require exactly
seven unique records, and admit the candidate/reference file types and bounded
extents before hashing. Bind I420 and PCM reference sizes to the captured
contract before reading them. Parent received this finding.

## Integration note

`encoded.rs:210` calls `Gpu::frame`, which still asserts exactly 30000/1001
inside `qualify_project_picture.rs:422-448`. The marker project derives its
basis from the supplied source. The runner's marker generator therefore must
use that exact rate, or the reference helper must be generalized before adding
60 fps markers. At this rate a 120-frame marker also exceeds the new PCM
comparison's 192000-sample cap; at most 119 frames fit. This was sent as a
fixture integration constraint, not a false-pass finding.

## Other reviewed behavior

- Every retained reference picture is produced from the exact committed
  revision/range. Absolute project frame `range.start + ordinal` is converted
  back to the expected output ordinal by the shared helper.
- Direct PCM begins at `B(start)` and ends at `B(end)`. `RangeCaseSpec` computes
  their difference, preserving the 1601-sample nonzero NTSC interval.
- The runner checks the complete I420 extents and compares each plane of every
  ordinal. Decoded PTS/duration and geometry are checked independently.
- PCM comparisons use observed sample PTS, retain physical priming/padding,
  and compare only the authored interval. Marker events additionally require
  exact independently authored positions in canonical PCM and all three
  readers. No event-derived shift is applied.
- Fresh GOPs use new decoders and full suffix PTS/duration/hash comparisons.
- Live mutation and cancellation fixtures require progress strictly between
  zero and total frames; the byte-exhaustion case requires a native limit
  diagnostic; a later successful attempt exercises recovery.
- Probe packet/frame/decoded-plane counts and geometry are capped. The new
  path uses the existing developer probe's stream-probing setup; this review
  does not establish adversarial FFmpeg allocation or process containment.

No runtime pass or qualification claim follows from this source review.

## Follow-up disposition: source review complete and frozen

- SAR finding resolved: `inspect_video` now requires both explicit stream and
  codec-parameter SAR to equal one before checking each effective frame SAR.
  Negative tests independently mutate both headers. The existing missing-frame
  fallback still uses the validated square stream SAR.
- Report/work bounds finding resolved for the scoped fixture runner:
  `read_capture` opens nonblocking with no symlink following, requires a regular
  file of at most 32 MiB, caps the read, checks observed length and metadata
  stability, and requires exactly seven distinct expected cases. Candidate and
  reference admission precedes hashing and checks regular-file type, hard
  byte bounds, and captured exact lengths. Added tests cover oversized reports,
  missing/duplicate cases, wrong artifact extents, and symlinks.
- The reference timing helper now derives PTS, frame duration, and time base
  from the captured rational frame rate. The previous hardcoded NTSC constraint
  no longer applies. The explicit four-second PCM fixture cap remains enforced
  before reference hashing.
- The marker case additionally invokes the bounded visible-number decoder and
  requires the complete decoded identity list to equal `range(frame_count)`.
  This rejects missing, duplicated, and reordered visible ordinals independently
  of the full-plane comparisons and the existing exact timestamp checks.

No remaining actionable issue found in this focused follow-up. This disposition
is source-only; parent owns the native run, test execution, and result claims.

## Final SAR clarification

The final oracle permits the probe's real missing codec SAR value `0/1` to
inherit the validated explicit stream SAR `1/1`, recording that origin. It
does not require a missing codec declaration to become an explicit declaration.
Any present codec or decoded-frame SAR that contradicts square pixels rejects;
the stream SAR must itself be valid and square. The tests include the missing
codec case and independent contradictory stream/codec header cases. This
clarifies the earlier follow-up sentence about requiring both explicit headers.

Parent reports that the final normal and sanitized decoder runs passed. This
reviewer verified the source behavior only and did not execute those runs.
