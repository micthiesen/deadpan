# Combined Source Trim evidence, 2026-10-01

See [qualification](../../../../docs/qualification/combined-trim-2026-10-01.md)
for behavior, failures and limits. This qualifies a backend increment; native
Trim and full product acceptance remain open.

- Full workspace: 3,366 unit/integration
  tests and 2 documentation tests passed;
  no failures or ignored tests. Existing debug unwind-table linker warning retained.
- Strict workspace/all-target Clippy passed after replacing one stable comparison
  with the identical sort key. The exact correction is retained here.
- Corrected core: 132 tests passed.
  Final formatting passed on that same source inventory.
- Earlier focused runs cover 33 new tests in distinct scopes. Repeated runs do
  not add coverage. Initial failures and corrected expectations remain in logs
  and failure-notes.md.
- No native GUI was opened. The final process scan found no Deadpan executable.

Full workspace source: `9561470566396a8230900d62f66fdc5404f500f33e30ae95e3bf3e8f6453981a`.
Corrected lint/core/format source: `2be74140366adbc6b19cbffedc098ef32a02b100631970e161b0ddfc537f3366`.
The staging script verified that their only source difference is the retained
one-line sort correction, including both full-file hashes. No test changed.

`summary.json` is derived from original recorder reports and logs. The recorder
and summarizer are the existing committed source-trim tools in
`tool-references.json`. Source inventories include new untracked implementation
files; Git diff hashes alone do not. `environment.json` retains host, toolchain,
fixture hashes and cleanup. Reviews and frozen-stage manifests preserve the
original findings as well as the corrected handoffs.

`SHA256SUMS` seals every file in this directory except itself.
