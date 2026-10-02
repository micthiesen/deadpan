# Compact Original layout, 2026-10-02

The selected/copied Original fixture now retains a 141-point stopped picture at
960×640 with shipped shortcuts. Previously its time strip, transport and empty
Placed sounds panel left only about 50 points. Compact Original now shows the
actual sound count beside Beats and uses smaller read-only clocks. Focusing
Sounds reveals its list in Your edit while retaining both cursors, the selected
beat and copied range. The inactive panel retains its position in the widget tree.

Transport rows reserve their measured text geometry. The same galleys draw the
buttons, live clock and context. Idle Monitor shares the action row when it fits;
long configured paths retain their complete labels and use another row. In the
tested six-key fixture, the stopped picture is 109 points. Preparing and Playing
retain the previous texture without stretching it while the new frame prepares.
These are observations of the retained fixture, not universal size guarantees.

This increment starts at `868203900bdc3653cdb03c4126acbf29c0624192`.
Only app source changes; core schema 43 and database 52 are unchanged.
DP-05, DP-20, all other requirements and Gates A through G remain open or partial.
See [retained evidence](../../tools/media-qualification/evidence/2026-10-02-original-layout/README.md).

## Verification

- Locked base-app suite: 579 app tests and three headless tests pass.
- Locked `ui-harness` suite: 615 app tests and the same three headless tests pass.
- Formatting and both app/all-target Clippy configurations pass, with warnings
  denied. Overlapping test suites must not be added together.
- Focused debug replay: 296 ordinary and 315 long-path checks pass, each with
  the separate 130,696-case Kestrel audit over 62 reservations.
- Final full release replay: all 27 ordinary scenarios and their 3,698 checks
  pass, plus the separate Kestrel audit. The generated-picture scenario is
  explicitly skipped because no accepted-bundle fixture was supplied.

Final source inventory:
`8acf9a5bcf7e2170af545f1935c02911199f1f507a3f3f9eef204379cd3f71ef`.
The final release binary SHA-256 is
`e9bcc1853ac9e00538a1e487d0825de2e3edbb3b3906a1bc53ec3a0162fb1d3f`.
Final minimum/default, long-path and active-transport captures were inspected;
controls remain visible and the retained frame keeps its proportions.
Each recorded command retains its before/after source inventory, exact invocation,
exit and log. The prior full-workspace gate is preserved; unchanged backend
sources were not retested for this app-only layout increment, following the
project's scoped-check rule. The existing nonfatal debug linker warning about
the 16 MB `__eh_frame` limit remains in the retained logs.

The two new scenarios use a real qualified Original, committed catalog sound
placement, distinct Original/Edit cursors and a selected/copied Original range.
They check minimum/default windows, 1×/2× native scale, first resize paint,
complete labels and hit rectangles, actual fitted picture geometry, pointer
and reverse-Tab Sounds entry, restored list visibility and catalog focus.
Pointer Play, focused Enter and accessibility Click are combined with scale
changes. Injected Preparing/Playing updates exercise transport layout without
starting an audio device. Each scenario finishes stopped with no queued delivery.

## Review and retained failures

Independent read-only review identified two production issues and accepted their
corrections. It found no remaining issue in the scoped layout/focus changes.
The reviewer did not run Cargo or launch the app.

1. A fixed transport reserve could be exceeded by valid long custom key paths.
   Measurement and painting now share complete text geometry, including wrapping
   and live status. Both shipped and six-key paths have production replay coverage.
2. Controls below the picture could change transport after a resized GPU target
   had already been submitted. The retained pointer/scale witness submitted
   1200×282 where final layout required 1200×270. Before rendering, a first-pass
   guard lets pointer, focused Enter/Space and pending accessibility Click run.
   The existing discard check then prevents submission at obsolete dimensions.
   The unchanged pointer assertion passes; native Enter and accessibility Click
   also pass with scale changes and retained picture identity.

Root image review of the initially passing full release run found a third issue:
a retained 600×109 texture was stretched into a 600×135 viewer while playback
had revoked its decoded frame. The original visibility check compared with
intended canvas geometry, so it missed this distortion. The new mesh witness
derives its centered uniform fit from the actual retained target and fails
before the correction. Painting now fits that target's raster aspect inside
the viewer until a replacement is submitted. Independent source review accepted
this change for raw and composed targets. Picture-height assertions now inspect
the actual painted mesh as well as its clips and aspect.

Harness corrections are retained separately from product findings. The first
build had a `String`/`&str` error. Tab lookup initially confused the enabled
Your edit tab with its disabled breadcrumb, then a rectangle query panicked on
an AccessKit node without bounds. A copied-register explanation legitimately
scrolls below the compact inspector, so its state is checked throughout and its
paint only at default height; primary selection controls must remain visible.
Reverse-Tab setup initially assumed `:source` always focuses Sources, though it
can preserve Viewer focus. The replay now establishes actual Sources focus
before testing the transition. The native-activation extension also needed its
Queryable trait import. Failing reports and command logs remain in the evidence.

## Limits and lifecycle

Hardware: Apple M5 Max, macOS 26.5.2, Rust 1.97.1 and the pinned FFmpeg development
prefix. The real picture fixture is `cfr-bframes.mp4`, SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
Replay uses the production project service, SQLite, decoder and offscreen Metal.
Input, picker selection and accessibility actions are scripted; playback delivery
is injected. This does not qualify physical keyboard layouts, OS IME delivery,
VoiceOver, actual audio devices, listening, physical display color or performance.

The long variant admits a private startup map. Personal settings are neither
read nor changed. No ordinary native window was opened. Short-lived replay
processes exited after their checks, and the final process inventory contained
no Deadpan process. The generated-picture scenario remains separately qualified
by its accepted-bundle fixture; this increment does not supply that fixture or
claim complete visual/product acceptance.
