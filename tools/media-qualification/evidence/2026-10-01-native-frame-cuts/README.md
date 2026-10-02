# Native frame-cut evidence, 2026-10-01

See the [qualification and limits](../../../../docs/qualification/native-frame-cuts-2026-10-01.md).
Base commit: `bde973ad611b8cf1bdcbcc65783e1019a5ec315e`.

- `checks/`: exact commands, exit results, tracked diff hashes and source
  inventories before/after each run. Logs and inventories use lossless gzip.
- `replays/`: complete reports, including the two deliberate failing witnesses,
  the initial correction and final delete-range/workspace/marks regressions.
- `summary.json`: report-to-command links, binary/host/fixture identities,
  source hashes, check counts and explicit skips.
- `images/`: two inspected final rendered captures at 960×640.
- `scripts/`: the recorder, gate and collection scripts plus the three staged
  replay patches. Text suffixes keep evidence copies outside runtime source
  inventories; patches use gzip. Temporary absolute paths record this run.
- `SHA256SUMS.json`: every retained file except the inventory itself.

The broad workspace gate predates the input/replay review corrections. All
final app checks and rendered regressions share source inventory
`f4b664d41ab7e41c7b36c8466c376cd6b989abe9590a42f49a0123428662e3c0`.
The collector checked current sources against that inventory, command output
paths against the corresponding reports, and each expected failure name.
Replay metadata identifies executed binaries; these are local build records,
not signed provenance. Overlapping test/audit counts are not additive.
