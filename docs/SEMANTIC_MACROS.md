# Semantic macros

`q` plus a letter records a Macro in a project register. While recording, `q`
saves and Escape cancels the draft. `@` plus a letter runs a saved Macro;
`3@a` runs `a` three times in one transaction. The command equivalents are
`:record a`, `:record-stop`, `:record-cancel` and `:macro a 3`.
Both binding families are configurable through `macro.record` and
`macro.execute`. Names are case-insensitive a–z.

The current vocabulary is relative frame motion, frame cut and named Macro
call. Motions and cuts retain their requested counts, including when they
clamp at a group boundary. A cut also retains its selected copy register.
The planner resolves each instruction against the preceding staged edit in
the same ordinary Sequence group. Temporal occurrence scopes, text/range
selectors, additional edits and broader semantic dot-repeat remain required.
This is partial DP-06 implementation, not full macro acceptance.

## Recording and input ownership

Recording performs normal edits. A motion enters the body when dispatched;
a cut or call enters only after its matching successful receipt. A pending
operation blocks the next recorded action until that result arrives. Failed
operations do not enter the body. Saving writes a named Macro without adding
an Undo entry or replacing the default copy. Cancelling leaves completed
edits undoable and preserves the previously saved Macro.

The footer shows the register, instruction count and save/cancel controls.
Unsupported actions refuse explicitly. Changing the project, register bank,
ordinary group, pane or cursor outside the supported recording path cancels
the draft. A refresh failure after durable success retains that success and
stops recording with reopening guidance.

Prefix and command entry capture the project session, revision, bank version,
group and cursor, including an absent eligible target. A late letter or command
cannot replace that capture with a newly available context. Text, IME,
focused native controls and Kestrel reservations retain priority. Since egui
has no `Key::At`, logical `@` requires the pressed key's immediate native
`Text("@")` companion. Physical mode explicitly uses Shift+2. Physical input
across real keyboard layouts still requires qualification.

## Planning and atomic execution

`SemanticProgram` contains typed instructions, never raw keys, process hooks,
stored cursor timestamps or preselected node identities. `plan_semantic`
incrementally executes its body against a private document and staged register
bank. It applies each ordinary leaf once and returns a resolved `Compound`,
final context, register writes and an entry trace. Motion-only programs return
no authored request and add no history.

A call resolves a named Macro at call entry and freezes that body for all of
its counted repetitions. Later calls see earlier staged register writes.
Overwriting a running Macro's register with a cut does not change its active
body. Calling a copy or pasting a Macro is a type error. Recursive calls fail;
call depth is at most 16, a body at most 1024 instructions and 128 KiB, and an
expanded run at most 4096 instructions. Calls, motions and clamped no-ops all
consume fuel. Existing Compound step, document and capture limits also apply.

The service binds the actual saved head and bank version, previews the whole
Compound with ordinary media admission, and prepares its final runtime bank
before committing. Every leaf and capture allocation is fresh. One run saves
one reversible history entry and one final bank result. A later failure saves
neither. Undo/Redo replays frozen resolved instructions and never rereads a
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
entrypoint. A macro edit clears the current frame-cut-only dot-repeat candidate.

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
  "count": 3
}
```

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
