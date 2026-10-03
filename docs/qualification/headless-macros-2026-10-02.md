# Headless Macro qualification, 2026-10-02

## Scope

`macro inspect`, `macro --json` and `--dry-run` now share typed semantic
preparation and store admission across the standalone CLI, `deadpan-app
--headless`, and an open native project's authenticated writer. Requests bind
the project, committed revision and bank version. Runs supply an ordinary
Sequence parent and absolute Edit cursor.

Save changes only the named Macro. Motion-only execution returns a position
without changing the database or native cursor. Authored runs retain one
Compound history entry and their intermediate copied slices. Native runtime
copies are prepared before commit. Bank-only saves retain an exact project,
revision and bank-version receipt through refresh and compact-reply failures.
Unread native edit, copy and Macro continuations delay remote publication.

No project schema, macro vocabulary, keyboard binding or rendering semantics
changed. Core schema remains 43; database schema remains 55. This advances
DP-06 and DP-21 without completing either requirement or any gate.

## Environment and identities

- Apple M5 Max, 128 GiB RAM, macOS 26.5.2 (25F84), arm64.
- Rust 1.97.1 (`8bab26f4f`), Cargo 1.97.1 (`c980f4866`), locked dependencies.
- FFmpeg development prefix: `/tmp/deadpan-ui-ffmpeg/prefix`.
- Base commit: `6a5719062d59f96eff936305139a2c6f1507cea0`.
- Final complete source inventory SHA-256:
  `7682d1e2c6e05ce4ae0ecb6647dd12f384c1d4ae499f957d1282208e07e901cc`.
- Final `ui-harness` binary SHA-256:
  `f030217b15d0ada61a0af04128e882fb43f94ae4684c8bb2c4e4db52b8c65200`.

The [evidence directory](../../tools/media-qualification/evidence/2026-10-02-headless-macros)
retains command metadata, source inventories, complete compressed logs, replay
results, selected rendered captures and review notes. Every recorded check
compares source inventories before and after execution. `SHA256SUMS` covers
retained evidence, excluding itself. Scratch projects and authenticated discovery
secrets are not included.

## Checks

| Check | Result |
| --- | --- |
| Initial app, CLI and store tests with locked dependencies | 1,472 passed, none failed or ignored |
| Final CLI and default-feature app tests | 1,011 passed, none failed or ignored |
| Final app tests with `ui-harness` | 687 unit and 4 headless integration tests passed, none failed or ignored |
| Strict all-target Clippy for app, CLI and store with `ui-harness` | Passed |
| Strict all-target Clippy for the default-feature app | Passed |
| Workspace formatting and `ui-harness` build | Passed |
| Rendered Macro workflow | 137 checks passed across 402 steps |
| Production Kestrel compatibility audit | 487,568 routing cases, 62 reserved bindings, no conflicts or source drift |

The Kestrel audit compared the live source SHA-256
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`
with the retained source identity. The replay ran the real project service and
Metal pipeline against the qualified `cfr-bframes.mp4` fixture.

The initial affected-crate run passed 1,472 tests with no failures or ignored
tests. It covered app, CLI and store units, integration tests and doctests.
Its source inventory was
`c827ae290b962e7f6c74b03e8ae4949254aea8fdb10da270ec77294d473cb4f1`.
The subsequent correction boxed the optional receipt inside `LiveError` and
adapted its two constructors and compact-reply assertion. Store code and tests
did not change after that run.

Coverage includes:

- Consistent document/bank inspection beside a writer; save-preview equivalence,
  capacity and version exhaustion; preservation of the default copy and redo.
- Actual CLI subprocess save, inspect, run, preview and rejection paths, plus
  the native binary's headless entrypoint.
- Authenticated native-owner Save/Run and dry-run, explicit nested scope,
  exact final cursor, intermediate copy provenance and restoration after reopen.
- Complete document Undo/Redo comparisons, fresh revisions and retained bank
  contents; late failure preserves revision, history, redo, capture and bank rows.
- Specific project/revision/bank/type/count/context errors using a valid Macro
  fixture; strict program and envelope decoding and raw request-size limits.
- Refresh failure after both bank-only and authored saves; compact replies retain
  exact receipts; unread native Save, motion and copy results remain deliverable.
- Rendered remote save and motion preserve both visible cursors, selection and
  pane. Authored remote execution updates the live copied slice and bank while
  keeping the independent native cursor at frame 17; one native Undo restores
  the complete prior document.

The main agent inspected two retained captures:

- [Remote authored run](../../tools/media-qualification/evidence/2026-10-02-headless-macros/screenshots/macros-123.png):
  the native Edit cursor stays at frame 17 while the copied slice is `[61..62)`
  and the edited duration is 116 frames. The picture, selection, controls and
  complete footer remain visible at 960 by 640.
- [Reopened register inventory](../../tools/media-qualification/evidence/2026-10-02-headless-macros/screenshots/macros-124.png):
  saved Macros retain their types and instruction counts, including the newly
  saved headless Macro `h`. The register list is readable and scrollable.

The replay reached its intermediate screenshot allowance; semantic checks
continued and named captures were retained. Debug linking reported the existing
`__eh_frame` size warning. Neither warning was suppressed, and this run makes
no release-build or performance claim. The final process check found no running
`deadpan-app` process.

## Review and corrected failures

Cross-layer peers reviewed the store, CLI and native service changes. A separate
peer reviewed the rendered assertions. This was peer review within the existing
agent team, not a fresh-context full-branch review.

Review tightened three tests: the rendered check now compares the actual native
copied slice and bank version with SQLite; CLI history checks compare the whole
document with only revision normalized; guard tests use a valid body so an
unrelated missing call cannot hide a missing guard. No code findings remained.

The first lint invocation named nonexistent `ui-replay`; Cargo refused before
checking source. The corrected `ui-harness` invocation found `LiveError` had
grown to 152 bytes. Boxing its optional register receipt fixed the lint issue
while preserving its JSON shape. Both rejected runs are retained; corrected
strict lint passed.

## Limits

The concurrent snapshot test is smoke coverage and does not force every possible
interleaving. Coherence follows from the shared SQLite read transaction.
Malformed detailed socket output and omitted output-receipt handling were
reviewed but not directly fault-injected; compact-reply preservation and refresh
failures are tested. Socket loss can still leave an unknown outcome and never
causes automatic replay.

Rendered replay uses scripted picker paths and offscreen native event routing
with the real service and Metal pipeline. It does not qualify physical keyboard
layouts, OS IME, VoiceOver, display color or acoustic output. No new native window
session was needed for these headless changes. Broader Macro instructions,
semantic selectors, occurrence scopes, full dot-repeat and remaining product
requirements stay open.
