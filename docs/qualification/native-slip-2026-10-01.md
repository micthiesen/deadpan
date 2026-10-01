# Native Source Slip, 2026-10-01

The native `:slip +5f` workflow passed service, input, presentation, rendered
replay and native keyboard/save/reopen verification. It compares real composed
Before/Proposed pictures while preserving both editor cursors and beat duration.
Apply requires the exact current Proposed picture to be submitted at the current
viewer size. See the [contract](../SOURCE_SLIP.md#native-stopped-picture-preview)
and [retained evidence](../../tools/media-qualification/evidence/2026-10-01-native-slip/README.md).

The native increment is based on backend commit
`8c282ac8a3147e784b212852540b5e44e28382b1`. Core schema 39 and database 48 are
unchanged. The [backend qualification](source-slip-2026-10-01.md) remains separate.
No full requirement or delivery gate is complete.

## Review and corrections

Independent service, UI and harness reviews found no outstanding actionable
findings. The retained UI review led to two lifecycle corrections before testing:

- A failed proposed decode retains the submitted texture, caption, canvas and
  geometry, exposes its error and disables Apply.
- Entry, draft changes, cancellation, save and context invalidation immediately
  cancel pending picture admission, including decoded pictures awaiting GPU
  submission. Service publications reconcile the draft before decoder replies.

Pure regressions cover stale successes/errors before replacement dispatch and
an already decoded but unsubmitted proposal. The exact external head-change
publication ordering with a withheld decoder reply was reviewed statically;
that combined sequence was not exercised by the real replay.

The first visual replay passed its assertions but inspection found missing-glyph
boxes in two arrow labels. The final build spells those keys Left and Right.
Those two strings are the only source differences after the unit/default runs;
retained hashes and a reconstructed source comparison prove this distinction.

## Results

| Check | Result |
| --- | --- |
| Focused Slip tests with `ui-harness` | 17 passed. |
| All app tests with `ui-harness` | 464 app tests and 3 headless integration tests passed. |
| All default-feature app tests | 428 app tests and 3 headless integration tests passed. |
| Workspace/all-target Clippy with `ui-harness`, warnings denied | Passed. |
| Final formatting | Passed. |
| Final focused rendered Slip replay | 67 Slip checks passed. |
| Production Kestrel routing and live registry comparison | 17,360 cases over 62 reservations passed; no conflicts or drift. |
| Final release build | Passed in 244.33 seconds. |
| Full release performance replay | 3,655 checks passed in 20.59 seconds. |
| Native cancel, apply, Undo/Redo, quit and reopen | Passed; both native processes exited 0. |

No tests failed or were ignored in the app suites. Overlapping focused, default
and feature runs are reported separately. Unit/default source manifest:
`9d93fbf93dc03951747ea1b2c300f06af54f5cfb470f60bca987259f78bf78b4`.
Final build/lint/replay manifest:
`44ed42b615d9adebf9da17a5bc7419d688ec2df0c9fe9ca73ed9917ef7ddff36`.
All backend crates and build configuration match the preceding checkpoint,
which passed 3,114 workspace tests and two compile-fail documentation tests.

The full release report contains 22 executed app scenarios and Kestrel, with no
findings, failed checks, failed timing checks or timeouts. Generated-picture
qualification is explicitly skipped because that scenario needs its own real
fixture. Measured picture p95 values were 1.52 ms for warm navigation, 5.59 ms
for cached Repeat and 5.61 ms for Hold. These are replay measurements, not physical
display latency. The debug link retains an existing `__eh_frame` warning. The
passing full replay log also retains an AAC `get_buffer() failed` diagnostic
during Gain; its cause was not established by these checks.

## Picture and interaction evidence

The real `cfr-bframes.mp4` fixture supplies Original ordinals `[10,24)`. At Edit
frame 3, the independent oracle checks ordinal 13 before Slip, 18 after `+5f`,
3 for `-100f` clamped to `-10f`, 4 after one reverse nudge, and 15 after the
complete `l/l/h/Shift+l` batch. It checks measured PTS, decoded/displayed ordinals
and typed presentation/geometry identities through the actual store, decoder
and Metal route. Nested ordinary Sequence groups and a single neutral Partition
retain Source and live ancestor camera paths.

The replay withholds an actual proposal decode before Apply, releases late
service and decoder replies after cancellation, and checks text/IME/button
ownership, zero movement, endpoint inspection, entry refusals, one commit and
Undo/Redo. Native controls were inspected at 1280×820 points. The offscreen
960×640 compact capture has one layout pass and a ready submitted picture at
that raster. Proposed, Before, compact clamp, saved result and nested Partition
captures are readable; their named checkpoints each used one layout pass.
The focused visual report's only finding records the bounded intermediate
screenshot allowance; its named final captures are retained.

The live Kestrel source was
`/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift`, SHA-256
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
No new modified global shortcut was added.

## Native verification and cleanup

An isolated developer bundle ran the exact release executable, SHA-256
`c1d93f04b775841e092ead66b6a520918f52de5f9fb1e1ce66d9cae87f399eb6`.
Native input selected the first five-frame Partition in the retained group,
with Original cursor 7 and Edit cursor 3. `:slip +2f` showed source slate 015;
Before showed 013. The amount field retained Return without saving, clamped
`-100f` to `-10f`, and one reverse nudge moved to `-9f`. Temporary picture
inspection left both editor cursors fixed. Escape restored the group, fragment
and Beats focus. All 20 database tables matched the pre-preview backup.

A fresh preview followed by native Tab navigation to Apply and Return saved
one transaction. Only the selected physical Source's linked video/audio starts
changed, from -10 to -12, and its old audio cache identity was invalidated.
Durations, framing, sample bindings, sibling mapping and root sounds remained
exact. All 16 unrelated tables remained unchanged. Revision/history counts
were 13/8 before, 14/9 after Apply, 15/9 after Undo and 16/9 after Redo. Undo
restored the original document; Redo restored the candidate, each with a fresh
revision identity. Reopening retained all 20 tables and showed slate 015 again.

Both native instances were quit and exited 0. The final native inventory and
process scan found no Deadpan instance, and the writer lock was released. The
assigned desktop settings were unchanged. Content was 1280×820 points at 2×
scale. Accessibility names and keyboard focus were visible in the native tree;
VoiceOver was not exercised.

## Remaining scope

This stopped-picture preview has no waveform or audio audition. Full
[Trim mode](../spec/DEADPAN_SPEC.md#77-trim-mode), In/Out, adjacent Roll,
ripple/overwrite, physical Source growth, `,v` entry and Tab mode switching remain
required. Nested or treated Partitions, Repeat occurrences, FitBeat/incoherent
windows and audio-only picture lead/tail are unsupported. Nested ordinary
Sequence groups are supported.

Synthetic IME checks do not establish OS IME delivery or physical non-US layouts.
The developer bundle retains external host dependencies and is not a signed,
relocatable release. Complete playback, export, accessibility, performance corpus
and packaging acceptance remain open.
