# GUI review, 2026-10-15

A review pass over the native workspace for hierarchy, spacing, color, focus,
keycap teaching, selection scope and minimum-size legibility, with the two
known DP-20 items: the Gain waveform visibility failure and minimum-size
picture priority. Evidence is offscreen Metal replay through the production
router, widgets, project service and shared GPU picture path, compared with
the [design targets](../design/README.md). Native OS input, VoiceOver and
physical display behavior were not exercised. DP-20 remains partial.

## Gain waveform visibility

The [terminal-picture record](terminal-picture-2026-10-11.md) retained a
reproducible release 120 Hz `gain` failure. It was already resolved by the
later [gain-scroll correction](gain-scroll-2026-10-11.md) (`335596bf`): the
replay sent discrete wheel corrections before egui's scroll smoothing had
settled. That fix waits for native scrolling to settle; the complete-graph,
axis-label and paint-clip assertions are unchanged.

This review reproduced the original command on `07140b40` before any change
here: release, performance mode, 120 Hz, three runs, all pass (297 checks,
app SHA-256 `1dd007f8…cec165b5`). The debug visual run also passes. The final
binary is recorded below. No application change was needed for this item.

The minimum-size capture still shows a tight fit: the 96-point measured
waveform sits in a 109-point scroller, so its heading scrolls out while the
graph is framed ([capture](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/gain-waveform-minimum.png)).
That is recorded as an open layout finding, not a failure.

## Minimum-size picture priority

At 960×640 the viewer measured 163 points in Your edit and 149 points in
Original; other compact phases measured 111–175 points. Three height costs
were not doing work for the user at that size:

- the header kept its 12-point vertical margins;
- the displayed-picture caption had its own row above the read-only clocks;
- compact Your edit reserved the ordinary 28-point clock row although the
  row is text height there.

Below 700 points the header now uses 6-point vertical margins, keeping its
28-point controls. The caption joins the single-line clock row after the
duration difference and truncates there, with its full text in the
accessible label and hover; the project-clock rate moves into the row's hover.
The compact Your edit reserve is computed from the same fonts the row paints.
The caption still advances only with GPU submission, as before. A skipped
automatic ID keeps every later viewer control's widget identity unchanged
across the 700-point threshold; the `room-tone` scale-change check found
that regression in the first attempt and passes after the correction.

| Phase at 960×640 | Before | After |
| --- | ---: | ---: |
| Your edit, `workspace` | 163.0 | 204.9 |
| Original, `workspace` and `original-layout` | 149.0 | 178.6 |
| `original-layout` selected/copied phases | 143.0 / 175.0 | 172.6 / 204.6 |
| `original-layout` long-label phase | 111.0 | 140.6 |
| Default 1280×820 Your edit | 298.0 | 298.0 |

The `workspace` replay now asserts at least 200 points for Your edit and 175
for Original at the minimum window, with the caption, Original clock and the
Play, Loop selection and Monitor controls fully painted and unelided. The
before values fail those bounds. Populated-sound compact Your edit
(`sound-placement`, 145 points) keeps its separate caption and full clock
row; it gains only the header margin.

Captures: [Your edit before](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/minimum-edit-before.png),
[after](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/minimum-edit-after.png);
[Original before](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/minimum-original-before.png),
[after](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/minimum-original-after.png).
Footer keys, focus pill, pending prefixes and the two-row hint budget are
unchanged.

## Other corrected defects

- **Header status wrapped at the minimum width.** The header split its width
  into equal thirds. At 960 points the right third had about 40 points left
  after Undo, Redo and Keys, enough for `Saved` but not `Draft preview`, so
  Trim, Camera and Slip drew a second 30-point header row
  ([Trim before](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/trim-minimum-before.png)).
  `Working`, `Not saved` and `Read-only` are wider than that space too. The
  title stays centered on the window while the side groups use the space
  beside it; only a title that reaches its existing one-third truncation width
  can still push a wide status onto a second row. Trim at minimum is one row and its junction pictures grow from
  115 to 147 points ([after](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/trim-minimum-after.png)).
- **Camera Cancel was centered.** Apply was a left-aligned full-width row and
  Cancel directly below it was a centered sized button, contrary to the
  side-pane command rule ([before](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/camera-cancel-before.png),
  [after](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/camera-cancel-after.png)).
  Both now use the same row form; the label, key and accessible name are
  unchanged.
- **`backups` painted an unsettled start card.** With no project open,
  submitting `:backups` needed a fourth layout pass: closing the command
  footer flips the start card from compact to full height on the last allowed
  pass, and the card's single cached height then disagreed. The baseline
  binary fails `Every painted frame has a settled layout` identically. The
  card now remembers its measured height per compact state and width, so the
  final pass finds the correct height and no retry is ignored.

## Review findings not changed here

Each is a concrete observation from the named capture; none is blessed as a
baseline. The first seven findings below, the Gain waveform's tight fit and
the populated-sound viewer height were fixed later the same day; see the
[follow-up record](gui-findings-2026-10-15.md).

- Compact beat cards (64 points) show no thumbnails, unlike the design
  target, and a partly scrolled first card is cut mid-text without a fade or
  count ([minimum-edit-after](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/minimum-edit-after.png)).
- A pending card thumbnail paints the same black as an authored Background
  (`recipes-060`, Repeat card while its thumbnail is queued).
- Camera's full-width Apply and Cancel rows reach beneath the inspector
  scrollbar, hiding part of their right border ([camera-cancel-after](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/camera-cancel-after.png)).
- Trim at minimum leaves a band of about 44 points between the junction
  pictures and the waveform heading ([trim-minimum-after](../../tools/ui-feedback/evidence/2026-10-15-gui-review/captures/trim-minimum-after.png)).
- Help's search row teaches `Type to filter`, `Enter keep filter` and `Esc
  leave search` as plain text rather than keycaps (`menus-044`).
- The 192-point inspector at 960 truncates the beat title and several values
  (`Original vi…`); full text remains in hover and accessibility.
- The success notice (`Source inserted and saved`) keeps a 24-point row at
  minimum size after the visible change already confirmed success.
- Existing replay warnings remain: Camera… needs inspector scrolling at
  1280×820; the transcript rail needs scrolling for `Your edit · 2 of 3 words
  kept` and `Recognize again`; many frames settle only on a second layout
  retry (58 in `place-slice`), none ignored.
- `place-slice` failed once (`Expected one slice picture allocation, found 0`)
  in an intermediate run and passed in the two later runs with the same
  layout code; the check compares the displayed label at check time with the
  label painted earlier. Treat as intermittent until reproduced.

Hierarchy, color tokens, the lavender focus pill and selection outlines,
keycap rows, Original/Your edit clocks and selection scope labels otherwise
match the design contract in the reviewed default and minimum captures.

## Verification

Baseline: 13 visual scenarios on `07140b40` pass (debug app SHA-256
`0c82e2b9…62e86f1bbe`); `backups` fails its ignored-retry check and
`audio-warming` times out waiting for its real PCM horizon with that same
binary.

| Run | Source | Result |
| --- | --- | --- |
| All visual scenarios, 3 jobs | `07140b40` plus this change (app `0c26ae56…4d37498`) | 80 pass, 4 fixture skips, `audio-warming` timeout; 8,114 checks |
| Affected scenarios, 3 jobs | `723cb9dd` plus this change (app `804630c5…dc0711`) | `workspace`, `camera`, `menus`, `original-layout`, `place-slice`, `room-tone`, `trim`, `backups` pass; `audio-warming` timeout; 1,784 checks |
| Gain, release, performance, 120 Hz, ×3 | `723cb9dd` plus this change (app `6d5f6e28…183640ad`) | 296 Gain checks each, all pass |

`audio-warming` fails identically on the baseline binary, before any change
here, and its wait does not involve the changed layout; it remains an open
finding for its owner rather than a regression from this review. The four
skips need explicit `--project` fixtures. The full-run summary is retained as
[`full-visual-summary.json`](../../tools/ui-feedback/evidence/2026-10-15-gui-review/full-visual-summary.json).

Strict Clippy for `deadpan-app` with and without `ui-harness` (all targets,
`-D warnings`) passes, as do the 292 `preview::` app tests and rustfmt on the
changed files. Replay is the evidence for the layout changes; the new
`workspace` bound is the regression check for picture priority.

Scratch outputs remain under `/tmp/gui-review`: `before*` (baseline at
`07140b40`), `after1b`, `after2`, `after3` and `full`. The reference host is
the M5 Max Mac on macOS 26.5.2 with Rust 1.97.1 and qualified FFmpeg 8.0.3.
