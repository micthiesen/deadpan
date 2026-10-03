# Exact sibling selections, 2026-10-03

This increment adds an exact structural selection boundary for upcoming text
objects. It does not add the `ib`, `ab`, `ig` or `ag` keyboard workflow, and no
product gate is complete.

## Behavior

`SliceCaptureSelection::Children { first, last }` identifies an inclusive span
of direct Sequence children. Copy, Group, Repeat and `DeleteChildren` retain
empty groups at the selection's endpoints, where picture time alone cannot
identify the intended structure. A zero-duration span still contains exact
subtrees and pastes at an explicit sibling slot.

Historical register admission recaptures the same identities and full payload.
A cut cannot pair this capture with a time-only deletion, or with different
endpoint identities that happen to cover the same video frames. Cut, paste and
Undo use the existing atomic project service and durable history. Independent
root sounds remain outside captured ownership and transform once on deletion.

See [the selection contract](../STRUCTURAL_SELECTIONS.md) for exact scope,
serialization, timing and remaining object integration.

## Review

Independent reviews covered capture validation and ownership, mutation timing
and marks, and store/native receipts and placement. One confirmed finding was
misleading empty-forest copy and placement labels. These now say empty contents
and show the selected roots. A suspected zero-duration Repeat timing issue was
dismissed because Repeat preflight already refuses an empty body.

## Core, store and audio checks

- Core: 195 unit tests, 39 edited-slice integration tests and 71 insertion/edit
  integration tests pass. The new cases cover exact sibling slots, all-empty
  forests, parent-effect exclusion, owned marks, malformed wire payloads,
  historical recapture, independent paste identities, exact Group/Repeat and
  all 15 inclusive spans in a five-child deletion fixture.
- Store: all 90 unit tests pass. New tests refuse same-time cuts with different
  structural identities without changing history or registers, then exercise
  valid cuts, reopen, Undo and Redo.
- Audio: all 73 composite-edit tests pass. The new real-WAV case deletes a
  nested three-frame forest before a Preserve Retime at NTSC rate. It compares
  all 1,602 retained prefix and 11,211 retained suffix stereo samples with no
  tolerance, including cold and reverse irregular reads. The destination's
  exact sample allocation is one sample shorter than the old suffix interval.
  A separately placed sound has a seven-sample offset and retained mapping.
  Its once-transformed routes and every bit of the resulting 12,813 authored
  stereo samples match the existing range-deletion reference.

Retained failed runs exposed two fixture mistakes: the new audio sound lacked
its required source qualification record, and the new app proposal used an
invalid zero draft ID. Both fixtures now enter through the existing validation
contracts; those production contracts were unchanged.

The first replay command used the internal module name `splice`; the CLI
requires `place-slice` and rejected the command before replay. The next run
passed its new rendered labels and layout checks but failed a fixture assertion
that prohibited all picture work for empty contents. At Destination focus, the
preview correctly requests the saved destination picture. The corrected
assertion permits that exact committed workspace and frame while rejecting
copied or proposed source work. The displayed frame and caption must still
remain unchanged.

## App and rendered verification

The default app passes 748 tests and the optional `ui-harness` configuration
passes 784, including four headless integration cases in each. These counts
overlap. Strict workspace Clippy, strict optional-app Clippy and `cargo fmt
--all -- --check` pass with locked dependencies. This is a scoped integration
gate; the full workspace test suite was not rerun for this increment.

The final `place-slice` replay passes all 846 checks. The new case saves an
all-empty two-root Children capture through the public store API, reopens the
project, selects register `f`, and uses production `:splice`, Enter and Undo.
It verifies the restored historical selector, fresh runtime identity, exact
same-time sibling slot, both imported roots and their nested child, no source
endpoints or source-picture work, a retained destination picture, one commit
and restored source structure. Capture and placement do not invent audio time.

Paint checks and manual image inspection cover the source card, destination
picture, exact slot and controls at
[960×640](../../tools/media-qualification/evidence/2026-10-03-sibling-selections/screenshots/forest-placement-minimum.png)
and [1280×820](../../tools/media-qualification/evidence/2026-10-03-sibling-selections/screenshots/forest-placement-default.png).
The intermediate screenshot allowance was exhausted, but named checkpoints
retained their reserved image capacity and semantic checks continued.
The production shortcut audit passes 3,319,728 routing cases against 62 live
Kestrel reservations with no conflicts or registry drift. No binding changed.

## Environment and evidence

Base: `4deae8e54816d218e749d6d3397b156c381e6a83`.
Apple M5 Max, 128 GiB, macOS 26.5.2, Rust/Cargo 1.97.1, locked dependencies and
FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

Final source inventory:
`871d666c631572a2acdaa73067154188ce4e250e5d67a81289fe6a5429312f89`.
Final replay executable:
`a64ede8b150c0f8d031c02bb90bbd6d9148b623edf607fdbb320ff3bebd8c3d5`.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-03-sibling-selections/metadata.json)
contains exact commands, source inventories, failed and corrected logs,
compressed replay reports, review notes and inspected captures. `SHA256SUMS`
covers all evidence files. Production sources are unchanged across all passing
checks. Earlier inventories differ only in the two test-fixture corrections
and the optional rendered replay; metadata lists those paths for each check.

## Limits

The rendered case seeds its Children register through the public store API
because keyboard text-object production is not implemented. This is not evidence
of a completed native text-object workflow. The next work includes tagged
Visual object state, semantic scope transitions, native object grammar and the
separate beat-owned temporal attachment lifecycle required by `ib` and `ab`.

Native window, physical keyboard, OS IME, VoiceOver, acoustic delivery and
large-project performance are not qualified by this increment. No native app
window was opened for this backend boundary. Full DP-01 through DP-24 and Gates A
through G remain open or partial.
