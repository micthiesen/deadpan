# Audio preparation activity, 2026-10-11

Playback preparation already ran on its own worker and retained qualified PCM
across compatible seeks. The native workspace now shows what that worker is
doing and how long the current playback request has waited. Original, Edit and
catalog Sound use the same request-bound observations.

## Behavior and ownership

The worker reports plan admission, source verification/decoding, stretch policy
checks, processed output, remaining input admission and final cache writes.
Stretch counters come from actual 48 kHz frame positions in the current stage.
They are not an overall percentage or an estimated completion time. Nested
source and stretch scopes restore their parent's activity on success or failure.

The controller samples one small coherent observation at most every 100 ms
during preparation. Its elapsed counter measures the age of that request.
Updates retain the existing ticket, session, project, revision, proposed-content
identity and output generation checks. They carry no heard sample coordinate,
cannot activate the device, and clear when playback starts or stops. Space
continues to cancel preparation. No device callback reads or writes a monitor.

A replacement request gets a new monitor without invalidating reusable PCM.
DSP scheduling, exact sample values, cache admission, bounded storage and failure
handling are unchanged. The observer lock covers only a small value copy; source
I/O, DSP and cache writes run outside it. Preview and playback remain outside
the background job coordinator.

## Evidence scope

Apple M5 Max, 18 physical cores, 128 GiB; macOS 26.5.2 (25F84), Rust 1.97.1,
with the qualified FFmpeg 8.0.3/Opus 1.6.1/dav1d 1.5.4 prefix.

The complete locked audio and playback suites pass **503 and 73 tests**,
respectively, with none ignored. The final native transport selection passes
**21 tests**. Final focused controller and unused-input-admission tests also
pass, as do strict workspace Clippy (all targets, `ui-harness`) and formatting.
The full suites preceded the elapsed-counter saturation correction and final
test-only additions; the final focused checks cover those changes.

The decoded-WAV regression compares exact PCM with and without observations,
forces private-file output, checks actual monotonic produced-frame counters,
reuses a warm cache with a new monitor and retains source admission on hits.
Cancellation and an unavailable unused-input tail restore the parent activity
and publish no failed stage. The existing four-minute continuous PCM oracle
also runs in the full audio suite.

The controller test parks the real preparation worker before its first batch,
observes progress while the fake output device remains inactive, then supersedes
the request. Only its replacement activates. Native transport tests reject
foreign request/session/project/revision/content/generation observations and
keep the heard sample unchanged.

Offscreen Metal replays exercise the production native layout with injected
progress states. They inspect actual paint visibility, cancellation controls,
both editor clocks, selection and retained picture state at compact and normal
sizes, including long custom key sequences. Their displayed sample counts and
elapsed times are injected test values, not performance measurements. Retained
images are renders from those replays, not live-window screenshots.

The initial four replays pass **963 checks**. Visual review of the subsequent
59-check Sound replay found mid-word wrapping despite passing its visibility
assertions. Preparation details now occupy a full-width row below the Sound
heading. The final Sound replay passes **61 checks**, including actual glyph-row
word boundaries at 960×640 and 1280×820. Both final renders were inspected.
The earlier render and report remain marked as pre-fix evidence.

The first compile exposed missing fields in replay-only `Update` constructors.
The first controller regression exposed an incorrect fake clock in the new
test, which was corrected without weakening the production watchdog. A
library-only transport command ran zero tests; the final command explicitly
selects the native app binary. Those failures and the empty selection remain
in the evidence. Replay warnings retain input-driven layout retries, the
scrolled source rail and the intermediate image budget; none is an ignored
assertion failure. The debug linker also retains its existing large
`__eh_frame` warning.

## Remaining work

This change exposes active playback preparation. It does not implement future
stage prewarming, persistent cross-restart PCM reuse, or the remaining cache
lifecycle and long-work scheduling. The per-batch cooperative deadline and
source-admission limits still apply. No new real-device, acoustic, live-window
or physical-input qualification is claimed. DP-09 and DP-16 remain partial.

Commands, failures, test logs, executed binary hashes, source pins, complete
replay reports and selected images are retained in
[`2026-10-11-audio-progress`](../../tools/audio-qualification/evidence/2026-10-11-audio-progress/).
