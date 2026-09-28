# Root sound ripple evidence

See the [qualification record](../../../../docs/qualification/root-sound-routing-2026-09-27.md)
for behavior, review findings and scope. `verification.json` indexes commands,
terminal results, uncompressed log hashes and test counts. Each compressed log
has its exact command report. Source manifests retain code and build-input hashes;
the final workspace gate checks the unchanged source identity. Documentation and
evidence additions are outside that code manifest.

The copied `runner.py.txt` is the executed serial Cargo wrapper. It redirects
output to a retained log, records a live PID, waits for that same process and
records its terminal result. It has no retry or timeout loop. The evidence
collector is retained as `retain-evidence.py.txt`.

The final checks are `fmt-check`, `clippy-workspace-03` and `workspace-tests`.
Earlier focused successes include `core-survival`, `plan-boundaries`,
`store-routing-02`, `audio-focused-final` and `playback-events-02`.
The previous-version CLI and positive migration fixture have separate immutable
[provenance](../../../../crates/deadpan-store/tests/fixtures/v35-sound-route-history.provenance.json).
Its binary hash and old doctor report are in `baseline.json`.

Earlier failures remain visible. They include a singular test-target typo,
fixtures that supplied invalid empty Source nodes or incomplete edge policies,
an obsolete expected Delete rejection, fixture SQL load ordering, a test's Arc
serialization mistake, and strict lint corrections. Review also required real
production fixes for bounded immutable envelope indexing, physical Hold clipping,
integral event survival and widened virtual endpoint arithmetic. Later tests
cover those counterexamples. No repeated successful run is used to erase a
failure, and no process-launch or access workaround was required in this gate.

This evidence covers the implemented backend subset. It does not qualify native
sound controls, physical output, listening, export equivalence or release gates.
