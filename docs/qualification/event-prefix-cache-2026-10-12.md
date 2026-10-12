# Shared event treatment prefixes, 2026-10-12

A registered sound with gain automation, a mute interval, EQ, saturation,
balance and both continuous returns could exhaust the existing 16,777,216-unit
plan-work budget while preparing canonical playback PCM. The new real-media
loop regression exposed this during reference preparation, before any device
callback. A traced attempt with an incomplete fix stopped at sample 40,960.

Independent event processing now caches each ordered treatment prefix in fixed
1,024-sample tiles within its immutable preparation graph. Overlapping EQ input
windows share their pre-filter prefix. Dry output, reverb and delay share the
complete treated input. Prefix tiles add no processor or extra history; their
keys include the graph parent, voice, prefix boundary, allocation, audible
support and input edges. Existing temporal tiles remain distinct.

PCM residency and prepared-frame accounting include the tiles. Plan-work,
dependency admission, cancellation and deadline limits remain unchanged.
Cached blocks retain their dependencies and processing depth. The graph still
ends with the request: this is not persistent filtered caching or a realtime
latency qualification. Source samples, envelope clocks, effect order, saved
delivery routes and current silence gates keep their existing semantics.

## Verification

The debug loop regression passes for root and attached events using registered
44.1 kHz mono PCM through the ordinary store, source admission, shared bus,
limiter, playback feeder and output callback. A simulated device supplies
delivery reports; no audio device is opened.

Two short windows cross a mute exit or the event's terminal edge. Each starts
after substantial effect history. Both start normally and resume in a later lap.
Eight generations check 147,456 stereo frames and 576 callback reports across
producer boundaries and many loop seams. Every callback sample equals the full
canonical render times the independent 37.5% monitor gain; delivery coordinates
remain monotonic. Reopen preserves the recipe, and playback changes neither the
document nor history.

The allocated-Tail regression independently constructs the complete event
input with direct EQ convolution, exact automation, saturation and balance.
It applies the selected send, qualified Tail effect and the Hold's default
2 ms fade. Root and attached events cover ordinary-order reverb and reordered
delay, with both continuous returns enabled. Complete output and shuffled cold
seeks match that reference; a sound's continuous returns do not feed its own
allocated Tail. Samples after the authored ring are exactly zero.

All 706 audio/playback tests pass, with none ignored. Both new regressions also
pass in release builds. Strict workspace/all-target Clippy, formatting and the
app/CLI release build pass. The existing treated-event H.264/AAC export passes
all 30 pictures, four audio comparison blocks and one verified-zero-offset
window. Minimum block SNR is 43.3605 dB; maximum level difference is 0.023134 dB.
[Retained evidence](../../tools/audio-qualification/evidence/2026-10-12-event-prefix-cache/README.md)
includes logs, source and executed-binary hashes, the emitted movie and its
project package and verification report.

## Retained failures and limits

The initial loop reference requested more than the limiter reader's per-call
bound; it now uses bounded blocks. That exposed the actual plan-work failure.
Caching only the complete input was insufficient. A temporary trace pinpointed
repeated pre-EQ gain evaluation at 16,777,209 units plus a ten-unit charge; the
trace was removed after extending reuse to every required prefix.

The first Tail test did not compile because it called a sibling's private
fixture helper. Its local Hold recipe corrected that. Its first PCM expectation
then omitted the Hold's automatic fade: the observed first sample was exactly
the expected ring multiplied by 1/192. The independent reference now includes
that authored output edge. These failures are retained with successful runs.

The primary agent implemented and self-reviewed this work alone. No independent
review, live GUI, physical output or listening evidence is claimed. Partial
attached-sound interval routes, broader nested loop cases, persistent filtered
caching and the remaining mastering/performance work remain open. No requirement
or gate is newly complete.
