# Native Move service and store admission

## Status

Implementation and focused tests are written and owned files are formatted.
No Cargo or native UI was run by this worker. Root owns integrated compilation,
test execution, preview/navigation/receipt application and documentation. This
report does not claim native Move qualification or full Section 9.7 completion.

## Stable shared contract

`project::splice::Operation::{Copy, Move}` is explicit on every `Proposal`.
`Operation::default()` is Copy; production proposals initialize the field.

`Prepared` now retains `parent: NodeId` and `movement: Option<Movement>`:

```rust
pub struct Movement {
    pub source_parent: NodeId,
    pub source_before: FrameRange,
    pub destination_before: ProjectFrame,
    pub removal_after: ProjectFrame,
}
```

`Prepared.range` is the final inserted range, `node` is its first direct child,
and `removed` remains replacement-only. `Prepared::validate_result()` verifies
that the final parent contains a contiguous complete forest spanning the exact
range with that first child. Copy still requires its one imported root to span
the entire result. Move keeps multiple roots without manufacturing a group.

`CommittedEdit.range_selection: Option<CommittedRangeSelection>` retains:

```rust
pub struct CommittedRangeSelection {
    pub session: u64,
    pub project: ProjectId,
    pub parent: NodeId,
    pub range: FrameRange,
}
```

The existing receipt revision, scope and selected_node carry the matching saved
revision, destination path and first child. Only Move fills range_selection;
other service commits initialize None. Root applies the selection only after the
matching project/session/revision workspace is visible, through its once-only
completion path.

## Service behavior and authority

- Prepare edited source endpoints first, independently of destination success.
  Existing capture validation and historical-view seals remain unchanged.
- Reject Original+Move, Move+Replace, stale captured source, self-interior/core
  scope failures and exact native no-op. A rejected destination keeps valid
  copied endpoints. No-op produces “Already at this position; no change” and no
  ready/committable draft.
- Refined current source parent/range and explicit pre-edit destination enter one
  retained `MoveRange` request. Allocate only core's required joint Split IDs and
  one timing base; core owns its actual consecutive timing slots. No paste IDs,
  placeholder root or independent delete/insert commands are used.
- Store `preview_edit_slice` narrowly admits MoveRange through the unchanged
  `prepare_command` transaction and seals the exact proposed/base documents with
  its validated current source revision. `slice_capture_revision` still applies
  only to copy commands. Move gets no historical-media exception. Strict Original
  proposal admission and standalone copied-source admission are untouched.
- Resolve final forest metadata from the admitted snapshot. Commit the exact
  prepared request once, retaining the forest range in the durable receipt before
  refresh. Duplicate successful commit requests return the existing receipt.
  Saved-refresh failure keeps the old workspace and saved/Reopen message.

## Owned files

- `crates/deadpan-app/src/project.rs`: receipt contract.
- `project/splice.rs`: explicit operation, movement metadata and bounded forest
  validation using immutable plan durations and ordinary Sequence boundaries.
- `project/service/splice.rs`, `project/service/splice/request.rs`: source-first
  Move preparation, retained command, final forest, commit receipt.
- `project/service.rs`, `project/service/delete_range.rs`,
  `project/service/moment.rs`: default None receipt fields and existing copy
  request node handling.
- `project/tests/splice.rs`, `project/tests/edited_slice.rs`: explicit Copy in
  existing fixtures and new Move submodule declaration.
- New `project/tests/edited_slice/move_range.rs`: five service regressions.
- `crates/deadpan-store/src/slice_preview.rs`: narrow Move preview admission.
- `crates/deadpan-store/tests/edited_slice/preview.rs`: sealed Move regression.

## Written verification

Five service tests cover:

1. Multi-root preview with stable IDs, no history writes, exact retained command,
   one commit/duplicate receipt, exact forest selection and Undo restoration.
2. Both exact boundary no-ops, own interior and replacement rejection, valid
   endpoints after failure, superseded ready draft invalidation, Original refusal.
3. Historical copy remains usable after Undo while Move rejects the stale source;
   a fresh capture permits Move without writing during preparation.
4. Local refinement of a ten-frame copy to `[2,4)` and destination 7 uses the joint
   three-cut result `[5,7)`; saved-refresh failure retains that exact receipt,
   duplicate commit writes nothing, reopen matches the proposed document.
5. Meaningful boundary reparenting keeps temporal positions while recording the
   destination group scope and first child.

Store test covers sealed current Move/base identity, read-only history neutrality,
stale source/destination, bad IDs, own-interior refusal, writer requirement,
preview/commit identity, Undo freshness and live-capability revocation on close.
Existing forged historical capture and unrelated-command rejection tests remain.

Worker checks: explicit Rust 1.97.1 rustfmt with skip_children=true on owned
files, then scoped git diff --check, both passed. Root execution is pending at
this report point. Suggested filters:

- app `project::tests::edited_slice::move_range`
- store `move_preview_seals_only_a_current_validated_command_and_leaves_history_unchanged`
- regressions app `project::tests::splice` and `project::tests::edited_slice`,
  store `--test edited_slice`.

## Remaining verification and scope

Main preview site mapping, controls, native keyboard routing, final once-only
selection application, display/transport admission and native Metal QA belong to
root. Independent frame/PCM proposal-versus-commit tests belong to the media
worker. No production preview/worker/playback changes were made here. Occurrence
interiors, role-only moves, cut-to-register and persistent/named registers remain
open under the existing product contract. No migrations, subprocess launches or
UI interactions were introduced.
