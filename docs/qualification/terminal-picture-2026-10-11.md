# Final playback picture handoff, 2026-10-11

Automatic playback completion now finishes the final included picture before
switching to stopped preview. It preserves a useful decode across the audio
endpoint, without extending or restarting audio. Twelve deterministic endpoint
cases and seven real-device runs pass. The latter present all 8,206 pictures,
including one final picture after the UI admits End. This fixes cancellation
of that picture; it does not establish physical display or acoustic timeliness.
DP-16 and DP-20 remain partial.

## Failure and change

The [previous qualification](generated-slices-2026-10-11.md) retains a failed
30.5-second 1080p60 run. Edit 1829, its final Original picture, was requested
with 236 samples left before the audio endpoint. Decode took 5.781 ms; the UI
received it after 10.283 ms. The End handler immediately cancelled it and
requested the same picture as stopped preview. The correct picture eventually
appeared, but the useful transport request never reached Metal. That historical
failure remains unchanged.

The playback controller already stops and pauses the device before publishing
Ended. The app now retains that ended run until its final included picture
reaches GPU submission. It keeps an existing request, or schedules the missing
final picture once the current request finishes. The scheduler records the
actual submitted serial. Presentation must match that serial and output
generation, be free of errors and pending rendering, and have successfully
submitted the current picture. An older displayed picture cannot complete the
handoff. The ordinary stopped request then restores exact inspection eligibility
and refines a proxy when needed.

The captured clock remains at the exact excluded endpoint. The controls show
`Finishing picture` with `Stop`. No worker, queue, authored state or audio time
is added. Manual stop, navigation, edits and failures still cancel pending work;
Space during End creates no resume token. A stale Playing or Preparing update
cannot reactivate an ended run. Sound-only and Trim audition have no transport
picture to finish. Original, committed Edit and proposed Edit share the same
request gate.

## Verification

The initial full release app suite passes 1,115 tests, with nine explicitly
ignored fixture tests; all four headless integration tests also pass. After
the final control wording and early-return cleanup, all 20 transport and 22
presentation tests pass on the final source. Strict workspace/all-target Clippy
with `ui-harness`, the ordinary app build check without that feature, the
release build and rustfmt pass. The first Clippy run requested `?` in the new
Option-returning helper; that corrected failure is retained.

The new `terminal-playback` replay runs six cases for each of Original and
Edit: final decode still pending, decoded but not submitted, not yet requested,
Space stop, navigation and device failure during End. Real decoded pictures
reach offscreen Metal; device updates are explicitly injected. All 76 checks
pass, including exact endpoint retention, final ticket preservation, cancelled
work rejection and unchanged history. Unit tests additionally check generation
mismatches, failed rendering, proxy/refinement ordering and late updates.

The final binary also passes `playback-feedback`, `original-playback`,
`sound-playback`, `place-slice`, `trim`, `original-layout` and
`original-layout-long`. Move comparison waits for the final picture before
checking its existing exact endpoint behavior. The retained layout warnings
describe retries that settle before paint, not ignored retries or persistent
oscillation.

`gain` **fails** its resized waveform visibility check before playback. The
prior qualified binary fails with the byte-identical message and widget
geometry: the measured stereo waveform has an accessible rectangle but no
recorded paint after bounded scrolling. Both traces have no active transport
at failure. This is an unresolved DP-20 observation, not a passing regression.
Reproduce with `deadpan-app --ui-check --scenario gain --mode performance
--hz 120 --output NEW_DIRECTORY`. The retained reports identify the resize
and scrolling steps. No live GUI or additional agent was used.

## Real-device playback

The fixed series uses the M5 Max / 128 GiB / macOS 26.5.2 reference Mac, Rust
1.97.1 and qualified FFmpeg 8.0.3. The app opens private portable copies of
the existing real LTX fixtures, uses 5% monitor gain and disables proxies.
Full UI and picture work submit to offscreen Metal at 1280×820 points, 1×,
with a requested 120 Hz cadence. No compiler or inference runs alongside
these replays. Every result is retained; there are no retries in this series.

| Run | Presented / expected | After admitted End | Composition p95 / max ms | Whole-replay peak MiB |
| --- | ---: | ---: | ---: | ---: |
| Long 1080p60, 1 | 1830 / 1830 | 0 | 13.972 / 24.440 | 805.7 |
| Long 1080p60, 2 | 1830 / 1830 | 0 | 13.922 / 20.773 | 861.8 |
| Long 1080p60, 3 | 1830 / 1830 | 0 | 13.968 / 21.271 | 865.8 |
| Long 4K30 | 615 / 615 | 0 | 25.222 / 26.185 | 1916.9 |
| Two-Hold 1080p60 | 180 / 180 | **1** | 14.707 / 21.408 | 770.7 |
| Two-Hold 4K30 | 90 / 90 | 0 | 27.736 / 42.475 | 1948.1 |
| 1080p60, 30 laps plus next first picture | 1831 / 1831 | 0 | 12.866 / 14.255 | 881.7 |

The two-Hold 1080p run exercises the real terminal handoff. Its final Edit 179
request uses retained sample 143693, with 307 samples before endpoint 144000.
The worker takes 5.588 ms. The UI receives the reply after 10.502 ms and admits
End before presenting it. The **same transport ticket** submits after 13.943 ms
and completes composition after 15.324 ms, before the stopped replacement.
It is explicitly recorded as `after_audio_end: true`; the heard clock remains
144000. No earlier picture completes after the admitted End update.

All runs report zero underruns, device faults, proxy pictures or skipped
pictures. Sampled clock lag is at most one frame. Ordinary runs reach their
exact sample endpoint; the loop retains monotonic delivery and its captured
window. Callback silence counters are 1344 frames for each long 1080p run,
1600 for long 4K, 1408 for each short run and zero for the loop. They include
nonplaying/terminal fill and are not starvation or acoustic-sync measurements.

Coverage now explicitly includes the terminal handoff. `after_audio_end`
means the UI has admitted the controller's End update; a false value does
not prove the physical device or display had not already reached its endpoint.
The table and traces therefore do not claim that every picture was on a
physical display before audio ended, or that the historical strict
during-playback failure became a pass. The deterministic replay verifies the
cancellation fix independently of the varying real-device/UI cadence.

Executed app SHA-256:
`06257df42aa1b44240001e4b06dffe0636099a69d980656622669e62e86f1bbe`.

The [evidence manifest](../../tools/media-qualification/evidence/2026-10-11-terminal-picture/manifest.json)
retains source and executable pins, tests, command results, source snapshots,
fixture identities, all application traces, the Gain negative control and the
single-session review. Scratch binaries and media remain under
`/tmp/deadpan-terminal-picture-20261011`. Broader effects, mastering, formats,
native-window presentation and physical timing remain unqualified here.
