# Native Move evidence, 2026-09-30

See [qualification](../../../../docs/qualification/native-move-2026-09-30.md)
and the [placement contract](../../../../docs/SLICE_PLACEMENT.md).

`summary.json` records exact commands, source manifests, test counts, replay
results and environment. Full replay reports and larger logs use gzip.
`manifest.json` hashes the retained files. Source manifests cover tracked and
untracked implementation inputs; the collector rechecks the final gate hashes.

The final source differs from the full workspace/replay build only by the
single help sentence in `help-correction.diff`. Final app tests, formatting and
strict Clippy cover that correction. The collector verifies the exact one-string
delta. `app-ui-final` overlapped the text edit and is superseded by
`app-ui-help-final` for final-source claims.

Initial compiler and replay-fixture failures remain beside their corrections.
Image review caught a five-point copied-timeline overflow at minimum size and
an unsupported arrow glyph. Final images retain Copy, Move, both viewport sizes,
the committed selection, explicit no-op and same-timing group reparenting.
Review reports record the corrected receipt-order and terminal-sample defects.

`native/` retains the exact release-binary identity, native key sequence,
consistent SQLite snapshots and verification. Copy/cancel and historical Move
rejection preserve all 20 tables. Commit adds one revision/history entry; Undo
restores every authored field except its fresh revision. Sixteen unrelated tables
remain identical. The temporary app exited and released its writer lock.
The reserved user window and its Space were never inspected or changed.

Final release replay passes 3,314 checks. Its optional accepted generated-picture
fixture is explicitly skipped. Audio delivery in ordinary UI replay is injected;
separate media tests check decoded PCM. Two uncorrelated FFmpeg diagnostics remain
unexplained, as recorded in `decoder-diagnostics.md`. Native CUA pictures were
inspected with visible-screen clipping; retained full layouts are Metal replay.
No native IME, full accessibility, listening, large-media or release qualification
is inferred from these checks.
