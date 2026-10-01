# Independent review: combined Trim geometry

Scope: `/tmp/deadpan-source-roll-20261001/staged-trim-geometry/trim-geometry.patch` at SHA `c6815f4445f1a404f1c909f86679f789bb6d69387205319da37c1e65fe6ea556`, its README/manifest/base hashes, `combined-trim-design.md`, `trim-design-decisions.md`, spec §§6.3 and 7.7, and current `source_edit::edge` behavior. Read-only review; no Cargo/native execution.

## Findings

1. No actionable correctness finding in the staged geometry scope.

The candidate equations and affine constraints agree: A's source allocation/effective interval use `(I, O+R)`, its media maps shift by `-S`, B uses `(R, 0)`, Ripple translates the selected pair and suffix by `K=O-I`, and Overwrite leaves scope/project duration fixed while reporting B's `[U+R,V)` as pre-overlay placement. The fixed-scope bounds constrain A's proposed range; `requires_overwrite_overlay` is set for nonzero In/Out, and the staged documentation explicitly leaves overlap resolution, neighbor clipping, and structural installation to the later overlay stage.

The hidden-window endpoint rules preserve a hidden side only when the corresponding delivered endpoint remains integral at the allocation edge; otherwise they move its exact fractional pad. The simultaneous builder retains that rule independently for both ends. Prefix conversion and the physical-duration inequalities cover both `old_duration + prefix` and the prefixed final allocation end; Ripple project bounds and Overwrite scope bounds are checked separately. The phase hint removes physical prefix before comparison, reports the exact nonempty old/new allocation intersection when present, and selects the old closed end/start for wholly later/earlier allocations, including equality at the half-open boundary. Missing, invalid, and nonadjacent right neighbors remain unavailable without blocking non-Roll intent; Roll requires the captured adjacent eligible pair.

The staged tests include literal geometry and source-index expectations for fractional pads, VFR, signed PTS, prefixes, dormant/absent audio, disjoint/partial phase anchors, policy bounds, and overflow. I did not run or compile them; the manifest records only scratch rustfmt and apply-check.
