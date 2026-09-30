# Durable render checkpoint evidence, 2026-09-29

Two real project encodes survived writer reopen and passed fresh isolated
verification before publication. Independent FFmpeg and AVFoundation readers
checked the published files: 138 pictures, 221,021 authored sample frames,
414 planes, 102,643,200 code values and nine complete GOPs.

- [Summary](summary.json): exact commands, source inventories and resolved coverage.
- [Qualification](../../../../docs/qualification/render-jobs-2026-09-29.md): scope and limits.
- [Native archive](native-artifacts.tar.gz): schema-39 fixtures, coherent schema-40
  database backups, retained candidates, final movies/reports and decoded references.
- [Member inventory](archive-members.json) and [readback audit](archive-audit.json):
  every archived regular file was read and rehashed.
- [Review disposition](review-disposition.md) and [playback fixture review](playback-race-review.md).
- [Exact changes after native execution](source-change-after-native.patch.txt).

The full workspace invocation returned 101 with three failures. Complete corrected
CLI command, render-store and playback targets passed; preserving the other full-run
targets yields 2,284 passing tests, zero remaining failures and zero ignored tests.
Final strict all-target workspace Clippy and formatting passed. Original failures
and corrected runs remain in their named logs and JSON journals.

Native source inventory: `694db72c44d8a15f687d2f317a4926e7af413187e1f52d7b78bde5a39667a475`.
Final source inventory: `306a884f2d4618700019f240e01fc007fb8d6c3127954b38f7e341e584959482`.
Later source changes are one tuple alias, doctor schema metadata and test fixes;
production playback and native render operations are unchanged.

Archive: 144 files, 217,724,403 raw bytes,
3,820,549 compressed bytes; SHA-256 `e5c57ab2165d0ddb5c80bcbcef09e153c4f4d0e94da08032975eda77a675f539`.

Restart coverage uses orderly writer closure. This does not qualify injected
process/power loss, durable publication reconciliation, automatic encoder policy,
native Render or a complete product export. All requirements and gates remain
open or partial. No new native sanitizer or GUI run was needed for this boundary.
