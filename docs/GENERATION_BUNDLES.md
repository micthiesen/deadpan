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

Database schema 9 retains immutable request plans and separate bundle receipts,
adding optional measured source spans and retained input identities for admission.
A missing plan means a legacy protocol-1 request. Migration never invents a plan
or upgrades a legacy receipt into admission evidence. Schemas 1 through
8 migrate on a consistent copy, validate complete history and operational rows,
retain a pre-migration backup, then promote through SQLite's backup transaction.

`record_bridge_generation_request` checks the plan against the request's project
duration, rate, native grid, and conditioning. Its typed completion is retained
through validation, cancellation, restart recovery, retry, and stale relevance.
`record_generation_bundle_ready` checks the native, sampled and provenance objects'
actual length and BLAKE3 digest before opening the SQLite transaction. Receipts with
admission evidence also require the context manifest and both prepared inputs.
Legacy three-object receipts remain inspectable but cannot be accepted. It atomically stores
the receipt, terminal state, and eligible latest selection. Legacy and modern
selection APIs reject cross-kind use. Selection remains metadata; consumers must
read through verified snapshots because files can become missing or corrupt later.

## Host qualification

Pin the artifact workspace and call `capture_bridge_conditioning` before worker
launch. It freezes the exact context JSON and both prepared input byte streams,
checking SHA-256, byte length, descriptor-relative containment, disjoint input/output
scopes, cancellation and one shared capture deadline. The strict context must match
the request's plan and manifest identity. Frames are opaque at this boundary;
capturing them does not qualify image decoding, source-clock coordinates or color.
Snapshots expose only read/seek, and their BLAKE3 identities form a typed
`ConditioningReceipt`. Identical left/right inputs can share an object identity.

Qualify only after clean process-group teardown. Reconstruct the original request
from persisted intent and pass the already retained inputs in `BridgeQualification`.
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
   Check bounded counts, strings, valid hashes, and unique asset identities. Require
   the worker's complete context to equal the independently captured host context,
   including both prepared input declarations and the color interpretation.
3. Snapshot native bytes with the same controls. Derive canonical native and
   sampled FFV1 masters under the remaining shared deadline, using
   [`canonicalize_bridge`](MEDIA_CONVERSION.md). Independently decode outputs and
   verify exact picture/timing contracts.
4. Produce a bounded immutable provenance envelope containing the original worker
   report's exact UTF-8 bytes, original request/declaration, both generated-object
   identities, host media-validation reports, measured native/sample source spans,
   and conditioning receipt. Envelope schema 3 uses profile `deadpan-ffv1-bridge-3`.
   Earlier envelopes lack some of this evidence and must not be treated as equivalent.
   Stop serialization at its budget.
5. Return both masters, host provenance and all three immutable input snapshots.
   Publish these through the store's [generated-media API](GENERATED_MEDIA.md).
   Construct admission evidence from the measured spans and retained inputs. Ready
   and the dedicated [acceptance transaction](GENERATION_ACCEPTANCE.md) verify all
   six objects. The store trusts host qualification; it does not decode them.

Provenance records the worker's asset/runtime claims. Parsing and matching these
claims does not independently prove which model bytes executed. Installed-pack
attestation and source-clock/color checks remain required. Extra bounded backend diagnostics are retained without granting
them authority. Host decoded-media results remain separate from worker reports.

The standalone `retain_bridge_conditioning` example captures prepared input bytes
before generation into a new host directory. `prepare_run.py` invokes it before
writing the launch configuration. `qualify_bridge_bundle` reloads those retained
bytes and verifies their identities separately from the completed worker's
workspace. Its configuration requires a `conditioning` object containing that
host workspace, input scope, manifest declaration and capture limits. Never use
post-completion worker inputs as pre-launch evidence. The examples do not publish to a project or
select Ready. Its output directory may contain partial diagnostic files after an
I/O failure; the project publication API has the durable byte-storage contract.

## Remaining integration

This implementation does not qualify seams, useful motion, appearance, source
speech preservation, audition, application responsiveness, model installation,
or distributable packaging. The dedicated store acceptance API binds a current
selected bundle, verified dependencies, and one undoable edit. Generic store
commands still reject new generated authored artifacts. No background result may
change the authored provider automatically. History dependency inventory, cleanup,
portable copy and application rendering remain open.

The [real-media integration tests](../native/deadpan-media-worker/tests/bundle_qualification.rs)
exercise exact pixels, malformed/missing or substituted provenance, cancellation
and size limits, deleted worker inputs after host capture, missing objects,
six-object publication, Ready persistence, explicit acceptance, undo/redo/revert
and package relocation with all worker input/output files removed.
They use a synthetic model-provenance fixture and do not establish model quality.

[Measured local generation and repository verification](qualification/model-bundle-2026-09-21.md)
records the actual model run, output identities, limitations and review results.
