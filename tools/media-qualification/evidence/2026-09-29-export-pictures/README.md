# Committed encoder-picture evidence

See [qualification](../../../../docs/qualification/export-pictures-2026-09-29.md)
and [the output contract](../../../../docs/EXPORT_PICTURES.md).

This run uses the production captured-revision picture reader, Metal renderer,
working readback and SDR I420 conversion. All 118 checks pass. It retains 52
actual frames and 31 complete-plane f64 references, including every frame of the
accepted Generated fixture and an odd-canvas Original. Across 93,396,906 values,
the largest absolute difference is one code; none exceeds that fixed tolerance.
The reference shares geometry but separately implements the numerical color,
filtering and code conversion. No encoded file is produced.

## Inventory

- `summary.json` records terminal checks, test totals, source identities and limits.
- `commands/` keeps every terminal journal and compressed log, including the
  initial SHA-256 formatting compilation error and corrected strict lint.
- `sources/` contains complete source inventories, including new untracked files
  at execution time. The source used for corrected lint, build, Metal, tests and
  final formatting is identical. Documentation/evidence were finished afterward.
- `metal-artifact.json` records Cargo's exact executable selection and unchanged
  before/after SHA-256. `run-metal.py` is the exact externally timed invocation;
  its scratch paths are retained for attribution, not installed paths.
- `metal-report.json.gz` retains every frame identity, timestamp, hash and numerical
  comparison. `frames.tar.gz` contains all 83 raw I420 outputs/references, totaling
  188,608,212 uncompressed bytes; `frames.json` hashes each member.
- `fixture-reference.json` identifies the existing retained Generated manifest
  and six-object archive. The current fixture manifest was byte-identical to it.
- `review.md` retains three independent source reviews and their limits.
- `manifest.json` hashes every file except itself. Run `python3 verify.py` here
  to check the inventory, journals, exact build artifact, tests, all archived
  planes and every reported numerical error without extracting the archive.

No user media, project database, model, executable or native library is published.
The two freshly authored Original fixture packages and previously retained
Generated package remain private scratch inputs. The committed source fixtures
and bundle integration test reproduce them.

## Reproduce

Use the pinned FFmpeg prefix described in [Development](../../../../docs/DEVELOPMENT.md).
On a Metal-capable macOS host, with new scratch destinations:

```sh
DEADPAN_GENERATED_PICTURE_FIXTURE_ROOT=/tmp/new-generated-fixture \
  cargo test -p deadpan-media-worker --test bundle_qualification --locked \
  real_bundle_acceptance_is_explicit_durable_and_reversible_after_relocation -- --exact
cargo run --release -p deadpan-cli --example qualify_project_picture --locked -- \
  /tmp/new-output-report.json /tmp/new-output-frames \
  /tmp/new-generated-fixture/accepted.deadpan
```

Use an external process timeout as well as the example's cooperative deadline.
For retained execution evidence, build with Cargo JSON, select its exact example
artifact and hash it before/after running, as the recorded wrapper does. Omitting
the third example argument explicitly skips Generated coverage.

This establishes real encoder-input pixels and clocks for these synthetic SDR
fixtures. Final-render isolation, complete effects/mastered audio, encoder/mux
verification, atomic publication, native Render, full-size performance and release
acceptance remain open. Unchanged app replay and native OS interaction were not
rerun for this backend-only change.
