# Chosen-boundary return from existing paths, 2026-10-25

An existing animated camera path can return at a chosen boundary through
ordinary Split, scope navigation and Camera Reset. This qualifies DP-08.2's
§8.2 operation. It also found and fixed a native Camera crash on long target
labels in a compact viewer. DP-08 remains Partial and every delivery gate
remains open.

## User workflow and scope

1. In Your edit, move to the return boundary and press `s`.
2. With the right fragment selected, press Enter to reach its copied original
   framing owner. The fragment's outer Partition has no framing of its own.
3. Press `,f`, then `r` to preview the reset. Enter saves one framing edit;
   Escape restores the entry. Backspace returns to the fragment.
4. Undo restores the path. A second Undo restores the unsplit owner.

Split retains each side's complete original context and animation clock. Reset
removes only the chosen copied owner's framing, leaving the earlier path and
ancestor framing intact. This returns to the view entering that owner; it does
not erase intentional ancestor framing. The existing `:zoom off` command also
removes a selected owner's own framing, but clearing an unframed outer
Partition does not remove its child's path.

The specification asks for a return at a chosen boundary, not a new interval-mask
format. Section 7.4 expressly permits ranged framing to refuse when it cannot
preserve an existing path. Ranged Off on an Envelope/Follow remains an explicit
refusal. No path is flattened or target center sampled to simulate a return.

## Native defect and fix

The first painted workflow completed its Envelope case, then panicked opening
Camera on the live Follow. A backtrace from the unchanged executed binary
locates `camera_overlay`'s badge positioning: an unbounded label was wider than
the canvas, inverting the `f32::clamp` bounds. Both failed reports remain
failed, including the useful backtrace rerun.

`paint_camera_badge` now elides a single line before positioning, accounts for
actual glyph ink, remeasures measured overhang and bounds the badge rectangle.
The inspector retains the target's full name. The real-paint regression checks
48 combinations of short/long labels, 24×24 through 640×360 canvases, 1/1.25/2×
display scales and anchors beyond both edges. It checks ink inside the badge
and clip, one-line elision and full layout metadata. No extra layout retry or
input route is introduced.

A retained finite binary32 witness also shows that subtracting a capped width
from a fractional canvas maximum can round below its minimum. The placement
helper explicitly floors that upper bound, and a separate regression exercises
the exact witness with both off-canvas anchors and an oversized badge.

Final focused checks pass **83 tests**, strict normal/UI-harness lint and
formatting. Independent general and test-validity reviews report no remaining
findings, including a second review of the production badge fix.

The debug, final release visual at 60 Hz and separate release performance-mode
at 120 Hz runs each pass **489 zoom checks plus the shortcut audit**. The new
workflow uses actual Split, Enter, Camera Reset/cancel/save, Backspace and
Undo/Redo. It compares all 120 picture mappings, exact retained path coordinates,
the complete changed document, unchanged audio plan, source cursor and one
ancestor framing layer. Both cases use a nonzero RetainedOutput clock, offset
15/2 and duration 180. The native Follow remains live at boundary 80 and also
exercises movement, Lost hold, manual correction and later fallback.

All four final 960×640 return draft/saved captures were inspected. The long
badge fits and elides; Apply/Cancel, scope and keys remain visible. Reports keep
36 second-retry frames, 25 sequences with distinct retry causes and the visual
intermediate-image allowance warning. Named checkpoints remain captured;
ignored-retry and repeated-same-cause guards pass. The debug warning banner is
not present in the final release images.

Both final release runs compare all 62 Kestrel reservations over 21,885,504
production-router cases without conflict. The supplied live source matches
fixture SHA-256 `368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
No shortcut assignment changed.

## Actual media, sound and history

Two release tests start with an already-framed 120-picture A/V Original:
one retained Smoothstep/Cubic Envelope, one retained animated Follow. The
headless tests Split at 80 and clear the copied inner Source with the same
typed framing command that native Camera uses. The parent keeps its separate
11/10 framing. All provider pictures, prefix poses and original inner clock
coordinates remain exact; the suffix loses only the selected path.

Both compare all **192,192 stereo sample frames** of canonical authored/limited
PCM and every limiter gain bit under different read partitions, after Split,
reset, Redo and portable copying. Marks survive reset exactly; full Undo/Redo
restores complete documents apart from revision identity. The portable copies
verify their complete 11-revision Envelope and 12-revision Follow histories.

| Emitted-file result | Envelope | Follow |
| --- | ---: | ---: |
| Picture comparisons | 120/120 pass | 120/120 pass |
| Presented audio sample frames | 192,192 | 192,192 |
| Audio offset | Verified zero | Verified zero |
| Minimum neighbor margin | 31.1893 dB | 24.9318 dB |
| Observable / unobservable neighbor comparisons | 238 / 0 | 212 / 26 |
| Old-path negative control | Exactly frames 80–119 fail | Exactly frames 80–119 fail |

The negative controls retain verified zero-offset audio. Positive encoded audio
uses lossy AAC verification, not bit-equality to PCM: minimum block SNR is
29.5347 dB and maximum level difference 0.0438 dB. Declared AAC media time is
2,048; no event alignment or relaxed AAC-block tolerance is used.

The media Follow's target span ends at 75, so its return occurs in fallback;
the separate native case supplies live-at-return coverage. Targets are authored
regions over counter footage, not tracker-accuracy or human quality evidence.
Picture preservation uses before/after canonical-plan equivalence and the
emitted verifier, not a new independent framing or DSP oracle. No listening,
physical input, VoiceOver speech or display-latency claim is made.

The first media run fails at its new Redo witness because that witness compared
the freshly generated revision ID. The corrected witness excludes only
`revision_id`, then reruns both complete cases successfully. No production
assertion or pixel/audio tolerance was weakened. Raw parent streams and parsed
reports remain; the existing generic Render/verify helpers do not retain their
individual raw subprocess streams.

## Identities and evidence

Runs use Rust 1.97.1, locked dependencies, qualified FFmpeg/ORT prefixes, an
Apple M5 Max and macOS 26.5.2. The media receipt pins 1,704 source files; final
UI receipts pin 1,335. Before/after source maps match, and an independent audit
rehashes the inputs and actual output files. Release CLI/worker hashes are
post-run readings; their receipt's binary maps are empty. UI visual and quiet
receipts additionally pin the executed App before/after.

- Final release App: `29aaf0039a41525ddaae7d8fa90abb7fc086d9f998e2a6ea876b1b9a833e1cd1`.
- Release CLI: `e70a3baad7f57861ca13a70a3054e5722da91ebdf4841ac484c02e99f66fd47c`.
- Media worker: `70114641de62f0c9c839734f3dddab1221b01888661a43ed5583e40ca9ed90fe`.
- Export test: `7022a06ddef646f17293cd67b106fe5a78b41116df24acf2aeb76c59f08305e4`.

The final quiet replay records input-to-state p50/p95 0.438/1.128 ms (588 samples),
and input-to-picture completion 2.039/10.587 ms (112 samples, maximum 95.495 ms),
with no failed/timed-out samples. These mixed small-fixture offscreen samples
do not close Section 25 workload targets. Full gate/all-scenario qualification
was not rerun for this narrow Camera fix and operation proof.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-25-abrupt-return)
contains the failed and passing receipts, source/binary maps, generated projects,
actual movies, full canonical PCM witness, complete picture/history records,
positive/negative reports and selected final images. Binaries, caches and full
source archives are excluded. The byte/hash omission ledger names sidecars and
redundant failed fixture files; raw failure logs remain.
