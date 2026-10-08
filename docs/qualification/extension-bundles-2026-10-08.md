# Complete extension candidate qualification, 2026-10-08

`qualify_extension` now produces a complete private candidate bundle after
validating the exact request, independent provider capability, pre-launch inputs,
worker provenance, canonical native and sampled media, and all rejection checks.
It round-trips the saved envelope before returning. **DP-12 remains Partial:**
store completion/Ready, accepted extension media, native controls and the larger
measured generation envelope remain unfinished.

## Boundaries

`ExtensionGenerationBinding` retains the V3 request's immutable fields and its
actual extension plan. `SelectedExtensionProvider` retains the host's context
count, generated-frame formula, dimensions, native rate and maximum output
duration. Both have strict wire readers; provider selection is independent of
worker declarations and does not itself attest installed model bytes.

Schema-3 worker provenance must agree on direction, operation, exact binding and
context, provider IDs/versions, seed, native SHA-256/length, generated interval
and all seven rational timing values. Runtime/model/source receipts retain the
same bounds as Bridge. Extra diagnostic fields survive as exact original bytes;
they provide no media authority. Historical schema-1 contexts cannot become
current input evidence by being embedded in a newer worker report.

Qualification requires the explicit `NativeExtensionV3` completion variant.
After caller-owned clean generation-worker teardown, it snapshots and validates
provenance before decoding native output. One shared deadline covers snapshots,
conversion, joins/motion, Vision inspection and the saved-envelope round trip.
Both canonical movies come from one immutable native snapshot. The sampled
movie contains exactly the authored generated-only interval; native context
pictures remain available for evidence.

Host profile `deadpan-ffv1-extension-1` requires every report and input receipt.
`StoredExtensionProvenance` verifies the envelope object, exact saved manifest,
request/provider/plan, canonical conversion reports and measured spans, then
recomputes pixel, face, mouth and selected-region policy. It does not launch a
model or detector or open saved paths. Its opaque `ValidatedExtensionEvidence`
requires later consumers to independently verify every referenced media/input
object and admit media bytes with the returned decoder contracts. It does not
grant authored acceptance or a store Ready state.

The developer `qualify_extension_media` example now runs this complete path.
Its configuration requires `selected_provider` and `QualificationLimits`, with
conversion limits under `media`. Output includes both movies, full host
provenance and all input objects, including continuity signatures. It uses the
host's pre-launch receipt and refuses changed retained inputs.

## Verification

The initial focused runs passed 167 model tests (one configured skip) and 22
real-media integration tests. Tests cover both directions, one-frame and
eight-frame output, required fields, all seven exact clocks, binding/provider
changes, retained input deletion, cancellation and byte limits. Provenance
contradictions fail before a missing native file or unavailable helper can be
opened. A generated flash hidden by downsampling fails before Vision launches.
Saved records reject changed policy, metrics, objects, manifests, duplicate
keys, missing/null reports and operation/profile substitutions. Existing Bridge
qualification, acceptance and relocation tests also pass.

Additional final assertions retain exact worker formatting and check every
returned media/input reader is rewound for publication. The final repository
gate passed 5,372 workspace tests, 1,071 UI tests and two documentation tests,
with locked dependencies, formatting and strict workspace/UI Clippy. Ten
workspace and two UI tests were skipped by the existing gate configuration.

Nextest reported pipe-closure warnings on
`replacement_coalesces_pending_work_and_drops_stale_success_and_failure` and
`historical_recapture_rejects_forged_same_time_child_parent_bounds_and_contents_without_writes`.
Their assertions passed; the cause of these warnings remains unproven.
The [retained evidence](../../tools/model-qualification/evidence/2026-10-08-extension-bundles/manifest.json)
contains exact command results, compressed logs, source and executed helper
hashes, and machine/compiler details.

Independent reviews covered binding/provenance against the actual Python
producer and qualification/stored-reader ownership and validation. No actionable
production findings remained. Root separately reviewed the pure report/receipt
extraction and real-media integration tests.

These tests use real conversion and pinned Vision with explicitly synthetic
model loading claims. This increment does not claim a new inference run,
real-person quality calibration, saved extension acceptance or a larger model
envelope. Earlier real worker measurements remain identified by their original
source and schema; they are not relabelled as current candidate admission.
No new packaged or native UI run was performed: this increment changes host
validation, developer tooling and tests, with no new app control or worker
protocol. Existing Bridge real-media acceptance/relocation remains covered by
the integration tests.
