# Compact empty Sounds workspace

The minimum 960×640 workspace could leave only about 77 points of picture when
an Original moment was copied. The unused 64-point Placed sounds strip consumed
space below the timeline. This increment moves its empty-state entry into the
Beats heading in short, single-Original Your edit workspaces. It retains a
visible Sounds focus target and `,s` hint while preserving the copied range,
paste actions, playback controls and timeline cards.

The complete Sounds label is measured before the remaining width is assigned
to scrolling group breadcrumbs. Empty structural panels retain their IDs.
The ordinary Sounds panel returns at default height or after a sound is placed.
Room tone remains an overlay over the same compact background; Gain and Camera
retain their own layouts. Normal Tab continues to cycle panes.

## Review

The first draft could emit the Sounds pane ID twice when a pointer click changed
Original to Your edit during paint. One captured placement decision now governs
the entire pass. Independent review then found that a late discard could follow
a GPU submission at the old viewer size. The placement check now follows viewer
header actions and precedes picture submission. A guaranteed native command
footer closure is also invalidated before paint, while command execution and
text consumption keep their established order. The existing render guard skips
discarded passes. Independent follow-up source review found no further concrete
production defect.

The first CPU run exposed the new fixtures' unconsumed egui texture deltas.
Each output now explicitly clears those unused upload records; geometry
assertions are unchanged. All 300 app/harness tests then passed. Independent
replay review distinguished Sounds entry, which retains the accepted picture,
from direct viewer-tab and `:sequence` transitions, which intentionally clear
it. The latter now require zero release-frame submissions, a correctly tagged
pending view and the exact settled view/target. A retained failed visual run
also caught a test assumption that Original has a project canvas. Its geometry
oracle now follows the production full-viewer fallback when canvas is absent.
These are test corrections; they do not change picture semantics.

## Verification

The base is the pushed native gain checkpoint `f9b1a14`. This increment changes
three production app files and three existing replay modules. Pure component
tests cover long nested headings and parent navigation. Extended real
writer/media/Metal scenarios cover empty/placed/undo transitions, nested scope,
copied controls through playback phases, display scaling, retained identity and
the actual release-frame GPU target.

Final source inventory
`783956201811134574d665523081dbae2be239bfaf067ecb134cfcf6847932dc`
passes formatting, strict workspace/all-target lint in both configurations and
265 normal-app tests (263 unit and two headless integration). The 300 optional
app/harness tests (298 unit and two headless integration) passed on
`4a867916bcb1d0e4ada4585812f4f9a7df148ad6fb91fc2bca7a76cf6fae3274`;
only the sound-placement replay helper changed afterward. The completed unit
results were preserved, with final compilation, lint and real paint covering
those helper corrections. Backend sources are unchanged from the native gain
checkpoint's complete 2,029-test workspace gate.

Focused visual replay passes sound placement 217, room tone 221 and nested
pause 73 checks on the final source, each with the live Kestrel audit. Workspace
81 and Gain 266 checks plus their audits passed on `54722ed2`; only their shared
replay helper changed afterward, and those scenarios do not call it. Every
audit passes 5,456 routing cases against 62 live reservations without drift.
These separate, overlapping populations are not one combined unique-test count.

The parent inspected actual minimum/default, native 2×, nested-group and
populated-Sounds captures against the single-Original workspace board. The
minimum copied-range Hold picture is 143 points high, compared with the prior
77-point layout. Other exercised compact states retain 169 or 175 points.
Copy/paste, transport, monitor and parent-navigation controls retain complete
paint and hit clips. The focused empty Sounds entry stays readable beside
scrollable breadcrumbs; the normal separate sound list returns when populated.
This qualifies the specific responsive layout, not full visual parity with all
features in the generated board.

The [retained evidence](../../tools/media-qualification/evidence/2026-09-28-compact-workspace/README.md)
includes complete command/log/source records, original failed replays and
selected inspected images. One failed replay clicked a clipped Browse
accessibility rectangle over the File header; bounded real wheel input, settled
geometry and strict paint/hit checks now precede that click. Another batch
exposed eager rectangle evaluation on accessibility containers without bounds
in a shared helper. A lazy predicate fixes that helper. Passing independent
workspace/Gain results were kept; only the three affected scenarios were rerun.
No product clipping threshold or assertion was relaxed.

The final locked release workspace build passes in 448.61 seconds. The complete
release performance replay passes all 2,323 checks across 18 scenarios in
27.97 seconds, with no findings, failed timing samples or timeouts. On this
Mac17,7 M5 Max with 128 GiB RAM and macOS 26.5.2 (25F84), warm navigation to
completed picture has p95 6.699 ms, cached Repeat 9.721 ms and Hold fallback
11.917 ms. Navigation CPU p95 is 0.942 ms in the warm case and 1.765 ms in the
10,000-beat case. These are harness input-to-GPU-completion and CPU measurements,
not physical display or audio-device latency.

## Limits

The picture-height target applies to the tested empty-Sounds copied-range
workspace. It does not qualify populated sound lists, arbitrary font settings
or every pane's undo-focus combination. Transport feedback in these layout
scenarios is explicitly injected; it does not establish PCM, device, listening,
physical-keyboard, OS IME or display-latency acceptance. The full editor and
DP-20/DP-24 requirements remain incomplete.
