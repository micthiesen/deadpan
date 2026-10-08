# Extension store integration after temporal-context qualification

Read-only design, 2026-10-07. No repository changes or checks performed for this report.

## Current boundary

The development V3 worker, generated-only sampler/converter, strict extension
conditioning receipt, native-spaced definition context, structural coverage and
host shot rejection now have concrete implementations. This does **not** provide
durable V3 request admission, Ready, acceptance or automatic renewal.

`ExtensionInputs.continuity` is currently an in-memory CLI report. The schema-1
`ExtensionContext` manifest binds the K PNGs and optional opposite PNG, but does
not bind that continuity report, its intervening support or detector result.
Persisting the manifest alone would lose the evidence that made the context
eligible. A hidden cut/source jump between K samples must invalidate a request
even when every PNG supplied to the model remains identical.

The present hardcoded boundaries are:

- `store/generation.rs`: `StoredGenerationRequest.bridge_plan`, nullable SQL
  `bridge_plan`, bridge-only plan validation and allocation.
- `store/generation_attempts.rs`: V3 explicitly rejected; native declaration
  parsing/protocol restored from whether `bridge_plan` is present;
  `BundleValidationReceipt.plan` is Bridge, `BundleInputObjects` is manifest/L/R.
- `store/generation_origins.rs`: `RequestOrigin.bridge_plan`; input binding is
  duration/rate/canvas/L/R; bundle proof constructs `NativeBridgeV2` directly.
- `store/generation_intents.rs::capture_inputs`, preparation fulfilment,
  `boundary_transition.rs` and `boundary_replacements.rs`: the same L/R binding.
- `cli/generation_context.rs::identity_in`: another L/R implementation. It
  compares origin/current inputs, then returns the original manifest hash.
- `core/generated.rs::GeneratedArtifact.sampling`: `BridgeSamplingMap`.
  Acceptance constructs this map; model-free reads use `StoredBridgeProvenance`.

## 1. Share the exact input contract below CLI

Add a focused `deadpan-store::generation_inputs` module. It owns deterministic
input identity derived from the document, compiled plan and retained measured
indexes. It performs no decoding, inference or sidecar reads on the writer.
Move `GenerationInputBinding` here and re-export it temporarily from origins to
limit churn. CLI capture, resolver, intent births, accepted origins and automatic
closure must all use this implementation.

Suggested types, with strict versioned serialization and validated constructors:

```rust
enum GenerationCaptureSpec {
    Bridge,
    Extension {
        direction: ExtensionDirection,
        native_rate: FrameRate,
        context_frames: u32,
        capture_policy: ExtensionCapturePolicy,
    },
}

struct GenerationInputBinding {
    duration: FrameDuration,
    frame_rate: FrameRate,
    canvas: [u32; 2],
    inputs: GenerationInputs,
    region: Option<CapturedTargetIdentity>,
}

enum GenerationInputs {
    Bridge { left: Option<GenerationPictureIdentity>,
             right: Option<GenerationPictureIdentity> },
    Extension {
        capture: ExtensionCaptureSpec,
        samples: Vec<RelativePictureIdentity>, // chronological, exactly K
        opposite: Option<RelativePictureIdentity>, // explicitly unconditioned
        support: Vec<RelativePictureSupport>, // all affine spans + terminal
    },
}
```

Each support entry retains its exact relative half-open definition interval and
provider identity plus first/last measured ordinals in playback order. The
terminal is explicit, not an invented epsilon-sized interval. Its closed
endpoint matters at cuts. Include the capture/continuity policy version. Use a
small shared provider table if needed to avoid repeating large generated-object
identities in every span. Missing endpoints remain distinct from authored black.

All positions are relative to the conditioning anchor. Do not hash revision IDs,
absolute definition coordinates, instance/node paths, captions, gain, or framing.
Those fields prove where a capture came from but are not model inputs. Keep the
saved target record (including corrections/provenance) when a region is selected,
as the current CLI resolver does. Controls/mode are immutable request facts.

The binding is a structural eligibility descriptor. Detector measurements are a
separate host receipt; they are not rerun on the writer. A changed support
descriptor requires a new worker-side qualification even if K identities agree.

### Required shared read APIs

```rust
capture_input_binding(document, plan, target, capture_spec, options, pictures,
                      budget) -> Result<GenerationInputBinding, InputError>
capture_input_bindings_batch(..., shared_budget) -> Result<Vec<_>, InputError>
```

`GenerationPictures` currently exposes only single-picture identity. Extend it
with a bounded span observation, implemented by `QualifiedGenerationPictures`
using its existing per-receipt cache. Factor the exact `selected_ordinals` math
out of CLI into a pure plan/index helper, for example
`DefinitionPictureSpan::source_ordinals(&SourceFrameIndex)`. CLI and store must
not maintain competing exclusive-end/reverse/endpoint-clamping implementations.
Accepted sampled masters have exact retained CFR/count authority; legacy
unqualified Accepted/Still remains explicit unsupported evidence.

Preserve one compiled plan and one aggregate work/metadata ledger across a
transition. Existing per-query 512-span and 64 MiB limits must not silently reset
for each target/chunk. Distinguish per-input maximums from transition-wide limits.
Reject before cloning/allocating beyond bounds; no partial successful binding.
Keep typed unavailable causes rather than comparing diagnostic text.

## 2. Keep preference, resolved operation and duration plan separate

Add explicit `GenerationModePreference::{Automatic, Bridge, ExtendFromLeft,
ExtendFromRight}` to captured options. Resolve once against the exact authored
definition and available declared capability: Automatic uses Bridge with L/R,
FromLeft with only L, FromRight with only R; neither yields an unavailable
preparation retaining the authored fallback. An explicit choice never changes
direction silently. Interior extensions retain an unconditioned opposite seam.

Persist the resolved `GenerationCaptureSpec` alongside the preference before
asynchronous capture. Accepted-origin renewals retain their original resolved
operation and controls. A renewed capture remeasures inputs for that operation;
it cannot borrow today's UI defaults or silently become a bridge. Re-evaluating
Automatic after a later edge change should be a separate explicit policy,
versioned in the intent contract, rather than accidental worker selection.

For automatic insertion with no installed model, the birth still records intent
and deterministic fallback atomically. A declared capability/capture policy can
be retained without runtime availability; do not claim an arbitrary K/rate was
qualified merely because the store can encode it. The current development K9,
E8, 768x320, 24fps envelope remains explicitly development-only until approved.

## 3. Add explicit operation sums; never infer V3 from optional endpoints

In jobs add `GenerationPlan::{Bridge(BridgeGenerationPlan),
Extension(ExtensionGenerationPlan)}` with strict operation tags. Shared getters
provide output count/rate, native count/rate, dimensions, capture spec and protocol
version. Preserve both plan constructors and their distinct arithmetic.

In core add `GeneratedSamplingMap::{Bridge(BridgeSamplingMap),
Extension(ExtensionSamplingMap)}` and use it in `GeneratedArtifact`. Shared
getters retain existing count/rate/interpolation/native_position APIs; temporal
duration reporting dispatches on operation. Extension's retained native count
is K+E, generated interval is E, usable authored output is N. Shortening uses the
sampled master's prefix; extending within original N reuses it. Context handles
never become extra output. Never derive speed from today's shortened duration.

A new development schema can refuse old packages, without migrations. Preserve
BridgeSamplingMap/BridgeGenerationPlan arithmetic and primitive wire tests;
wrapping the authored artifact in an explicit sum is permitted. Existing bridge
admission/provenance semantics and actual media tests remain mandatory.

Store request `plan: Option<GenerationPlan>` is the least disruptive shape if
V1 remains supported: None means only V1, Some Bridge means V2, Some Extension
means V3. Record/parse tagged candidate declarations and check the operation
against request plan, constraints direction, raster, rate, N and provider.
Never decode the same untagged native manifest as V2 solely because a plan exists.

## 4. Preserve authorisation and relevance through every lifecycle

- `generation.rs`: persist the capture spec and immutable origin input binding
  with the request. Keep manifest `context_sha256` separate from the descriptor
  hash. Independent allocation validates/rederives the descriptor at the exact
  origin target/revision. Schema bounds cover both before JSON allocation.
- `generation_preparations.rs::fulfil_generation_preparation`: accept the plan
  sum, check claim/current activation, captured operation, duration, target and
  descriptor against birth/current authority, then allocate request and attempt
  atomically. A claimed extension cannot fulfil as Bridge or change K/direction.
- `generation_intents.rs`: operation-aware birth capture and chronological
  replay, including Unavailable, renewal, cancellation, capacity, retirement and
  Redo. Immutable birth projection and retained origin remain the authority;
  operational cancellation/failure must not synthesize new permission.
- `generation_context.rs`: call the shared capture logic with request/preparation
  capture spec. Compare independently recaptured origin/current bindings. On
  equality return the original manifest hash, as today; do not hash absolute
  `DefinitionPictureSample` or trust host-passed JSON as current proof.
- `generation_origins.rs`: request origin stores `GenerationPlan`, capture spec
  and full binding. Rebuild against immutable original revision during full
  validation and prove exact operation/declaration/bundle/acceptance. Current
  moved/scoped target is never substituted into original worker evidence.

Keep existing cancellation/acceptance behavior: explicit provider/revert closes
automatic intent; operational failure does not; explicit acceptance closes the
head while preserving compatible request relevance for other Ready variants.

## 5. Final-fallback closure needs actual support dependencies

`decisions.rs` currently consumes `&[[MismatchTerm; 2]]`. Replace this with a
bounded flat term array and per-node ranges, not a fixed K+2 array. Add an
aggregate term/edge bound and charge construction, reverse indexing, SCC traversal
and evaluation. Keep one fixed SCC partition, dependency-first evaluation,
all-retained then all-fallback fixed-point tests, and the explicit conservative
cyclic-group reason. Deterministic order and nonrecursive bounded memory remain.

Derive dependencies for every model sample, opposite reference, **and every
intervening support provider**, including a generated Hold entirely between two
K coordinates. Deduplicate repeated terms when valid, but never discard hidden
providers merely to fit an assumed degree. A single context can cross hundreds
of short generated Holds under Retime; actual degree can exceed K+2.

Reuse plan-branded Hold witnesses. Generated accepted/fallback alternatives may
change all covered ordinals, not just a span's first picture. Extend the narrow
fallback span observation to prove the entire queried terminal Hold interval;
fallback is currently Freeze or Background, so its alternative is constant.
Cutaways and implicit gaps do not become that Hold's dependency. Do not recompile
one complete plan per alternative or invent an arbitrary epsilon endpoint.

Compare descriptor constants and each dependent atom against the required origin
binding. Structural support changes are a constant mismatch; each provider atom
can use current/fallback equality to form an OR term. Apply final decisions to
the full binding, then independently verify **every retained** accepted input
against the final provider set. Renewal births use those final bindings, including
live InsertedPause origins. Preserve the existing reversible base-command wrapper
and exact allocation/Compound/isolation/history proof.

## 6. Ready and acceptance require a real extension admission branch

Generalize `BundleInputObjects` into operation-specific input collections:
Bridge retains manifest/L/R; Extension retains manifest, ordered K context PNGs,
optional opposite and a host continuity evidence object. Provide `objects()` for
all retention/cleanup/portable-copy callers; remove hardcoded three-input loops.
Bind ordered roles, lengths and hashes, allow identical input PNGs by content,
and reject contradictory aliases/output-input aliasing. Keep aggregate limits.

The extension host provenance/receipt must retain and bind:

- V3 request, explicit plan/direction/provider and complete conditioning receipt;
- capture-policy version, full relative support descriptor and detector report;
- all supporting immutable source/generated identities and measured index facts;
- native and sampled objects, independent conversion receipts and full exact
  clocks/counts/color/no-audio verification;
- operation-specific generated-interval quality, single-anchor region/geometry,
  and present/absent/unconditioned seam reports with honest coverage.

Implement `StoredExtensionProvenance` beside `StoredBridgeProvenance`, then a
narrow accepted-evidence sum dispatching by artifact sampling operation. Extension
validation must not reuse Bridge's N+1/M-1 timing or require a second conditioned
endpoint. Current worker completion plus canonicalization is insufficient for
Ready until these reports exist. No models, installed pack or current Hold
duration may be required for reading accepted media later.

`generation_attempts.rs` then admits CompletedExtension distinctly, restores its
V3 lifecycle, records the host-qualified extension bundle, and revalidates all
objects at selection/acceptance. `generation_acceptance.rs` keeps its existing
atomic relevance/scoped-isolation path, constructing the correct sampling variant.
Origin proof must match the exact request/attempt that the acceptance used.

## Independent write boundaries and next complete slice

1. **Core/jobs contracts owner:** new sampling/plan sums and captured mode/spec;
   core document/command invariants and exhaustive pure consumers. Freeze this
   interface before other owners change shared call sites.
2. **Identity/closure owner:** new store generation_inputs, generation_pictures,
   origins input capture, intents/preparations, boundary_replacements/decisions,
   boundary_transition and CLI generation_context. Root owns schema/lib/audit/
   history integration. No concurrent separate writer to origins/preparations.
3. **Qualification/admission owner:** models extension provenance/quality/seam/
   region readers plus store generation_attempts and generated-media retention;
   request/acceptance integration is root-owned to avoid overlaps with owner 2.
4. **Root/host:** CLI request/capture/fulfil pipeline, generated-reader dispatch,
   preview/accept/revert/variant and timing consumers, packaging/provider gating.

Preferred next bounded milestone is the complete **deterministic metadata path**:
shared operation-specific input binding, all-support relevance, variable-degree
final-fallback closure, durable births/renewals/claims/history/retirement. Keep V3
Ready rejection until the separate real admission branch is complete. This is a
useful independently verifiable slice, explicitly not completion of one-sided AI.
Then land an actual V3 request-to-qualified-Ready-to-explicit-accept vertical slice,
including model-free reopen/export. Do not enable the native capability between
these milestones.

Required regressions: hidden one-frame cut and non-anchor Source slip with all K
unchanged; unchanged group move/gain/Camera; absent versus black/opposite seam;
fractional rates and reversed support; >K+2 generated dependency chain/cycle and
permuted order; alternative fallback restores another accepted context; stale
claim/late V3 completion/wrong operation; pending and Fulfilled renewal/cancel;
Undo/Redo, compacted births/origins and tampering; every input/support object
survives cleanup/portable copy; exact extension acceptance, shortening, revert,
model-free read/export; unchanged bridge real-media/admission tests.
