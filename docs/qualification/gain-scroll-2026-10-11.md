# Gain scrolling replay qualification, 2026-10-11

The complete Gain replay passes 296 checks in an offscreen visual run and in
release runs at 60 and 120 Hz. The previously recorded waveform visibility
failure was caused by replay input sequencing. The correction is confined to
the harness; production rendering, scrolling, audio and commands are unchanged.
This resolves that specific DP-20 observation. Full DP-20 remains partial.

## Diagnosis and correction

The [terminal-picture qualification](terminal-picture-2026-10-11.md) retained
the same Gain failure on the prior and current binaries. The replay sent
discrete mouse-wheel events, waited eight frames, then aimed another correction
using the resulting waveform rectangle. Pinned egui 0.36.2 smooths those events
over multiple frames. Eight frames at 120 Hz were insufficient, so each new
correction competed with the preceding motion.

Added diagnostics record the graph, outer paint clip, viewport and native
scroll state. At 960×640 the waveform is 96 points high and the clip is
109 points high, from y=398 to 507. Successive diagnostic allocations were
y=566–662, 566–662, 467–563, 396–492 and 388–484. The final attempt overshot
the top by ten points. The [failed offscreen capture](../../tools/audio-qualification/evidence/2026-10-11-gain-scroll/captures/failure-compact.png)
shows the partly clipped graph. The earlier diagnostic's empty text-paint
list did not establish absence of waveform paint: its accessible image label
is not itself drawn as text.

Waiting for `InputState::is_scrolling()` to become false fixes that overshoot.
The first such run passed the waveform checks but exhausted the existing
6,000-frame trace cap later in keyboard traversal, because it repeatedly sent
large 2,048-point discrete bursts. That failure is retained.

The final replay uses complete trackpad Start/Move/End gestures for precise
placement. End arrives on the following frame, after Move has been consumed;
putting them in one input batch would discard the delta. Native scrolling must
then settle within one second of replay time. Both default and compact
viewports also exercise an explicit discrete mouse wheel: its full -24-point
movement is measured after smoothing finishes, with unchanged document
revision. These are input events through the real UI, not direct scroll-state
changes.

The complete-graph, axis-label, hit-target and keyboard-focus assertions remain
unchanged. The global trace cap remains 6,000 frames. Failure reports now retain
the measured placement attempts instead of only an accessible image rectangle.

## Verification

| Run | Gain checks | Recorded frames | Result |
| --- | ---: | ---: | --- |
| Debug offscreen visual, 120 Hz | 296 | 5273 | Pass |
| Release, 60 Hz | 296 | 3229 | Pass |
| Release, 120 Hz | 296 | 5459 | Pass |

Each also passes the production-router Kestrel audit. The Gain checks exercise
actual qualified media, canonical PCM waveform measurement, writer proposals,
exact trim/envelope/mute editing, Apply/Undo, stale updates, Retry and four
complete forward/reverse keyboard circuits through populated controls at the
two viewport sizes. Device delivery and labelled analysis failures are
explicitly injected. This is not an acoustic test.

Strict workspace/all-target Clippy with `ui-harness`, the locked release build
and rustfmt pass. The change adds no production behavior, so verification uses
the failing end-to-end regression and full corrected replays rather than a
unit test duplicating the waiting loop. An initial diagnostic compile tried to
serialize egui rectangles directly; explicit coordinate arrays corrected it.
A debug performance invocation was correctly refused by the existing release
guard. Debug linking reports an oversized compact-unwind-table
warning; debug timings are not used as performance evidence. All logs are
retained.

The primary session inspected the final
[default waveform](../../tools/audio-qualification/evidence/2026-10-11-gain-scroll/captures/waveform-default.png),
[compact waveform](../../tools/audio-qualification/evidence/2026-10-11-gain-scroll/captures/waveform-compact.png),
[default curve](../../tools/audio-qualification/evidence/2026-10-11-gain-scroll/captures/curve-default.png),
[compact curve](../../tools/audio-qualification/evidence/2026-10-11-gain-scroll/captures/curve-compact.png)
and [keyboard-focused Retry](../../tools/audio-qualification/evidence/2026-10-11-gain-scroll/captures/retry-compact.png)
against the scoped [gain waveform board](../design/boards/gain-waveform-board-v3.png).
Both channel scales and owner-frame endpoints are visible; the wide curve
keeps its exact fields beside it. Before/Draft, audition, Apply and Cancel
remain outside the scroller. The sparse waveform is the fixture's actual
impulse audio. This does not establish complete workspace parity with the
generated board.

The screenshot allowance warning limits intermediate captures only; named
checkpoints and all semantic checks continue. Other retained warnings describe
layout retries that settle before paint. There are no failed assertions,
ignored layout retries or persistent layout oscillations in the passing runs.
No live GUI, browser or additional agent was used.

Release app SHA-256:
`29bfc8298b63cfc2dfc87af351cf86e401d39acc15e25d3aef2d05a740ec2e9d`.

The [evidence manifest](../../tools/audio-qualification/evidence/2026-10-11-gain-scroll/manifest.json)
retains reports, failures, commands, binary/source pins, source snapshots,
selected captures and the single-session review. Scratch binaries and complete
capture sets remain under `/tmp/deadpan-gain-waveform-20261011`. The reference
host is the M5 Max / 128 GiB Mac running macOS 26.5.2, Rust 1.97.1 and qualified
FFmpeg 8.0.3. Physical input devices, OS IME, VoiceOver and physical display
behavior remain outside this offscreen qualification.
