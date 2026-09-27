# Footer layout qualification, 2026-09-27

Command entry now closes into a correctly anchored footer within its input frame.
Previously the footer borrowed the taller command panel's prior position, leaving
a blank band until the next frame. Original keyboard hints could also extend past
the right edge at 960×640. Both defects are addressed in this increment. DP-05,
DP-20, DP-24 and all delivery gates remain partial or open.

## Layout and input boundaries

An egui bottom panel first uses its cached height. When its measured bottom does
not meet the available bottom, the app requests a layout retry before presentation.
A post-footer command or Camera mode change also requests a retry. Up to three
passes cover final text consumption, mode closure, and the new footer's measured
position; ordinary frames remain single-pass. An oversized footer does not request
an unproductive sizing retry when it cannot fit the available viewport.

egui drains volatile native events before retrying. The app additionally consumes
project, playback and dialog replies once per outer frame, reconciles Camera with
those replies, and counts smoke/diagnostic input frames once. Repeat continuation
and playback picture scheduling wait for the final pass. Known discarded viewer
dimensions never allocate or submit a replacement GPU target. A pending Camera
capture whose picture is presented only in a retry waits for the next repaint.
Revision guards, writer capacity and authored commands are unchanged.

Shortcut pairs reserve the width of their actual key and label galleys, including
keycap margins, before the wrapping parent places them. The same galleys are
painted. Compact Original boundary and whole-video reuse labels reduce wrapping
at minimum size while retaining explicit context and visible command/help keys.

## Review and verification

Independent review identified the default two-pass limit as insufficient when a
mode closes after the footer. The final change explicitly requests that transition
and permits three passes. Review also prompted final-pass picture scheduling and
checked Camera entry, once-only worker delivery, text ownership and Repeat dispatch.
The configured `check` agent type was unavailable, so an existing independent
agent performed the general review. No unresolved correctness finding remained.

Independent static image review found the Original shortcut overflow. The focused
unit regression reproduced a clipped Tab key against the old layout helper; it
passes with measured pairs. That first failing test also exposed an unapplied
egui texture delta during panic cleanup; the test now explicitly clears its unused
texture delta before assertions. Both the original failure and corrected result
are retained. It did not hang or require restarting a test process.

The first final app suite passed 214 tests and failed the existing nested async
insert test: its first mailbox read already contained the successful commit,
while the assertion required an intermediate uncommitted update. The test now
uses the existing controlled-worker harness to hold real preparation until after
that intermediate assertion, then verifies the committed nested scope. It keeps
the same production service and media preparation; no delay, retry or weakened
assertion hides the race. The failed suite and focused corrected result are retained.

The first final menu replay then caught lost text when Escape immediately followed
initial command focus in a combined resize/text batch. The diagnostic recorded
no focused widget before TextEdit: egui only installs its Escape focus filter on
a widget focused in a preceding pass. New command/search focus now requests a
same-frame retry to initialize that filter. It does not force focus back or
interrupt IME. The existing text-exit unit no longer inserts an idle frame after
focus acquisition, and the real menu replay retains the combined input regression.
Diagnostic traces were removed from source; their failed run remains evidence.

The final workspace run passed 1,819 tests across 145 test and doc-test
executables, with no failures or ignored tests. The optional UI harness suite
passed 244 unit tests and two headless integration tests. Strict workspace and
UI-harness Clippy passed. The workspace compilation preceded only the final
movement of the picture-loading indicator into the status row; subsequent
UI-harness checks, visual replays, native smoke and release measurements include
that placement. Earlier development runs are retained separately.

Final static image review confirmed the minimum-size command field, its help,
closed-mode footer and complete Original shortcut row. The mode, focus cue and
loading state remain legible. The pending Repeat notice leaves its shortcut row
visible; Camera exposes its unsaved draft and Apply/Cancel actions, then returns
to the normal saved state. No remaining footer regression was identified; the
minimum-size picture limitation below remains open.

Final visual replay passed all 201 scenario checks: menus 37, rapid input 60,
workspace 80, Camera 12 and playback feedback 12. Each run also passed the
production Kestrel compatibility audit. These replays include first-frame
geometry/mode assertions, once-only input dispatch, combined resize/text/Escape,
queued Repeat completion and Camera cancellation. Native smoke initialized the
Apple M5 Max Metal device, presented its window and completed shutdown normally.
All 469 recorded visual replay frames have aligned footer edges; 435 use a
single layout pass, and the remaining transition frames use two or three.

The [retained evidence](../../tools/ui-feedback/evidence/2026-09-27-footer/README.md)
contains losslessly compressed reports, six reviewed captures, command records,
logs and source identities against base commit
`2fd33e152e848c79896a16caef09938eebe7c83f`. Failed developmental reports remain
separate from final runs. No process was restarted because an observation timed
out; the workspace test process ran once to its terminal success in 965.47 seconds.

## Release responsiveness

The final optimized binary is
`ce84c9c202bf14c4744dfb9c16e5141dcf8b88d47bb151997a3675f68920cc5a`.
Both performance scenarios passed on Apple M5 Max, macOS 26.5.2 and Rust 1.97.1,
using the retained `cfr-bframes.mp4` fixture, actual offscreen Metal completion
and no screenshot readback. Rapid input passed 319 scenario checks and edit
latency passed 448; each also passed the 3,472-case Kestrel routing audit against
62 reserved bindings and the local source.

| Warm operation | Samples | p95 | Target |
| --- | ---: | ---: | ---: |
| Navigation input CPU | 120 | 0.70 ms | <8 ms |
| Navigation input to picture completion | 120 | 5.20 ms | <80 ms |
| Cached Repeat input to picture completion | 40 | 7.76 ms | <50 ms |
| Hold fallback input to picture completion | 40 | 8.59 ms | <100 ms |

The reports retain all samples, including cold initialization and queue-control
phases, separately from these warm-operation distributions. OS file-cache,
power and thermal state were uncontrolled. These tiny-fixture measurements do
not qualify physical display latency or full-size playback, export or inference.

## Visual limits

The saved ImageGen workspace remains the target for picture priority, quiet
panels, restrained lavender focus/selection and readable mode/clock/key guidance.
The footer fix removes incidental motion and clipped shortcuts. The 960×640
picture is still small while command entry and the beat strip are both visible;
responsive beat/playback layout remains improvement work. Thumbnail cards, full
editing grammar and native accessibility acceptance remain open.

The harness uses real SQLite commands, measured source decoding, production event
routing and offscreen Metal composition. It does not qualify physical key delivery,
native IME, VoiceOver, scanout, acoustic output, full-size media, AI or export.
