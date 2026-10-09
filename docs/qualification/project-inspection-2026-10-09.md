# Concurrent project inspection

Source base `1e1e560a`, Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1
and pinned FFmpeg 8.0.3. Core format 47 and database schema 75 are unchanged.
Implementation and review are by the sole working agent.

## Change

Specification 20.5 permits two app windows to inspect one project. Previously
the second native Open returned `ProjectAlreadyOpen`. It now opens a fixed
read-only database snapshot while the first owner keeps its writer lock and
authenticated command endpoint. Explicit reopen refreshes the snapshot, or
acquires write ownership once the first owner closes. The header and refusal
messages explain the view and how to reopen it. Loss of the owner alone never
promotes the viewer.

The snapshot is a bounded SQLite backup to private temporary disk storage,
not a long-lived transaction against the live database. Its source transaction
ends before validation and viewing. The copied connection is reopened read-only
and query-only; media retains the existing package readers. No schema or
authored-state changes are introduced. Newer-schema native views also use this
copy boundary.

## Verification

Focused checks pass: 14 selected tests including snapshot isolation, writer
checkpointing, size/deadline refusal, two real native project services, unchanged
owner discovery, refused edit/Undo, explicit refresh, handover and the existing
newer-schema package-byte check. The store witness commits while the view is
open, truncates WAL with `(0, 0, 0)`, reads the old view and removes its private
files on drop. It preserves the writer's session marker exactly.

Initial compilation caught unsupported unsigned SQLite column reads and a
test's incorrect validation-return assumption. The first executable tests
then exposed macOS's `/var` temporary-root alias failing SQLite no-follow
admission. Resolving that trusted root before creating the private database
fixed all three failures. Logs remain at
`/tmp/deadpan-inspection-focused-20261009*.log`.

Formatting and strict workspace Clippy pass. The full default-feature workspace
run passes 5,481 tests (453.070 s, 14 slow, 10 existing skips). The full
UI-feature run passes 1,095 of 1,096 (230.358 s, eight slow, two existing skips):
one room-tone refusal test read a pending background update after the service
cleared its admission bit, before publishing the command's error. Its bounded
wait now requires the refusal itself before asserting the unchanged document
and captured response. All five room-tone tests pass with both feature sets; runtime
room-tone behavior is unchanged. Workspace doc tests pass. The existing debug
linker warning about the compact-unwind table remains. Full logs are retained
in `/tmp/deadpan-inspection-verify-20261009.log` and
`/tmp/deadpan-inspection-finish-20261009.log`.

The final sorted changed-Rust-file map has SHA-256
`ac30eff6f7f5cbc36d955aa4de2cfb8c0d34e090941fc263a66324e979c958c9`
and is retained at `/tmp/deadpan-inspection-source-20261009.json`. The only
Rust change after the first full suite was the room-tone test wait.
The release build completes in 3m 29s. Its app SHA-256 is
`1dfdc3e0b6a0e4e483cc8eb5a6dc97908232253667adcfd7fd52984bcbcd1f08`.
Release `backups` passes 58 checks and `storage-failure` passes 13, plus one
Kestrel audit each. The expanded backup scenario uses a real second project
service, navigates and renders the captured revision, refuses its split,
retains the view during an owner Undo, refreshes on Open and acquires a new
writer only after the owner closes. The report preserves 18 second-retry
layout frames and seven runs of consecutive retries with distinct causes;
storage-failure preserves three second-retry frames. No ignored-retry paint
check fails. Backup capture reaches its intermediate-image allowance while
retaining named checkpoints. Its inspected `backups-121.png` shows the
Read-only header, real decoded picture, focused Beats and complete refusal
message. These are offscreen replay images, not native screenshots. Reports,
images and binary hashes are under `/tmp/deadpan-inspection-release-20261009`.

## Native two-instance check

Two separate native processes use developer wrappers `Owner.app` and
`Viewer.app` in `/tmp/deadpan-inspection-native-20261009`. Their executed debug
app SHA-256 is
`6d14d39624faddbd9c186c34ab32a0129227c5c10b084fba2f7235e8118055e0`;
helper hashes are in `debug-binaries.json`. Only Accessibility and native
keyboard input were used, with no native screenshot.

Both open the same disposable real-media project. The owner shows Saved and
two beats; the viewer shows Read-only with the fixed-view/reopen explanation.
The viewer navigates to Edit boundary 20 and its `:split` is refused without
changing the edit. The owner splits at 20 and shows three beats while the
viewer still reports two. Explicit native Open refreshes the viewer to three
beats, still Read-only. After Cmd-Q closes the owner, the viewer's `u` still
refuses. Explicit Open then shows Saved; `u` saves and returns to two beats.
The viewer quits, neither native process remains, and full project validation
passes. The final authored dump equals the initial one except for its new Undo
revision identity. Dumps, validation output and the final process check are retained in
the same directory. This does not qualify clean-machine packaging or physical
keyboard delivery.
