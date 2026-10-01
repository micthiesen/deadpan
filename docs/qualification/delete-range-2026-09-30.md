# Visual range deletion, 2026-09-30

## Behavior

In Your edit, `v`, motions, `d` removes one nonempty half-open interval of linked
picture and sound. Active and finished ranges work in either direction. Empty
selection remains distinct from no selection and cannot fall back to deleting a
whole beat. Without a selection, `dd` retains its whole-beat behavior.
`:delete` captures its range or beat on command entry, including missing/empty
targets, independently of the Original copy register. Changed scope, session or
revision rejects the captured command. Native fields, composition and focused
controls keep their input; counted or held activation does not repeat the cut.

One `DeleteRange { parent, range, identities, timing }` transaction shares
replacement's ordinary Sequence endpoint splitting. Source, ordinary Hold and
supported fragment endpoints work, with complete intervening composites.
Original clocks are captured before Split; suffix owners retain their old entry
before removal. Root sounds transform once and marks/permissions follow their
existing policies. Aligned terminal or full deletion creates no unused clock.
Only an edit needing both split capture and a suffix needs a second ordinal.
The service returns the exact join and selects its retained beat. One Undo
restores the complete authored document.

A successful database commit survives failed preview refresh: the service keeps
its receipt, reports that the range was saved and asks the user to reopen before
editing or undoing. The stale delivered workspace cannot consume that receipt's
cursor/selection. A deterministic injected failure verifies persisted state,
reopen and exact Undo, while preserving coalesced import completion behavior.

Core 34/database 43 are unchanged. Historical Delete/ReplaceSource behavior and
frozen adapters remain intact. Partial Repeat/Retime occurrence targets,
role-only deletion, general motion/text-object operators and edited-content
registers/copy/move remain required. Deletion currently does not fill a register.
No DP requirement or delivery gate is completed by this increment.

## Exact picture, PCM and persistence

The before replay reproduces the missing route: `gg20lv10ld` leaves 120 frames,
cursor 30 and pending `d`. The corrected replay removes `[20,30)` once, leaves
110 frames and cursor 20, and compares actual decoded pictures at the join:
new frame 19 is old 19; new frame 20 is old 30. The nested fixture exercises a
nonzero ordinary Sequence scope, rejects partial composite endpoints, removes
whole composites and restores the complete document with one Undo.

Six actual decoded-PCM tests use finite nonzero 44.1 kHz mono samples at
30000/1001 project fps and a 48 kHz mix. The primary seven-frame witness removes
`[1,3)`, retaining old samples `[0,1602)` and `[4805,11211)` at new
`[0,1602)` and `[1602,8008)`. Every retained sample compares exactly. Tests read
the final 256 samples cold, then all samples in reverse irregular chunks, then
the entry again. An independent scalar source-entry oracle supplements the
original-document comparison.

Additional cases cover nested Source/RoomTone and outer siblings, earlier
InsertTime bindings, complete Repeat plays with RoomTone gaps, and Preserve
output against independently prepared full stretch history. The aligned case
independently verifies the retained 2/5-sample entry and that terminal deletion
leaves an unbound prefix unbound. These are RawStageAudio checks, not final edge
fades, mastering, device output or an acoustic comparison.

Two store tests and one CLI subprocess test cover read-only preview with a held
writer, unchanged database rows before commit, exact preview/commit equality,
one-command history, mixed historical/current commands, reopen, Undo/Redo and
stale rejection after Undo. Invalid IDs and timing overflow leave authored rows
unchanged. The core additionally checks all 120 nonempty ranges over Source
lengths one through eight, marks, permissions, node budgets, retained treatments
and both zero/one-clock ordinal limits. SQLite disk/commit fault injection is
outside these new tests.

## Production replay and review

The final range replay passes 98 scenario checks; the existing slice-placement
regression passes 400. Each passes all 11,904 production routing cases against
the live Kestrel source, SHA-256
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
Checks include active/reverse/finished/empty selections, captured absence,
stale session/revision/group, ordered input batches, counts, held keys, synthetic
IME and native-control focus. At 960×640, actual paint clips contain valid,
empty and missing-target command guidance plus the exact range and `d` hint.
Selected 1280×820 and 960×640 screenshots were inspected. The capture allowance
was reached later in the replay; remaining semantic/paint assertions continued.
The missing-target hint has a paint-bounds check but no retained screenshot.

Independent core review found two unnecessary clock-allocation restrictions;
both were fixed and rechecked. Independent native-service review found that a
refresh failure could misreport an already saved edit and consume its selection
against a stale workspace. The receipt/warning correction and injected recovery
test resolve that finding. The narrower stale-workspace guard preserves valid
coalesced registration updates. Review records are retained with the evidence.

## Full verification

The locked workspace passes 2,685 tests and the UI-feature app passes 380, with
none failed or ignored. Formatting and strict all-target workspace Clippy with
the UI harness feature pass on Rust 1.97.1. The verification controller remained
live through a conversation refresh and recorded the passing workspace result, then ended
before starting the UI-feature and lint stages. Its terminal output was
unavailable; the cause is not established.
Only the remaining checks were started separately. The passed workspace run and
the process observations are retained without overwriting them.

The resumed runner resolved Homebrew Rust 1.98 rather than pinned Rust 1.97.1:
its UI tests passed, but Clippy failed on two newly introduced constant-chunk
lints in unchanged `audio_session.rs`. All 162 workspace test binaries and the
native/replay binary retain the 1.97.1 standard-library compiler identity;
their hashes and the two 1.98 UI binaries are recorded. The corrected runner
explicitly uses `rustup run 1.97.1` for formatting, Clippy and the UI-feature
tests; all three pass, with 380 UI tests and none failed or ignored. The final
UI binaries also retain the pinned compiler identity. No source change or
repeated workspace run was needed for that correction.

## Native verification

A separate debug QA bundle, `dev.thiesen.deadpan.range-qa`, opened the retained
private fixture. The user's Cursor QA window and Space were untouched.
Keyboard input rejected an empty selection, showed Edit `[5,10)` with the cut
hint, removed exactly five frames and returned the cursor to 5. One Undo
restored 120 frames. Consistent SQLite backups verify exactly one DeleteRange
and one Undo, complete authored restoration except the fresh revision, unchanged
rows in all 16 unrelated tables and a released writer lock.

The first pass did not inspect the picture. A separate Redo/Undo pass then
visually inspected the rendered fixture slates: `010` at Edit 5, `004` after
one `h`, and `010` after `l`. Undo again restored 120 frames. A second database
comparison verifies exactly one Redo and one Undo, no new command, identical
history and all authored fields restored. Both native observation records are
retained. CUA images were inspected in-session but the available API did not
provide saved screenshot paths; retained screenshots come from the GPU replay.
The isolated bundle exited and released its writer lock after both passes.

## Retained failures

All failed attempts remain in the evidence rather than being overwritten:

| Attempt | Cause and correction |
| --- | --- |
| `core-first` | Missing `FrameRange` and `SplitIdentities` qualification in the new command declaration; corrected before the targeted run. |
| `core-range` | A fixture counted empty split wrappers as treated fragments, and a node-limit fixture separately exhausted audio-binding work; corrected the assertions to measure their stated properties. |
| `app-first` | A large harness JSON macro exceeded recursion depth; new metadata is assigned after the existing object. |
| `build-fifth` | The new accessibility focus query lacked its `Queryable` trait import; added it before rebuilding. |
| `clippy` | The resumed runner used Homebrew 1.98 and encountered two new lints in unchanged media code; rerun with explicit pinned 1.97.1. |
| `verification-pinned-controller-first` | A missing quote in the scratch runner caused a syntax error before any check started; corrected and syntax-checked before relaunch. |
| `replay-first` | The harness observed worker idle before consuming its committed mailbox; it now waits for the revision or the old pending-operator outcome. |
| `replay-second` | A no-op assertion compared the pre-Undo revision, despite fresh Undo IDs; it now compares the immediate entry snapshot. |
| `replay-third` | A lone synthetic repeat press was recomputed by egui as a first keydown; the fixture now establishes held-key state and asserts command admission directly. |
| `replay-fourth` | Clicking Monitor did not grant keyboard focus; the witness now uses AccessKit Focus on the Keys control. |
| `replay-fifth` | Tab had moved focus to Sounds, which correctly omitted the range-cut hint; the layout witness explicitly returns to Your edit first. |

These harness corrections preserve the original behavior assertions; they are
not evidence of additional product failures. The core and refresh issues found
by independent review are separate production corrections described above.

## Evidence and limits

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-delete-range/)
includes command logs, failed and final replay reports, source manifests, PCM
oracles, selected images, database backups and independent review records.
Runs use Rust 1.97.1, the pinned LGPL FFmpeg prefix, Apple M5 Max and macOS 26.5.2.
The debug replay binary is
`d2c7c9c226709175a2091ec67c924fcc90e4280a6e00724f489f534c16dc2fb9`.
Final source inventory SHA-256 is
`66108b6f2b03fe21a0011fa43a30d94a07841ab3587cc61ac0a83621c8b336d4`.
Source identities from earlier parallel runs are retained separately.

Power, thermal state and the OS file cache are uncontrolled. These checks do not
establish latency targets, physical IME/layouts, VoiceOver acceptance, listening,
export equivalence, release packaging or the full editing workflow.
