# Repeat operator qualification, 2026-10-03

Status: this increment passed its implementation checks. It does not close a
product gate.

## Change

[Repeat selections](../REPEAT_SELECTION.md) adds ordinary Sequence range and
whole-child wrapping, a safe count setter, typed keyboard selectors, recording
and dot. It retains composite endpoint contexts and exact old audio clocks,
moves independent root sounds once and saves each authored operation atomically.

The keyboard supports `rr`, a leading total-play count, `r` plus a motion,
motion-distance counts and Visual `r`. Command entry captures its target.
Delayed whole-beat continuation is restricted to an exact successful wrap;
motion terminals retain the original capture. Repeat does not consume register
intent. Counts cannot cross custom operator aliases; distinct operator families
with overlapping ancestor prefixes are rejected.

## Environment and identity

Base: `738f2dd337ba38e1548acc77eb86eacaa15f43f7`.
Apple M5 Max, 128 GiB memory, macOS 26.5.2 (25F84), Rust/Cargo 1.97.1.
Checks used locked dependencies and the selected FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix`. The native app and initial five rendered replays
used one debug UI-harness binary. A final help-text correction changed only
`preview.rs` and the remapped-key replay:

- Final runtime source inventory:
  `b81fce1c2111c873ebb7f4a3bacbaeb6a8a7a1935d037f27c901446e782abcf6`.
- Final binary SHA-256:
  `989e1aa5786eb8d4be35c55de3d5ac0de0c1eb32cb2f4ac773e7170a3ead77c2`.
- Native and initial replay source inventory:
  `32bdffade83d9ed2d1cfbc7e2d3c6ee487e6205f9c29fe6b020562b84a88e805`.
- Native and initial replay binary SHA-256:
  `d091476cb397a52d831d2bf8fb17b1e4b2c87fa9199100bb6d9a51f81ee11fef`.
- Fixture `cfr-bframes.mp4` SHA-256:
  `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.

The [evidence directory](../../tools/media-qualification/evidence/2026-10-03-repeat-operator)
retains commands, source inventories, successful and failed logs, compressed
replay reports, native accessibility observations and selected screenshots.

## Automated checks

- Core, picture-plan and audio suites: **1,732 passed**, no test failures.
  The run's global source guard returned 1 because eight independent app router
  and harness files changed. The tested packages and their dependencies did not
  change; the retained manifest comparison records those exact eight paths.
- Final formatting and UI-harness build passed on the final source inventory.
- App (UI harness feature), store and CLI rerun: **1,560 passed**, including
  729 app tests, four app headless tests, staged qualification, dry-run refusal,
  Compound history and reopen. Source stayed at
  `32bdffade83d9ed2d1cfbc7e2d3c6ee487e6205f9c29fe6b020562b84a88e805`.
- After the help correction, all **729 app tests and four headless tests**
  passed again on the final source inventory. Repeated tests are not added to
  the unique test count.
- Strict Clippy passed for all six affected packages with all targets, and again
  for the app with `ui-harness`, with `-D warnings` and unchanged source. After
  the help correction, app Clippy passed again with default and UI-harness
  features. Other package sources stayed unchanged.
- Independent static reviews covered core identity/timing/allowance transforms,
  native captured targets and receipt ownership, and keyboard remapping.

Together the two successful package runs contain **3,292 passing tests**. The
initial core/plan/audio source is identical to the final inventory throughout
those packages and their dependencies; only independent app files changed.

The new actual-PCM checks use the verified 44.1 kHz fixture through `StageAudio`.
They compare every retained sample at 30000/1001 fps, including added outer plays,
and use cold reverse chunk reads. A partial Preserve range retains complete DSP
history on every play. Count reduction keeps surviving plays and the suffix
exact. These are decoded PCM checks, not acoustic or full mastered-output tests.

The new picture-plan check compares every output point through a partial framed
Retime, including all added plays, and checks each live framing owner once. It
does not independently qualify decoding or GPU composition.

## Rendered keyboard workflows

All five production event replays passed in visual mode with an initial
1280×820-point viewport. The keymap replay also uses 960×640 and was repeated on
the final help-corrected build. Counts below use that latest keymap run:

| Scenario | Passing workflow checks |
| --- | ---: |
| `repeat-operator` | 224 |
| `rapid-input` | 60 |
| `macros` | 372 |
| `dot-repeat` | 381 |
| `keymap` | 57 |
| Total | 1,094 |

The new Repeat replay covers leading versus motion counts, empty/missing Visual
refusals, fresh dot targets, register preservation, captured command absence and
staleness, delayed receipt handling, recording and counted macro replay
with one Undo. It compares saved SQLite documents as well as visible state.
Rapid-input retains the existing queued whole-beat behavior. Selected intermediate
images reached the deliberate screenshot cap; semantic checks and named image
checkpoints continued. Reports retain their explicit skipped checks.
The final keymap replay renders configured Repeat operator `b` and frame motion
`ah`. Keyboard scrolling reveals the complete `3bah` versus `b3ah` explanation
at the minimum window size; its text fits the actual paint clip.

Each replay also passed the same **3,319,728-case** production shortcut audit
against **62** Kestrel bindings. Live and retained source SHA-256 both equal
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
No conflicts or source drift were found. These repeated audits are not added to
the workflow count. Visual replays ran alongside lint work and establish no
release response-time or thermal-performance claim.

## Native verification

A developer wrapper launched a private copy of the closed replay package.
Actual native key delivery and accessibility/screenshot observations verified:

1. At boundary 20, select register `a` and start `r`. The exact pending path and
   valid next-key guidance remain readable at the bottom of the window.
2. Complete `r3l`. A six-frame Repeat spans 20–26 with two total plays. Project
   duration grows from 120 to 123; register `a` remains selected and empty.
3. One `u` restores 120 frames. Move to boundary 40 and press `.`. The same
   three-frame selector now creates the Repeat at 40–46.
4. Undo, then select 50–54 with `v4l` and press `3r`. A twelve-frame Repeat spans
   50–62 with three total plays; total project duration is 128 frames.
5. Undo restores 120 frames. Dot without a new Visual selection explicitly
   refuses and preserves both the document and selected register.
6. Cmd-Q closes the app with exit 0. A targeted process check confirms absence.

The native screenshots show the inspector, beat cards, cursor, separate Original
and Edit durations, register state and dot guidance together. Lower inspector
rows remain in its scroll area; the footer stays visible. This does not qualify
VoiceOver, OS IME, all physical keyboard layouts, audio output or release packaging.
The developer wrapper retains external host-library dependencies.

## Findings retained

Review found that overlapping custom operator prefixes could carry a motion
count across another prefix and append later digits. The fix rejects overlap
between distinct families and tracks the count's actual prefix depth for
same-family aliases. A poisoned count stays refused through the terminal key.
The regression expectations distinguish a completed alias's pending state from
the optional `OfferInsert` hint action.

An actor test initially expected an empty child to wrap. The existing Repeat
model requires a positive child duration. The corrected test checks atomic
refusal without selecting another child at the cursor.

The first app run passed 728 tests and failed one old pending-help assertion
that still required the phrase "other edit kinds are not supported yet". The
assertion now requires the actual supported "cut or Repeat" wording. The failed
run is retained separately from the rerun.

Final review found hard-coded `r`/`l` examples in the new Repeat help. Examples
now use the configured preferred bindings, while the row lists available aliases.
The macro reference also lists Repeat wrapping. The final rendered remap check
would reject the original hard-coded examples. Native keyboard behavior was
tested before these help-only changes; app tests, lint, formatting, build and
the remap replay were repeated afterward.

The `dot-repeat` log contains one FFmpeg `decode_slice_header error`. All 381
checks passed, including 169 settled-picture identity checks. The scenario does
not intentionally corrupt media. Rapid navigation can cancel preview work, but
the log does not establish cancellation as the cause. The diagnostic remains
unexplained, consistent with the unresolved observation in the
[earlier native Move record](native-move-2026-09-30.md). It is not classified as
harmless. Native Repeat QA emitted no corresponding diagnostic.

The debug build retains the existing linker warning that `__eh_frame` exceeds
the compact-unwind offset limit. Successful build and tests do not qualify
release exception-handling performance.

## Remaining scope

Count setter recording and dot, temporal-occurrence navigation, text/role
selectors and remaining editing operations are not implemented by this increment.
All DP requirements and
Gates A through G retain their existing open or partial status.
