# Public Point suppression output ownership, 2026-10-31

The final reference-labelled mask returned by StageAudio::read_point_domain now
owns its admitted destination storage and a closed controller-associated charge.
Holding a result continues to consume residency during later requests; dropping
the local request or controller cannot invalidate the result. The public Rust
field is an immutable PointSuppressionMask and PointDomainAudioBlock no longer
derives Clone. JSON, signed reference labels, PCM and ordinary semantics remain
unchanged. No production cap, prior canonical weight or refund changes.

This is the public destination prerequisite only. Internal SignalBlock/ReadBlock,
policy input storage, Bound callbacks and transfer/aggregate outputs remain
ordinary Vec storage. No Point-policy admission, internal lease handoff, complete
preparation, total-process memory, latency or listening claim follows.

## Before witnesses

Before production changes, two qualified serial native runs per source variant
captured the current complete 744-request body and additive ownership cases.
All prior immutable payloads stayed exact. New independent fixtures are:

- owned-suppression-native-baseline-v1: 27,674,397 bytes,
  SHA-256 3d6338e4e4b0f5617647ab6e0fa4d80bf8882a87c87a55efe306002c3011dbdc.
- owned-suppression-output-baseline-v1: 2,163,557 bytes,
  SHA-256 a52d53b51b86311ed3794e38bfeec9b8ce5c3c9027ad8d6ee7b7516c993e18a2.

The latter contains five private reads and three direct public Point reads.
Both checked 514-frame alternating SilentHold/Source aggregate reads actually
return 257 mask ranges at capacity 257 through shared equalizers and shared
input. This is observed output, not a fabricated inline-buffer premise.
Public SilentHold returns [16,32); public Source and genuine Bound cases return
empty masks. All successful result snapshots remain exact after request/provider
and controller destruction. Public internal work is inaccessible and is not
replaced with a surrogate ledger. Existing requested Vec layouts describe
ordinary unadmitted input storage, not allocated-byte/RSS measurements.

## Ownership and accounting

Only the mutable controller publishes charges; output threads can only release.
A lazy closed Arc account survives through unique Box-backed masks. Admission
precedes both destination Vec and Box allocation. Mapping retains its ordinary
input through the final control check. Failed mapping/work/quota checks free
pending storage before releasing the payload charge. Every closed account handle
uses Arc::into_inner, including concurrent final drops, with no Weak/raw escape.
The pinned safe unbox helper frees Box storage before Vec, then releases its
charge. The last account handle retires its shell after the Arc allocation frees.

Pinned Rust 1.97.1 aarch64-apple-darwin actual layout probes and real-code tests
establish: account Arc 24 bytes/3 PCM-equivalent frames; destination Storage 40
bytes/5 frames; each range 16 bytes/2 frames. A one-range output retains 10 frames
including the account base. The mask shell retains the old Vec's 24-byte size and
alignment. These are requested layouts under this compiler, not allocator or RSS
observations. Static controller/output metadata and existing PCM/dependency
copies remain outside this mask account.

All three real ceiling paths include held masks: central make_room_for, Original
warm prediction and optional Original cache publication. Cache PCM is not billed
again as mask storage. New work receipts classify accepted account, vector,
storage, per-range mapping and live-account arithmetic units separately. Existing
canonical query work is unchanged. Storage work is accepted immediately before
Box construction; failed mapping does not claim that phase. A two-range initial
constructor's six successful units and every shorter accepted prefix reconcile
against its actual request ledger without refunds. The ordinary test lane keeps
the original Vec iterator/map/collect specialization and creates no account.

## Verification

All 21 Audio executables pass in both source variants: Main 1,044 and isolated
shipping 1,038 tests, each with 14 ignored captures. The isolated library passes 457
and Main's library 463; 581 integration tests pass per variant. Main retains its
seven explicitly filtered unfinished Split research tests; the isolated copy
does not contain them.

Sixteen added checked tests cover the immutable surface/layout, account lifetime,
concurrent final drops, public output retention, exact shell-only error cleanup,
the literal six-unit accepted phase prefixes, initial stop/mapping errors,
cross-request pressure, both real Original cache gates and two complete frozen
native bodies. The public source test performs real WAV reads; SilentHold
suppression is established by policy rather than inferred from zero PCM.

Native executables run serially across roots. After strengthening the two
reviewed assertions, fourteen relinked executables were re-executed per variant;
seven byte-identical final executables retain their prior passes. Three larger
integration targets use four test threads; library/oracle runs use one.
The only subsequent edits are three scoped tool-lint expectations documenting
required Box deallocation order and actual iterator witness operations.
The archived lint-only source proof verifies all remaining runtime source bytes
are identical; final post-lint libraries are recompiled and re-executed, 463/457.

Main cargo xtask check --no-test passes, including strict Audio/Core/Plan/Store
linting. The no-Git isolated copy passes explicit all-target Audio Clippy with
-D warnings and formatting. Audio doctests pass with zero examples in both
variants. Hygiene passes; Main's target is reduced from 40.0 to 19.7 GiB, freeing 19.21
GiB. No display, device listening, model, app startup or packaging check is
claimed for this backend-only change. Plan/Core tests are retained from their
preceding unchanged shipping qualification, not rerun here.

Independent source review found no material ownership or quota defect. It
recommended placing the storage work checkpoint immediately before Box::new
and tightening shell-only cleanup and per-phase prefix assertions. Those
refinements were applied and verified. The cheaper mechanical
agent located the three ceiling paths and API callers; it declined the test
implementation while the new module was absent. The parent implemented and ran
those native cache tests. Standalone probes and source review are evidence
separate from native project execution.

All 28 immutable payload hashes remain fixed. The complete eight-case body agrees
in ordinary and owned modes. The default native regression preserves every field
of all 744 pre-output observations, including current Point/raw/retained/Original
receipts, 264 successes and 480 typed errors. Seven inherited unfinished Split
research cases remain filtered and general sound-bearing Split stays disabled.
Requirement scores and Gates do not change.

The source audit preserves Main's 2,861 and the isolated copy's 2,533 unowned
initial inputs exactly. Main's pre-existing test code inside the one differing
owned file is unchanged after removing only the new module hook; the shipping
index uses the tested isolated file. No unowned source is published.

Evidence: [compressed inventory](../../tools/media-qualification/evidence/2026-10-31-point-suppression-output/inventory.json).
Inventory SHA-256:
c5709e04a5bb8431522a15c15dfc1bbddfa48843d38d11e53ba14c3320f072c6.
Its 539 compressed records retain 428,678,346 expanded bytes in 61,143,220 stored
bytes. Full Main test-file research and executable copies are excluded.

## Next

Extend explicit ownership through internal SignalBlock/ReadBlock and real
Bound/transfer/aggregation/cache seams, then admit SignalSample Point policy on
the actual PointCeil grid using a sealed final-mask handoff. The ordinary input
Vec in this checkpoint is not an admitted policy lease. Keep exact before bodies,
capacities, issuer ownership and overlapping replacement allocations; do not
substitute Root-grid relabeling or an unadmitted clone. Other providers, Graph
containers and the remaining DP-01 through DP-24/Gates A through G goal stay open.
