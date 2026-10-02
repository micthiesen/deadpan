# Native combined Trim, 2026-10-01

Native `,v` and `:trim` now expose one unsaved In/Out/Slip/Roll draft with
Before/Proposed boundary pictures, an admitted Edit waveform and optional
junction audition. Apply saves the complete accepted intent once. See the
[Trim contract](../COMBINED_TRIM.md) and [waveform contract](../EDIT_WAVEFORMS.md).

This increment starts at `89f312429a4b4face407b3614bde47d0c0b12991`. Core schema
43 and database 52 are unchanged. All product requirements and Gates A through G
remain open or partial. The [retained evidence](../../tools/media-qualification/evidence/2026-10-01-native-trim/README.md)
contains commands, failures, source inventories, rendered captures and native
SQLite verification.

## Verification

These overlapping checks are reported separately; their counts must not be added.

| Scope | Result |
| --- | --- |
| Full base workspace tests | 3,421 unit/integration tests and both documentation tests passed; none ignored. |
| Full workspace/all-target Clippy | Passed with warnings denied. |
| Corrected base app suite | 503 app tests and 3 headless tests passed. |
| App with `ui-harness`, after audition correction | 539 app tests and 3 headless tests passed. |
| Final formatting and app-feature/all-target Clippy | Passed with warnings denied. |
| Final painted Trim replay | 164 checks passed. |
| Live Kestrel compatibility | 18,352 routing cases across 62 reservations passed; local source matched. |
| Full release replay after audition correction | 3,785 checks across 23 workflows passed. |
| Subsequent focused release Trim replay | 134 checks passed. |
| Native normal release app | Sustained loop, keyboard/pointer pause, Apply, Undo/Redo, reopen and shutdown verified below. |

The broad workspace gate used source inventory
`9291e1ead7429b00123743fbfc57a26891ac6f468fe066d5a4d63dd94b24fc24`.
Final formatting, lint and painted replay used
`ca88130583b3b8ca3a5de424ac6625fcef04c17a11456341c99bb5c80878bb13`.
Only `preview/trim.rs`, `preview/trim/controls.rs` and the Trim/Slip replay files
changed after that broad gate. The complete inventories and the explicit final
check/replay binding are in the evidence environment record.

The full release replay used
`7f64e4fa0e11eba76243b24871529e7d6b8cb76a3cb500ce1cf22daab4ab4743`.
The focused release replay and native runs used
`a51de6546df98588d032ba45789341d74f4b47e12bb99fee6932dca07d449f71`.
Later changes affect feedback scroll protection, its settling repaint and their
replay checks. Those later changes were verified through the final painted replay
and lint; the native binary does not claim to contain them.

The release replay explicitly skips the separately supplied real Generated Hold
fixture scenario. Retained logs include nonfatal FFmpeg decoder messages during
some passing workflows and the debug linker's compact-unwind size warning.
Neither is omitted or treated as proof of a supported media interpretation.

## Review and corrections

Independent service, picture, waveform, input and replay reviews checked captured
target absence, revision/session identity, ordered clamping, literal neighbors,
current-raster Apply and durable receipts. Picture slots publish together after
GPU submission; a hidden, clipped or obsolete pair cannot authorize Apply.
Waveform admission binds the exact snapshot and root sample interval. Optional
audio failure preserves valid structural Apply and the retained picture pair.

Early checks corrected a synthetic audio fixture missing its qualification shape,
an isolated `mac_cmd` modifier assumption and an assertion about generic empty
playback windows. Trim itself continues to reject empty audition context. Strict
lint findings were corrected without suppressions. The first rendered wait
helper also needed to measure a restored viewport before judging its raster;
the product correctly refused the obsolete raster.

Image review found feedback extending beyond the minimum window. Fixed picture,
waveform and scrollable-feedback regions now stay within their paint clips.
Native Tab reaches feedback; Up/Down, Page Up/Down and Home/End scroll it. A new
immediate Tab-to-Up witness failed before a discarded pass initialized egui's
focus filter, then passed with that correction. Picture preparation follows any
such layout retry. Final captures include default and minimum layouts, complete
handle details, paired boundary frames and visible audio failure.

The first full release run exposed an outdated Slip replay expectation. Authoring
correctly enabled both `audio_editorial_edges`, already required by core tests
and the Slip contract. The corrected oracle still compares the complete wrapper,
editor state and exact Undo restoration. Original failed reports remain retained.

Native debug audition reported `Starved`. Temporary logging showed that Space
was routed once correctly, but playback had already stopped before the next key.
The error existed in accessibility data below the visible feedback clip. The
temporary logging was removed. Separate keyboard and pointer pause assertions
now check the paused state before resume. Injected matched failures also prove
that the error is painted while the exact draft, cursors, pair and Apply survive.

New errors move to the top of feedback. Red/green witnesses cover an End key in
the failure's frame and a native wheel impulse whose smoothing continues later.
The latter exercises 18 smoothing frames and then permits deliberate scrolling.
A small-impulse witness follows requested repaint deadlines instead of forcing
idle frames: the old code requested 450 ms, while the correction guarantees a
settling wake within 200 ms. The final run reached idle through a delayed wake
and then scrolled normally. A test-only `Vec<Value>` reporting error was corrected
before that red/green run. Held-scrollbar protection was reviewed against pinned
egui source; the rendered runtime witness covers wheel input.

## Native release result

The ordinary optimized executable has SHA-256
`9bcbcdede5a68b09cabd97c4305e4a071354637b6b2ecee6c0d8bfaff247f71e`.
Native CUA input operated the existing desktop/window bounds. The disposable
fixture contains a 14-frame Source followed by a 120-frame Source in an ordinary
Sequence. Original cursor 7 and Edit cursor 3 were deliberately different.

A Slip +2 draft retained zero In/Out/Roll and Ripple. The Proposed incoming
picture showed slate 012, with the outgoing exterior explicit. Its waveform
covered Edit samples `[0,36000)`. The real-device loop remained active across a
recorded 12-second observation interval, then paused/resumed through Space and
paused through the native button. The pair and both editor cursors stayed fixed.
The paused waveform retained a nonzero heard-position marker.

The release result supports unoptimized PCM preparation as the explanation for
the debug starvation. It does not isolate the expensive stage or establish
general device throughput. No queue allowance, limiter tolerance or fault policy
was changed, and no acoustic listening result is claimed.

One Return applied the proposal. At retained Edit cursor 3, the native screenshot
showed slate 015. SQLite backup snapshots after Apply, Undo, Redo and reopen prove:

- History grew from 5 to 6 entries for one complete `apply_source_trim` command.
- Apply, Undo and Redo each used a fresh revision ID; revision counts were
  8, 9, 10 and 11.
- Undo restored exact authored data; Redo restored the exact applied authored
  data apart from its new revision ID.
- Both source mappings moved from -10 to -12. Duration 14, edit window, the
  120-frame right Source and the parent group were unchanged.
- Operational tables were unchanged. Reopening changed none of the 20 tables.

Both release instances exited zero after Cmd-Q. The final process scan found no
Deadpan executable, and the writer lock was independently acquired. Earlier
cancel-only native runs also left all 20 tables unchanged. Native screenshots
were inspected in conversation; retained PNGs come from the offscreen harness.

## Limits

The replay uses real qualified source receipts, project service, decoded frames,
Metal submission and measured waveform PCM. It injects audition delivery and
scripts pickers. It does not prove physical keyboard/layout behavior, OS IME,
VoiceOver, physical display color, acoustic quality or the full performance
corpus. This host is an M5 Max on macOS 26.5.2 with Rust 1.97.1. The short CFR
fixture does not establish large-project Trim latency or complete editor use.

Admission remains limited to direct Source or neutral unity Source Partition
children in ordinary Sequences. Broader treated/composite/occurrence editing
retains explicit refusals. The developer bundle depends on host libraries and
does not establish signing, relocatable distribution or release readiness.
