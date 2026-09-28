# Measured gain waveform review

Base: e3f25db, the pushed compact-workspace checkpoint. Parent owns all Cargo,
formatting, tests, GPU and git execution. Audio and playback workers used
disjoint crate ownership; parent owns app integration and documentation.

The design was mapped read-only before implementation. It reuses the canonical
definition PCM reader and the existing preparation thread. It has no second
media cache owner, waveform device, cross-request peak cache or authored command.
The proposal identified exact PointCeil terminal geometry, cumulative work
budgets, terminal audible-prefix deferral and resource lifetime as key risks.

Imagegen generated three retained boards with exact prompts. Version 1 retained
misleading grey amplitude in the dB graph and said playback stopped. Version 2
corrects those semantics; version 3 moves the amplitude plots into the same
column. The generated axes remain approximate: coded plots share exact geometry.
The final board is a target, not implementation or measurement evidence.

Independent app review found that coarse-level painting still visited all fine
bins already covered. The scan was bounded to 4,096 leaves, but did unnecessary
work. The correction stops at measured_end and starts each finer level at the
uncovered tail index. Follow-up review confirms covered prefixes are not rescanned.

Independent audio review found no concrete issue in signed extrema, complete-bin
coverage, final short bins, exact owner endpoint clipping, aggregate reservation
lifetime or shared canonical admission/deadline control.

Independent playback review found no concrete issue in synchronous play priority,
controller-owned output quiescence, terminal-prefix deferral, exclusive cache
ownership, stale ticket rejection, replacement, failure or shutdown. Tests cover
the relevant source admission and worker lifetime boundaries. The reviewers did
not run tools that compile, test or operate the GUI.

Cold decode/Preserve cancellation remains cooperative. Controlled blocking tests
prove ordering and resource lifetime, not real cancellation latency. Any recorded
debug cold-analysis/fake-device-prefill timings must retain those qualifications.
No physical-device, acoustic, IME or display-latency claim follows from this work.

Formatting, strict lint in both workspace feature configurations and all 32
focused waveform tests pass on source c839083a2d81bab1cc1d66035e7c8482ee1bdd466ed0428d6343045b28cb2166.
The complete locked normal workspace gate subsequently passes 2,062 tests,
including compile-fail documentation checks, with zero failures or ignored tests.
Optional app, painted replay and release results follow below.

The first strict feature compiler pass completed with three app integration
errors: the new renderer needed eframe's egui import, an existing Draft test
fixture needed its new waveform field, and the replay needed to use actual
RawInput viewport resizing rather than a nonexistent Driver::resize method.
Those are corrected together. The failed terminal command/log/source inventory
is retained; focused runtime tests had not started in that invocation.

The first painted Gain run failed the new Retry check: native Tab had focused
the button, but its pre-scroll painted rectangle was below the scroller clip.
The replay now allows hz/5 ordinary frames, matching the populated-control
traversal, without wheel input or direct focus changes. Independent source
review confirms production requests ScrollAnimation::none(); this observes
layout/paint settling, not an animation. Strict text and full hit-clip checks
are unchanged. No product focus defect was identified by the reviewer.

The next painted run passed Retry and populated Tab/Shift+Tab checks, then failed
an older unchanged-recipe containment loop that also inspected pre-scroll
geometry. The replay now shares one bounded settling helper across these three
loops and records actual focus rectangles in containment failures. Only the
replay module changes after the full normal/optional correctness gates.

The 290-check Gain replay passed on 5e7ab59f, but image inspection found a real
wide-layout defect: exact key fields started beside the waveform, then scrolled
away when the curve was visible. The production correction uses a waveform
column row followed by a curve/fields column row. Independent review confirms
equal widths, stable explicit field IDs and distinct automatic column IDs;
the empty right waveform column remains instantiated. New painted assertions
require the complete curve and its Time/Value fields to be simultaneously
visible. Normal and optional app gates are rerun for this app-only change;
backend/audio/playback sources are unchanged from the full workspace pass.

Final source inventory 71d2f31fd90bfdf5d5adc92b9555c3461a9d4536fffdab99d2a657d9ca2022c2
passes formatting, strict workspace/all-target lint in both configurations,
267 normal app tests and 302 optional app tests. Final painted Gain passes 291
checks and room tone 221, each with the 5,456-case live Kestrel audit. The parent
inspected five final default/minimum waveform, curve and keyboard Retry images.
The wide curve and exact fields now remain simultaneously visible. Board v3 is
the target, with generated axes treated as approximate rather than authoritative.

The final locked workspace release build passes in 573.883 seconds. The full
performance replay passes 2,348 checks across 18 scenarios in 33.092 seconds,
with no findings, failed timing samples or timeouts. Warm navigation input CPU
p95 is 0.963791 ms and input-to-GPU-picture p95 6.918584 ms (120 samples); cached
Repeat is 10.152708 ms and Hold fallback 18.324167 ms (40 samples each).
10,000-beat navigation CPU p95 is 1.80725 ms (160 samples). Hardware is Apple M5
Max/macOS 26.5.2 with Rust 1.97.1. These small-fixture offscreen measurements
do not establish physical display, acoustic, IME or full-editor acceptance.
All verification processes reached terminal outcomes without abort/restart loops.
