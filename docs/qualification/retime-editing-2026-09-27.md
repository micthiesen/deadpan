# Retime editing qualification

This increment adds native structural speed editing through the existing picture,
canonical audio and reversible command paths. It does not complete a DP
requirement or delivery gate. [The editing contract](../RETIME_EDITING.md) records
the implemented scope and remaining long-input Preserve limit.

The native `:retime` and `:wrap-retime` commands accept a positive exact decimal
or fraction and an explicit preserve/tape pitch choice. They resolve once from
the selected stage's input span, show the quantized output duration, and commit
through the revision/session/scope-checked service. Inspector entry uses the
same path. Cancelling text entry makes no edit; applying unchanged ordinary
Retime parameters makes no new native revision. The Original remains pinned.

Core `WrapRetime` retains the complete selected subtree. `SetRetime` retains its
input selection and framing, accepts only ordinary Edit purpose, and changes only
the selected stage's duration and pitch. Changed output allocation drops that
stage's own binding while preserving its child and unrelated clocks. Exact
no-ops preserve binding state; inverse patches restore it. Corresponding
occurrence edits use normal isolation and stable Repeat identities.

Core schema 28/database schema 34 use the frozen core-27 adapter for old
histories. The saved pre-change CLI authored the database-33 fixture, including
three source splices, abandoned history and a pending redo. Its 39 revisions and
18 history edits retain earlier binding and gap cases. The reconstructed SQL was
independently opened and validated by that same old binary. Producer, binary,
input fixture, commands and SQL hashes are retained. New direct and occurrence
Retime commands are rejected by every older core command grammar.

The `retime` application replay drives pointer entry/cancellation, command
duration feedback, creation, adjustment without compounding, explicit nesting,
undo to the protected Original baseline and a non-destructive Source command.
It uses production widgets and project/picture workers. Playback preparation is
tested separately with actual decoded PCM; this scenario makes no acoustic
qualification claim.

Initial test work corrected a source selection that exceeded the WAV fixture's
8,197 samples. A delegated rerun ended without a retained result; after its handle
was unavailable and neither the log nor Cargo lock had an owner, the parent
repeated the focused PCM run. The corrected two PCM tests passed. Independent
review requested a stronger oracle for the split-Partition wrap case: comparing
one rendered plan under different chunk sizes alone does not prove retained
processing history. The final test adds independent sample expectations rather
than weakening that requirement.

Three independent reviews covered command/interface correctness, exact timing
and retained audio history, and migration/history integrity. The timing review's
PCM oracle finding was addressed and re-reviewed. The final audited increment
contains 31 source paths; no material review finding remains open.

The full workspace run passed 1,700 tests, failed two and ignored none. One
failure caught the CLI doctor's previous core-27/database-33 test expectations.
After that run, only `doctor.rs` and its integration test changed: the report now
advertises migration through database 33 and partial native speed, Original and
selection-loop audition. All 17 CLI project-command tests then passed, including
the corrected core-28/database-34 assertion. The old CLI output in the migration
fixture remains unchanged. The other failure is the unchanged
`directories_fifos_and_sockets_are_rejected_without_blocking` test: its Unix socket
creation returns `PermissionDenied` / `Operation not permitted` in this sandbox.
The workspace invocation is therefore not reported as fully passing.

Formatting, strict workspace Clippy, the workspace build, doctor, strict
`ui-harness` feature Clippy and all 232 app/harness unit and integration tests
passed. The six new core tests, four focused migration tests and two actual-PCM
tests also passed within their broader suites. Final formatting and workspace
lint were repeated after the CLI correction. Source seals retain the 561
source/configuration paths used during the full gate and identify the two later
CLI changes separately. The final source matches its review and verification
seal.

The actual `retime` visual replay ran on macOS 26.5.2 with Rust 1.97.1. The shortcut
audit passed all 3,472 routing cases against 62 current Kestrel reservations with
no conflicts or source drift. Metal initialization then failed with
`CustomNativeAdapterSelectionError("No adapter found")` before application
construction. Zero `retime` scenario steps, assertions, captures or timing
samples ran. Release performance replay was not repeated after that shared
renderer failure. The new painted inspector, its ImageGen target match, live
keyboard/focus behavior, accessibility and latency remain unverified. Existing
ImageGen targets and exact prompts remain intact; all ten asset/prompt pairs
passed their hash checks.

The [retained evidence](../../tools/media-qualification/evidence/2026-09-27-retime-editing/README.md)
contains the actual logs, failed GUI report, source identities, review record and
fixture audit. The contributed UI harness and all earlier pending work are
preserved. Git metadata is read-only in this session, so no commit or push is
claimed; a verified patch/archive checkpoint is retained in
`/tmp/deadpan-retime-20260927/checkpoint`. The full-project goal remains active and
no requirement or delivery gate is promoted.
