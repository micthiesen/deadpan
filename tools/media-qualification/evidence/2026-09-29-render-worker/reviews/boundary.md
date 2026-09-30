# Render host and protocol review

Result: no actionable findings in the current reviewed sources.

Scope: `render_worker/protocol.rs`, `protocol/tests.rs`, `mod.rs`, `host.rs`,
`host/tests.rs`, and `tests.rs`. Review is independent of the child implementation.

Evidence inspected:

- Every received response is semantically validated, version checked, and bound
  to both request and attempt identity. Completion additionally matches the
  complete captured contract, authored-document SHA-256, fixed artifact path,
  exact byte count, and admitted byte budget. Unknown nested fields and
  unsupported pixel policy variants fail deserialization.
- Wire clock and geometry claims are recomputed with the same exact origin
  sample boundaries, rational time base, nearest-even raster rule, and shared
  dimensions/readback bounds as the captured encoder contract. They do not
  deserialize or replace that trusted contract.
- Full-document hashing writes the complete validated snapshot through a bounded
  hash writer with cancellation/deadline checks. Authored maps use ordered
  BTreeMaps, so reopening the same revision does not make a different hash due
  to randomized map ordering. Same-labelled revision tests exercise changed
  authored content, not merely changed revision identifiers.
- The host preserves the first specific child diagnostic when a later nonzero
  exit also generates a supervisor fault. Progress regressions cancel the work
  and remain failures; progress callbacks stop after a known failure/cancellation.
- The host accepts completed bytes only after the generic supervisor releases
  completion alongside clean process/group/pipe termination. Independent
  descriptor-relative snapshotting verifies the declaration's hash and length;
  every Y/Cb/Cr plane of every frame is then checked against the declared legal
  code ranges. The admitted snapshot survives workspace and package removal.
- Snapshot cancellation/deadline interruptions map back to their corresponding
  host error variants. Read-only project opening is followed by the outer
  control check before its result is propagated. Final admission checks the
  same deadline and cancellation state after copying and plane validation.

Verification limits: source review only. No Cargo, formatting, tests, native
execution, repository changes, commits, or pushes were performed. Existing and
new tests were inspected but not treated as executed evidence. Full encoded
export, audio, publication, and all child details are outside this review.
