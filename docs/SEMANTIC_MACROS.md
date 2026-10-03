# Semantic macros

`q` plus a letter records a Macro in a project register. While recording, `q`
saves. Escape clears an existing Visual selection and records that action;
without a selection it cancels the draft. `@` plus a letter runs a saved Macro;
`3@a` runs `a` three times in one transaction. The command equivalents are
`:record a`, `:record-stop`, `:record-cancel` and `:macro a 3`.
Both binding families are configurable through `macro.record` and
`macro.execute`. Names are case-insensitive a–z.

The current vocabulary includes relative frame and beat motion, group start/end,
Visual selection begin/finish/clear, frame, beat or Visual cut, selected-beat or
Visual yank, typed operator motions, Repeat wraps and count setters, named Group,
neutral Ungroup, register paste or Visual replacement, and named Macro call.
Motions and cuts retain their requested counts, including when they
clamp at a group boundary. Copy, cut and paste retain the selected register name.
The planner resolves each instruction against the preceding staged edit in
the same ordinary Sequence group. Temporal occurrence scopes, text objects,
analysis-dependent motions, additional edits and broader semantic dot-repeat remain required.
This is partial DP-06 implementation, not full macro acceptance.
See [Repeat selections](REPEAT_SELECTION.md) for total-play versus motion counts,
captured targets and structural range wrapping. `:repeat N` records a count setter
for an existing Repeat and a selected-beat wrap for another kind. Setters require
an explicit selected direct-child Repeat and no Visual selection on replay.
[Named grouping](GROUP_EDITING.md) retains the exact name and selector. Ungroup
requires a selected neutral Sequence with no Visual selection. Both preserve the
copy bank and pending register choice, and use one atomic history entry per run.
See [operator qualification](qualification/operator-motions-2026-10-02.md) for
typed selectors, pending input, exact capture provenance and receipt ownership.
See [Visual qualification](qualification/visual-macros-2026-10-02.md) for the
selection, replacement, persistence and native receipt checks.
See [copy/paste qualification](qualification/macro-reuse-2026-10-02.md) for
checks, rendered captures, retained failures and remaining limits.

## Recording and input ownership

Recording performs normal edits. A motion enters the body when dispatched;
a copy, cut, paste or call enters only after its matching successful receipt. A pending
operation blocks the next recorded action until that result arrives. Failed
operations do not enter the body. Saving writes a named Macro without adding
an Undo entry or replacing the default copy. Cancelling leaves completed
edits undoable and preserves the previously saved Macro.

While recording, `v` begins or finishes a Visual selection. `h`/`l`, `j`/`k`
and `gg`/`G` retain their frame, beat and group-boundary intent. `y` copies a
nonempty Visual range and finishes its extension while retaining both endpoints;
`d` cuts it, and `p`/`P` replaces it. Cuts and replacements clear the selection.
Without Visual selection, `yy` copies the selected beat, `dd` cuts it, and
`p`/`P` paste after/before it. `y` or `d` followed by `h`/`l`, `j`/`k`, `gg`/`G`
copies or cuts the exact half-open range from the entry cursor to that motion's
destination. A yank leaves the cursor and selected child in place; a cut selects
the join. Use one positive distance count before the operator or its frame/beat
motion, such as `5dl` or `d5l`. Supplying both counts refuses. Whole-beat operators
accept only no count or one; group-boundary motions do not accept counts.
An empty motion interval refuses without falling back to a whole beat.
These actions use the same staged planner and receipts as replay. Both measured
Original ranges and edited slices can be pasted. An absent selection is retained
as absent; it never silently becomes the beat under the cursor. An empty group
may be copied, and pasted copies receive fresh identities and become selected
even when they add no picture time. An empty destination Sequence admits slot
zero without a selected child.

Recording and execution may start with an existing Visual selection. Such a
program can depend on the invocation range, or begin a new range relative to
the invocation cursor. Empty, absent, forward, backward, extending and finished
selections remain distinct. An empty range refuses copy, cut or replacement;
it never falls back to the selected beat. `:record-cancel` always discards the
draft. Escape during pending work cancels the draft and relinquishes cursor
ownership; already queued authored work still finishes.

The footer shows the register, instruction count and save/cancel controls.
Unsupported actions refuse explicitly. Changing the project, register bank,
ordinary group, pane or cursor outside the supported recording path cancels
the draft. A refresh failure after durable success retains that success and
stops recording with reopening guidance.

Prefix and command entry capture the project session, revision, bank version,
group, cursor, selected child and oriented Visual selection, including an absent
eligible target. A late letter or command
cannot replace that capture with a newly available context. Text, IME,
focused native controls and Kestrel reservations retain priority. Since egui
has no `Key::At`, logical `@` requires the pressed key's immediate native
`Text("@")` companion. Physical mode explicitly uses Shift+2. Physical input
across real keyboard layouts still requires qualification.

## Planning and atomic execution

`SemanticProgram` contains typed instructions, never raw keys, process hooks,
stored cursor timestamps or preselected node identities. `plan_semantic`
incrementally executes its body against a private document and staged register
bank. `SemanticRegisterBank` keeps the entry map and its observed version
together at that boundary. The planner applies each ordinary leaf once and
returns a resolved `Compound`,
final context, register writes and an entry trace. Navigation/selection-only programs return
no authored request and add no history. Headless Macro runs also return sorted
`mark_changes`: full `before`/`after` logical mark records that differ between the
entry document and final plan. Fragments and unresolved reasons are retained;
intermediate instruction changes do not masquerade as final losses. Dry-run and
commit share this comparison.

`SemanticInstruction::Yank` and `Cut` carry a `SemanticSelector`: explicit
selected beat, explicit Visual selection, or `SemanticMotion` (frames, beats,
group boundary). Motion resolution is shared with standalone semantic movement.
The trace retains the exact `SliceCaptureSelection` and historical revision for
media admission. A selected empty child stays a structural capture, and its cut
uses `DeleteRipple` with no split IDs. Range cuts use one `DeleteRange` with its
exact allocation requirements. A child cut chooses its literal following sibling
even when adjacent empty children share the same time coordinate.

Native operator prefixes capture session, revision, bank version, ordinary scope,
cursor, selected child, Visual state, pane and chosen register. A changed context
invalidates the capture permanently, including after returning to the same
position. Single operations and recorded operations share `Operation::Apply` and
the same saved-receipt handling. Original range `y` stays immediate and immutable.

Context tracks the selected direct child and Visual endpoints separately from
the absolute cursor. An extending selection's head must equal the cursor;
a finished selection remains independent of later navigation. Both endpoints
must lie within the ordinary Sequence's absolute bounds.
Frame motion selects the right-hand child, or the final child at the scope end,
as native navigation does. Yank preserves both coordinates. Paste selects its
new root and puts the cursor at the insertion boundary. Each later instruction
uses that staged selection. Visual replacement uses one ordinary `ReplaceSlice`
or `ReplaceSource` leaf, with exact disjoint split and import identity pools.
Its trace retains both the removed and inserted intervals. Original source mappings come from the saved
qualification's measured index and are checked again at store admission.

A call resolves a named Macro at call entry and freezes that body for all of
its counted repetitions. Later calls see earlier staged register writes.
Overwriting a running Macro's register with a cut does not change its active
body. Calling a copy or pasting a Macro is a type error. Recursive calls fail;
call depth is at most 16, a body at most 1024 instructions and 128 KiB, and an
expanded run at most 4096 instructions. Calls, motions and clamped no-ops all
consume fuel. Existing Compound step, document and capture limits also apply.

The service binds the actual saved head and bank version, previews the whole
Compound with ordinary media admission, and prepares its final runtime bank
before committing. Every leaf and capture allocation is fresh. A run with edits
saves one reversible history entry and one final bank result. A yank-only run
saves its copies without changing the document, Undo or Redo. A later failure
saves neither. Undo/Redo replays frozen resolved instructions and never rereads a
mutable Macro body. Register contents survive Undo.

## Persistence and feedback

Database schema 55 stores `RegisterValue::Macro` with neither capture revision
nor capture-step provenance. Original and Edited copies require exactly one
such reference. Macro-only banks may omit the default copy. Payload type,
canonical bytes, size and digest are validated on reopen and checkpoint.
Unused schema-54 packages require recreation under the development policy;
existing older supported adapters remain.

Each native request carries a session-scoped identity. Exact retries of a
successful request return its retained receipt; changed metadata cannot reuse
that identity. The last successful receipt remains independent of later failed
feedback. A committed macro remains successful if workspace refresh fails,
while the old view cannot consume its cursor. A cursor moved while execution
was pending is not retargeted by the delayed result.

Native and headless execution use the shared core planner and store Compound
entrypoint. An authored named Run clears the current dot-repeat candidate.
A supported direct Apply cut, Repeat wrap or count setter establishes its effective selector and
parameters after commit, including during recording. Recording `.` stores that
effective instruction; later playback does not consult session repeat state. Bank-only
operations preserve the candidate. See [semantic repeat](SEMANTIC_REPEAT.md).

## Headless inspection, save and run

The standalone CLI and `deadpan-app --headless` share these commands:

```text
macro inspect <project.deadpan> [--register a]
macro <project.deadpan> --json <request.json> [--dry-run]
```

Inspection reads the project revision and bank version in one SQLite snapshot.
It reports register types and saved Macro bodies without returning copied media
payloads. JSON register names use lowercase a-z. Request files are limited to
132 KiB, including the existing 128 KiB program bound. Use the inspected
identities in a version-1 request:

```json
{
  "protocol": 1,
  "project_id": "PROJECT_ID_FROM_INSPECTION",
  "expected_revision": "REVISION_ID_FROM_INSPECTION",
  "expected_bank_version": 0,
  "dry_run": false,
  "operation": {
    "type": "save",
    "register": "a",
    "program": {
      "instructions": [
        { "type": "move_frames", "forward": true, "count": 2 },
        { "type": "cut_frames", "operation": { "count": 1 }, "register": "b" }
      ]
    }
  }
}
```

To run, replace `operation` with the following object and update the expected
bank version after saving. `parent` is an ordinary Sequence node ID; `cursor`
is an absolute Edit-frame boundary within it. A positive count repeats the
whole Macro in one transaction. The optional `new_revision` names a fresh outer
revision; omission allocates it on the host.

```json
{
  "type": "run",
  "register": "a",
  "parent": "SEQUENCE_NODE_ID",
  "cursor": 20,
  "selected_child": "SELECTED_DIRECT_CHILD_ID",
  "visual_selection": null,
  "count": 3
}
```

`selected_child` is independent of `cursor`. Omission or `null` means no selected
beat. A motion may establish one; yank requires one, and paste requires one
unless the destination Sequence is empty. A stale or non-direct child rejects
the request. Optional `visual_selection` contains `anchor`, `head` and
`extending`; omission or `null` means no Visual selection. For example,
`{"anchor":30,"head":20,"extending":false}` is a finished backward range
independent of the cursor. The trace includes before/after child, Visual
selection and Edit positions, plus `removed_range` for replacements.

For example, this body copies the selected beat to `b` and pastes it after that
beat. Set `before` to `true` for a paste before the selected beat:

```json
{
  "instructions": [
    { "type": "yank_beat", "register": "b" },
    { "type": "paste", "register": "b", "before": false }
  ]
}
```

A paste-only body may use a previously saved Original or Edited register.
Each counted repetition selects its newly pasted root before the next begins.

This body selects the next four frames and replaces them with register `b`:

```json
{
  "instructions": [
    { "type": "begin_selection" },
    { "type": "move_frames", "forward": true, "count": 4 },
    { "type": "replace_selection", "register": "b" }
  ]
}
```

`yank_selection` and `cut_selection` also take a register. `finish_selection`
retains the endpoints and stops extending; `clear_selection` removes them.
`move_beats` takes `forward` and a positive `count`; `move_scope` takes `end`.
These instructions keep the same bounded work and atomic failure rules.

The existing ordinary Sequence reducers still refuse range endpoints inside
a composite child. Enter that group or select its complete boundaries. Visual
macro support does not yet add partial Repeat/Retime occurrence editing or
the full semantic text-object grammar.

Save dry-run checks the same program, bank capacity, version and storage rules
as save. It does not execute the body. Run dry-run resolves the whole program
and admits its Compound without saving history, captures or registers.
Motion-only runs return the resolved position without writing or moving the
native cursor. The JSON request's `dry_run: true` remains effective when the
command-line flag is absent.

An open native project executes through its authenticated writer. The host
prepares runtime copies before committing and publishes the saved bank even
when workspace refresh fails. Successful bank writes retain
`committed_registers` with project ID, revision ID and bank version, independently
of detailed output; authored runs also retain `committed_revision`. A compact
reply preserves both receipts if full detail exceeds transport capacity.
Socket uncertainty never authorizes automatic replay. Remote operations wait
for unread native edit, copy and Macro continuations and do not create their
own native cursor continuation. See [live project access](LIVE_PROJECT.md).
