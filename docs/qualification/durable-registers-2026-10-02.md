# Persistent slice registers, 2026-10-02

Named and default Original/Edited copies now survive closing and reopening their
project. Copies leave the timeline and Undo/Redo unchanged. A cut saves its
deletion and register contents atomically. The keyboard paths and visible
placement preview are unchanged; see [the contract](../NAMED_REGISTERS.md).

This increment starts at `e7d472be5a9433bb87ef16c4cdb2dc3c4c5edfd4`.
SQLite schema 53 adds a project bank outside timeline revisions; the document
schema remains 43. Canonical payloads are deduplicated and bounded to 64 MiB
across 26 named slots and the default slot. Reopen validates historical
provenance and supplies fresh runtime identities. Checkpoints retain the bank.

Macro content and atomic bounded execution, semantic dot-repeat, the remaining
DP-06 grammar and mode maps, and full recovery qualification remain open.
No DP requirement or Gate A through G is complete.

## Automated verification

The [retained evidence](../../tools/media-qualification/evidence/2026-10-02-durable-registers/README.md)
records exact commands, exits, source inventories, review findings and logs.

The complete locked workspace run passed 3,527 tests and failed six stale
fixture assertions, with zero ignored. After the fixture corrections, the
complete affected targets passed all 22 CLI and 133 migration tests. Together
these runs cover 3,534 distinct passing default tests. The optional app
`ui-harness` suite passes 635 app tests and three headless tests. These suites
overlap and must not be added together.

The full workspace run used source inventory
`927fd8a4ac82ca71044563e7393250225f0edd2a860c38e97e6607c679141d9d`.
The affected targets, optional app suite and final formatting check use
`9d743fd6422333faf0604749aa72c1f40eb33ee9a6b82fc38abb1da23d8691ae`.
Source inventories include tracked and untracked code. The existing nonfatal
debug linker warning about the 16 MB `__eh_frame` limit remains in the logs.

Strict workspace/all-target lint passes. After the final help correction, both
base-app and optional UI all-target lint also pass with warnings denied.

The full release replay and corrected delete-range rerun cover all 28 ordinary
scenarios and 3,896 checks. The named-register scenario passes 170 checks and
delete-range passes 310. The separate production-routing audit passes 148,552
cases over 62 Kestrel reservations, with no conflict or source drift.
Generated-picture replay is explicitly skipped without a real accepted-bundle
fixture. No accepted evidence is fabricated for that scenario.

The full replay used the `9d743f…` source inventory above. The corrected deletion
replay uses `06506d24ec8df44f318ad8b2bd7ae3c8e5b08487b3c53e1a64a39db2b08b89d2`.
The final help-text correction uses
`0edf23127768e2982a8cd33695be2ac207b92025426e1ace5150d96993720a40`.
Only two replay label selectors and one help sentence changed after the passing
test suites. The source inventory remains unchanged during each recorded check.
The final release build and focused register replay pass all 170 register checks
and the same routing audit after that help correction.

## Native verification

The native app saved register **z** as Original `[0..8)` using `v`, `8l`, `"z`
and `y`, then quit with exit code 0. The bank advanced from version 12 to 13.
Consistent SQLite snapshots prove only `register_state`, `register_contents`
and `registers` changed. Timeline, history and the other named slots are exact;
the default slot aliases **z**.

The final executable reopened that package and showed the restored Original and
Edited inventory. `"z:splice` displayed the exact `[0..8)` source and both endpoint
pictures. `12l` moved the proposed destination from Edit 0 to Edit 12; Escape
cancelled without changing the source or timeline. Native accessibility also
confirmed the corrected help sentence. The app quit with exit code 0, no Deadpan
process remained, and the writer lock was available. Every table and register
value after reopening matched the post-save snapshot. SQLite integrity and
foreign-key checks passed.

The final binary SHA-256 is
`ec4448746e0fe788d15f9e8a5bb4693343118179ef8a768f36c1f025f8381630`.
Rendered inventory and selection captures were inspected at 960×640 and
1280×820. Native screenshots show the restored inventory and placement.

## Review and retained failures

Independent review found three correctness issues, all fixed:

- A hostile database without expected uniqueness constraints could collapse
  duplicate slot names during loading. Bounded validation now rejects duplicate
  names and payload IDs before decoding; field type and size checks precede
  SQLite grouping. Tests cover direct reads, checkpoints and read-only reopen.
- A fast paste could capture the prior value while an accepted copy was still
  saving. New placement waits for that write's typed acknowledgment, including
  when a later refused intent supersedes its UI confirmation.
- Close deferred by render shutdown retained runtime copies and marks. That
  path now clears them after the writer releases, while SQLite retains the bank.

A concern about reconstructing captures under Repeat/Retime was refuted:
ordinary Sequence ancestry is already required by core capture admission.
The reviewer ran neither Cargo nor the native app.

The first recorded workspace build failed to compile a malformed negative-test
qualification ID. A second recorded build was interrupted to fix the same issue
in another fixture. Both failures are retained and neither counts as a pass.

The six full-suite failures were one CLI doctor assertion expecting schema 52
and five synthetic migration fixtures that restamped a modern database without
removing its new register tables. All eight such fixture constructors now remove
only a verified-empty bank. A new test exercises six authentic-v1 databases
with injected register tables, proving strict refusal and preservation of the
original and backup cells. Production migration checks were not weakened.

The initial release replay's deletion paint assertion matched both the fully
visible deletion footer and an unrelated instruction in a scrolled inspector.
Independent review confirmed the collision from paint bounds and the screenshot.
The assertions now match each action's complete guidance sentence and retain
the same visibility checks. A first focused rerun exposed using the range-cut
sentence for frame cuts; that selector was corrected to its actual wording.
All 310 deletion checks then passed. Native QA also found one obsolete help
sentence describing the copy as session state; it now explains reopening.

## Limits

Tests run on Apple M5 Max, macOS 26.5.2, Rust 1.97.1 and the pinned FFmpeg
development prefix. Cross-project transfer and the complete crash/recovery
matrix remain unqualified. Offscreen replay cannot establish physical keyboard
layouts, native IME, VoiceOver, audio-device behavior or display color.
Native checks use synthetic OS input on the current layout. `j` scrolled the
help inventory; CUA `Page_Down` attempts did not establish native page-key
behavior. The developer wrapper retains build-host libraries and provides no
signing or release qualification. The fixture is `cfr-bframes.mp4`, SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
