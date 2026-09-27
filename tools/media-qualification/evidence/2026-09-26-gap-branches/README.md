# Repeat gap branches verification

See [the qualification record](../../../../docs/qualification/gap-branches-2026-09-26.md)
for implemented behavior, scope and remaining work.

- `gate-1` through `gate-3` retain the initial Clippy failures and their source
  snapshots. `gate-4` is the complete stable-source workspace and optional-feature
  run: 1,534 workspace tests passed, two failed, none ignored; formatting,
  Clippy, build, doctor and the harness's 180 unit/two integration tests passed.
- `post-validation` retains the focused check after updating the CLI schema
  assertion. Only that test file changed after the full gate; production source
  hashes stayed unchanged. The Unix-socket fixture still cannot bind in the
  sandbox. It was neither skipped nor weakened, and the overall gate stays failed.
- `durable-cli-final` records 43 public CLI invocations, 11 revisions and seven
  history entries. No history or initial snapshot was seeded. The initial run is
  retained too; its reported command count omitted its final snapshot call.
- `old-binary` contains producer logs for the two core-22/database-28 fixtures.
  Their SQL, scripts and provenance live in `crates/deadpan-store/tests/fixtures`.
  The binding-bearing fixture explicitly seeds only its first snapshot; later
  edits and history were authored by the preserved old CLI.
- `review.json` records three independent gap reviews and the resolved legacy
  rejection-test finding, plus the separate production UI-harness integration
  review. No review findings remain.
- `verification.json` joins gate, focused follow-up and CLI identities.
  `sha256.json` covers the retained files. No binaries, media caches or live
  project databases are included here.

This increment used headless tests. The completed harness's earlier real-Metal
visual/performance results and images retain their separate build identities in
`tools/ui-feedback/evidence/2026-09-26`. A static comparison with the ImageGen
target identified the shallow viewer and dense controls; it is not a new native
GUI run or physical-display qualification.

No commit or push was possible because this session's Git metadata is read-only.
The complete shared checkout, including the finished UI harness, is retained in
`/tmp/deadpan-gap-branches-20260926/checkpoint` before handoff.
