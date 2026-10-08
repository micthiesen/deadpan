# Durable AI extension inputs, 2026-10-07

This milestone saves extension operations and their complete input identity
through edits, automatic replacement, history and reopening. DP-12 remains
Partial. Extension output admission and explicit acceptance are still required;
the app and CLI continue to execute qualified Bridge jobs only.

## Implemented behavior

Schema 73 stores a tagged Bridge or Extension plan with an independently
captured input binding. The store derives that binding from the immutable
origin document and qualified media indexes, then derives it again during
full validation. A saved descriptor cannot authorize itself. SQL type and
length guards bound the descriptor to 512 KiB before allocation. Earlier
unused development schemas refuse opening, as authorized for this goal.

Extension bindings retain the direction, native rate, context count and policy,
every model sample, the unconditioned opposite seam, every structural span's
first and last measured picture, the closed terminal picture, and the selected
region's saved definition. Relative coordinates allow unrelated timeline moves;
raw footage, context or region changes invalidate the binding. Editorial gain,
captions and framing remain outside model inputs.

The host preparation report, current-request relevance, accepted Bridge origins
and automatic replacement decisions now use this shared descriptor. A real
footage regression changes a cutaway between all sparse model samples and proves
that relevance changes even though every model sample remains the same.

### Replacement and history

Replacement decisions include dependencies throughout temporal support. The
solver accepts a bounded variable number of terms per Hold and handles cycles
without recursive traversal. Shared limits cover traversal, spans, terms,
measured receipts and retained metadata. Fallback substitution over a span
requires a private witness from the canonical picture plan; a public sample,
changed span or foreign plan cannot supply that proof.

Automatic intent resolves its operation once from actual endpoint presence.
Authored black counts as a picture; absence does not. If neither endpoint exists,
the unresolved intent survives until an endpoint arrives. A resolved direction
remains captured when context later becomes too short or an anchor disappears.
Restoring usable context creates fresh work with the saved operation and controls.
Resource exhaustion rejects the transaction rather than becoming durable
evidence that footage is missing.

Operational preparations and durable intent remain separate. Cancellation of
work does not itself recreate an old activation. Undo never resurrects a
cancelled preparation; Redo may create fresh authority from the original saved
operation. History replay validates request chronology and scope before using
captured controls, including when a newer request changed the extension
direction, motion or context settings after the original acceptance.

Protocol 3 attempts can retain queued/running stages, failure, cancellation and
restart recovery. Extension completion, Validating, Ready, legacy selection and
Bridge acceptance routes refuse unsupported extension results explicitly.

## Verification

Verification finished on 2026-10-08 on the reference Apple M5 Max / 128 GiB Mac,
macOS 26.5.2, Rust 1.97.1 and the pinned FFmpeg 8.0.3 prefix.

The repository gate completed in parts:

- Formatting and strict workspace and UI-harness Clippy pass.
- The full workspace run executed 5,251 tests: 5,245 passed and six fixtures
  failed. After test-only corrections, all 147 tests in the six affected targets
  pass. This completes coverage of all 5,251 current workspace tests, with the
  original 10 declared skips retained.
- All 1,071 UI-harness tests and both doctests pass; two declared UI skips remain.
- Real footage tests detect changed support between unchanged model samples,
  preserve relevance across editorial changes and reject changed or forged regions.
- Real-store replay covers both extension directions with nondefault 30 Hz/K17
  context and newer motion/guidance controls after an earlier accepted Bridge.
  Full validation, reopening and Undo/Redo preserve the captured operation.
- Missing/restored context, V3 fulfilment and refusal, strict wire formats,
  receipt bounds, private span witnesses and historical request chronology pass.
  The replacement solver also passes its independent 600-case oracle, wide
  dependency limits and 100,000-node chain.

Nextest marked two tests as having delayed pipe closure in separate runs. Both
pass once in serial isolation without another report. One is a pure picture-plan
test with no subprocess launch. The original observations remain in the logs;
this does not establish their cause.

Logs, command exits, source hashes before and after the six fixture corrections,
and hashes from the final Cargo executable inventories are retained in
[`2026-10-07-extension-inputs`](../../tools/model-qualification/evidence/2026-10-07-extension-inputs).
No production Rust changed after the full workspace run. Skipped tests, real
inference, packaged export, native visual inspection and performance are not
claimed by this milestone.

## Review fixes and retained failures

Independent reviews covered temporal support and replacement decisions, then
durable intent and history replay. They found two persistence defects: replay
could recapture an accepted pause using older controls, and shared resource
exhaustion could be saved as missing-input authority. Both are corrected with
regressions. Historical source requests also require a version that existed at
the replayed scope clock.

Initial compile failures were remaining consumers of the old Bridge-only field.
Focused tests exposed Serde's permissive internally tagged unit variant, an
invalid Bridge duration fixture, a nonexistent-request assertion and obsolete
receipt-cache and missing-context expectations. Strict deserialization and the
fixtures were corrected. The missing-context history test initially omitted its
context resolver and assumed Undo would revive cancelled work; it now follows
the existing durable-intent contract. No production recovery rule was relaxed.

The gate first stopped on an unnecessary return and a large enum; the measured
binding is now boxed. The full workspace run then found four stale schema-72
assertions, one old SQL column count and a Bridge fixture inserted at the definition
edge. The corrected fixture inserts between two actual endpoints and asserts
Bridge capture; edge extension remains separately covered. All affected targets
were rerun, and schema 72 was added to the explicit unchanged-on-refusal cases.

## Still required

Bind temporal continuity evidence into the retained extension manifest. Implement
operation-specific output quality admission, the accepted artifact and provenance
sum types, reopening and export, native operation controls and explicit acceptance.
Exercise generated neighbors and cross-provider gradual transitions. Qualify
longer durations with the real model before advertising them. This milestone
adds no inference, packaged export, visual UI or performance qualification.
