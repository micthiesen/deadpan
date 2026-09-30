# Durable automatic render review

Base: `4594f5b043f5fccd84da9c4c8f8f88f47dceee8e`.
Reviewers read source only; the parent owns all builds and native runs.

## Pure policy, storage and migration

The independent teardown/evidence reviewer found a P2 audit cost regression:
validating each attempt reread its job allocation history, and decision audit
reparsed the same revision per decision. The store owner changed full audit to
reuse validated intents and compact revision summaries. Targeted reads retain
their own allocation-head validation. Deterministic read counters cover the
retained 26-attempt history, automatic histories growing from 6 to 96 decisions,
and corrupt heads/output bases. The reviewer rechecked the fix and reported no
remaining findings.

The pure-policy implementer separately reviewed the parent durable adapter and
found that `Cursor<Vec<u8>>` would grow past the intended bound. The parent
changed the sink to `Cursor<&mut [u8]>` over a fixed 64 KiB allocation and added
an oversized-observation regression.

## Workflow, provenance and native dispatch

The independent workflow reviewer traced the queued qualification, atomic
decision/Encoding transaction, worker-held admission, cancellation cleanup,
checkpoint retry, destination reconciliation and exact decision provenance.
The review included the native private worker entrypoint and real qualification
example. It reported no actionable findings.

One focused test then exposed a fixture lifecycle error: replacing the real
worker's command sender shut that worker down before the public retry call.
The failure reproduced as `Unresolved("stage worker stopped")`. Retaining its
idle sender fixes the fixture while preserving production liveness checks.
The reviewer independently checked the fix and confirmed that the separate
synthetic release-channel failure test still closes the intended channel. No
existing test was removed; the workflow test file grew from 13 to 20 tests.

## Parent integration review

The parent inspected the combined tracked diff and new source files, retained
failed invocations, and corrected test-only enum access and SQLite integer
bindings found by compilation. Exact verification results belong to the command
journals and qualification report, not these source-review observations.

The full workspace run exposed one old migration expectation. The store owner
parsed its preserved assertion: the only difference was the new empty
`render_encoding_decisions` table-name marker (57 entries versus 56). Removing
that one marker made the old arrays identical in order. The failure reproduced.
The test now excludes the new table from old-cell comparison and separately
requires it to be empty. The parent reviewed the complete two-line/array change;
no production source changed. Both affected migration checks pass.

The first real automatic workflow published successfully, then its example
failed a raw JSON Value comparison. The retained diagnostic proves that the
stored and published intent/decision JSON are identical. Seven observed peaks
differed only because Value promoted f32 to f64 while wire JSON used the shortest
f32 spelling; every pair has the same exact f32 bits. The example now decodes
strict DTOs and uses exact typed equality after receipt length/hash checks.
The independent workflow reviewer checked this correction and all four evidence
call sites, finding no issue. No tolerance or production policy changed.
