# Native root-beat editing, 2026-09-23

This increment connects existing core transactions to native root-beat Repeat,
Delete and Hold-duration editing. It starts from
`41dc86ee13257143f383785c5fec9f7bdc47dee2`, whose
[CI run](https://github.com/micthiesen/deadpan/actions/runs/35901281683) passed.
Core schema 11 and database schema 16 are unchanged. The environment is Apple
M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1, Metal and pinned LGPL FFmpeg 8.0.3
at `/tmp/deadpan-media-compatible-xyhilms4/prefix`.

## Implemented boundary

`rr` or counted `rr` deliberately wraps a root beat. `:repeat N` updates an
existing Repeat, preserving its gap, or wraps another beat. `dd`/`:delete`
deletes one root beat. `:hold-duration Nf` changes an existing root Hold using
positive integer project frames. Targets capture session and revision; the
service rejects stale or hidden targets, allocates identities and commits through
the existing core/store path. All edits retain durable undo/redo.

Completion reports an explicit resulting selection. Wrapping selects its new
wrapper, setters retain their node, and deletion chooses successor, predecessor
or an explicit clear. Markers survive background progress and duplicate delivery.
Registration preserves the active viewer context, including after history clears
the prior marker. Active generation relevance remains protected by the store's
existing guard; this service does not invent missing context observations.

The keyboard grammar distinguishes default repeat count two, explicit one and
invalid zero/overflow. Unsupported ranges, post-operator counts, counted deletion
and command arguments fail explicitly. Held keys cannot finish an edit operator.
Frame motion selects the right-hand root beat, while explicit selection of a
zero-duration row survives refresh, context return and subsequent beat navigation.

Command entry retains same-frame text/IME input before Enter/Escape processing.
Clicking away cancels it, and visible command mode suppresses normal editing
even before blur is processed. Pointer-button batches defer shortcut/command
submission until widget focus is resolved; text events still reach their widgets.

## Native observations

A temporary development `.app` wrapper opened a disposable copy of the earlier
native-created project. SQLite's backup API copied the database; the CLI migrated
that copy and inserted an explicit 30-frame Background Hold and another Source
beat as fixture setup. No arbitrary-boundary Hold insertion is implied.

The native open panel loaded three beats totaling 270 frames at 30000/1001 fps.
Source `rr` only showed the explicit insertion hint and preserved the sequence.
`:sequence`, then `3r`, showed a persistent pending operator; the final `r`
created a three-play 360-frame Repeat and selected the wrapper. The document
total became 510 frames. `120l` displayed sequence frame 121 at the start of
the second play. `:repeat 1` changed the same Repeat to one play and restored
270 frames without another wrapper.

`120l` then `:hold-duration 45f` selected and changed the Hold at that boundary,
producing 285 frames. `dd` removed it and selected the following Source at
boundary 120; `u` restored it, and Ctrl-R/u exercised redo and undo. `j/k`, Tab
and Shift-Tab navigated beats and real panes. Search accepted literal editing
keys and native Option-E/E composition as `rr dd 3rré` without changing the
sequence. Escape returned to pane focus. An unsupported `1s` duration reported
the explicit frame-unit requirement without an edit.

The large native window retained picture dominance, readable root selection,
mode/context/boundary status and discoverable command help. This review prompted
visible total-play counts on Repeat tiles/status/accessibility labels and clearing
the Source insertion hint when changing context. An attempted native zoom action
did not change window dimensions; this run does not establish minimum-size layout
quality. Full VoiceOver, CJK composition, non-US physical keyboard coverage and
the full editorial acceptance session remain open.

After an initial Quit, an accessibility lookup returned a fresh empty window.
A subsequent Quit without another app lookup left no review process. The saved
project independently validated as five nodes and 285 frames. The development
wrapper is not a signed or portable distribution.

## Independent review and tests

Service review found no additional admission/persistence defects. General review
found that registration could steal Sequence context after Undo/Redo cleared a
completion marker; preserving the active view fixes that case. Keyboard review
found visible command entry could lose focus while global delete keys remained
active, followed by a same-frame pointer/focus routing edge. Both were fixed and
rechecked. Parent integration also fixed inconsistent split/coalesced completion
behavior and navigation over zero-duration rows.

Actor tests cover Repeat gap preservation, deliberate nesting, stale
session/revision and hidden targets, atomic invalid commands, deletion selection,
durable history/reopen and edits during held real import preparation. Pure
parser/selection tests and headless egui frames cover counts, text/IME guards,
empty rows, completion ordering, toolbar entry focus, click-away and pointer
batches. The first click-away test overasserted focus on a synthetic pane; its
final assertions cover command cancellation and focus release, with pane routing
covered separately. An intermediate unused-result warning was corrected.

The initial full repository gate passed 838 tests and native startup/shutdown;
a second pass after the first focus fix passed 839. The final pointer-batch pass
passed 840 tests, Clippy with warnings denied, formatting, build, `doctor` and
native startup/shutdown. Source hashes remained unchanged throughout that gate.
[Command results, logs and source hashes](../../tools/media-qualification/evidence/2026-09-23-native-editing/)
retain the exact evidence. Most verification is
headless; the live checks above supply appearance and native interaction evidence.

Full Hold insertion and pure Split need the retained sampling/envelope domains,
exact resume anchors and mark lineage described in
[splice prerequisites](../STRUCTURAL_SPLICE_DESIGN.md). Nested editing, complete
keyboard grammar, playback, full audio, generated-provider UI, export and release
qualification remain open. All DP requirements and Gates A through G remain
open or partial.
