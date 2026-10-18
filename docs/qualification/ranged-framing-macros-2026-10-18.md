# Ranged framing macros, 2026-10-18

Ordinary ranged Zoom and Creep record spatial intent and resolve it again on
invocation. This advances DP-06 and DP-21. One-play framing authority remains
open; this slice does not make ordinary macros edit a Repeat definition on
behalf of one play.

Base revision `14703b44`, Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1,
and the qualified FFmpeg 8.0.3 prefix.

## Intent and geometry

`Zoom { kind, center }` stores scale, optional starting scale, eased curve and
Keep or OriginalCenter. Each staged instruction resolves its selected direct
child and current exact Time or Object range. Recorded owner identities,
range fractions and sampled target points are absent. Backward selection
orientation survives. A preceding duration edit or nested call changes the
range's current owner duration before construction.

The former native builder is now shared pure core code. Outside-range static
poses remain intact for a smash; a creep holds its arrival pose. A path or
follow refuses when its outside behavior cannot be preserved. Whole-beat
presets retain their existing SetFraming behavior. Sampled selected, current,
named and face targets refuse ranged recording before target analysis; they
remain available interactively outside recording. An implicit selected target
cannot silently become Keep when a saved target or follow exists.

The host evaluates the staged render plan and projects the Original center
through the same composed geometry and Q32 quantization as native Camera.
Source geometry comes from exact saved qualification, measured PTS and
interpretation. Accepted Generated geometry comes from the retained admitted
native plan, sampling map, actual asset records and exact sampled PTS. Shared
metadata helpers preserve source interpretation and generated aspect cropping
in both decoded presentation and decoder-free resolution. The resolver opens
no media, creates no decoder or picture raster and reads no proxy. Missing,
clipped or unqualified geometry refuses truthfully.

Native pending framing retains its full entry capture. A successful ordinary
ranged Apply establishes invocation-relative dot intent. Recording waits for a
pending exact picture before accepting another action or saving. Escape and
record-cancel revoke that request before it can submit a delayed edit.

## Tests, review and retained failures

Core coverage checks forward/backward Time ranges, Object ranges, boundary
poses, staged duration changes and nested calls, exact inverse, refused late
steps, no-op allocation, strict wire shapes and bounded scales. A real nested
FollowSpeed pause regression retains an integral root duration of 163 frames
with fractional inner clocks, then frames the outer owner without rounding.
An initial review concern about the public root-duration query was refuted by
this regression; no duration-query change was required.

Six host tests cover qualified SAR, rotation, aperture and measured PTS,
nested and captured Hold geometry, a clipped center, live Follow through
retiming, default-gap offsets and tampered or unsupported providers. Resolution
also succeeds with linked media unavailable. Generated contract tests verify
raster, sampling, crop, PTS and refusal checks without decoding.

Four CLI integration tests use real source registration, dry-run/commit parity,
changed owner durations, backward ranges, nested calls, staged Hold edits,
atomic late range/path/provider refusals, unchanged bank and marks, and full
baseline Undo/Redo after reopening. The accepted-generated case runs actual
conditioning, durable attempt lifecycle, media-worker qualification,
publication and acceptance. Only inference is synthetic. It compares decoded
accepted metadata and projected center against semantic framing. It does not
qualify MLX inference.

The first generated test run failed during its direct store Undo, after
successful generation and framing. That writer omitted the generation context
resolver used by production CLI history. Installing that resolver on both
writer opens corrects the fixture without weakening generation relevance.
The failed log is retained.

Independent review found a pending whole-beat Zoom could survive Escape when
there was no Visual selection. Idle Escape now cancels that pending entry.
The replay holds a real stopped-picture reply, checks recording save refusal,
cancels, then releases it and verifies the document and register bank remain
unchanged for ordinary and recording cases. Final read-only review found no
supported issue and ran no competing Cargo process.

## Scoped gate and native replay

The final scoped `cargo xtask check` passes strict normal/UI Clippy and all
111 selected tests. The UI-harness runtime configuration also passes all 111.
These overlapping selections are not a full workspace gate. The initial normal
run reported one leaked output handle on an existing macro copy test; its
isolated rerun passes cleanly. The final normal run has no leak warning. The
final UI run reports one on the existing native composition/held-key test;
its isolated rerun passes in 0.017 seconds without the warning. These are
retained harness diagnostics, not failed assertions.

Debug app linking reports `__eh_frame section too large (max 16MB)` for compact
unwind offsets. No warning was suppressed. The optimized app and CLI build
without that warning.

The first release visual replay passes 136 checks plus the Kestrel audit.
Its five new captures were inspected at 1280×820: the exact selection,
recording body count, framing path, sampled-target refusal and cancellation
message are visible. The final harness additionally runs the new cases at
the minimum 960×640 workspace size. This changes verification only.

Minimum-size review found the ordinary cancelled-zoom capture still said
“Preparing the stopped picture for framing…” after its request was gone.
All pending-entry exits now take the request through one helper which clears
only that owned status. Unrelated messages and errors remain intact. The
replay asserts its absence after delayed delivery in every cancellation case.
The preparatory report, images and source/binary identities remain separate
from final results. The follow-up scoped gate passes all 111 tests and both
strict Clippy configurations; independent review finds no further issue.

## Final release and measurements

Final release visual and separate quiet performance `zoom` replays each pass
136 checks. Each also passes the live Kestrel audit: 62 reserved bindings,
21,885,504 routing cases, no conflict or source drift. The final app SHA-256 is
`9d4347b9e710c8015341d211193aef7d6dc84a4cd257169b9155a877bf44d041`;
the CLI is `993a6972b381d385f17b41d092ea25221f11ac5baf9776104b54b9d9f2f78894`.

The new production-key cases record and save typed ranged smash and creep
instructions, replay at backward or changed ranges, apply dot to a different
beat, refuse current/face target sampling without edits or appended steps,
and cancel real delayed picture replies with and without recording and a
Visual range. Full authored-document oracles verify paired Undo, unchanged
bank and retained independent Original cursor. All seven retained final
960×640 captures were inspected. The exact selections, path indicator,
recording/body count, complete refusal and cancellation feedback are visible;
ordinary cancellation has no obsolete preparation status.

The visual report retains its intermediate-image allowance warning, 17 frames
needing a second layout retry and seven input-driven runs of consecutive
retrying frames. No ignored retry or repeated idle retry fails a check.

The quiet timing run has 223 completed input-to-state samples: median
0.541833 ms, p95 1.010959 ms, maximum 1.596250 ms. Its 55 completed
input-to-picture samples have median 2.025750 ms, p95 10.457583 ms and maximum
97.281917 ms, including initial project work. Neither metric has failed or
timed-out samples. No build or other replay from this task ran concurrently.
AC power and 80% battery were observed; thermal state and OS caches were
uncontrolled. These short-fixture offscreen timings do not establish physical
input-to-display or long-clip budgets.

The [retained evidence](../../tools/media-qualification/evidence/2026-10-18-ranged-framing-macros/)
contains final and preparatory reports, source/binary/fixture identities,
source diffs, checks and failures, review, inspected captures and reproducible
collection scripts. Its verified inventory contains 67 files.

## Remaining limits

One-play macro authority, broader placement intent, target-sampled dot, selectors
and parser acceptance remain open. Sampled-target ranged recording refuses as
specification §7.4 requires. No picture composition,
DSP or encoder algorithm changes in this slice, and no new encoded-output run
is claimed. Physical input, speech and listening remain owner verification
under the [project scope](../REQUIREMENTS.md#to-verify-owner).
