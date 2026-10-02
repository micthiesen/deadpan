# Named session registers, 2026-10-02

`"a` now chooses a named copy of an Original moment or an editable slice.
Successful named copies and cuts also update the default copy. `"ap` pastes
that named content; `"a:splice` opens the existing visible placement workflow.
The footer shows the name and type, and `:registers` opens a live inventory.
See [the contract](../NAMED_REGISTERS.md) for lifetime and captured intent.

This increment starts at `885b46b69272fc6957b5647b5c9fcb40d88afff8`.
Only app source changes; core schema 43 and database 52 are unchanged.
Registers remain session-only. Persistence, macro content and atomic execution,
semantic dot-repeat, and the other DP-06 requirements remain open. No DP
requirement or Gate A through G is complete.

## Verification

The [retained evidence](../../tools/media-qualification/evidence/2026-10-02-named-registers/README.md)
records exact commands, exits, source inventories and logs, including failures.
Source manifests include tracked and untracked code. Sources did not change
during any recorded check. Overlapping suites must not be added together.

- The locked base-app suite passes 592 app tests and three headless tests.
- The final locked `ui-harness` suite passes 628 app tests and three headless
  tests. Formatting and both app/all-target Clippy configurations pass with
  warnings denied.
- The full release replay passes all 28 ordinary scenarios and 3,831 checks,
  plus the separate Kestrel audit. Generated-picture replay is explicitly
  skipped because no real accepted-bundle fixture was supplied.
- The production routing audit passes 148,552 cases over 62 Kestrel reservations,
  with no source drift or shortcut conflict.
- After the feedback correction, the final release build and focused replay
  pass all 136 register checks plus the same separate routing audit.

The base suite and full replay use source inventory
`563331e1f337fc258f2d100cbce8eac87286e4e6887857f041c9fde1454a4a86`,
before the final cancellation-message correction. The final UI suite and both
Clippy configurations use
`00f8a915f9d240f968d4c9676edf7c5e07a387ed8c86dc9edda23c98d408bc4e`.
The prior full-workspace gate remains applicable to unchanged backend sources;
it was not repeated for this app-only increment. Existing nonfatal debug linker
warnings about the 16 MB `__eh_frame` limit remain in the logs.

The final release binary SHA-256 is
`c04d111ccc04c2612a345c1fe895e636d551aeb7f3bcee0a8659358dfc94954a`.
It was reopened for native verification of the corrected named-cancellation and
default-copy messages, then quit. The final process inventory contained no
Deadpan process; the writer lock was available and the SQLite dump was unchanged.

## Behavior covered

The focused scenario retains independent Original ranges **a** and **b**, an
edited Hold in **c**, and a durable cut in **d**. Exact before/after insertion
checks include source pictures, full-document Undo and historical paste after
later edits. Empty **z** refuses without default substitution or a history write.
Named `:splice` uses the captured slot, and cancelling leaves the source intact.

Real service replies are held only at delivery to test pending-copy ordering:
selecting another name cannot redirect a capture, newer writes supersede older
results, and filling a slot cannot supply content to a command that captured it
empty. Blur rejects stale named commands for copy, cut, frame cut, paste and
placement. An actual project close/reopen also rejects an open named yank.
Capture, history, source qualification, decoding and rendering use production
code. Native fields retain quote text.

Rendered 960×640 and 1280×820 captures were inspected. Pending quote instructions,
selected name/type, copied range and the inventory remain readable. A native
current-layout check used Shift+Quote, `a`, `y`, `:registers`, empty `"zp`, and
`"a:splice`. Both endpoint pictures appeared; `12l` moved the proposed destination
from Edit 0 to Edit 12 while preserving the Original range [0..8). Escape cancelled
the draft. Its complete SQLite dump was unchanged after quitting, the process
exited with code 0 and the writer lock was available.

## Review and retained failures

Independent read-only review identified a missed pointer-copy call signature
and two stale-command cases. The common copy path now receives pointer copies;
Escape/blur cancels captured named intent, and command entry binds its session.
The reviewer did not run Cargo or launch the app.

Native QA then found that Escape cleared the chosen name but left an instruction
saying the next action would use it. Explicit cancellation now replaces that
message. The unnamed/default choice no longer promises cancellation because it
is already the normal copy behavior. Replay checks preserve the bank while
checking the new cancellation feedback.

Retained harness/build failures are distinct from product findings. The audit
initially expected 28 prefixes instead of the new 32; its formula now includes
the register family while retaining exact conflict assertions. A cut helper
needed its existing replay feature gate. One replay incorrectly required an
unrelated status message to remain byte-for-byte unchanged across a valid delayed
service snapshot. It now checks bank, selection, revision and history integrity,
plus absence of stale copy confirmation. The original failure remains retained.

## Limits

Hardware is Apple M5 Max, macOS 26.5.2, Rust 1.97.1 and the pinned FFmpeg development
prefix. The qualified fixture is `cfr-bframes.mp4`, SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
Offscreen replay uses production SQLite, decoder and Metal services. Picker
selection and input are scripted; some playback scenarios inject delivery.

The native check uses synthetic OS key delivery on the current layout. It does
not qualify physical hardware, every layout, native IME composition, VoiceOver,
audio-device output, listening, physical display color or performance. The local
developer wrapper retains build-host library dependencies and supplies no signing
or release qualification. Persistent registers and recovery remain required.
