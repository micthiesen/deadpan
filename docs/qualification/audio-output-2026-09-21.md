# Prepared macOS output qualification, 2026-09-21

This increment adds a real, narrow macOS output adapter and a bounded prepared
PCM handoff. It does not connect audio to the app, complete the voice graph or
qualify release playback. Base revision:
`16120b7a90ffb313729bf5f173ec9299453781f5`; its
[macOS CI run](https://github.com/micthiesen/deadpan/actions/runs/35671253608)
passed. No persisted schema or normative specification changed.

The [output contract](../AUDIO_OUTPUT.md) defines packet admission, channel-scoped
generation identities, prepare/prefill/activate, bounded callback work, permanent
faults, explicit EOS and starvation recovery. [Dependency audit](audio-output-dependency-audit-2026-09-21.md)
records pinned CPAL/rtrb/CoreAudio behavior independently of these measurements.
Full notices and archive/source provenance live in
[the native crate](../../native/deadpan-output/THIRD_PARTY.md).

## Environment and actual device

Apple M5 Max MacBook Pro (`Mac17,7`), 128 GB memory, macOS 26.5.2 (`25F84`),
Rust 1.97.1, `aarch64-apple-darwin`. Hardware probes use the workspace release
profile with thin LTO, one codegen unit and overflow checks. Unit/integration
tests use the ordinary test profile. Power, OS cache and competing system load
were not controlled for performance acceptance.

All three probes selected `coreaudio:BuiltInSpeakerDevice`, reported as MacBook Pro
Speakers, already configured for 48,000 Hz, two channels, F32. CPAL reported a
15–4096 frame buffer range and an active 512-frame buffer. Every recorded callback
contained 512 frames. The adapter requested the default buffer size and rechecked
the reported route/format after construction and before start. CPAL's internal
physical-format attempt remains a documented side effect; unchanged reported
virtual settings do not prove unchanged physical format.

The first probe submitted a 200 ms 440 Hz tone at -42 dBFS with 10 ms ramps;
the rest of its PCM was silent. The second and third probes were entirely silent.
No probe changed system volume, opened an input device, or used user media.
Hardware serials, UUIDs and unrelated input-device inventory are excluded from
the public evidence. The executable's observed load commands use installed
Apple frameworks/system libraries, not an external audio runtime. This does
not qualify a signed/notarized application bundle or minimum OS target.

## Functional and measured results

The first two actual-device probes pass ten declared checks each; the final
probe passes thirteen. The final probe verifies
every nonempty callback's exact contiguous sample coordinate, rather than only
total counts:

- A prepared two-second region emits exactly 96,000 frames without starvation
  or a callback fault, followed by explicit EOS.
- A seek discards queued old content and emits exactly `[900000, 904800)` in
  the new generation, with no gaps or duplicate submitted positions.
- Deliberate starvation becomes silence. Late same-generation refill stays
  silent instead of resuming against a drifting clock.
- Pause invalidates the old generation; fresh preparation and activation emit
  exactly `[2000000, 2002400)` after native resume, even when the stale queue is
  completely full. Muted callbacks run before prefill to release those slots.
- An injected permanent fault is visible through `check_route`, silences
  callback reports and still allows the actual native stream to pause. No new
  callback reports arrive during the 40 ms observation after pause returns.
  This is an injected controller fault, not a physical disconnection.
- Host timestamps remain nondecreasing, telemetry does not overflow and no
  backend error flag is observed. CPAL can drop error notifications under
  contention, so this is not proof of an exhaustive zero-xrun count.

| Observation | Initial tone probe | Silent probe 2 | Final silent probe 3 |
| --- | ---: | ---: | ---: |
| Recorded callbacks | 322 | 303 | 346 |
| Maximum measured render body | 5,750 ns | 16,292 ns | 15,250 ns |
| Median measured render body | 3,458 ns | 3,625 ns | 3,250 ns |
| Reported playback-minus-callback estimate | 12,916,667 ns | 12,916,667 ns | 12,916,667 ns |
| Telemetry records dropped | 0 | 0 | 0 |
| Backend error flags observed | 0 | 0 | 0 |

The render timing covers timestamp validation and queue rendering only. It
excludes telemetry publication, CPAL's preceding/following work and Apple's
driver. Each measured body is shorter than the 512-frame buffer's 10.667 ms
duration; this is not an entire-callback deadline or product latency result.
Playback timestamps are host latency estimates, not acoustic measurements.
The initial JSON used the overly broad name `callback_cost_ns`; the final API
and probe call the same measured extent `render_cost_ns`. The initial report is
retained unchanged. The final probe additionally includes stronger continuity
assertions, the corrected pause-on-fault control path, full-queue resume and
permanent-fault visibility.

## Headless verification and review

Twenty-one new tests cover fixed and irregular PCM partitions, contiguous sample
positions, full queue admission/retry, invalid samples and arithmetic bounds,
partial/stale packet disposal, prepare/activate interleavings, empty explicit
EOS, starvation/restart, pause, foreign-channel token rejection, checked channel
and generation exhaustion, continuity faults, post-fill seek/fault invalidation,
clock mapping and 200 producer/consumer generation handoffs. A thread-local
allocator wrapper records zero allocation, zeroed allocation, reallocation and
deallocation calls inside the queue callback's content, silence, seek, EOS and
failure paths. It does not wrap upstream/native callback work.

Implementation checks exposed and corrected a large packet enum and a style
lint without suppressions. Review during implementation also corrected the
restart-before-prefill race, token reuse across recreated channels and a
faulted-feed early return that could skip the device pause call. A formatting
check after renaming the timing field required one line to be reformatted.
The final exact repository gate passed **753 tests, zero failures and zero
ignored**, plus formatting, strict workspace Clippy, build and diagnostics.
Earlier passing gates are retained separately; the final gate was rerun after
the timing-field and review corrections.

Independent general and focused concurrency reviews completed. The general
review identified a full-stale-queue resume trap and fault visibility after
an unchanged route query. The adapter contract and actual-device harness now
exercise muted callback restart before prefill; `check_route` reports permanent
faults directly. The reviewer rechecked both changes with no remaining findings.
The focused review found no actionable concurrency issue. Both reviewers ran
the 21 output tests; the focused review also passed strict Clippy and the macOS
target check. Neither reviewer opened hardware; the main session owns the three
actual-device probes.

## Retained evidence and remaining work

[Evidence directory](../../tools/audio-qualification/evidence/2026-09-21-output/)
retains all three full callback JSON reports, process logs, gate scripts/logs,
sanitized environment information, source hashes and review disposition.
The hardware harness is
[`qualify_output.rs`](../../native/deadpan-output/examples/qualify_output.rs).
Ordinary tests and CI never open an audio device.

No native UI, focus, keyboard or application lifecycle code changed, so GUI
review and app startup smoke were not repeated. The output stream's own
start/pause/resume lifecycle was exercised directly. There was no physical
headphone disconnect, device/rate switch, suspend/resume, loopback/listening
corpus, long-run drift measurement, full editing/inference stress, sanitizer run
for upstream libraries, model work or app playback/export integration.

The stock CPAL exceptional OS-clock error path may allocate before our closure.
Full callback fault-path qualification, notification/recovery policy, output
conversion for other configurations, monitoring gain, application audio master
clock/video synchronization, complete voice DSP and signed packaging remain
open. All product requirements and Gates A–G retain their existing partial/open
statuses.
