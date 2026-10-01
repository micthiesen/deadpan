# Atomic adjacent Source Roll

Planning review, 2026-10-01. No implementation or runtime verification was performed for this document. The normative scope remains `docs/spec/DEADPAN_SPEC.md` §§6.1–6.5 and §7.7. This proposes the first backend Roll operation after qualified ripple Source Trim; it does not complete Trim mode, overwrite, waveform comparison, or composite editing.

## Recommended boundary

Implement one revision-bound command for two explicitly captured, ordered, adjacent direct children of an ordinary Sequence. Admit each side through `source_edit::admit`: a qualified Source or an existing neutral unity Partition around a Source, under ordinary Sequence ancestors. Each side needs an exact edit window, a supported explicit affine picture map, full measured source context, and coherent linked A/V mappings before its independent audio offset.

Require literal sibling adjacency. Do not skip empty children, find a different neighbor after entry, or infer a missing side at commit. Reject unsupported structure and effective picture windows extending into audio-only lead/tail. Check both current and candidate effective windows. These are explicit initial eligibility limits, not reductions of the eventual spec.

The two sides need not have equal rates or the same asset. Each side's A/V clock must be internally coherent; the common delta is measured in project frames. The single-Original store profile separately enforces its asset policy. Split siblings using the same immutable Original are a primary case.

Suggested command shape:

```rust
RollSources {
    parent: NodeId,
    left: NodeId,
    right: NodeId,
    delta_frames: i64,
    left_wrapper: Option<NodeId>,
    right_wrapper: Option<NodeId>,
    timing: AudioTimingId,
}
```

Only the contracting direct Source can need a fresh neutral wrapper, so at most one wrapper is present. Keeping two named optional fields makes the supplied identity's role explicit. Validate presence exactly against the resolution, freshness, distinctness, node budgets, and timing allocation against the new revision. Existing wrapper identities survive. Roll has fixed total duration; a ripple/overwrite mode parameter has no meaningful effect and should not be accepted as a cosmetic option.

Expose `ProjectDocument::source_roll(parent, left, right, delta_frames)`. A resolved zero is a validated preview with no transaction and exact `before == after`; the authored command rejects it before creating a revision.

## Exact edge and clamp arithmetic

Let the old left and right output ranges be `[T,J)` and `[J,U)`. Their integer lengths are `L_l` and `L_r`. For side `i`, let:

- `C_i = [c_i0,c_i1)` be the integer physical-local delivered allocation.
- `W_i` be the persisted exact selected linked interval.
- `E_i = W_i ∩ C_i = [a_i,b_i)` be the effective exact interval, of positive width `w_i`.
- `H_i = [v_i0,v_i1)` be the full measured picture support in that owner's physical-local coordinates.

Positive `d` moves the seam later. Apply the same integer delta to left Out and right In:

```text
C_l' = [c_l0, c_l1 + d)       E_l' = [a_l, b_l + d)
C_r' = [c_r0 + d, c_r1)       E_r' = [a_r + d, b_r)

left output  = [T, J + d)
right output = [J + d, U)
```

The pair end, parent duration, all ancestor durations, and suffix positions stay fixed. Preserve both fractional padding values on each side: `a_i - c_i0` and `c_i1 - b_i`. Never replace exact endpoints with floor/ceil values just because output duration is integral.

Intersect the two exact edge limits before making either candidate:

```text
left lower:  max(1 - L_l inclusive, -w_l exclusive)
left upper:  v_l1 - b_l inclusive
right lower: v_r0 - a_r inclusive
right upper: min(L_r - 1 inclusive, w_r exclusive)
```

Equivalent whole-frame limits are:

```text
d_min = max(1 - L_l, 1 - ceil(w_l), ceil(v_r0 - a_r))
d_max = min(floor(v_l1 - b_l), L_r - 1, ceil(w_r) - 1)
```

Clamp the requested delta once to `[d_min,d_max]`, then use that one applied value for both sides. Retain exact rational bounds, inclusivity, limiting side, and reason for the preview. At equal bounds, an exclusive constraint wins. A tied-reason list is ideal; a deterministic controlling side plus reason is sufficient if the report does not imply that it was the only limit. The currently valid pair must admit zero. Arithmetic overflow is an error, not a source-handle clamp.

Audio lead/tail does not further shrink a picture-supported handle. Audio support is intersected with the exact linked window, leaving silence where the qualified source has no audio. Intentionally absent audio remains absent; dormant linked audio remains present and may become audible.

## Reuse the geometry, not two Trim operations

Extract the pure edge-limit and Source-candidate work from private `source_trim::{limits,candidate}` into a narrow internal helper in `source_edit` or `source_edit/edge.rs`. Give the candidate a named internal type containing allocation, W, E, Source, physical prefix, and wrapper requirement. Keep existing Trim resolution and behavior unchanged.

Do not call two complete `source_trim` queries and combine them. Those queries package unary ripple output ranges, root sound operations, and target/suffix timing windows. They also check hypothetical document duration `total ± d`; a valid fixed-duration Roll must not fail because an intermediate unary extension would overflow. Independent clamps could produce different deltas.

Do not reduce Roll to two authored Trim commands. That would expose an intermediate tree, add two history entries, route root sounds through insert/delete policies, and risk temporary mark loss. Resolve both candidates against the same old tree and validate one final document.

### W and physical owner maintenance

Reuse the current Trim candidate's minimal window policy independently on each side:

- Left Out keeps hidden selected tail support when the moved effective edge equals the allocation edge; otherwise move the authored fractional W end.
- Right In keeps hidden selected head support when the moved effective edge equals the allocation edge; otherwise move the authored fractional W start.
- Require `W_i' ∩ C_i' == E_i'` exactly.
- If W does not change, preserve its mapping representation and selected filter support exactly.
- If W changes, picture selection is `W_i' ∩ H_video_i`; audio selection is `(W_i' ∩ H_audio_i) - audio_offset_frames_i`, using the existing empty-support policy when disjoint. The persisted audio selection is pre-offset.

Keep temporary negative coordinates exact until deciding physical growth:

```text
p_i  = max(0, -c_i0')
D_i' = max(D_i + p_i, c_i1' + p_i)
```

Translate allocation, W, explicit map starts, and selected supports by `p_i`. Preserve full measured source spans, slope, and independent sample offset. Do not invoke generic mapping setters, which intentionally clear edit intent. Do not refit `FitBeat`; it remains outside admission.

For this pair operation, left Out never needs a prefix; positive Roll may grow its physical tail. Negative Roll may prepend the right physical Source. Never shrink physical owners. A contraction changes the existing Partition or introduces a fresh neutral one. An existing wrapper remains even if it later exposes the whole owner.

## One old-tree audio capture, no new reanchor

For retained right content at old physical coordinate `x`, the project coordinate is unchanged:

```text
old: J + (x - c_r0)
new: (J + d) + (x - (c_r0 + d))
```

After a physical prefix, both `x` and the new allocation start gain `p`, which still cancels. Left retained content also stays at its old project coordinate. Therefore Roll does not shift retained target or suffix audio. It should not add the target/suffix reanchor steps used by ripple Trim.

Proposed reducer sequence:

1. Resolve and validate the complete pair against the old document.
2. Detach the root sound bus through the exact fixed-duration capture described below.
3. Call `audio_binding_lifecycle::capture_unbound_audio_bindings` once on the old soundless tree. Retain all existing lattices, resumes, chronological reanchors, and historical layouts unchanged.
4. Install both Source candidates and both final allocations together. Rebase only the right owned binding if it gains a physical prefix, using `OwnedAudioBinding::rebase_local`.
5. Reconcile lineage, transform marks once, prune after the complete tree is installed, validate, and restore the unchanged root sound bus.

No `capture_for_composite_insertion`, new phase-only layout, `append_steps`, or suffix-owner traversal is needed merely because the seam moves. Existing historical steps are not rewritten. If all owners are already bound, the supplied timing identity need not become retained data. Bound-budget validation and ordinary capture limits still apply.

Independent audio review found no counterexample in `audio_bound::bound_at`: rebased local anchor `x+p` and transform offset `-p` produce the same absolute sample boundary and retained reference/resume. This is an algebra and source-review conclusion, pending Roll-specific PCM tests. If future admitted structures change the retained affine map, they require their own timing design before admission.

The transferred sample count comes from absolute boundaries. At 30000/1001 fps, `B(3)=4805` and `B(4)=6406`: moving seam 3 to 4 transfers 1601 samples, not `B(1)=1602`. Pair duration telescopes exactly:

```text
[B(J+d)-B(T)] + [B(U)-B(J+d)] = B(U)-B(T)
```

Neutral Partitions add no new filter/fade boundary. A changed W may change selected support and its endpoint filtering; do not claim bit-identical old edge PCM where the source selection itself changed. Use retained interior and independent full-context references to prove phase.

## Root sounds, effects, and marks

**Root sounds:** reuse the fixed-duration branch of `sound_routing::RootSoundEditCapture::prepare` currently used by `MoveRange`. It detaches sound events/routes, retains old total duration, then restores those exact objects after checking unchanged duration. Add Roll to that path and the structural command classifier. Do not represent identity as `RootSoundOperation::Replace`; Replace removes selected sound support even when lengths match. Insert followed by delete is also wrong. Existing allowances remain unchanged: no Hold issuer is added, removed, renamed, or moved by this admitted operation.

**Effects:** when a physical Source grows, preserve its old framing owner domain with `Framing::prepend_owner_frames(p, old_duration)`, including tail growth with `p=0`. Translate gain keys/mute ranges with `with_owner_prefix(p)` and owned sample clocks with `rebase_local(p)` only for a positive prefix. New physical regions use existing endpoint-pose and authored treatment semantics. Parent/ancestor clocks stay unchanged because their duration and global position stay unchanged.

**Marks:** run one old-to-final transform. Only the right Source can gain a prefix, so `transform_marks_with_source_prefix` is sufficient without generalizing it to many owners. Translate physical content-point anchors by that prefix before reconstruction, preserving edge sentinels and unresolved anchors. Retained ancestor-local content points stay at the same global positions. Cropped physical Source-local or Source-PTS bindings may remain Bound while occurrence queries report unavailable; do not force stored crop loss. Ancestor/occurrence reconstruction applies existing loss policies where content disappears. Preserve multi-binding marks through Split siblings and do not revive previously unresolved bindings. Test both biases at the moving seam.

## Public report and store boundary

`SourceRollResolution` should expose:

- Parent, explicitly selected left/right children, slots, physical Source IDs, assets, and qualification IDs.
- Requested/applied integer delta, exact lower/upper limits with side/reason/inclusivity, integer limits, and clamp report.
- Fixed pair output range, old/new seam, and per-side before/after output ranges.
- Per-side before/after Source, allocation, W, E, physical prefix, and wrapper requirement.

Do not expose unary ripple root operations or suffix timing windows that imply work Roll does not perform. The result is descriptive evidence, not an authority token. The store re-resolves the command against the captured revision and checks both qualification receipts, full spans, asset records, and final Sources through the existing Slip/Trim admission pattern. A zero preview must still validate revision/new-revision metadata, explicit target pair, wrapper absence, timing allocation, and both receipts; it returns no transaction. A raw resolved-zero commit fails without a new revision.

Suggested files: `core/src/source_roll.rs`, `source_roll/apply.rs`, narrow helper extraction from `source_trim.rs` to `source_edit`; command/lib/exhaustive classifiers and fixed-duration sound capture; `store/src/source_registration/roll.rs` with store wiring; CLI typed preview dispatch. Root owns format/version integration. Reuse current closed historical command grammars rather than admitting Roll into older schemas or creating a migration family for unused development formats.

## Required independent witnesses

| Area | Witness |
| --- | --- |
| Limits | Left handle versus right positive-width/output limits, reverse direction, exact fractional width, tied strict/inclusive bounds, one shared clamp, zero identity, checked overflow. |
| Geometry | Different fractional padding on both sides; different internally coherent affine rates; all direct/Partition combinations; hidden W retained, then crossed; right prefix and left tail growth; only contracting direct side allocates a wrapper. |
| Refusal | Reversed/same/nonadjacent children, an intervening empty child, unsupported composite or treated wrapper, incoherent A/V map, unqualified/full-span mismatch, current or proposed effective picture outside measured support. |
| Indexed picture | Signed-origin VFR fixture with known ordinals/PTS on outgoing `J+d-1` and incoming `J+d`; pair end and later picture identities unchanged; fractional terminal padding; exact inverse. |
| PCM | 30000/1001 seam 3→4 transfers 1601 samples; reverse direction; 44.1 kHz source; independent 17-mix-sample offset; unbound/bound/resumed and chronological-reanchor owners; audible newly exposed prefix; dormant activation and intentionally absent audio. |
| Effects | Retained Source camera domain through prefix/tail growth, live ancestor framing unchanged, shifted gain/mute keys and bound owner sample phase. |
| Root bus | Exact event/route/allowance identity for a sound crossing the moved seam and sounds outside the pair; no new route step or seam silence. |
| Marks | Both seam biases, retained ancestor positions, physical right +prefix, cropped Source marks stored/query distinction, multi-binding Drop/KeepUnresolved, never revive old unresolved anchors, exact serialized inverse. |
| Budgets | Existing reanchor-term budget is not consumed by Roll; capture/node bounds still enforced; resolver avoids artificial unary total-duration overflow. |
| Store/CLI | Either receipt missing/changed rejects even zero preview; stale pair/revision and duplicate identities fail unchanged; preview matches commit; one history entry; reopen, Undo, and Redo restore both sides with fresh revisions. |

## Remaining choices and risks

Prefer the command/report names above, but settle names with store before coding. Full held-picture policy for audio-only lead/tail remains future explicit work; direct Source targeting alone does not establish eligibility. General nested, rate-changing, and composite Roll remain required by the product scope after their timing/admission behavior is defined.

The chief verification risk is exposing previously hidden audio through existing historical resumes while preserving its exact absolute sample phase. The no-reanchor design is simpler and follows the unchanged retained transform, but must earn its PCM evidence before shipping. The other important risks are accidentally reusing unary root-sound routing, rounding fractional W endpoints, and running mark transforms against an intermediate tree.

## Subsequent editorial-edge correction

A final Trim audit found that retained sampling transparency alone does not satisfy the new-cut fade contract. The planned Roll/overwrite operation must record the changed incident editorial edges independently of raw filtering support and retained phase. Preserve unchanged continuous joins and exact Hard precedence. These notes predate that correction and are design input, not implementation evidence.
