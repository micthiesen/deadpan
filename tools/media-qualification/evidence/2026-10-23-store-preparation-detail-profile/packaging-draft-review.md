# Store preparation detail profile: isolated diagnostic

Draft outline only. Complete after measurement terminates and Root verifies the
package. Do not enter timing values before service-summary.json is present.

## Scope

- Private timing instrumentation in the five files listed by
  formatted-overlay-audit.json; no production optimization.
- Environment gate: DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE=1.
- Fixed workload: large-first-pause UI harness. Report its exact fixture,
  binary, authored-input and SQLite sidecar guards.

## Qualification evidence

- Build: exact command rows and outcomes from build-receipt.json.
- Parser: synthetic valid-shape acceptance and rejected-case count from
  parser-tests.json. Parser checks do not qualify runtime behavior.
- Workload: three alternating rounds each for root-control, profiling off and
  profiling on. Include all raw report checks and guard results.
- Source identity: base and overlay IDs, full original/final source maps,
  before/after copies of the five modified Rust files, and six guarded inputs.
- Independent review: summarize independent-review.json.

## Measurements (fill only from terminal evidence)

- App-level commit and offscreen composition: report min, median and max for
  n=3 in each of root-control, off and on.
- Store phase timings and containment residuals: report parsed phase and
  residual summaries separately. Parent totals include children; do not add
  nested phases.
- Keep the three series distinct. Results are diagnostic; they establish no
  p95, product target pass, physical display latency or attribution beyond
  recorded phase boundaries.
- Preserve warnings and skipped checks exactly as shown by the raw reports.

## Inventory

- Package path:
  tools/media-qualification/evidence/2026-10-23-store-preparation-detail-profile/
- Fill total_files, total_bytes and inventory SHA-256 from the completed,
  independently verified inventory.json.

## Draft corrections applied

- Phase call counts follow profile_schema.json, including the zero-count debug
  phase. Each saved parse and trial profile is checked against a fresh parse of
  captured stderr; the synthetic parser suite must reject all 15 invalid shapes.
- Baseline source snapshots are checked against original-source-identities.json;
  final formatted sources are checked against the complete live Rust source map.
  The package retains a generated formatted diff and labels the worker patch as
  pre-format. The new profile module has no baseline file.
- Both built and qualified control binary manifests are retained and their
  executable bytes are checked before packaging. The source archive, complete
  source tree, target tree, binaries and fixture databases remain excluded.
