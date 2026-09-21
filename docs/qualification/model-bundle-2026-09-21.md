# Native bridge bundle qualification, 2026-09-21

This slice implements model protocol 2, host qualification of native footage and
provenance, and schema-8 storage for complete Ready bundles. It does not authorize
generated authored Holds. All product requirements and delivery gates remain open
or partial.

The tested source is the commit containing this report, based on
`63531f78799d5329f5c9fa7f246a9144f9dc156f`. [Retained JSON evidence](../../tools/model-qualification/evidence/2026-09-21-native-bundle/)
records the original request/declaration, exact worker source hashes, model asset
receipts, environment, host media reports and cross-process object readback.
Input images, model weights and generated footage remain local.

## Actual local generation

One supervised MLX generation used the already-installed pinned LTX source
`3392d75934120b7e69eefbe55893f7ef82be92a4`, LTX q4 pack
`56a5866d638ecfe37c54d348e88938235185c2d4`, Gemma pack
`86cc6a8dedbc456dd0e4af01a9d09f396f77e558`, seed 1, and the existing pair of prepared
768 × 320 RGB fixtures. Preparation explicitly assumes sRGB; original untagged
movie color remains unqualified. The requested Hold has 30 interior frames at
30000/1001 fps. The exact plan chooses 25 native frames at 24 fps and excludes both
conditioning endpoints from the sampled result.

The worker completed with clean process-group teardown and no supervisor faults.
The elapsed host run was 118.49 seconds, including startup, source/model checks,
inference and snapshot capture. The worker recorded 75.68 seconds inside its
backend, 14,499,708,928 bytes process peak RSS, and a 17,767,443,772-byte MLX peak
counter. These are different counters, not additive totals. The provenance
retains 31 model asset receipts and 102 loaded LTX source hashes.

The host independently qualified the complete bundle in 5.04 seconds through the
pinned LGPL FFmpeg 8.0.3 helper. Its native and sampled decoded RGB hashes match
the worker's independently calculated Python outputs exactly:

| Master | Contract | Decoded RGB SHA-256 | FFV1 bytes |
| --- | --- | --- | --- |
| Native | 25 frames, 24/1 fps, 768 × 320 | `be4ffce12b048457bcab1779eefe8683f41c242d013be7f488c25416491efe1a` | 4,544,157 |
| Sampled | 30 frames, 30000/1001 fps, 768 × 320 | `eda362044e57353435b56756be4143419a3bb6ed52bb2f849102b3eff962c71a` | 5,450,240 |

The immutable host provenance envelope is 43,689 bytes and retains the worker's
exact original UTF-8 report alongside host decode evidence. The two masters and
provenance were then individually published through the generated-object API,
the package was relocated, and separate processes reopened and verified all three
objects. No model was consulted during readback. This developer object-storage
probe makes no Ready selection or authored acceptance.

A second real protocol-2 run cancelled at the inference stage. It finished in
56.02 seconds including startup/model checks, reported `Cancelled`, clean teardown
and no supervisor faults, and admitted no candidate bundle. This measures the
whole cancellation probe, not latency from cancellation request to process exit.

Environment: Apple M5 Max, arm64, 128 GiB RAM, macOS 26.5.2 (25F84), Rust 1.97.1,
AC power, with no reported thermal or performance warning. Files were already
downloaded, cache state was uncontrolled, and source review/store fixture work
ran concurrently. This single run is functional evidence, not a performance
distribution or an editing-load benchmark. The development adapter still uses
the developer FFmpeg CLI for preprocessing and lossless RGB H.264; only the host
converter uses the selected LGPL shipping candidate.

## Reproduction and coverage

Build the jobs examples, prepare a fresh run with `prepare_run.py --frames 30
--fps 30000/1001 --seed 1`, and run `qualify_model_worker` using its saved host
configuration. `verify_run.py` checks the frozen native copy and provenance.
Run the `deadpan-models` example `qualify_bridge_bundle` with the saved
`qualification-config.json`. Its paths must point at the local pinned helper,
completed workspace and a new output directory. The archived configurations
retain the exact original invocation values; they do not include the inputs.

The real-media fixture test composes qualification with project storage: Ready
fails before any objects exist, fails when provenance is missing, succeeds after
all three are verified, survives reopening, and leaves authored state unchanged.
Additional tests reject mismatched request bindings, invalid or duplicate asset
receipts, duplicate JSON keys, missing provenance, wrong native identity,
cancellation and size-limit failures. Artifact copying is tested for deterministic
mid-copy cancellation and deadline interruption.

Review required stronger provenance fields, cancellable file snapshots and
bounded serialization; those corrections are included. Store review also found
an eviction availability mismatch and constructor invariants bypassed by JSON
deserialization. Final review added an explicit host-selected capability check,
including nearest legal native count, and rejected native/sample object aliases
when their video contracts disagree. Equal-contract dedup remains valid.
The captured real model output was requalified and republished through the final
host path after those changes. Their corrections and regressions precede the final repository
gate recorded with this evidence.

The final repository gate passed formatting, workspace Clippy with warnings
denied, 319 Rust tests with none failed or ignored, workspace build, and headless
diagnostics reporting database schema 8/core schema 5. All 83 Python checks passed:
20 audio, 54 model, five FFV1 report and four native harness tests. The initial
workspace run failed one CLI assertion that still expected schema 7; its failure
log is retained, and the corrected schema-8 assertion passes. The updated
review regression also needed a Clippy correction for a redundant boxed-value
allocation; that failed lint log is retained and the final gate passes.
The updated
independent media verifier also passed an actual legacy snapshot and rejected an
altered protocol-2 provenance copy. Source hashes were compared before and after
the final gate.

## Remaining qualification

The host checks the structure and binding of model/runtime claims; installed-pack
attestation remains separate. Host-managed conditioning retention, source clocks,
color transforms, seam/identity/motion quality, audition, explicit durable
acceptance, history ownership, app inference scheduling and packaging remain
unimplemented or unqualified. Passing hashes does not establish those properties.

No GUI or native-startup check was repeated because this slice changes no window,
focus, keyboard interaction or lifecycle. GUI aesthetics, accessibility, IME and
natural keyboard navigation remain required as the editor develops. The C codec
adapter is unchanged; existing ASan/UBSan evidence is retained rather than claimed
as a new sanitizer run.
