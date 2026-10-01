# Native Slip UI review

Reviewed `/tmp/deadpan-native-slip-20261001/ui/ui.patch` at SHA-256 `f8703835707d16e2d5dd84f838111bb4a51162c891162118eae80dce50794949` against `native-slip-design.md` and the service API. Scope included command capture, Slip UI/router, preview presentation, update reconciliation, and the final display-retention/invalidation correction.

## Findings

No remaining actionable correctness findings.

The final patch closes both display lifecycle gaps I found. Slip-specific decode errors retain the prior submitted texture, label, canvas, and geometry while the new request remains unqualified for Apply. The UI also cancels and invalidates pending picture identity at Slip entry, amount/comparison/inspection changes, cancel, saved commit, and captured-context invalidation. Service updates reconcile before polling the worker reply. The pure Presentation regression covers an ordinary late reply at entry, a proposed reply already decoded but not submitted, stale success/error replies before replacement dispatch, preservation of the last accepted display, and later recovery.

The footer may cause a bounded extra resize when proposal handles/status become ready and when the displayed-ready status text changes. At a fixed successful raster the state settles. If rendering fails, `render_failed` keeps the Apply ticket unavailable and stops further render attempts, so the footer cannot alternate indefinitely. Record the final size in the assigned-desktop UI replay.

## Verification

Read-only review; I did not run Cargo, native UI, or media tests. The patch author reports scoped rustfmt and combined service/UI/harness apply-check passed; runtime checks remain with the root agent.
