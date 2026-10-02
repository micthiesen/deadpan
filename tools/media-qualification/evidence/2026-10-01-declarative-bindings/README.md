# Declarative binding evidence

See [qualification and limits](../../../../docs/qualification/declarative-bindings-2026-10-01.md).

- `checks/` retains exact commands, completion codes, logs and source inventories.
  JSON inventories and logs are gzip-compressed without timestamps.
- `summary.json` indexes command records and full replay reports by source and
  executed binary identity. Overlapping test counts must not be added.
- `red/` is the isolated base commit plus the new held-H witness. It fails the
  intended assertion. `old-source.json`, `old-source-inventory.json.gz` and
  `old-source-replay.patch` identify those inputs independently of Git metadata.
- `stale-binary/` is the first attempted current-source replay. It actually ran
  the old binary because both source trees shared a Cargo target directory.
  `artifact-collision.json` records the matching hash and old audit count.
  This report does not qualify current-source behavior.
- `green/` is the rebuilt debug editing replay. `rebuilt-binary.json` confirms
  its new identity before execution. `full/` retains the release visual replay.
- Header-repair records retain the pinned archive digest and restored headers.
  Configure and install-headers logs document recovery of developer includes;
  recorded runtime binary/library hashes remain unchanged.
- `review.md` records independent findings, the correction and final review.
  `scripts/` retains the actual execution and collection scripts as text.
- `images/` contains the root agent's inspected rendered frames.
  `SHA256SUMS` seals every retained file other than itself.

The first failed setup test and stale-binary replay remain in the evidence.
Neither is replaced by a passing result. No ordinary native window was opened.
