# Single-Original workflow and design review

This slice implements specification 1.1's focused creation and editing foundation.
It does not complete the full editor. The [workflow contract](../SINGLE_ORIGINAL.md)
and [design targets](../design/README.md) separate implemented behavior from the
remaining range, sound-placement, playback, AI and export work.

## Build and environment

Review base: `cc97c4dabc8ce0302d52224abcdea5827496e41e`. The change retains core
schema 11 and raises the database to 17. Hardware is an Apple M5 Max with 128 GiB,
macOS 26.5.2, Rust 1.97.1, Metal and the selected LGPL FFmpeg 8.0.3 developer prefix.
This is a development build, not a signed release or clean-machine installation.

The [repository gate evidence](../../tools/media-qualification/evidence/2026-09-23-single-original/report.json)
records all six commands, with the
[source/fixture identities](../../tools/media-qualification/evidence/2026-09-23-single-original/source-hashes.json)
and [test totals](../../tools/media-qualification/evidence/2026-09-23-single-original/summary.json).
The final run passed 879 tests with zero failures or ignored tests, formatting,
strict workspace Clippy, build, doctor and the Metal native-window smoke test.
Source and fixture hashes remained unchanged throughout. This includes the
diagnostic-label correction made after the first complete successful run.

Earlier gate attempts were explicitly interrupted for native layout fixes; they
are not counted as completed gates. One stopped during Clippy, another during
tests after formatting and Clippy had passed.

## Automated coverage and review

- Store profile tests cover full measured initialization, atomic rollback/retry,
  undo through branches to the protected baseline, reopening, second-picture
  rejection after all beats are deleted, same-original reuse, audio-only import,
  stale/canceled/closed preparation and changed original bytes. A forged generic
  reimport branch cannot acquire a valid single-Original history.
- Native host tests use injected Documents roots, exclusive numbered collisions,
  source-derived names, failed initialization and retry, switching/canceling,
  shared command history, pinned source selection and automatic sound streams.
- First-audio native/media tests compare actual PCM, timing, priming and indexes
  with explicit selection for WAV stream 0 and MP4 audio stream 1. Missing audio,
  unsupported selection, malformed inputs and opening budgets remain guarded.
- Independent review found the native Open flow could not open prior schema-16
  projects. It now invokes the store's backed-up migration before replacing the
  current session. Authentic schema-16 success/failure tests retain the original
  database and current active preparation as appropriate. Follow-up review found
  no material compatibility issue.
- Independent UI review found `j/k` could return from Original to Your edit
  without a visible destination hint. The footer and help now teach that action.

## Native interaction evidence

The first review binary SHA-256 was
`a55e0fceef0448a8984a8ebb71969103651f5fe41f53b6b0577d95a8536b4e6e`.
The real native app was controlled through CUA, with its accessibility state
checked after actions. The source was the repository's generated
`native/deadpan-source/tests/fixtures/cfr-bframes.mp4`, copied to a task scratch
path as `One original review.mp4`; SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
It has 120 video frames at 30000/1001 fps and original audio at stream 1.

| Interaction | Observed result |
| --- | --- |
| Cancel the initial picker, then use `⌘N` to choose the video | No initial package; successful selection creates `Documents/Deadpan/One original review.deadpan`, selects its full 120-frame edit and disables baseline Undo. The source was outside Documents. |
| Type `3r`, then `r` | Visible `PENDING 3r`, completion/cancel hint, then one Repeat with three total plays and 360 frames. Original remains 120 frames. |
| Tab to Inspector, Return, replace entry with `repeat 2`, Return | Native command field owns input; the Repeat becomes 240 frames and focus returns to Beats. |
| Undo twice, then another `u` | 360, then 120 frames. Further undo leaves the full Original baseline intact. |
| `dd`, close, reopen the exact package, then `u` | Empty edit keeps its pinned Original. Reopening retains zero edited frames; undo restores the 120-frame baseline. |
| `⌘Return` | A second reference to the same Original is inserted after the selected root beat. Edit becomes 240 frames; Original remains 120. |
| `?`, Escape | Readable keycap reference opens and closes, with actual commands and explicit current limits. |
| `:source`, `12l`, `rr` | Original boundary 12 and displayed source frame 13. Repeat is rejected with guidance to Your edit/reuse; edit stays 240 frames. |
| `⌘I`, choose `pcm-stereo-48000.wav` | A separate audio catalog entry appears; Original boundary 12, displayed frame 13 and edit duration 240 remain unchanged. No picture or blank time is inserted. |
| `/`, Option-E, E, `?` | Search contains `é?`; no help or structural edit dispatches. Clearing and Escape restore pane focus. |
| Open the prior native schema-16 review project | Native Open migrates to schema 17, reports the preserved backup, and retains its generic Source/Sequence workspace, Repeat, Hold, source references and history. |
| Select its Hold, Tab to Inspector, Return, submit `hold-duration 11f`, then `u` | The 45-frame Hold becomes 11 frames; sequence duration changes from 285 to 251. Undo restores 45 and 285. Its Background picture and Silence policy remain explicit. |

An initial Open-panel attempt selected the library directory instead of its
package and reported the `.deadpan` extension error. Selecting the exact package
through Go to Folder succeeded. No stored project was changed by the invalid Open.
The review app was quit and process absence checked before replacing its binary.

## Aesthetic findings and final comparison

The first native capture measured 2984 × 1858 pixels. Its charcoal surfaces,
restrained lavender selection, large fitted picture, separate sounds, conditional
inspector and persistent keycaps follow the generated workspace. The live review
also caught three defects absent from the initial headless checks: the viewer
covered the beat-strip header/card tops, the Original caption crowded its border,
and Saved/Add sound symbols rendered as missing glyphs. Sound completion also
incorrectly suggested insertion was available. The corrected native build
`ad663af559771b8dc1658038c2641b822476f6df31ba760a4041035674ef67e0`
showed separate header, complete card tops, a clear Original caption, readable
Saved/Add sound labels and honest catalog-only sound status.

Dynamic insertion revealed another defect: a fitting card could become a narrow
sliver. The first headless reproduction caught an excessive reveal offset on
append; clamping it passed that regression but did not fix native insertion into
the middle. Build
`9f60f53c2e0267686bf9eccb2e8e019184064af748186c46401369ae2435c1ec`
still showed a narrow third card after inserting a new second card. Selecting
the fourth did not repair it; selecting the third did. This required another
renderer investigation, not a passed visual check. The same build verified the
replacement Hold `H` glyph; the Repeat arrow rendered correctly already.

The final native review binary SHA-256 is
`c5ea24dcd89656ae3928279ac768e8df2da8ca1abd9bcf5f74e761d5c4ec35af`.
Cards now paint and interact with their explicitly reserved rectangles, using
stable node identities. The native replay opened four beats, undid to three,
then inserted a new second beat with `⌘Return`. The immediate capture showed
all four complete cards and text, with the new second selected at boundary 120.
Clicking the fourth moved to boundary 360; `k` selected the third at 240 while
all four remained complete. Captures remained 2984 × 1858 pixels. The app was
quit and its process absence verified afterward.
The subsequent source change only updates `doctor` capability labels for
schema-16 migration and the single-Original/Documents foundation. The final
repository gate covers that change; the native review above covers the GUI.

Headless coverage includes actual egui panel/card/text/clip geometry at
960 × 640 and 1492 × 929 points, 1/2/4/9 beats and scrolling, plus dynamic
1→2→3→4 append and stable-identity undo/middle-insert cases. The append test
failed before the reveal clamp. The middle-insert case, including 2× pixel scale,
pointer state and tessellated fill coverage, did not reproduce the residual
native sliver before the explicit-rectangle change; the native replay supplies
that evidence. A separate test verifies focus and Enter follow a moved node.
All 28 focused preview tests passed. Independent review found no actionable
focus, accessibility, clipping or virtualization issue in the final renderer.

The final composition matches the generated workspace's hierarchy and palette:
pinned Original and sounds, dominant picture, conditional inspector, complete
structural cards, distinct cursor/selection/focus, duration comparison and visible
keys. It deliberately omits conceptual controls whose implementations remain open.

Native window zoom and two corner-drag attempts did not change the captured
dimensions. Minimum-size layout therefore needs deterministic geometry evidence;
this report does not claim a native minimum-size resize passed. VoiceOver, CJK
composition, physical non-US layouts, display color qualification and full
keyboard-only product acceptance remain open.

The generated boards use illustrative interview footage and future workflow
controls. The running app uses actual decoded test footage and truthful type
tiles, with no pretend thumbnails, playback, range paste, sound overlay, model
installation or export. The original 1.0 spec and earlier design explorations
remain unchanged and explicitly superseded. All eight board/prompt identities
and all five archived specification files were hash-checked; changed local
Markdown links resolve.
