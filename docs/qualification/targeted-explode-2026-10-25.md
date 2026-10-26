# Targeted Repeat escalation Explode, 2026-10-25

Targeted escalating Repeats can be exploded into independently editable ordinary
plays while retaining live targets, per-play scale and gain, canvas containment
and the following gap. The materialized framing stores the containment rule
explicitly rather than freezing a sampled target center. Existing ordinary
Follow framing remains unconstrained unless that rule is present.

This qualification concerns DP-08.4. General sound-bearing structural
transformations, scoped sampled-target ranges and the remaining delivery gates
are separate open work. No claim of full editor or release completion follows.

## Representation and temporary Camera behavior

An affected play and its following gap become an ordinary Sequence with a fixed
scale Follow and the original target identity. Add and Multiply escalation use
the same exact quantized scale and target projection as the compact Repeat.
The first identity play needs no escalation wrapper. Per-play gain, override
contents, target references, marks and reversible history remain ordinary data.

`FollowCentering::CanvasClamped` bounds a successfully resolved target center
when scale exceeds one. At or below one the center is unchanged. Missing,
hidden, wrong-asset and out-of-span targets retain the authored fallback; the
rule never clamps that fallback as a substitute for resolution. The default
Unconstrained field is omitted from serialization, preserving existing Follow
encoding. Unknown policy names refuse rather than silently changing behavior.

Camera preserves this policy through opening, target replacement, Keep zoom,
creep and arrival/curve adjustments. Its controls explain containment above
100%. An inner Camera draft reprojects both outer ordinary Follow layers and
targeted Repeat escalation layers in provider order. Escape restores every
entry layer. Layer updates distinguish escalation and ordinary framing even
when both belong to the same instance, and validate the batch before mutation.

Clamped centers move when scale changes. Updating the locked center fields must
preserve an unfinished decimal in Scale, Arrival text, its curve and active IME
composition. A new regression initially failed because that center update lost
the composition validation error. The corrected continuation passes and keeps
Apply unavailable during composition. It does not establish physical OS IME
delivery or speech behavior.

## Pure and actual-media evidence

Four Plan tests cover 32 combinations of Add/Multiply, increasing/decreasing
scales, moving/Lost/manual-corrected targets, missing visibility, wrong assets,
out-of-span targets, default and owned captured gaps, nested overrides and
fractional Retime. They compare every provider picture, exact clock and ordered
pose before and after Explode, serialized commands/documents, inverse history,
independent later-play edits and subsequent global target corrections. A
separate edge oracle checks the exact clamped Q32 result.

Two release actual-media tests use the qualified cfr-bframes A/V Original at
30000/1001. A 30-picture selection repeats three times with 6- and 9-picture
gaps, making 105 pictures and exactly 168,168 stereo sample frames. Moving,
Lost and corrected targets, child/Repeat/ancestor framing, a selected-play
audio override and one-dB-per-play escalation all remain present.

Both emitted MP4s pass complete picture and zero-offset audio verification
against both the compact Repeat and exploded revision. Complete canonical
authored/limited PCM and gain bits match across different read partitions.
Undo/Redo, a temporary independent later-play edit, a subsequent target
correction and portable histories pass: 22 revisions and 21 replayed entries
in each case. These are shared-renderer/DSP consistency checks, not independent
DSP correctness, detector accuracy or listening evidence.

The positive files have minimum luma/chroma SNR of 48.8142/53.7047 dB,
minimum audio block SNR 30.5825 dB and maximum level difference 0.05452 dB.
AAC media time is 2048 and measured audio offset is zero. Minimum neighboring
picture discrimination margin is zero, so this run does not prove a positive
index margin for every cropped or frozen neighbor.

For Add, replacing the live center with a static one fails 28 pictures. For
Multiply below unity, the outer unconstrained Follow cancels the inner
translation without clipping; that static-center control correctly passes.
Removing only the second materialized Follow layer then fails exactly 39
pictures, frames 36 through 74. That negative checks the layer's contribution,
not target tracking in isolation. The original test incorrectly expected the
Multiply static-center case to fail; its failed receipt remains retained and
the corrected test changes no verifier tolerance.

## Retained failures and qualification limits

Early compile/lint failures were fixture consumers of the newly explicit Follow
fields. A later test attempt exhausted disk space while incremental caches
grew. Only this task's rebuildable incremental directory was pruned after all
Cargo processes finished; the recovery receipt records the failure and byte
counts. Subsequent runs disable incremental compilation. No source, accepted
media or authored project was removed.

The first native replay advanced 80 additional frames after This play had
already retained Edit frame 204. It reached frame 243, beyond the saved target
span, and its assertion incorrectly expected live-target reprojection there.
The corrected setup retains 204, checks the displayed plan before Camera and
keeps the original reprojection assertion. The failed native report remains
failed.

The next replay passes that Camera workflow and every picture/audio-plan
observation, but its complete-document oracle initially normalizes only the
commit revision. The native semantic transaction has a distinct leaf revision
for the retained gap timing, whereas the direct typed oracle uses one revision
for both. A retained-package rerun confirms that exact request structure in
SQLite. The corrected comparison normalizes only allocations absent from the
input, preserves shared references and compares every other authored field.
An independent review checks this normalization; both earlier failed reports
remain retained.

The third native run reaches the decimal witness, but the fixture has moved
focus from Scale through Apply/Cancel before sending the new text. Removing
those unnecessary Tab presses keeps native text focus and adds an intermediate
210% assertion. The fourth run passes that assertion and the exact 200.5%
two-batch witness. It then finds a real compact-layout defect: containment
help extends to y=549.625 while the 960×640 pane clip ends at y=536. The help
now occupies the existing area above Apply, replacing generic Follow help
rather than adding another row below it. Static and animated compact controls
are checked by the final replay. Review retains the animation guidance too:
Scale changes the whole animation while Arrival/Curve keep its timing. The
combined explanation fits, but a subsequent native run catches Cancel text
extending to y=556.4375 beyond the y=555 clip. Shortening the curve caption to
`Arrival curve · Enter` removes its extra wrapped line; Space remains a working
activation key and is documented in the control's help. Original failures
remain failed.

## Strict scope reconciliation

Earlier DP-08 remaining-work text bundled several already-qualified items with
the missing targeted transformation. Ordinary/native occurrence selection and
captured-pause Metal rendering have specific retained qualifications; they are
not wholly absent. Framed-child Ungroup remains a real limitation under
DP-03.3, and complete effects remain under DP-16.

A concrete native gap remains: a contained Time range in This play cannot newly
sample a named/current target. Direct Zoom/Creep refuses, and native scoped
Split also refuses, so Split-to-fragment composition does not provide that
result while preserving the surrounding Repeat. Whole-owner scoped targets
and ranged creep retaining an existing Follow already work. Section 7.4 permits
an explicit refusal when an existing outside path cannot be preserved; it does
not remove ordinary interactive sampling from scope.

The fixed coarse DP-08.1/.2/.3 deliverables do not claim every combination of
selector, target and occurrence. Their existing scores stay unchanged. The
missing composition is made explicit in the already-partial DP-04.7, DP-05.3
and DP-05.4 criteria, and keeps strict DP-08 Partial even when the coarse
operation checklist is fully scored. No criterion or denominator is changed
to move the percentage.

## Checks

The filtered normal lane runs 324 tests: 323 pass and the new IME validation
case fails before its fix. Its focused corrected continuation passes that case;
the 323 unrelated passes are retained rather than represented as a new full
run. The explicit UI-harness unit lane passes 44 tests with 1339 skipped. Both
real-media release tests pass. Final strict normal/UI all-target Clippy and
formatting pass through `cargo xtask check --since 7fe6fe85 --no-test`.

The source maps for the media and UI-unit runs differ from final source only
in the subsequent native replay fixture, Camera help and curve-key caption.
Production Core, Plan, CLI, renderer, audio and Camera behavior inputs remain
identical; the final native run exercises the changed layout. Build and replay
receipts retain their own complete before/after source and executed-binary
identities rather than treating Git HEAD alone as the executed source.

## Final native and evidence result

The final release visual replay at 60 Hz and separate quiet performance-mode
replay at 120 Hz each pass **580 zoom checks plus the Kestrel audit**. Both
execute App SHA-256
`9a324d7620a08a07a0b7e2d4d3d9ae3cfca5166885405b50befa02e8bde16c11`,
with all 2203 recorded source files and the binary unchanged across each run.
The same final source passes strict normal/UI target lint and formatting.

The workflow exercises child Camera reprojection and Cancel/save, native
`:explode`, complete 368-picture/provider/pose/audio-clock/gain comparisons,
the complete authored document apart from fresh identities, independent
ordinary-play navigation, target replacement, scale text, separate decimal
batches, Keep creep, Arrival/Curve edits, reopen and durable Undo/Redo. Final
cleanup restores the entire prior 120-picture document. All five new named
captures were inspected, including both 960×640 static and animated Camera
panes. Containment, animation guidance, fields, Apply/Cancel and footer fit.

The visual report retains 42 second-retry frames, 26 sequences with distinct
retry causes and the intermediate-image allowance warning. The ignored-retry
and repeated-same-cause guards pass. Each run compares all 62 live Kestrel
reservations over 21,885,504 production-router cases without conflict; live
source SHA-256 is
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
No key assignment changed.

These are offscreen real-Metal native UI replays with injected input, scripted
dialogs and simulated audio delivery. They do not measure physical display
cadence, OS IME delivery, real pointer input, device sound or listening quality.
Actual PCM and emitted-file checks are the separate media cases above.

Retained receipts, failed attempts, complete generated media projects, encoded
outputs, histories, source/binary identities and the five inspected images are
under [the evidence collection](../../tools/media-qualification/evidence/2026-10-25-targeted-explode/README.md).

An independent collector audit and a separate root readback verify all 319
retained payloads and metadata, including lossless decompression, with zero
failures. The immutable collection stores 41,469,234 bytes representing
585,574,396 source bytes; 1173 omitted rebuildable/sidecar/unselected-image
entries retain their sizes and hashes. Inventory SHA-256 is
`5ed22121b429c509ffe2e5bb8a60e421bbfa9183e17e065f6c5ea13e56670ec1`.
The source audit confirms exactly the three disclosed App-only differences
between media and final release maps, with 2200 identical files. All failed
attempts retain their original results; the captured SQLite/WAL history witness
confirms the native transaction's outer/leaf identity distinction.
