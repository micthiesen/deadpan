# Native workspace layout qualification, 2026-09-27

Production Metal captures now show a larger picture, visible common inspector
actions, readable first-frame errors and pinned sound audition controls. This
pass compares the coded interface with the saved single-Original workspace,
moment-reuse and sound-audition boards. DP-05, DP-20, DP-24 and all delivery gates
remain partial or open.

The [evidence index](../../tools/ui-feedback/evidence/2026-09-27-layout/README.md)
links compressed original reports, a readable summary, commands, logs, source
identities and selected inspected images. Earlier failed runs remain failures.

## Changes and visual comparison

- Frame navigation sits beside Original / Your edit. Compact controls and beat
  margins return vertical space to the picture. At 1280×820, the viewer grows
  from approximately 242 to 330 points of height, about 36%. Compare the
  [baseline](../../tools/ui-feedback/evidence/2026-09-27-layout/baseline-workspace.png)
  with the [revised workspace](../../tools/ui-feedback/evidence/2026-09-27-layout/workspace.png).
- Camera and Insert pause precede descriptive inspector fields. Duration and
  speed remain directly accessible; additional framing presets have a labeled
  disclosure. Whole-Original reuse remains in the left column.
- Errors use a premeasured bottom panel, including their first paint and wrapped
  text after resizing. The empty panel retains widget identity so closing a
  command or clearing a notice does not swallow the next pointer press. Command
  entry reserves its field and hint area immediately.
- Sound audition controls remain below the scrolling catalog. Their labels,
  clock, wrapped buttons and panel use the same measured text geometry. Selecting
  a sound reveals its catalog row; legacy projects reveal the selected sound
  rather than the previously selected video.
- Shift+Space is written out, and speed previews use “to” between durations.
  The former arrow glyphs rendered as missing characters in actual captures.
  Bindings and Kestrel reservations are unchanged.

Full-size inspection included the default and 960×640 Sequence and Original
views, Camera, moment selection, speed entry, wrapped error and paused sound
at both sizes. The enlarged workspace target's picture hierarchy, quiet dark
panels, lavender selection, distinct pane focus and visible keys are better
reflected in the implementation. Minimum-size Original still has a smaller
picture because its range controls remain visible. The narrow sound catalog
scrolls above its transport; this pass does not claim every catalog row is
simultaneously visible.

The boards remain design targets. Thumbnails, transcript, waveform, attachment
editing, full occurrence navigation, model workflows and export remain required.
No placeholder control or fixture establishes those capabilities.

## Replay and review

| Run | Result | Interpretation |
| --- | --- | --- |
| `visual-01` | Fail; 11/14 UI scenarios passed, 462 checks | Restored-access baseline exposed incorrect plain-label queries in moment/speed checks, an early Resume-label check, and actual clipping of the speed hint. Camera scrolling and first-frame error clipping were retained warnings. |
| `visual-02` | Fail; 8/14 passed, 410 checks | Development layout exposed navigation clipping and changed automatic widget identities when a notice panel appeared/disappeared. The moment scenario also tried clicking new-context controls before their next paint. |
| `visual-03` | Fail; 13/14 passed, 494 checks | All scenarios except sound passed. Sound reached a real clipped Loop button and failed with `No active sound playback request`. Rapid input retained seven explicit busy rejections. |
| `sound-playback-04` | Pass; 32 checks | Fixed sound panel passes its complete scenario, including actual first-resize text visibility, paused sample/context retention, keyboard/pointer loops, stale/fault handling and text ownership. |
| `workspace-04` | Pass; 69 checks | Actual picture/navigation bounds and minimum-size Original coverage pass. |
| `workspace-05` | Pass; 69 checks | Strengthened oracle additionally requires the painted mesh to fill its expected fitted canvas. |
| `retime-05` | Pass; 16 checks | Speed entry, exact hint, cancellation, updates, nesting and undo pass with readable duration text. |

Counts include the separate shortcut-audit assertion in every run. All audits
passed 3,472 production-router cases against 62 global reservations and the live
Kestrel source. Its SHA-256 was
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.

This is composite coverage of all fourteen scenarios, not a claim that one final
binary reran the entire suite. `visual-03` covers the unchanged scenarios;
focused follow-ups cover the affected sound, workspace and speed paths.
Plain text checks now inspect actual paint inside clip and viewport. Pointer
Pause must change state on release and paint Resume on exactly the next frame.
Context commands likewise get one explicit paint before clicking the new
context's controls. These checks do not wait through several failed frames to
claim eventual visibility.

General independent review found no issue in the first layout revision. Focused
review requested actual Resume text visibility and picture geometry checks;
both were added. Follow-up review identified the legacy sound reveal issue,
which was fixed, and requested the stronger fitted-picture assertion. No review
finding remains unapplied.

## Build and checks

The host is an Apple M5 Max running macOS 26.5.2 with Rust 1.97.1 and the pinned
FFmpeg prefix at `/tmp/deadpan-ui-ffmpeg/prefix`. The 120-frame
`cfr-bframes.mp4` fixture remains SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
Runs used actual Metal under restored full access. No adapter failure or
software rendering substitute occurred in this qualification.

The base commit is `872e50a045e16e53db616d5d2023c40156abf2a0`.
Each report retains its executable digest, dirty checkout metadata and lockfile
digest. The focused sound/workspace-04 executable is
`3a02ebd7390f3a5247787eccc681e71d0e1c66f56cb62ed1fe3a463bf81e0a34`;
final workspace/retime-05 use
`7f91e4b9f17b189e094f8d4ea683002cf923b427211df6c669574eb718cf69dd`.
The retained app patch and source-file hashes identify the final code separately
from later documentation/evidence changes. Intermediate binaries and source
bodies are not archived; their reports retain identities and outcomes.

- Default app tests: 206 unit and 2 integration tests passed.
- `ui-harness` app tests: 234 unit and 2 integration tests passed.
- Default and feature-enabled app Clippy with `--all-targets -- -D warnings`
  passed. The first default invocation found `clippy::manual_map` in notice
  construction; it was fixed without suppression. Both final invocations pass.
- Formatting passed. Final focused replay compiles the last text/oracle changes;
  tests preceded those changes. No full workspace gate was repeated for this
  app-only increment.

Cargo invocations ran serially under one parent-owned process, with terminal
exit results recorded before another command started. No timed-out observer
caused a duplicate test or replay process.

## Release responsiveness

Both focused release runs passed with executable SHA-256
`29831a0f12e020c4d462c838e17f7fd776ca0f8d31c91f8f8cc0c70fbea5a63e`.
They used the existing repaint callback wait and worker timing instrumentation,
with no screenshot readback. `rapid-input` passed 268 checks including its
shortcut audit; `edit-latency` passed 449. No performance threshold changed.

| Measured interval | Samples | p50 ms | p95 ms | p99 ms | Maximum ms | Gate |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Warm navigation input-frame CPU | 120 | 0.53 | 0.66 | 0.73 | 0.88 | Pass, p95 < 8 ms |
| Warm navigation input to offscreen picture completion | 120 | 3.55 | 4.67 | 4.96 | 5.06 | Pass, p95 < 80 ms |
| Cached Repeat input to observed commit | 40 | 1.57 | 3.08 | 3.52 | 3.52 | Diagnostic |
| Cached Repeat input to offscreen picture completion | 40 | 5.04 | 6.85 | 7.32 | 7.32 | Pass, p95 < 50 ms |
| Hold input to observed commit | 40 | 3.54 | 4.95 | 6.69 | 6.69 | Diagnostic |
| Hold input to offscreen picture completion | 40 | 7.40 | 9.11 | 18.87 | 18.87 | Pass, p95 < 100 ms |

All samples in these subsets completed without failures or timeouts. Navigation
excludes sixteen warm-up inputs. Each edit type excludes four warm-up cycles and
then measures forty edits, each followed by undo to the exact authored Original
baseline under a fresh revision. The 10,000-beat performance scenario was not
rerun in this increment; its full visual scenario passed in `visual-03`.

The earlier 75.26/138.65 ms Repeat/Hold misses remain in their original report.
This is the first successful host measurement after the
[repaint-wait correction](repaint-wake-2026-09-26.md). It establishes the current
small-fixture budgets, not a controlled estimate of how much speed came from
that correction versus host access or other changes. Power, thermals, OS cache
and external system load were uncontrolled. The release build finished before
measurement; no other test or replay ran concurrently. These intervals end at
offscreen GPU completion, not physical scanout or heard audio.

## Limits

Playback delivery in these scenarios is injected. Imports, SQLite commands,
source decoding, picture composition, event routing and Metal paint are real.
This does not establish acoustic quality, native IME delivery, VoiceOver,
physical key delivery, physical display latency or release packaging. Visual
readback timings are not performance measurements. Rapid edit rejection while
the project writer is busy remains interaction work.
