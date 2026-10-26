# AI Hold requirement reconciliation, 2026-10-25

DP-12 is Complete under the primary-Mac verification scope in specification
§29.1. This is an audit of existing implementation and retained evidence, not a
new inference run or a model-quality pass. All eight fixed DP-12 criteria
already scored 1, so this correction adds no acceptance-coverage percentage.
Strict completion rises from eight to nine requirements. All seven delivery
gates remain open.

## Evidence against the requirement

| Required behavior | Existing implementation and executed evidence |
| --- | --- |
| Immediate deterministic fallback, exact authored duration, separate explicit acceptance | [Insertion](ai-insertion-2026-10-07.md), [Generated Holds](../GENERATED_HOLDS.md) and the Store's revision-bound `accept_generation_bundle`: candidate readiness does not change authored media; acceptance is reversible. |
| Real local Bridge and one-sided Extension generation, both directions | [Packaged Bridge](ai-runtime-2026-10-05.md) and [native Extension](extension-native-2026-10-08.md). The latter retains eight actual 0.5/1/2/3-second direction cases with exact sampling, explicit acceptance, Undo/Redo and model-free portable exports. Scripted replays are separate evidence. |
| Capability and conditioning admission | Typed pack constraints and [conditioning color](conditioning-color-2026-10-07.md) qualify duration, geometry, retained inputs and BT.709-to-sRGB preparation before work. |
| Motion, lighting, endpoint, face/mouth and selected-region checks before Ready | [Motion/lighting](bridge-quality-2026-10-07.md), [endpoints](bridge-endpoints-2026-10-07.md), [landmarks](bridge-landmarks-2026-10-07.md), [regions](bridge-region-2026-10-07.md) and operation-specific Extension evidence retain coverage, rejection and explicit unavailable results. Host qualification recomputes the reports. |
| Variants, keyboard comparison/audition, rejection, regeneration, reversion and stale-job safety | [AI workflow](../AI_HOLDS.md), [boundary changes](ai-boundaries-2026-10-07.md), [scope](bridge-scopes-2026-10-07.md) and [provider controls](hold-controls-2026-10-07.md) exercise the shared attempt lifecycle and captured request ownership. Candidate pictures do not supply generated audio. |
| Shortening, lengthening and changed source boundaries | [Replacement](ai-replacements-2026-10-07.md), [boundary changes](ai-boundaries-2026-10-07.md) and [saved Extensions](extension-saved-2026-10-08.md) preserve accepted provenance and exact sampling, or atomically install the required fallback and replacement intent. |
| Current packaged runtime and portable accepted media | [Private AI launch trust](private-ai-launch-trust-2026-10-25.md) includes actual Check through both hosts, canonical inference and explicit acceptance, offline emitted-file verification, exact Undo and full history validation. The initial missing-directory and unsupported-flag driver failures remain recorded. |

The latest two interior candidates are still failures: Still entry motion is
1.253080/s against 0.500000/s, and Moderate unconditioned-exit luma is 37.185
against 32.000. Both remain rejected with no Ready candidate. They demonstrate
rejection/fallback behavior; they do not establish general seam or model quality.

## Obligations that stay open

- Resident model reuse and fixed text-embedding caching are unimplemented
  §13.5 work. The current MLX worker constructs a pipeline per invocation.
- The distilled 2B/macOS MPS and MLX precision comparisons, cold/warm workload
  distributions, peak memory and interactive performance measurements remain
  mandatory §13.1/§13.4 work under DP-24 and Gates A/E/G. Fresh-worker runs are
  not residency measurements or p50/p95 distributions.
- The rights-cleared real-person corpus, human seam/identity judgment,
  threshold calibration, listening, physical input/VoiceOver and physical
  sleep behavior retain their exact [owner procedures](../REQUIREMENTS.md#to-verify-owner)
  under §29.1. No absent implementation is excused by that rule.

Section 29 maps DP-12 to §12's Hold behavior. The older Partial row mixed those
implemented behaviors with §13 optimization and owner-only checks. Correcting
that classification does not remove the latter obligations, move them into the
model-manager requirement, or close a release gate. Multi-play batching,
split-screen comparison and configurable retention are optional refinements;
the required stable-play scope, keyboard comparison and visible retention
policy are implemented.

An independent read-only audit inspected source and retained evidence. Root
reconciled its findings with the fixed criterion ledger and the normative
scope. No new tests or inference execution are attributed to this audit.
