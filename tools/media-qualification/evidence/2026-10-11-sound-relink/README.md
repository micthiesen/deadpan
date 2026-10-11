# Catalog sound recovery evidence

See the [qualification record](../../../../docs/qualification/sound-recovery-2026-10-11.md).
The sole working agent implemented, reviewed and verified this change on the
reference Apple M5 Max. Source base: `fec738e4`.

- `checks/` retains command arrays, exit codes, timings and full output. The
  command-reference rewrite intentionally failed after updating the generated
  file. The final test run excludes only three unchanged keyboard audits that
  had already passed. Initial compile errors are retained.
- `replays/`, `replays-final/` and `replays-final-2/` retain the earlier runs,
  including two attempts to click a scrolling catalog row and an invalid
  stale-session fixture that received the correct already-open no-op.
- `replays-final-3/` contains the sound replay after those fixture corrections,
  its executed binary identity, semantic checks and selected PNG captures.
  The final production Original relink check is in `replays-final/`; the
  earlier recovery check is in `replays/`.
- `source-identity.json` binds the final reviewed app sources, Cargo lock,
  toolchain and actual synthetic media fixtures. `compression.json` records
  original byte lengths and hashes for compressed logs and reports.
- `scripts/` retains command runners, collection and validation helpers.
  `SHA256SUMS` covers every retained file except the manifest itself.

Full uncompressed reports, generated test packages and copied executable
binaries remain in `/tmp/deadpan-sound-relink-20261011`. The repository retains
reports and selected final pictures, not every intermediate screenshot.
These are real service/SQLite/media and offscreen Metal checks with scripted
picker answers. They are not live GUI, physical keyboard or listening evidence.
