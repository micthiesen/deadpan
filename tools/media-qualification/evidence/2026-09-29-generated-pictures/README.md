# Accepted Generated Hold picture evidence

See [qualification](../../../../docs/qualification/generated-pictures-2026-09-29.md)
and [the picture contract](../../../../docs/PROJECT_PICTURES.md).

The real bundle test converts the committed tiny RGB fixture to canonical FFV1,
accepts it, relocates its project and removes worker files. Native replay then
opens that retained project and checks all 30 exact RGBA frames, PTS and captured
framing through the production picture service and Metal. The six synthetic
objects and independent frame expectations are retained here. No user media,
project database, model weights, executable or library is published.

`verification.json` records all command outcomes; `summary.json` records replay
counts, failures, limitations and timing summaries. Full logs and replay reports
are compressed. Source inventories bind each run; `source-continuations.json`
lists the seven app files changed after the passing 2,094-test workspace gate.
Final app tests pass 268 base / 304 optional, strict lint passes in both
configurations, and release replay passes all 2,348 ordinary checks plus the
separate 367 Generated checks and its shortcut audit.

Earlier outcomes are deliberately retained:

- Two SHA-256 formatting lint failures, followed by the strict workspace pass.
- Initial Generated replays passed while inspector policy and footer visibility
  still needed image review. `visual-status-failure` is a passing but inadequate
  clipping-only check, despite its exploratory name.
- `visual-status-occlusion` correctly fails with the footer covered by later
  opaque panes. Inactive panel decoration caused the overlap; corrected captures
  show the mode/context/focus row.
- `visual-all-status-fixed` has one room-tone failure: its reveal loop stopped
  after the sample range, before the buttons below. The next scoped run,
  `visual-room-tone-final`, exposes the 117-point minimum picture after the
  footer fix. Its name is historical, not a claim that this was the final result.
- `visual-room-tone-compact` passes 220 checks plus the shortcut audit after
  revealing the complete action group and reducing the read-only clock row.
  The minimum copied-Original picture is 141 points; the 140-point assertion
  was preserved. The final release run repeats the ordinary scenarios.

`captures.json` labels selected actual default/minimum and failure images.
Generated visual captures use source `c434a299…`; final checks use `1803fb62…`.
The subsequent production change affects the compact single-Original clock row,
which the generic accepted-media fixture does not have. `review.md` records the
independent review and paint-heuristic limits. Image inspection remains necessary.

`manifest.json` hashes every retained file except itself. Run
`python3 audit-evidence.py` here to verify inventory, journals, source hashes,
report summaries and all six archived media objects without extracting them.
Scratch paths in reports are retained for attribution. The fixture export can be
recreated by the integration test's explicit `DEADPAN_GENERATED_PICTURE_FIXTURE_ROOT`
environment option; the harness requires the resulting project and adjacent
`generated-picture-fixture.json`.

These tiny pictures and offscreen timings do not qualify model inference,
app candidate acceptance, full-resolution performance, audio device output,
physical keyboard/IME/VoiceOver, final-render isolation, encoded MP4 output,
publication or release packaging. All requirements and gates remain open/partial.
