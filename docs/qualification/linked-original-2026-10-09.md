# Explicit linked Original creation

Source base `11509a53`, Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1
and pinned FFmpeg 8.0.3. Core format 47 and database schema 75 are unchanged.
Implementation, review and verification are by the sole working agent.

## Change

Specification 20.3 requires an explicit linked-source choice. Native New
previously always retained a managed Original; the linked checkbox applied
only to later sound or compatibility imports. The start screen now offers
**Link video in place…**, File offers **New Linked Project…**, and
`:new-linked` opens the same picker. Incomplete projects offer **Link Original
in place…** / `:original-linked`. Ordinary New and Choose Original remain
managed, independent of sound import options. Captured dialog intent and
deferred session changes carry ownership; bookmark creation stays on the
preparation worker. No second Original can replace an initialized one.

`project create-original <package> <absolute-video> --linked` provides the
headless equivalent. It publishes the package only after the complete measured
Original and its Undo baseline are ready. Existing managed and YouTube callers
keep their managed default. The searchable command reference describes both
native choices. Linking avoids a permanent project copy; verified private
working snapshots still protect decoding.

## Automated and replay verification

The service baseline test exercises managed and linked ownership, real media
qualification, a full 120-frame initial edit, the Undo floor, second-video
refusal, reuse, close and reopen. It verifies the retained path and macOS
bookmark. A failed audio-only qualification reopens as incomplete and retries
with a linked video. The headless test verifies both ownership choices and
validates each complete package. Existing stale initialization and interrupted
worker tests remain in the focused run.

The initial focused run passed 49 of 50 checks; the sole failure required
regenerating `docs/COMMANDS.md`. Its generator intentionally fails after writing
so the changed reference is reviewed before an ordinary passing run.

The full app and CLI UI-feature run passes 1,783 tests in 372.153 s, with
12 slow tests and nine existing skips. Nextest marked an unrelated pure
gain-parser test `LEAK` after its assertions passed; its isolated serial rerun
passes in 0.012 s without the warning. The warning's cause was not established.
Strict workspace Clippy and formatting pass. Logs are retained at
`/tmp/deadpan-linked-original-verify-20261009.log` and
`/tmp/deadpan-linked-original-finish-20261009.log`.

The final sorted map of 42 changed Rust files has SHA-256
`b4ad36f43469f0056c38320a3296c0ed1507e0d77cc07575cce373f1acd3d2fa`
and is retained at `/tmp/deadpan-linked-original-source-20261009.json`.

The initial `linked-original` replay stopped at a test helper that requires a
selected picture even though the scenario deliberately created an incomplete
project. Removing that inapplicable helper preserves the explicit incomplete
state checks. Visual inspection also found the bottom of Open project clipped
at 960×640; compact vertical spacing was reduced to keep the complete card
visible. No native screenshot was used.

The first release linked-creation replay passed 20 checks. The adjacent
YouTube replay exposed a real completion race: the service released its
admission bit before the UI consumed the successful Open workspace. The flow
treated idle as failure, lost the creation message and left its explicit
cookies selection behind. Open now returns an independent bounded completion
reply, including after draining an active render. The flow waits for both
that reply and its matching workspace. A still-running admitted Open keeps its
normal lifetime; the admission timeout does not terminate it. The replay now
holds workspace delivery deliberately across successful Open, then releases it
and checks the final title, message and cleared input. The service regression
checks an invalid Open and a successful Open held behind render teardown.
All 38 focused tests and all 319 project-service tests pass after this change;
the latter run took 47.479 s and marked two tests with pipe-close warnings.
Both pass an isolated serial rerun in 0.240 s without warnings; their cause
was not established. Final strict workspace Clippy and formatting pass.

The final release build completes in 2m 00s. Its app SHA-256 is
`bbec00017583e8d42d7bed4c89b5cd31a5c16e102f7787eec65fd1b7bad3a6d0`.
The release `youtube` replay passes 63 checks, including delayed workspace
delivery; `linked-original` passes 20. Each additionally passes 21,884,016
production-router cases against 62 pinned Kestrel bindings with no conflict.
The live Kestrel source was unavailable; the pinned source digest is
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
YouTube has one second-retry layout frame; linked creation has four and two
runs of consecutive retries with distinct causes. No ignored-retry paint
check fails. The final offscreen images were inspected: the minimum start
card, including Open project, fits; successful URL import shows the created
title, real fixture picture and complete confirmation footer. Reports and
binary hashes are in `/tmp/deadpan-linked-original-release-20261009-v2`;
the earlier failing reports remain in the corresponding directory without
`-v2`. The final log is `/tmp/deadpan-linked-youtube-finish-20261009.log`.

## Native check

The developer wrapper `/tmp/deadpan-linked-original-native-20261009/Linked.app`
ran debug app SHA-256
`3089d8a711be6d78e4aa9fdc3552cf453b1144a9d0a934bb8886a048fbab19e1`.
This precedes the replay-helper, compact-spacing and YouTube Open-reply changes. Helper hashes
are retained alongside it in `binaries.json`.

Native Accessibility and keyboard input verified the File menu entry, linked
picker title, cancellation back to the unchanged start screen, and command
entry through `:new-linked`. Selecting a disposable copy of the real CFR/B-frame
fixture created a project under system Documents/Deadpan, with 120 frames,
Saved status and disabled Undo. The authoritative inventory reported
`managed: false`, its external path and a platform bookmark.

After closing the project, its external test video was renamed. Native Open
found it through the bookmark, verified its content and reported the new
location. Original view displayed source frame 1 of 120. The authored JSON
dump before and after the move is identical, full validation passes, and the
app exits with no QA process remaining. Evidence is in the same temporary
directory. The closed test package was moved out of Documents to
`Native Created.deadpan` there and validated again. This does not establish
clean-machine or physical-keyboard behavior.

A portable copy of that native-created linked project then validated and
rendered while the external video path was unavailable. The normal Render
workflow completed its emitted-file verification and publication; the movie
SHA-256 is `6de22771a7c1fd4ac7b3968d06852c218d8dfa232fb7a5c4f012a328d6c4f08d`.
The source was restored afterwards. `portable-*.jsonl`, the movie and its
verification report are retained with the native evidence.
