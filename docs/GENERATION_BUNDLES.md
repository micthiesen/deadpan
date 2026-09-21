# Native bridge candidate bundles

[`deadpan-models`](../crates/deadpan-models/) qualifies a native bridge candidate
against its original host request. The worker declares native footage and
provenance. The host derives both canonical masters from the same immutable
native snapshot, using the persisted exact generation plan. Neither completion,
qualification, nor Ready changes an authored Hold.

## Wire and storage contracts

Model protocol 2 uses `generate_bridge` and `completed_bridge`. The request
contains a validated `BridgeGenerationPlan`; completion contains distinct native
and provenance artifact references, SHA-256 hashes, byte lengths, native video
geometry/rate/count, and the exact provider selection. A protocol-1 sampled
candidate cannot satisfy a protocol-2 attempt. Cancellation retains the attempt's
protocol and requires confirmed process teardown before becoming terminal.

Database schema 8 adds an optional immutable request plan and a separate bundle
receipt table. A missing plan means a legacy protocol-1 request. Migration never
invents a plan or upgrades a legacy receipt into media evidence. Schemas 1 through
7 migrate on a consistent copy, validate complete history and operational rows,
retain a pre-migration backup, then promote through SQLite's backup transaction.

`record_bridge_generation_request` checks the plan against the request's project
duration, rate, native grid, and conditioning. Its typed completion is retained
through validation, cancellation, restart recovery, retry, and stale relevance.
`record_generation_bundle_ready` checks every generated object's actual length
and BLAKE3 digest before opening the SQLite transaction. It then atomically stores
the receipt, terminal state, and eligible latest selection. Legacy and modern
selection APIs reject cross-kind use. Selection remains metadata; consumers must
read through verified snapshots because files can become missing or corrupt later.

## Host qualification

Pin the artifact workspace before worker launch and qualify only after clean
process-group teardown. Reconstruct the original request from persisted intent.
Supply `SelectedBridgeProvider` from the trusted host provider selection, separately
from the worker request. Qualification calls `validate_for` to check support,
grid/rate/count restrictions and the nearest legal native frame count. It retains
that exact capability in host provenance. The store remains provider-neutral;
its Ready API relies on this independent host qualification.
`qualify_bridge` performs these checks on a background service:

1. Validate the request, plan, provider, and native declaration. Snapshot the
   bounded provenance file through descriptor-relative containment and its
   declared hash, checking cancellation/deadline during copy chunks.
2. Reject duplicate JSON keys. Require a matching typed request binding, plan,
   seed and native identity, nonempty model asset receipts, source hashes,
   revision hashes, prompt/version, settings and conditioning/color descriptions.
   Check bounded counts, strings, valid hashes, and unique asset identities.
3. Snapshot native bytes with the same controls. Derive canonical native and
   sampled FFV1 masters under the remaining shared deadline, using
   [`canonicalize_bridge`](MEDIA_CONVERSION.md). Independently decode outputs and
   verify exact picture/timing contracts.
4. Produce a bounded immutable provenance envelope containing the original worker
   report's exact UTF-8 bytes, original request/declaration, both generated-object
   identities, and host media-validation reports. Stop serialization at its budget.
5. Publish all three immutable BLAKE3 objects through the store's
   [generated-media API](GENERATED_MEDIA.md), then record Ready. A failed database
   write can leave unreferenced objects; it cannot authorize an incomplete bundle.

Provenance records the worker's asset/runtime claims. Parsing and matching these
claims does not independently prove which model bytes executed. Installed-pack
attestation, host-managed conditioning retention and source-clock/color checks
remain required. Extra bounded backend diagnostics are retained without granting
them authority. Host decoded-media results remain separate from worker reports.

The standalone `qualify_bridge_bundle` example qualifies an already-reaped
developer workspace into new output files. It does not publish to a project or
select Ready. Its output directory may contain partial diagnostic files after an
I/O failure; the project publication API has the durable byte-storage contract.

## Remaining integration

This implementation does not qualify seams, useful motion, appearance, source
speech preservation, audition, accepted-media history retention, application
responsiveness, model installation, or distributable packaging. Generic store
commands still reject new generated authored artifacts. Explicit durable
acceptance must later bind a current selected bundle, verified objects, host
conditioning provenance and an undoable command. No background result may change
the authored provider automatically.

The [real-media integration tests](../native/deadpan-media-worker/tests/bundle_qualification.rs)
exercise exact pixels, malformed/missing provenance, cancellation and size limits,
missing objects, actual three-object promotion, Ready persistence and reopening.
They use a synthetic model-provenance fixture and do not establish model quality.

[Measured local generation and repository verification](qualification/model-bundle-2026-09-21.md)
records the actual model run, output identities, limitations and review results.
