# Measured gain waveform qualification

This increment adds a bounded **Beat audio · before effects** overview to the
native gain draft. The selected committed definition supplies signed stereo
extrema through the existing canonical PCM path. Gain edits and Before/Draft
audition keep that measured reference. Unknown audio, partial coverage and
failure are distinct from measured silence. See the [contract](../WAVEFORMS.md).

Scoped correctness, keyboard replay, native painted-layout and release checks
pass. The [retained evidence](../../tools/audio-qualification/evidence/2026-09-28-gain-waveform/README.md)
includes original failed runs, passing follow-ups and inspected captures.

## Design and review

The built-in imagegen tool produced a [new interface board](../design/boards/gain-waveform-board-v3.png)
and retained exact prompts. The initial output incorrectly kept amplitude
texture behind the dB curve and said playback stopped. The next two prompts
corrected those semantics and column alignment. The final board is the visual
target; generated grid positions are approximate. Coded waveform and gain plots
share one exact layout function and retain independent vertical scales.

Independent app review found an unnecessary fine-bin scan after coarse painting.
It was bounded by the 4,096-leaf limit, but still revisited covered prefixes.
The corrected renderer stops at the measured endpoint and starts finer levels
at the remaining tail index. Follow-up review confirms that correction.
Independent audio and playback reviews found no other concrete issue in exact
coverage, terminal geometry, resource reservations, admission, playback priority,
device quiescence or stale-request rejection.

The first Gain replay checked Retry immediately after native Tab acquired focus,
before the focus-driven scroll appeared in the painted layout. Its focused
button was below the clip, and the strict assertion failed. The follow-up uses
the existing populated-control test's normal-frame settling window, with no
wheel or direct focus manipulation. Production requests a nonanimated scroll;
the wait observes layout/paint settling, not animation. Independent review
confirmed that distinction and found no product focus defect. The failed run
remains retained, and the paint/hit assertions are unchanged.

That follow-up passed Retry and all populated focus checks, then exposed the
same immediate-layout assumption in an older unchanged-recipe containment loop.
All three geometry-sensitive keyboard loops now share one bounded normal-frame
settling helper. Containment failures also retain the actual focused rectangles.
This correction changes replay observation timing only; production code and
the completed backend/app test populations are unchanged.

The subsequent image review found a real wide-layout problem: the new waveform
and curve occupied one left column, while exact fields began beside the
waveform. Scrolling to the curve left its fields above the clip and the right
half empty. The corrected wide layout has a waveform row followed by a shared
curve/field row. Both columns remain instantiated, and the narrow stacked
layout is preserved. Independent review found no new ID or focus concern.
A new replay assertion requires Time and Value fields to be fully painted
beside the complete curve. This app-only correction receives fresh normal and
optional app tests, strict lint and focused paint checks; the unchanged audio
and playback results remain retained.

## Recorded checks

Base commit: `e3f25dbb4fd5b542cdc9016dee6725187d3ad2fd`.
The first strict compiler pass stopped on three app integration mistakes: an
egui import, the added field in an existing Draft test fixture, and use of a
nonexistent replay resize helper. They were fixed together. No runtime test
result is inferred from that failed pass.

Source inventory `c839083a2d81bab1cc1d66035e7c8482ee1bdd466ed0428d6343045b28cb2166`
passes formatting, strict workspace/all-target Clippy with `ui-harness`, and all
32 focused waveform tests. That population includes one existing regression,
31 newly added tests, and no ignored or failed cases.
The runtime tests compare real qualified-source PCM, nested Preserve/Tape,
room tone, Repeat gaps, selection exhaustion, fractional-rate terminal support,
gain/sound independence, odd-phase bindings, cumulative budgets and retained
prefixes. Engine tests cover no-device analysis, source re-admission, pending
replacement, playback priority, ended/starved terminal prefixes, factory failure,
worker panic and reservation lifetime through actual worker exit.

The full locked normal workspace suite also passes on that source: 2,062 tests,
zero failed or ignored, including compile-fail documentation checks. Its terminal
run took 1,551.83 seconds. Strict normal workspace/all-target Clippy passes.
All 302 optional app tests pass on the same source, using the exact test
executables emitted by the locked workspace `ui-harness` build. The normal and
optional populations overlap and are reported separately. The completed backend
gate is retained through subsequent app-only corrections.

Final source inventory
`71d2f31fd90bfdf5d5adc92b9555c3461a9d4536fffdab99d2a657d9ca2022c2`
passes formatting, strict workspace/all-target lint in both configurations,
267 normal app tests and 302 optional app tests. Final painted Gain passes
291 checks; room tone passes 221. Each also passes the 5,456-case audit against
62 live Kestrel reservations. The image allowance warning caps intermediate
captures only; semantic checks and named checkpoints continue.

The parent inspected final default/minimum measured-waveform and curve captures,
plus keyboard-focused Retry after a labelled injected failure, against board v3.
Signed left/right extrema, full owner-axis labels, separate amplitude/dB scales,
retained picture and fixed actions remain readable. The wide curve's exact
fields are visible beside it after the aesthetic correction. Minimum-size
controls use the existing scroller rather than squeezing every field into one
view. The fixture's sparse peaks are real decoded impulse audio. This is scoped
visual parity for these components, not completion of the entire product board.

The locked workspace release build with `ui-harness` passes in 573.88 seconds on
the final source inventory. Its complete performance replay passes 2,348 checks
across 18 scenarios in 33.09 seconds, with no findings, failed timing samples or
timeouts. On Apple M5 Max/macOS 26.5.2 with Rust 1.97.1, warm navigation p95 is
0.964 ms input CPU and 6.919 ms input to GPU picture completion (120 samples).
Cached Repeat picture feedback is 10.153 ms p95 and Hold fallback 18.324 ms p95
(40 samples each). Navigation among 10,000 beats is 1.807 ms CPU p95 (160 samples).
These are separate warm populations on the small fixture with offscreen Metal;
physical display latency, thermal state and OS file cache are uncontrolled.

A separate recorded invocation of the existing real-PCM fixture reports
511.714 ms from cold waveform request to terminal result for 193,600 samples,
then 4,214.041 ms from the next cold playback request to prepared output. This
is a debug test with a fake device, after analysis has released its media/DSP
owner. It demonstrates the cost of fresh admission and playback preparation;
it is not release throughput, real interruption latency or acoustic latency.

## Limits

The overview is an intrinsic selected-beat reference before effects. It is not
the final mix, arbitrary cropped-range extrema, timeline-wide waveform editing,
scrubbing, cross-request cache reuse or an encoded export. The 120-second sample
work cap and cooperative 20-second budget can leave a truthful unknown suffix.
Cold source admission retains separate cooperative bounds. Real cold decode or
Preserve cancellation latency is not yet qualified.

Controlled worker blocking proves ordering and ownership, not a performance
measurement. Debug PCM preparation and a fake output factory cannot establish
physical-device, acoustic, native IME or physical display-latency acceptance.
The full editor, DP-09, DP-10, DP-20 and DP-24 remain incomplete.
