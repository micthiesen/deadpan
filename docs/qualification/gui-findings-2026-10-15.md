# GUI review findings fixed, 2026-10-15

This record closes the nine layout findings left open by the
[GUI review](gui-review-2026-10-15.md). Evidence is offscreen Metal replay
through the production router, widgets, project service and shared GPU
picture path, compared with the [design targets](../design/README.md). Each
fix has a replay assertion that fails on the previous application code and
passes on the new code. Native OS input, VoiceOver speech and physical display
behavior were not exercised. DP-20 remains partial.

Captures are under
[`tools/ui-feedback/evidence/2026-10-15-gui-findings`](../../tools/ui-feedback/evidence/2026-10-15-gui-findings/captures/),
each as a `-before.png` and `-after.png` pair from the same replay step.

## Fixes

| Finding | Change | Replay assertion | Before | After |
| --- | --- | --- | --- | --- |
| Compact cards had no thumbnails | A thumbnail narrows to leave the text its 112-point minimum (70 × 52 at 960 points) instead of disappearing; ordinary cards keep their widths. | `workspace`: every fully visible card paints a textured thumbnail at all three sizes. | 0 of 2 compact cards | 2 of 2 |
| A partly scrolled first card was cut mid-text | The strip's cursor band counts wholly hidden beats at each edge (`‹ 4`, `2 ›`, stepping aside from the cursor badge, with an accessible label) and a cut card fades to the canvas. | `workspace`: counts match the cards outside the strip clip and every cut card has a fade at the clip edge, at three sizes. | no count, no fade | counted and faded |
| A pending thumbnail looked like an authored Background | Loading paints `Loading…` on a raised panel tone; a remembered decode failure paints `No preview`; only a Background is black. | `workspace`: the new card's first frame after `,i` shows `Loading…` on the placeholder fill; a unit test separates all three states. | black | `Loading…` |
| Camera Apply and Cancel ran under the scroll bar | Side-pane scrollers (inspector, Camera, moment, scoped, sound event, Original rail) put the floating bar in the pane's 12-point right margin; row widths are unchanged. | `camera`: Apply and Cancel end before the visible scroll bar's hovered extent. | rows end at 1268, bar 1258–1268 | bar 1270–1280 |
| Trim at minimum left a band above its waveform | The empty junction error row no longer takes height; an error overlays the pair's bottom edge on a backing, so the requested raster never changes. | `trim`: the waveform heading is within 20 points of the junction pictures at both sizes. | 39.8 points | 16.5 points |
| Help's search row taught keys as plain text | The key line uses keycaps (`/`, `j / k`, `PgUp / PgDn`, `Home / End`, `Enter`, `Esc`); typing has none. The line remains one accessible live sentence. | `menus`: those keycaps are painted while browsing and while the field owns the keyboard. | plain text | keycaps |
| The 192-point inspector truncated titles and values | The title wraps to two rows and values to three before eliding; word values use the body face, numbers stay monospace. | `workspace`: the title and `Original video`, `Original audio`, `Root beat`, `Linked` paint unelided at 960 points. | `Original vi…` | complete |
| The success notice kept a row at minimum size | Below 700 points a quiet notice joins the end of the footer's mode row when it fits whole; errors and long notices keep their row. The decision is retried in the same frame when the room changes. | `workspace`: `Source inserted and saved` paints on the `NORMAL` row; picture bounds raised to 225/200 points. | own 24-point row | on the mode row |
| The Gain waveform barely fit its scroller | Below 700 points the Gain panel is 248 points; the draft status moved to the end of the command row; the waveform caption joins its heading row. | `gain`: heading-to-graph extent fits the scroller at both sizes. | 109-point scroller, heading scrolled out | 123 in 133 |
| Compact Your edit with placed sounds measured 145 points | The compact viewer (caption on the clock row, text-height clocks) also applies with a populated Placed sounds panel. | `sound-placement`: at least 185 points at 960 × 640. | 145.0 | 200.9 |

Minimum-window viewer heights with these changes: Your edit 204.9 → 230.9,
Original 178.6 → 204.6, Gain 164 → 166, populated-sound Your edit 145 → 200.9.
Trim's junction pictures grow from 146.3 to 169.6 points at 960 × 640.

## Image review

The inspected captures match the design contract for hierarchy, tokens,
keycaps, selection and focus. Observations not changed here:

- The sound event inspector keeps its fixed 104-point label column, so a value
  such as `3203 samples` now wraps instead of truncating at 960 points
  (`sounds-minimum-after.png`). Complete, but a narrower label column would
  read better.
- The Gain capture at 960 points still shows the waveform heading cut at the
  top because the replay frames the graph with wheel steps; the heading and
  graph now fit the scroller together, which the new check enforces.
- The `sound-placement` allowance step revealed the button and then checked the
  `:sound-allow` hint beneath it; wrapped inspector values moved the hint
  below the fold. The replay now reveals the hint and checks the button with it.

## Verification

| Run | Binary (`deadpan-app` SHA-256) | Result |
| --- | --- | --- |
| Baseline, 7 affected scenarios | `0626c63d…3c87282b` (`8ce65ac6`) | all pass; source of the before measurements |
| New assertions on the previous application code, continuing past failures | `548c9b7d…118229ad0` | every new assertion fails, values in the table |
| All visual scenarios, 3 jobs | `6b4fca44…78c22caf` (`8ce65ac6` plus this change) | 80 pass, 4 fixture skips, `audio-warming` timeout; 8,133 checks |
| Rebased onto `4057c8d1`: `workspace`, `trim`, `camera`, `menus`, `gain`, `sound-placement`, `recipes`, `macros`, `slip` | `95e2d6f5…02c3b49d` | all 9 pass; 1,435 checks |

`audio-warming` times out waiting for its real PCM horizon exactly as on the
baseline in the [GUI review](gui-review-2026-10-15.md); it is not touched here.
The full-run summary is
[`full-visual-summary.json`](../../tools/ui-feedback/evidence/2026-10-15-gui-findings/full-visual-summary.json).
Strict Clippy for `deadpan-app` with and without `ui-harness` (all targets,
`-D warnings`) passes, as do the 295 `preview::` app tests and rustfmt on the
changed files. The intermittent `place-slice` label comparison from the review
passed in the full run.

The application, replay and documentation changes landed in `b1dc5df4`,
whose commit message describes a concurrent session's keyframe change that
was committed together with them.

The reference host is the M5 Max Mac on macOS 26.5.2 with Rust 1.97.1 and
qualified FFmpeg 8.0.3. Scratch reports remain under `/tmp/gui-fixes`.
