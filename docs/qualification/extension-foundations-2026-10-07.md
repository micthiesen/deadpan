# One-sided extension: timing and media foundations

DP-12 remains Partial. This change implements the exact timing, bounded context
queries and native media conversion needed for generation at video edges. It
does not yet connect extension to the model worker, durable acceptance or native
controls, and advertises no new provider capability.

## Contracts implemented

- `ExtensionSamplingMap` stores direction, project/native rates, context count,
  generated count, output count and the explicit `frame_centers_clamped` policy.
  Context and generated intervals partition the retained native movie. Every
  interpolation fetch stays in generated frames, including upsampling and a
  one-frame output. Existing bridge maps keep their exact wire format and phase.
- `ExtensionGenerationPlan` requires a finite provider envelope. It chooses the
  nearest legal generated count, with ties choosing the shorter duration, while
  preserving the exact authored count. Longer-than-supported requests fail
  before model loading. The core map supports general counts; this provider
  planner enforces K=1+8k context and positive E divisible by eight. The upstream
  extension argument is E/8 latent frames, separate from E decoded pictures.
- Timing facts distinguish inserted duration, generated duration, full native
  movie duration, context span and speed conversion. These are exact rationals.
  Strict versioned decoding rejects unknown direction, operation and policy.
- `scoped_hold_context` and its canonical-target batch sample chronological
  context at P/R project-frame spacing. Outer Repeat/Retime owners cannot
  provide missing neighbors. Dormant defaults, effective shared Play reads,
  owned branches, descendant retiming, cutaways, explicit black and accepted
  Hold fallback witnesses preserve the existing picture semantics.
- Context queries share one traversal budget and a separate 64 MiB retained
  metadata budget, with at most 64 frames per request and 64 requests per batch.
  Provider capabilities can impose smaller measured limits. Peak query storage
  is the cumulative bound plus one temporary walker sample bounded by document
  depth and per-node limits. These are structural samples; same-shot analysis
  and measured source qualification remain required host work.
- Media protocol 3 `sample_extension` and the private
  `canonicalize_extension` path retain the complete native movie and produce
  exactly N generated-only frames. Native C independently validates the
  interval and computes exact center-clamp sampling with half-up RGB rounding.
  The existing immutable snapshot, process ownership, shared deadline, decoded
  verification, audio removal and content hashing remain in use.

## Verification

The source checkpoint is based on `7c12d417`. Retained logs, command scripts,
source hashes and executable identities are in
[the evidence directory](../../tools/model-qualification/evidence/2026-10-07-extension-foundations/manifest.json).

- The first focused run passed 105 core/jobs/picture-plan checks, including the
  ten new context tests and existing bridge planning regressions.
- The broader run passed all **1,717 tests**, with no skips, across core, jobs,
  plan, media and the native media worker. Workspace all-target strict Clippy,
  UI-harness strict Clippy, affected doc tests and formatting passed.
- The real-media extension test encodes a small lossless 17-frame fixture with
  deliberately different context pixels and an audio stream. Both directions
  verify every decoded pixel, alpha, ordinal, timestamp, duration, hash and
  absence of output audio for N=1, 3, 8 and 12, including 30000/1001 output.
  The oracle does not call the production sampler. N=12 proves that neither
  edge leaks a context handle; N=1 checks half-up interpolation. Identity,
  geometry, scratch-limit and cancellation failures also pass.
- Existing bridge, ordinary conversion and proxy real-media suites passed in
  that same run. No new live UI controls were added, so no UI appearance or
  physical-device claim follows from this checkpoint.
- Eight adversarial regressions passed after registering the extension-plan
  campaign and adding both extension directions to helper-protocol seeds.
  A 30-second campaign ran **2,749,155 cases** across extension plans and the
  existing media JSON targets with no failures. The new plan target ran
  701,227 cases; the helper-protocol target ran 804,784. This is bounded parser
  and arithmetic evidence, not model or end-to-end lifecycle qualification.

Independent review found that a frame-count limit alone allowed deep Repeat
ancestry to retain excessive copied path strings. The separate metadata ledger
fixes it; a valid 96-level, long-identifier fixture now refuses before exceeding
the limit in both single and batch queries. Reviews of the core/jobs math and
native conversion found no further correctness issue. Review was read-only;
the tests above provide the execution evidence.

An initial compile failed because the new test formatted a SHA-256 byte array
through an unavailable `LowerHex` implementation. Explicit per-byte hexadecimal
formatting fixed the test; the failed invocation is retained. An earlier plan
compile also exposed a private helper visibility mistake, corrected before the
passing context and broader runs.

## Remaining integration

Extension still needs complete retained temporal inputs and same-shot checks,
input relevance and automatic replacement across every context dependency,
direction-aware output quality checks, the actual supervised Retake worker,
explicit candidate acceptance/recovery and native mode/timing/seam controls.
The finite advertised duration envelope requires real-model measurements in
both directions. Model-free reopen/export and original picture/PCM preservation
must then pass through the complete application path. The earlier K9/E8 scratch
probe established feasibility only. The dashboard estimate remains unchanged.
