# Structural Split qualification

This increment adds [pure Split](../STRUCTURAL_SPLIT.md) and its native keyboard
entry. It does not complete arbitrary cursor Hold insertion, shifted-fragment
sample resume or the full editor. Every DP requirement and delivery gate remains
partial or open.

## Build and repository gate

Review base: `789ee073e6918b6670c61f29cec78f2804332e2f`. Core schema 14/database
20 add Split's command grammar and a frozen core-13 replay adapter. Hardware:
Apple M5 Max, macOS 26.5.2, Rust 1.97.1, Metal and the selected LGPL FFmpeg 8.0.3
developer prefix. This is an unsigned development build.

The final six-command repository gate passed: formatting, strict workspace
Clippy, **968 tests with none failed or ignored**, workspace build, headless
diagnostics and native Metal window smoke test. Source and fixture hashes
remained unchanged throughout the gate. The [evidence directory](../../tools/media-qualification/evidence/2026-09-23-structural-split/)
retains command timings, logs, compressed test output and the source manifest.
Documentation verification confirmed all eight design images and prompts, five
unchanged original specification files and changed Markdown links.

## Automated behavior and independent review

- Core tests cover every primitive, root identity and original edge ownership,
  non-Sequence parents, compact billion-play repeats, sparse override copies,
  occurrence isolation, exact/fractional seam bias, all mark coordinate spaces,
  hidden bindings, independent owner/host mapping and atomic growth failures.
  270 sequential refinements retain a flat sibling structure. Every fixture
  checks zero duration delta, inverse restoration and serialization.
- Actual command-driven audio tests compare decoded fixture PCM before and after
  Split and refinement. They cover 44.1 kHz input at 30000/1001 project fps, signed
  placement, source offset, two-sample edge envelopes, mixed Preserve/FollowSpeed
  stages and room tone. Suffix-first and irregular reads equal the original;
  `StageAudio` and `SequenceAudio` agree where both apply.
- VFR picture tests preserve exact source coordinates and indexed frame selection
  through fractional Retime cuts and refinement. Native service tests preserve
  the measured pinned Original, 120-frame duration and picture plan through cuts
  at 37 and 66, stale/invalid requests, reopen, undo to baseline and redo.
- A headless CLI test compares dry-run and committed Split patches, unchanged
  preview storage, stale rejection and durable history.
- Migration uses an authentic DB19 fixture produced by `ProjectStore` from an
  isolated copy of the review base. It contains multi-binding marks, abandoned
  history, partial loss, undo and pending redo. Tests compare every old document,
  request, forward/inverse patch and retained history field. All DB1–19 reject
  new Split ingress and preserve the original plus backup on failed promotion.
- General and focused independent reviews found no remaining structural, mark,
  generation-relevance or migration defect. A separate scratch matrix checked
  54,372 owner/host/anchor/bias/cut combinations and preserved exact resolved
  position sets. The documentation's command tag was corrected from `type` to
  `command`. A proposed duplicate-event finding was refuted: equal physical
  ownership bindings resolve once, preserve all matching ordinals and survive
  partial owner deletion. Dedicated regressions retain this distinction.

Development checks initially caught test-fixture mistakes: a fixed revision
allowlist rejected actual new Split revisions, one test used the wrong mark loss
policy, and a test reused an identity after a no-growth edit. These were corrected
without weakening production checks. An initial app check omitted the required
FFmpeg environment variable; the qualified-prefix rerun passed.

Legacy fieldless occurrence variants also accepted unknown payload fields despite
`deny_unknown_fields`. Frozen core4–13 Delete/Ungroup/RevertGeneratedHold now use
empty struct variants, preserving valid JSON while rejecting the extra fields.
The new frozen core13 Mark wire closes the same hole for Bound state. Current
`MarkState` now also deserializes through a closed wire, preserving its public
enum and canonical JSON. A regression failed against the old schema-3 parser
before the fix; coverage includes all mark-bearing legacy documents/patches
and both current primary and fragment states.

## Native keyboard and aesthetic review

The initial review binary SHA-256 was
`dd48faa2cf3ad3d49c80372d6b80a391fcaaf37df8054459cc7c2c74df9a6866`.
The native source picker opened the generated `cfr-bframes.mp4` fixture, renamed
`Split review.mp4`, with SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
It initialized the complete video in system Documents/Deadpan.

Observed through the real native window and accessibility tree:

- `37l`, then `s`, produced 37/83-frame fragments, selected the right side and
  kept the cursor at 37, total at 120 and displayed picture at source ordinal 37.
- Another `29l`, then `:split`, produced 37/29/54-frame fragments at 0/37/66.
  Undo and Ctrl-R restored the corresponding two/three-beat structures.
- Tab cycling reached Inspector. `l`, then `s`, cut its selected fragment at
  boundary 67 and returned focus to Beats. Undo restored the 54-frame fragment.
- Sound search accepted `sé?` using Option-E composition. Neither `s` nor `?`
  dispatched an editor action while text owned focus.
- In Original view, `s` preserved the edit and displayed guidance to Your edit.

The retained oversized window initially produced a partial capture. Native
fullscreen exposed the complete 3024 × 1898-pixel layout. Comparison with the
[generated target](../design/boards/single-source-workspace-v1.png) shows the
same pinned Original/sounds hierarchy, dominant picture, lavender selection,
gold cursor, conditional inspector and visible editing keycaps. Actual fragments
are truthful type cards; conceptual thumbnails and unimplemented transport remain
outside this review.

Live review found a missing-glyph box for the initial Fragment symbol, inactive
Page Down scrolling in the Keys window, and a fast Escape→colon transition that
could drop command entry and expose following letters to Normal mode. The
Fragment now uses `F`; scrolling has measured geometry and ordered input
ownership. A second font check changed unavailable arrow symbols to `Up/Down`.
Headless regressions cover help opening, closing and reopening within a batch,
pointer/IME prefixes, composition lifecycle and real egui wheel-to-key scrolling.

Build `ba0ed61c98279cf9503ed169b7ecb57d9527c618035f247ca1d8247f86d30fd4`
passed native fast Escape→`:source`→`s` without editing, and `?`/Escape/`?`/`dd`
left all three fragments intact. Page Down reached the final reference sections;
Home then `j` moved one line, and `k` returned upward. The fixed hint remained
visible and legible. An earlier replay overlapped the separate smoke-test app:
later interactions showed no changes and computer use reported
`noWindowsAvailable`. A process sample showed an idle AppKit event loop, not a
blocked application thread. Closing and relaunching alone restored the above
interaction evidence; the failed attempts are not counted as passes.

This replay additionally found that opening File over help and pressing Escape
closed help while leaving the menu open. A second Escape still left the menu
open. In pinned egui 0.36.2, `Context::any_popup_open` observes popup registrations
from the current pass, but Deadpan routes keys before drawing File. The early
guard now uses persistent popup memory through `Popup::is_any_open`. A real-menu
regression failed against the old guard at pre-widget menu ownership, then
passed with the fix. Independent review and final native replay passed.

Final native binary SHA-256:
`7513b8097c6fa4819c31d190b7a520c9682342ac7fb9d0b8595ec5ff4373645e`.
At interior boundary 1 with three fragments, File over Keys consumed `s` and
`dd`; Escape dismissed File and kept Keys. The next Escape→`:source`→`s`
returned to Original without editing. Returning to Your edit and fullscreen
showed the complete layout and correct `F` glyph; `l` advanced to boundary 2
and Tab focused Inspector. The app then quit successfully. First-opening
pointer-down plus Escape in one input batch was not tested; the menu regression
covers an already open popup.

VoiceOver, CJK composition, physical non-US layouts, minimum-size native resize,
audio listening, playback, effects, generation and export are not qualified by
these checks. The unchanged-output audio evidence concerns headless PCM, not an
application device path. Fullscreen capture does not establish display color.
