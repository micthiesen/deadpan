# Isolated render-worker evidence

Actual committed pictures rendered through the private CLI child on Apple M5
Max/macOS 26.5.2. See [the qualification record](../../../../docs/qualification/render-worker-2026-09-29.md).

- summary.json: command outcomes, source bindings and measured coverage.
- commands/: terminal journals and compressed logs, including the initial
  compile failure and later passing checks. No command is left running.
- sources/: hashed source inventories for each command.
- metal-artifact.json: exact Cargo worker/example records and unchanged
  before/after binary hashes. run-metal.py is the captured launcher.
- metal-report.json.gz: direct and isolated frame metadata, numerical
  comparisons, live writer changes and cancellation/recovery results.
- frames.tar.gz and frames.json: all synthetic actual/direct/reference I420
  bytes and per-file hashes. No user media or model weights are included.
- pipe-inheritance.json: raw/fixed macOS descriptor-inheritance witness.
- reviews/: independent source reviews. Earlier findings are retained; the
  boundary review records their corrected behavior.
- fixture-reference.json: retained Generated fixture manifest and objects in
  the prior qualification bundle. Project databases and executables are omitted.

Run the bundle's verify.py with Python 3 to audit the manifest, results, artifact
provenance, every retained frame, numerical comparisons and cancellation/history
assertions. The [development guide](../../../../docs/DEVELOPMENT.md) describes
fresh native runs. The journal runner remains shared with the Generated-picture
evidence; this bundle does not add another copy.

These raw picture checks do not qualify a finished MP4, audio, full-size
performance, packaging, native Render interaction or any release gate.
