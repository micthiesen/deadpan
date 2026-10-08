# Extension project-context evidence

Development project capture, structural coverage and conservative transition
rejection. See [qualification](../../../../docs/qualification/extension-context-2026-10-07.md)
for the supported envelope and remaining work.

The compressed logs preserve both failed and corrected checks. `source.json`
identifies the base commit and every changed implementation, test and fixture.
`post-gate-binaries.json` hashes test executables selected from Cargo nextest's
workspace and UI inventories after the completed gate. Listing rebuilt some
unchanged artifacts after the doctest feature graph; these hashes identify the
post-gate executables, not pre-run hashes of the gate binaries. Inventory logs
retain that distinction. `manifest.json` hashes the evidence files. The fixture
generator and its lossless MP4 live in
`crates/deadpan-cli/tests/fixtures`.

## Review

- An independent plan review covered canonical walker equivalence, exact span
  endpoints, cutaways, Reverse, Repeat gaps, retiming and aggregate limits. It
  found no remaining defects. The plan tests include two property checks with
  64 generated cases each against direct canonical picture evaluation.
- Independent host review found a singleton-span bypass of gradual detection.
  Every span now receives the real padded check. The real-video regression
  verifies identical pixels and duration before and after one-frame splitting,
  and rejection in both forms.
- Review aligned the host with the worker's exact 1–120 fps development limit
  and bounded display-aspect expansion before allocating a conditioning raster.
  Tests cover excessive sample aspect, ordinary anamorphic geometry, and rates
  at and just beyond the worker limit.
- New extension manifests require canonical colour conversion. The historical
  bridge reader remains covered by the existing suite.
- The host continuity report is only an in-memory observation. It is not bound
  in manifest schema 1 and is not store admission or request relevance proof.
  [Next integration](next-integration.md) is a read-only design report, not a
  claim that its proposed store, quality or acceptance work is implemented.

## Retained failures

- Initial compile: checked `ceil` result was not unwrapped before subtraction.
- Initial source-jump fixture: removed audio retained an incompatible mapping.
- Fade fixture trials: millisecond timestamps could not prove singleton spans;
  replacement exact-tick media then lacked explicit transfer metadata. The
  final generator sets per-frame colour properties. Native admission rules and
  pixel assertions were preserved.
- First full gate: Clippy rejected manual ceiling division in an analysis test.
  It was changed to `div_ceil`; the corrected full gate is recorded separately.

No new real inference, V3 Ready admission, acceptance, packaged export or native
UI qualification is asserted here. Generated-neighbor capture and complete
cross-provider gradual checks remain open.
