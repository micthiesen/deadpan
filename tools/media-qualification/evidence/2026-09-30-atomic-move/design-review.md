# Atomic MoveRange design review

Reviewed `/tmp/deadpan-native-slices-20260930/next-atomic-move.md` against the normative specification and current core/plan ownership, Split, mark, audio-binding and sound-routing code on 2026-09-30.

## Judgment

**Change the root-sound policy, then proceed with the bounded ordinary-Sequence design.** One blocking design defect found. No additional blocking defect found in the pre-edit coordinates, joint cuts, identity handling, no-op contract or physical-owner reanchor approach. This is design review, not implementation or PCM verification.

## Blocking finding

### P1: A move must preserve the unchanged root sound clock

**Design location:** `next-atomic-move.md:228-261`, especially the proposed cut/gap maps at lines 235-236 and `RootSoundOperation::Move` at line 240. The proposed root-bus test and inserted-gap allowance rule must change with it.

The proposal deletes sound support under the selected source interval, shifts sound in the displaced interval and adds a silent interval at the destination. This treats an internal child move as an explicit root ripple deletion plus insertion. The root owner, its output extent and its clock have not changed.

The existing ownership contract explicitly distinguishes these operations:

- `docs/SOUND_EVENTS.md:220`: Move keeps ancestor-owned sounds in that ancestor's output clock.
- `docs/SOUND_EVENTS.md:233-237`: internal reordering does not transport a parent sound; explicit ripple insertion/deletion transforms host-local intervals.
- `crates/deadpan-core/src/sound_events.rs:44-60` (`validate`) requires every currently supported event to be owned by the root Sequence.
- `crates/deadpan-core/src/sound_routing.rs:447` (`RootSoundEditCapture::prepare`) currently maps named ripple operations. Existing `Command::Move` is guarded rather than establishing cut/gap semantics.

**Concrete failure:** root children A, B and C each last 10 frames. A root-owned sound occupies `[1,3)`. Move A's `[0,10)` to the original terminal seam `d=30`. The proposed map is `Keep[10,30), Gap(10)`, which removes that sound entirely. It should retain its root-local recipe and phase at `[1,3)`, now over B, subject to the current live Hold policy. A sound in B would also be incorrectly shifted by the proposed map.

**Correction:** preserve `sounds` and `sound_routes` exactly across MoveRange. Detach them while structural capture/Split helpers require a soundless document, then restore them after the final tree. Keep the original root extent and current route history. Retain/remap valid Hold and Repeat-gap allowances through transparent Split and evaluate the final live gates normally. There is no unconditional silent gap created in the root sound recipe by a move.

Remove the proposed `RootSoundOperation::Move`, three-Keep compiler/survival changes and related legacy route grammar work from this milestone. They are unnecessary once ownership is preserved. Add cases proving an unrouted and a previously routed root sound retain their serialized recipes/routes, integral sample labels, envelope phase and selected support through moves in both directions. Separately assert changes caused by live Hold gates and retained grants.

The parent acknowledged this correction during review and sent it to the designer. This report describes the reviewed proposal, before that revision.

## Remaining implementation obligations

These are already represented by the proposal, not additional blocking findings:

- **Live ancestors versus physical owners:** `audio_binding_lifecycle.rs:329-390` captures Source/Hold/nonunity Preserve recipes and Repeat-gap placements, not Sequence effects. `audio_owners.rs:554-620` retains the outer owner stack before entering a bound physical owner; `audio_bound.rs:338-351` starts the retained definition at that owner. Reuse this boundary. Source/destination Sequence gain and framing remain live on their changed local durations. Do not create Sequence bindings to preserve their old durations or capture source ancestors into the moved slice.
- **Cross-parent marks:** `marks.rs:387-429` reconstructs through the final parent chain and reports OutsideHost when the original host is no longer an ancestor. `tests/marks.rs:281` covers this existing policy. Keep that result and original unresolved coordinates; do not retarget a departed source-parent-local mark to the destination parent. Child-local marks retain their host identity.
- **Three cuts and no-op:** a plain Source requiring three distinct cuts needs seven fresh IDs under current Split rules: three for the first cut and two for each refinement (`split.rs:64-99`). Independent two-cut plus one-cut estimates would overcount. Resolve an exact same-parent boundary no-op before Split/capture, so partial range no-ops allocate zero nodes and timing records. Equal global time alone does not establish a no-op when ownership or empty-child order changes.
- **Exactly-once reanchors:** derive disjoint moved/displaced owner sets from the unchanged-time split tree, including hidden retained contexts, and use the same post-Split layout. Avoid applying two ancestor-suffix traversals sequentially. Preserve the nonunity Preserve stop rule and old sample entries. The proposed independent fractional-rate PCM oracle remains necessary.

## Limits

Read-only repository inspection with ripwire and direct source reads. No repository edits, Cargo, tests, native workers or UI actions. Only this scratch report was written. Native preview/commit integration and actual fractional-rate output remain unverified until implementation and the specified tests exist.
