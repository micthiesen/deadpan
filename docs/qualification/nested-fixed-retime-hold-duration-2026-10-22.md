# Nested This play Hold duration through one fixed Retime

This extends duration-only authority to explicit body choices at every Repeat
ancestor, with one fixed, unclocked Edit Retime below the innermost Repeat and
Sequence ancestry otherwise. Repeat depth follows the existing document bound.
Existing contexts, instructions, occurrence commands, outside-in isolation and
the ordinary reducer remain authoritative; no wire or schema changes.

Mixed Default/Play choices, selected-play batches, owned gaps, Partition,
additional Retimes, Retimes above/between Repeats, derived pause clocks and
unsupported root/attached sounds retain their refusals.

Evidence: [retained reports, sources and emitted media](../../tools/media-qualification/evidence/2026-10-22-nested-fixed-retime-hold-duration).
Host: Mac17,7, M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1, locked dependencies.

## Old-code witness

Base `95ddb9aebe8d001f32b01e4cd3d9abff0a471585`. The witness passes public
Rename isolation (11 nodes, no marks, two actual proof steps), raw
SetHoldDuration on the mapped Hold, exact geometry, all five other occurrences,
JSON and inverse checks. Only the final positive native availability assertion
fails, in 0.013 seconds. All 1,945 baseline Rust files are frozen.

The executed app test SHA-256 is
`71bd1c137cc75a14e85ca99e1623ffafb4627bfe1c4fa95a2b8f02b3c8eef4b9`.
The silent 42-frame fixture selects Outer 2/3 and Inner 2/2. Its fixed stage
starts at 21. Hold growth 2f to 4f changes exact visibility from [45/2,24) to
[45/2,51/2), and sampled visibility from [22,24) to [22,25). This control
makes no audio claim.

## Structure, clocks and history

Core cases cover two, three and five levels, all pitch policies, cropped
intrinsic starts, exact fresh pools, owned/staged zero-copy setters, wrong
pools, stale revisions, wrong roots and atomic refusals. In the sparse
three-level case, retained ordinal 1 occupies physical position 2 behind
independent ordinals 40/41 at every level. An earlier owned body is 7f.
Visibility is [113,115), then [113,116) after growth. Public outer Rename
owns seven nodes first; duration copies only the remaining 11.

The Preserve audio fixture selects middle/middle among nine 4f stages.
Frame 16 is exactly 25,625.6 root mix samples; PointCeil 25,626 must select
the actual mapped stage instance. All 11,212 input cells and suppression
values match an independent literal 44.1 kHz source-phase oracle.
Exhaustion, chunk partitions, captured/uncaptured clocks, all eight other
stages and the exterior owner's complete RoundEven intervals are checked.
Fixed total output remains 40f. Root output rounding is not input PointCeil.

The 108f store fixture retains an independently owned inner sibling. Fresh
pools are 6+4 nodes, then zero for staged setters. A saved nested macro becomes
one Compound. Admitted synthetic accepted-media shortening retains its
provider; extension beyond coverage commits fallback and preparation.
The original request and immutable accepted-artifact controls stay distinct
from the new isolated Play. Stale/late failures preserve document and durable
row counts. Undo, Redo and reopen preserve complete content. This qualifies
admission and history, not model inference.

## Verification and retained failures

All 46 focused tests pass in 28.777s, with 5,226 skipped. All 3,503 broader
tests pass in 163.574s, with four slow tests and 1,769 skipped. The broader
run includes all 205 affected normal app checks; all 207 UI-feature checks
pass in 15.260s. Strict changed-package and UI all-target Clippy checks,
formatting and whitespace checks pass.

The first focused run passed 44 and failed two test setups. A repeated-plan
fixture reused allocation IDs; its allocator now namespaces from the current
revision. The historical nested fixed-Edit refusal now uses Partition,
because its former case is supported. A sparse oracle was strengthened to
separate stable ordinal from physical position. No production workaround
was needed. The initial all-target compile found a moved NodeId in the new
export fixture; the fixture now clones it.

An adapted identity script initially requested an old manifest name and
stopped before execution. The corrected script checked frozen sources and
hashed six test binaries before execution. The first source, failed logs,
corrected sources and commands remain in evidence. Broad binary hashes were
captured around completion and rechecked afterward, not claimed as pre-run
identities. The UI listing was rebuilt after its run and is labeled as such.

The first focused run has one audio captured-output leak marker; the final
focused run has one app marker. Their exact retained binaries pass separately
with captured EOF in 3.460444s and 0.011178s respectively. Causes remain
unknown, and the original markers are retained.

Three independent source reviews covered general correctness, timing/scope
and test quality without findings. Follow-up review of corrected core fixtures
also found no issue. Root reviewed the complete implementation diff.

The first debug native replay passes 59 checks plus Kestrel over 508 frames.
Root inspected both 960x640 captures: nested breadcrumb and scope, separate
Original/edit clocks, focused Beats, Enter prefill, keycaps and retained
picture aspect remain legible.

## Release native and emitted files

The release app SHA-256 is
`885ea5354105e2aaa8781a2593af4100b8394a36147b5690c83747c541e7c3f6`.
Both release runs pass 59 workflow checks plus Kestrel, over 554 visual and
739 performance frames. Both final 960x640 captures were inspected, including
the actual This play explanation and two-level breadcrumb.

Quiet input-to-state p99 is 1.645041ms (172 samples). Commit p99/max is
84.063167ms (six samples); picture-completion p99/max is 9.027375ms
(44 samples). No measured failure or timeout occurs. The replay starts and
ends at 1280x820; editing and the named captures use 960x640.

Both reports retain 34 second-retry layout frames and three runs of consecutive
retrying frames with distinct causes. The visual report also retains its
intermediate screenshot allowance warning. No ignored retry or repeated
identical-retry failure occurs; no allowance or timing limit was raised.

The independent release export test passes in 23.868s. Four actual H.264/AAC
files verify all 328 pictures and 525,326 presented audio samples across ten
complete zero-offset windows. There are no failed pictures/windows, flags,
uncovered samples, low-SNR blocks or unobservable offset windows. Minimum
block SNR is 30.4694108259dB. Index-neighbor comparisons report 366 observable
and 282 unobservable cases; identical repeated/frozen pictures remain explicitly
ambiguous in that diagnostic.

Unit cases retain 91f and affine cases retain 73f. Fresh growth is 3f to 5f;
owned shortening stages 3f to 4f to 1f (unit) or 2f (affine). An independent
literal picture oracle composes three inner-play families using the existing
shared layout oracle. Unit cases compare nonzero 256-sample suffix and
unselected/exterior probes within 1e-6. Affine cases use canonical PCM/file
verification and the separate exact input-phase test, not a rounded-shift claim.

The [owner fixture](../../tools/media-qualification/evidence/2026-10-22-nested-fixed-retime-hold-duration/nested-fixed-unit-grow.deadpan.tar.gz)
retains all nine exact package files in 91,286 bytes, SHA-256
`209f1e0026646e0212a95356eb125d162ddf9212b8c5fdfa0161d885df42a8c2`.
It was independently extracted and rehashed, then validated and inspected by
the hashed release CLI. Full validation reports valid, 91f and 33 nodes.
The archive includes its managed Original, receipts and render candidates.

## Limits

No requirement or gate is newly complete. DP-06 and DP-07 remain partial.
Physical keyboards, VoiceOver speech and listening stay on the exact owner
checklist in Requirements. Small local replay measurements do not close the
full performance corpus; cache, power and thermal conditions are not controlled.
Mixed/selected fixed-Retime lengths and remaining temporal, selector, sound
and mastering/renderer workflows still require implementation and qualification.
