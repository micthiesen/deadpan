# Fixed-window Retime Hold-duration scout

Read-only investigation on 2026-10-20. No Cargo/test/PCM execution. Checkout remains frozen.

## Recommended next slice

Ordinary raw `Command::SetHoldDuration` under an unclocked Retime whose output duration stays fixed. Smaller than general updates to stages with derived pause clocks.

Concrete admitted candidate at 30000/1001 fps:

- Root Sequence: silent lead3, Retime output4.
- Retime: Edit purpose, FollowSpeed, mapping[0,4), child Sequence[Hold2, Source2]. No derived clocks or attached sounds/marks.
- Source: natural-rate 44.1kHz audio beginning at sample3100, sufficiently long measured span.
- Raw Hold2→3: child grows4→5, selected input[0,4) stays valid, root remains7frames. Source visible entry moves root5→6 and is clipped by the Retime at root7.

`command.rs:1386` only routes to `insert_time/retimed.rs::set_hold_duration` if `inside_clocked` is true. Here false. `insert_time/hold_duration.rs::apply` compares root structural totals and skips capture when equal. `audio_lineage::reconcile_shared` only changes lineage, not bindings. Thus this Source remains unbound at its new entry.

Exact expected phase discrepancy: B(5)=8008; B(6)=roundEven(9609.6)=9610. At new entry, recomputing from the live origin yields source3100+147/400 instead of the old entry3100. This is inferred from production exact mapping, not yet an observed PCM failure.

## Meaningful witness

Reuse ordinary_duration.rs's canonical mono44.1kHz fixture/provider. Independently prove the old first256 samples at literal root sample8008/source3100. Resize. Compare the new first256 at9610 with that literal old oracle, then all surviving visible Source samples against the old prefix, without alignment/tolerance. New visible interval is[9610,11211), 1601samples. Alternate chunking. Assert root duration7, no extra output, exact inverse, and old physical binding installed. Add an outside tail to prove no movement or changed PCM outside the Retime. Begin FollowSpeed/unity stage; add genuine nonunity only with analytically derived affine origins. No Preserve assumption.

## Likely files for bounded fix

- crates/deadpan-core/src/insert_time/hold_duration.rs and tests: local internal movement detection/capture rather than only root-duration comparison.
- crates/deadpan-core/src/insert_time/composite.rs: existing occurrence_owners_after explicitly refuses Retime ancestry, so cannot simply remove the root-total guard. Need a bounded local Retime-window aware path or reuse exact clock machinery deliberately.
- crates/deadpan-audio/tests/stages/ordinary_duration.rs: minimized failing oracle.
- deadpan-store durable test if behavior changes public command/history.

Preserve dedicated inside_clocked handling, generated fallback/Tail clamps, sound admission, existing bindings and no-op behavior. Start with no sounds/marks/nonunity processing; do not casually reset output stages or attach wrong-coordinate old clocks.

## Separate DP-07 stage-update gap

Updates to a stage holding exact pause clocks are deliberately unavailable, not an unobserved corruption claim:

- deadpan-app/src/project/retime.rs:28 rejects !wrap when workspace.clocked_retimes contains target, including integral owners with fractional descendants.
- deadpan-core/src/command.rs:1664 invokes derived_clock::sealed_preserved except sanctioned exact retimed mutation.
- derived_clock.rs::sealed_preserved requires surviving clocked owner's timing unchanged. Ordinary SetRetime changes recipe/removes own output binding, so it refuses. See docs/qualification/nested-speed-pauses-2026-10-18.md.

General stage update needs exact input/output/window semantics plus retained child physical clocks and PreserveInputProjection treatment. Keep separate from the fixed-window Hold resize witness.
