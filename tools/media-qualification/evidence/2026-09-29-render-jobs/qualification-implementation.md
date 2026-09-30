# Native durable render qualification implementation

Implemented source only:

- `crates/deadpan-cli/examples/qualify_render_jobs.rs`
- `crates/deadpan-cli/examples/qualify_render_jobs/evidence.rs`

No build, test, formatter, native program or commit was run by this agent.

## Invocation

`qualify_render_jobs SOURCE_PACKAGE SOURCE_REVISION GENERATED_PACKAGE GENERATED_REVISION WORKER REPORT NEW_OUTPUT_DIRECTORY`

The parent must supply two distinct copied, already migrated packages under `/tmp`.
Use the captured source revision `background` and Generated revision
`ui-generated-ready`, as present in the retained publication manifests. The
source fixture uses `[20,128)` and Generated uses `[0,30)`. The executable argument
is the real `deadpan-cli` worker binary, with its pinned runtime already linked.
Both REPORT and NEW_OUTPUT_DIRECTORY must be absent.

## Actual work performed when the parent runs it

Each case captures a document-bound hardware/no-B-frame engineering intent,
allocates an Encoding attempt, calls real `jobs::encode_and_retain`, and commits
its opaque retention token. It then closes the writer, checks that the old handle
is revoked, confirms read-only reopening does not recover the attempt, and opens
a new writer. The reopened writer must mark the old attempt Interrupted while
preserving its checkpoint and immutable intent. A pre-restart transition is
explicitly rejected. A fresh attempt ID and cancellation token select the retained
checkpoint, then real `jobs::verify_checkpoint` runs the production isolated
verifier. Its report is persisted and the live candidate goes through the actual
publication API. A final writer reopen verifies terminal evidence persistence.
There must be exactly two attempts per job and only one encode per case.

Structural encoding progress triggers a framing edit on `source`, undo, redo,
and one final undo. The final undo restores all authored content except the
required fresh revision ID. Exact SQLite authoring cell digests cover revisions,
history, state and redo. The digests must remain unchanged during job allocation,
checkpointing, restart, verification, publication and final reopening, apart from
those explicit live edits. The original captured historical document must remain
exactly equal throughout.

The JSON report is bounded to 8 MiB, progress to 4096 observations per stage,
movies to 512 MiB each, combined retained bytes to movie plus 256 KiB manifest,
namespace bytes to 1 GiB and namespace entries to 128. It includes native
manifest/contract, full verification, exact checkpoint identities, state records,
final movie/report references and hashes, progress, provider counts and the
checks above. A single 15-minute caller deadline covers both cases.

## Limits and independent content checks

This exercises orderly writer restart, not process-kill/power-loss injection.
There is no durable publication journal or crash reconciliation claim. The report
explicitly excludes native Render UI, automatic encoder choice, full audio/effects,
HDR and release qualification. No direct I420/PCM references are regenerated.
The parent may attach retained structural/Generated references from the prior
publication qualification only after proving equal full document hash and output
contract. The new report's `cases` have final `path`, `manifest`, `contract`,
`verification` and `publication` fields for that adapter.
