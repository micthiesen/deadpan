# Native encoder evidence

See [qualification](../../../../docs/qualification/native-encoding-2026-09-29.md)
for scope, measured results and unresolved capabilities.

- `summary.json` records final checks, tested source coverage and matrix counts.
- `normal-final.json.gz` and `sanitized-final.json.gz` retain the complete reports.
- `native-files.tar.gz` retains 836 files from the initial and final matrices,
  including MP4, complete reader PCM, partial outputs, probes and observations.
- `native-files.json` records every archive member's size and SHA-256.
- `manifest.json` records every other retained file's size and SHA-256.
- Command journals, logs, source inventories and review notes retain failures
  and fixes. Reproduction helpers use the original scratch paths and are stored
  as `.py.txt`; select new scratch paths before reuse.

The reports are compressed without changing their bytes. Product Render,
full mastering, publication and release acceptance remain open.
