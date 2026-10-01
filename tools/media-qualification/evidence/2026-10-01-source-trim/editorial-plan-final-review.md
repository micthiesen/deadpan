# Independent editorial fade review

2026-10-01. Read-only review of the corrected plan/audio batch. No Cargo, native
execution or checkout edits by this reviewer. Root is running the focused gates.

## Findings requiring correction

### 1. Retained fade width is not the delivered incident voice length

`audio_fades.rs:add_editorial` (lines 347–370 at review) infers a voice interval
from `span.start[0].length` and its progress. For a bound voice, that length is
the old virtual, separately rounded envelope width. It can differ from the
number of samples delivered by the actual retained leaf. Using it for a newly
authored edge can fade a one-sample fragment or reject a valid incident endpoint.

Concrete valid fixture at 48,000 fps, one project frame per output sample:

1. A Sequence contains four Hold(1) leaves.
2. A Preserve Retime maps their input [0,4) to output duration 6. Bind this
   Preserve to the reference document's project clock, with no resume.
3. In the current tree, FollowSpeed maps Preserve [0,6) to duration 9.
4. A unity Partition selects this output [5,9), preceded by Hold(5), with its
   start marker enabled. Thus the owner affine origin remains zero.

At output sample 5, q=10/3 and the selected third reference leaf has exact extent
[3,9/2), but allocated reference samples [3,4). It contributes only output
sample 5; the next leaf begins at sample 6. The new edge must have length 1.
Current virtual envelope width is B(27/4)-B(9/2)=7-4=3. The inferred voice
interval [9/2,15/2) gives the marker length 3, producing an unintended tiny fade.

Keep the old retained envelope behavior. Supply a separate incident sample
interval for new markers by inverse-projecting the retained sample-domain
endpoints through q/step. Intersect it with the marker's consuming B interval
before taking the whole-sample count. Preserve exact authored coordinates for
policy coincidence. Verify whole and paged queries, plus an end-edge counterpart.

### 2. The retained walker reintroduces an already transported owner marker

`bound_fade` transports the first walk's markers into `domain.seed.editorial`.
That capture includes the bound owner's own markers. The retained walk starts
at the same owner; `audio.rs` lines 567–580 adds its markers again without the
incident sample probes. Filtering the transported copy does not filter this
second copy.

Concrete valid fixture: a bound physical Hold/Source of duration 200 at 48,000
fps, with resume phase constant -10 and its own start marker. The marker's
q(B(start))=-10 is outside support, so its transported copy correctly drops.
At output sample 10, retained q=0 enters the voice. The duplicate exact marker
then merges over the retained edge at exact frame 0, replacing its progress 0
with output progress 10 and shortening width 200 to 190. This weakens the first
audible sample's existing fade despite no incident voice at the authored seam.

Suppress only marker sides explicitly transported for that bound owner and
occurrence, even if they are later filtered. Preserve genuinely inner markers.
Add the delayed-entry witness and the parent's marked opaque-owner witness.

## Correct aspects retained by the correction

- The first walk now captures outer markers while stopping at bindings. The
  retained walk selects the actual q leaf, so an unrelated current leaf no
  longer supplies a marker or Hard origin.
- Transported eligibility probes use q(B(start)) and q(B(end)-1), separately
  from exact authored coordinates. Allocation gates precede own-marker creation
  and include Repeat gaps, excluding later Partitions with overlapping hidden
  Source context while preserving marked inner Split owners.
- Exact retained edge coordinates project through the bound owner affine map.
  Hard precedence compares exact coordinates; rounded sample equality alone
  cannot merge policies.
- Markers do not enter raw support/envelope constraints. Raw bound domains,
  resampling and endpoint policy are unchanged. The new PCM path combines
  creative ramps by minimum and validates gains before mutating samples.
- Structural marker traversal, capture, origin deduplication and edge merges
  charge the existing bounded work budget. PCM blocks remain capped at 256
  samples. No unbounded traversal was found in this review.

The later-Partition, hidden-owner continuation, half-sample inner-owner and
ten-sample transported-ancestor tests address the earlier findings. They do not
cover the two cases above. Runtime results remain root-owned evidence.

## Addendum: both findings corrected

Re-reviewed the frozen follow-up production changes and their three regression
tests without editing the checkout or running Cargo/native code. Both reported
defects are resolved by inspection; no further finding in this bounded pass.

1. `add_editorial` now receives an explicit voice sample interval. Ordinary
   leaves use their envelope sample endpoints. Bound leaves inverse-project
   both retained sample endpoints through the same exact q/step map, then
   intersect that interval with the marker's consuming B range. The historical
   virtual width remains separate and unchanged. The new nested
   Preserve/FollowSpeed witness verifies marker length 1 on both start and end
   while the retained edge keeps length 3, and compares bounded versus whole
   queries.

2. Before transporting marker ranges, `bound_fade` records only sides belonging
   to the retained domain's entry owner and exact Repeat occurrence. The
   retained walk consumes this suppression mask once, before adding that
   owner's markers. Filtering cannot erase the suppression, and inner owners
   still introduce their own markers normally. The new ±10 resume witnesses
   compare marked and unmarked plans at delayed entry and early exit, preserving
   old progress and width. A bound opaque-owner witness separately proves its
   distinct inner marker survives.

The suppression mask is initialized empty for ordinary domain creation/capture,
and the interval/suppression calculations do not enter raw sampling support.
Work remains charged to existing bounded capture/marker traversal. Focused
runtime verification is pending with root; this addendum makes no test-pass
claim.
