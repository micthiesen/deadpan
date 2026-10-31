# Native query interval observer, 2026-10-31

A default-off, test-only observer records the exact lifetime of actual raw-domain
and Structural Point content constructors. Each row retains both ReadWork lanes,
independent native mask phase boundaries, host-eviction boundaries, the selected
lane and the constructor's literal completion claim. It changes no admission,
canonical weight, cap, existing receipt or production runtime.

Native phases and host eviction use accepted-debit hooks. Different actual
ReadWork ledgers invalidate attribution instead of being subtracted. Reset
epochs detect counters reset while an interval is live, including catch-up past
the old value. Disabled native tracing, unknown epochs, overflow and non-LIFO
closure also invalidate attribution. Storage is bounded to 16 active intervals
and 128 retained rows. The snapshot explicitly reports open/dropped rows,
nesting, misnesting and overflow; a complete-capture claim must check those
global fields before trusting individual rows. Nested rows are inclusive and
must never be added as disjoint work.

Fourteen new tests pass in both builds: seven bookkeeping cases, two private
epoch-overflow cases and five actual native-read cases. Actual checked Point,
held public Point, genuine Preserve Bound, Plan partial-prefix refusal and the
cache pre-constructor budget guard use real WorkControl and the qualified WAV
provider. Both work lanes reconcile independently. The held-mask interval
preserves 36 accepted units, its literal 26-unit completed query claim and ten
separately observed residency units. Its old invalid flag remains true.
The partial case retains actual boundary counters with no completion claim;
no query fee is inferred from a residual.

Three complete 744+10+4 after captures per build agree across both roots. Each
body's 758 snapshots
contain 478 interval rows: 403 raw-domain and 75 Point, including 477 completed
rows and one partial row. All completed rows reconcile exactly in both lanes
from the literal expected query debit, eight independent native phase deltas
and host-eviction delta. The captured partial row is the actual resident-quota
refusal; the accepted Plan-budget prefix is covered separately by the native
test. No row is foreign or unattributable; no snapshot is
open, dropped, misnested or overflowing. These actual matrix intervals are
sequential; same-ledger inclusive nesting and foreign overlap have separate
bookkeeping tests. Four public transfer cases legitimately contain zero query
intervals. No extra constructor is invented to increase coverage.

Observer-off captures remain exactly 34885733 canonical bytes, SHA-256
af7a292718365c34fa03692e2cbe58a2cfb3c6d444b5ae29d52647e1fdbbff14.
Observer-on captures are 35439545 canonical bytes, SHA-256
f64d3a00a10967cc3822083d21705bd454f2d33ff2198da30e25671092ab5a53.
Only variant/source-manifest identity is removed during canonicalization;
reset epochs, old invalid flags, errors and all actual counters remain literal.
Removing only the additive interval fields and restoring the old wrapper's
schema/scope reproduces the entire observer-off body exactly.

All Audio library tests pass, 515 Main and 509 isolated, with 20 manual captures
ignored by the default library run. Required Main strict check with --no-test,
isolated all-target Audio Clippy with warnings denied, formatting and both
hygiene checks pass. Every guarded build/run retains exact source and binary
identities before and after. All 30 immutable before payloads stay fixed.
Main's 2898 and the isolated copy's 2570 other initial named inputs remain exact;
all 15 owned source files agree across roots. Shipping uses the isolated copy.

The observer and new hooks compile only under cfg(test). Unchanged production,
integration and native-worker execution retain the preceding checkpoints'
qualification; they are not rerun or credited as new runtime evidence here.
No requirement or Gate acceptance changes. Plan policy producers, derived
bus/equalizer products and whole-preparation ownership remain open. The next
ownership seam retains the exact final SignalSample PointCeil policy Vec and
its independent lease through an exclusive ledger preflight and immediate
native custody transfer.

Evidence: [compressed inventory](../../tools/media-qualification/evidence/2026-10-31-native-query-intervals/inventory.json).
Inventory SHA-256: b2847fc1d3a1ff2ad0b1aad69771192bc515669cc1e0ac98adf76df2f42df03b.
Its 164 records retain 1344322077 expanded bytes in 125413285 stored bytes.
