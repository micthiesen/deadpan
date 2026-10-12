# Self-review

The primary agent reviewed the complete production/test diff without delegates.

- New windows only attach to a validated full identity chain. Their count must
  match the retained grids. The first window is complete; malformed, reversed,
  out-of-range, nonidentity and overflowing inputs fail before reads.
- Physical masks are intersected at each visited node. Empty intersections have
  one canonical boundary so silence spans cannot overlap or duplicate output.
  Query work still charges every visit and emitted span, and traversal remains
  iterative. The existing route node limit bounds additional stored deltas.
- Each transition's delta uses the same declared anchor on its two grids.
  Initial capture validation still uses the original occurrence's anchor and
  full recipe grid/allocation. A later crop does not retarget that input.
- Full identity moves delegate to the new window implementation. Default
  generic Window/Ripple routes retain their original mapping rules.
- Dense scalar sample copies independently verify every rounded historical
  selection, with signed origins, changing anchors and final expansion. The
  real decoder/Preserve regression independently prepares complete nested
  processing before selecting delivery and verifies hidden input availability.
- A clipped silent read still admits its complete source and rejects revocation.
  No media decoding or DSP occurs in deadpan-plan.
- This does not remove the partial-copy guard. Current envelope transport and
  core capture must use identical chronological windows before enabling it.

The initial PCM test failed because its own 257/330/400-frame requests exceeded
the existing 256-frame public reader bound. The corrected test uses bounded
blocks; no production limit changed. Both attempts are retained.
