# Ripple Trim design: core review

Read-only planning review, 2026-10-01. Reviewed
`/tmp/deadpan-native-slip-20261001/next-trim-design.md` against normative spec
sections 6.2–6.4, 7.5 and 7.7, and the current window, Slip, effect-clock and mark
implementations. No checkout edits, Cargo or native execution.

## Conclusion

The exact edge equations, whole-frame clamp bounds, fractional-gap preservation,
minimal window changes and grow-only physical-owner construction are sound for
the explicitly admitted Source/neutral unity Partition scope. One mark-policy
statement needs correction before implementation. The bounded scope does not
complete or replace normative Trim, overwrite or Roll.

## Actionable correction: distinguish stored marks from occurrence resolution

The design says Source PTS anchors use loss policy “when cropped away.” That is
not how the existing mark model works. Preserve the existing distinctions:

- `marks::Index::validate_bound` validates `Anchor::Source` against the asset's
  complete measured span, not current selected support or a visible occurrence.
  Cropping preserves its persisted Bound state. `AnchorIndex::resolve_target`
  later resolves a specified Source occurrence and may return `OutsideMapping`.
  This is also necessary for native Original marks, which remain usable after
  edited material disappears.
- A physical Source-local anchor is still valid inside the retained physical
  owner, even if the new Partition hides it. Its persisted binding stays bound;
  resolving it through the cropped tree can fail.
- Ancestor-local anchors decompose to content and reconstruct through the new
  crop. A removed content point can therefore invoke the configured loss policy.
  Concrete Occurrence anchors also validate their path to project time, and can
  become unresolved or be dropped when that path no longer includes the point.
- Existing unresolved bindings remain unresolved. Do not conflate a transient
  occurrence-resolution error with a persisted unresolved binding or automatically
  revive the latter during extension.

Concrete witness: physical Source S occupies `[0,10)`, and Out `-4f` introduces
Partition `[0,6)`. A Source-PTS anchor mapping to physical 8 and a Local(S,8)
anchor remain stored and bound. Resolving either through that Partition fails.
A Local(root,T+8) or Occurrence(S,8) mark can instead invoke loss policy during
the edit transform. Test both biases at physical 6 separately; the persisted
Occurrence validator currently projects without bias, while final boundary
resolution applies it.

Relevant symbols: `crates/deadpan-core/src/marks.rs::Index::{decompose,
reconstruct,validate_bound}`, `transform_marks`; and
`crates/deadpan-core/src/anchor.rs::AnchorIndex::{resolve_target,to_project,
project_boundary,source_position}`. Correct the design wording rather than
changing these established semantics as part of Trim.

## Exact arithmetic checked

For `C=[c0,c1)`, `E=W∩C=[a,b)` and `L=c1-c0`:

- In d produces `C'=[c0+d,c1)`, `E'=[a+d,b)`, length `L-d`.
- Out d produces `C'=[c0,c1+d)`, `E'=[a,b+d)`, length `L+d`.
- Hence `a-c0` and `c1-b` remain unchanged. The allowed whole-frame intervals
  correctly use In `[ceil(v0-a), min(L-1,ceil(b-a)-1)]` and Out
  `[max(1-L,1-ceil(b-a)), floor(v1-b)]`. The strict positive-width condition needs
  `ceil(width)-1`, including when width is integral.
- Keeping hidden W when the effective edge coincides with C, and otherwise
  changing only the fractional W edge, gives exactly `W'∩C'=E'`. It retains hidden
  filtering context without granting selected audio in fractional padding.
- `p=max(0,-c0')`, `D'=max(D+p,c1'+p)`, then translating C'/W' by p keeps all old
  physical content and the selected allocation within the new owner. No physical
  contraction is needed. Existing Partitions keep their identity.

No arithmetic defect found. Keep explicit tests for fractional width below,
exactly at and above one frame; padding on both sides; retained hidden W; both
handle clamps; crop/re-extension; prefix and tail growth; and exact no-op identity.

## Implementation points

1. Stage signed proposed W/C in checked exact values until prefix p is known.
   `SourceEditWindow::new` rejects a negative start. Construct the final typed
   window only after translation; do not reject valid before-zero extension by
   attempting to construct its temporary negative window.
2. Reuse Slip's complete-span qualification, common positive affine clock and
   selected-support agreement. Matching non-natural rates remain valid. Audio
   selection is `(W∩Ha)-offset`; empty linked audio uses `selected_audio`'s nearest
   support boundary. `None` stays absent. Mapping setters that clear W are not
   suitable for assembling this atomic candidate.
3. Apply `Framing::prepend_owner_frames(p,old_D)` whenever physical duration grows,
   including p=0 at the tail. Gain/mute and audio binding translation use p once.
   Ancestor framing remains live. The admitted positive W already excludes a
   zero-duration Source, so the framing helper's positive old-duration rule is
   satisfied. Preserve the same physical Source ID and create at most one wrapper.
4. Translate only decomposed content points belonging to the physically rebased
   Source by p before reconstruction. Host LeadingEdge/TrailingEdge sentinels
   retain their existing bias semantics; Source PTS and Sequence coordinates do
   not translate. Test direct physical, wrapper-local and ancestor-local marks.
5. Check all bounds, negations, whole-frame conversion, total output duration and
   identity/resource limits before installation. Overflow is an error, not an
   invented source handle. A resolved zero must return exact before==after and
   request no wrapper, root edit or timing capture.
6. Retain the design's independent PCM verification gate for In extension with
   dormant/resumed bindings. Exact coordinate algebra does not itself prove the
   old-entry sample phase or neutral-wrapper edge fade. Changed selected support
   can legitimately alter endpoint filtering; use the final-support oracle.

The visible picture-support-only admission remains an explicit first boundary.
Audio-only picture lead/tail, broader structures, overwrite, adjacent Roll and
complete boundary-picture/waveform Trim mode remain required follow-on work.
