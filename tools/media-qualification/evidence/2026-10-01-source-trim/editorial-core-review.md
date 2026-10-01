# Editorial edge core batch and plan review

2026-10-01. Core implementation is frozen for root verification. This agent ran
scoped rustfmt only, with no Cargo or native execution. Root reported the focused
Slip/Trim retry passing against source manifest
`af653a57f318bd1474bf900c637dd3833fbcc4177140505c44933bfb33343363`.

## Stable core behavior

- `AudioEditorialEdges { start, end }` is optional, closed persisted BeatNode
  intent. It changes no raw sampling support or existing edge policy.
- Current frozen audio contexts use schema 6. Old document, command, patch and
  frozen-layout grammars refuse the new field, including explicit empty values.
- A marked Partition retains its owner during Split and cannot be refined away.
  Hard is valid only on its marked sides. Ungroup refuses to lose a marked
  Sequence's intent.
- `source_edit::mark_edges` marks the selected allocation and actual incident
  ordinary-ancestor neighbor. It skips empty siblings and stops at a positive
  silent child. Trim calls one side; nonzero Slip calls both. Neither adds timing
  identities or alters root sound routes. Zero Slip refuses before mutation.
- New core witnesses cover exact inverse, old policy preservation, nested scope
  neighbors, positive silence, re-trim, Split/capture, closed JSON and historical
  frozen contexts. Full verification is still root-owned and pending.

## Plan review findings

1. **Corrected by plan author:** a retained leaf's exact edge coordinate was
   overwritten with the independently flattened current leaf's coordinate. A
   resumed Preserve voice can still select A while current geometry selects B.
   Retained A's Hard origin must keep its own exact coordinate. `bound_fade`
   now projects the retained leaf coordinate through the bound owner affine map.

2. **Author implementing:** an ancestor marker must not follow a later child
   merely because its full hidden Source envelope overlaps the marker. Gate
   inherited marker sides when entering each descendant allocation, before
   introducing that descendant's own markers. Include both ordinary and Repeat
   gap paths. This ordering retains a marked inner owner behind a Split crop.

3. **Author implementing:** bound marker capture must follow actual retained q,
   rather than a second independent current-tree walk. The smaller integration
   passes the first walk's EditorialCapture to bound_fade, inverse-projects it
   into the retained envelope domain, captures the actual q-selected leaf and
   projects exact ranges back. Keep raw-domain state unchanged. Use the consuming
   grid to compute B(marker), not the retained leaf grid.

4. **Open regression requirement:** transformed exact structural coordinates
   alone do not identify a transported ancestor marker's incident retained
   voice. Example: a marker lies at the inner A/B seam, but a valid resume puts
   q ten samples before that seam. The new output's first ten samples still
   belong to A. Exact-coordinate gating can drop its marker on A, then retain it
   on later B. A half-sample witness hides the missing A ramp because the first
   incident voice has length one.

   Keep exact authored coordinates for ramp progress and policy coincidence.
   For transported outer markers, one bounded solution stores separate retained
   eligibility probes: q at consuming B(start), and q at consuming B(end)-1.
   Descendant gates compare these probes with their retained allocated sample
   interval. This also excludes later Partitions with overlapping hidden support.
   Inner markers introduced after the outer crop retain their normal owner
   behavior. Verify a ten-sample resumed incident voice, a later hidden-context
   Partition, both edge directions and chunk-independent queries.

When gating removes marker entries, recompute the inherited marker count before
adding the node's own markers. AudioDomainSeed capture slices that prefix; keeping
the pre-filter count risks capturing own entries or an out-of-range slice.
