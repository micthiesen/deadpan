# Accepted Generated playback in the app, 2026-10-11

The app preview worker presents every frame of the qualified 1080p60 and 4K30
Original/Generated fixtures while real device audio runs, with no skipped
pictures, proxy pictures, underruns or device faults. This exercises the
[decoder cache](generated-playback-cache-2026-10-11.md) and
[entry preparation](generated-prefetch-2026-10-11.md) through the production
app, rather than the separate CLI performance reader. DP-16 remains partial.

## Workload and scope

The `generated-playback` [replay](../UI_FEEDBACK.md) opens a verified portable
copy, disables proxies, sets monitor gain to 5%, switches to Your edit and plays
from zero to the terminal boundary through ordinary production commands.
Only picker results are scripted. The real playback engine supplies device
estimates; the production preview worker decodes, and each complete UI frame
is submitted to offscreen Metal and awaited without screenshot readback.

Both synthetic Original test-pattern clips have real registered media receipts
and audio: 1920×1080 at 60 fps for 30 seconds, and 3840×2160 at 30 fps for 20
seconds. A native InsertTime command inserts a Freeze at Edit frame 15. The
15-frame 60 fps pause was correctly rejected as below the model's native-frame
minimum; normal SetHoldDuration lengthened it to 30 frames. The 4K pause stays
15 frames. Both therefore insert exactly half a second.

The real local LTX runtime generated separate candidates with seeds 812 and
811, respectively. Each has 17 native 768×320 pictures at 24 fps, qualified and
sampled to the exact project interval, then explicitly accepted through
`accept-hold`. The full generation calls took about 82 and 81 seconds. The
4K fixture construction additionally passed the existing synthetic-worker
hard-cut/pixel regression before exporting the independent fallback used for
real generation. Its failed too-short 60 fps attempt is retained as evidence,
not counted as successful generation.

Measurements use the M5 Max / 128 GiB / macOS 26.5.2 reference Mac, Rust 1.97.1,
qualified FFmpeg 8.0.3 decoder libraries, software decoding, a 1280×820-point
workspace at 1× scale and a requested 120 Hz replay cadence. Originals are
full-resolution; the viewer is the app's smaller picture area, not a 4K display
target. Each run starts a new app process and new private project copy; the OS
file cache is warm. No compiler or model inference ran alongside final playback.

## Final results

Both complete runs have **zero missing frames, zero transport skips, zero proxy
pictures, zero underruns and zero device faults**. The 1080p timeline is 30.5
seconds and the 4K timeline is 20.5 seconds. Every displayed frame has its own
worker and completed UI composition timing.

| Original | Total / Generated frames | Entry / return worker ms | Entry / return request-to-composition ms | Whole playback composition p95 / max ms | Whole replay peak footprint |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1080p60 | 1830 / 30 | 0.163 / 2.190 | 9.16 / 11.13 | 14.00 / 19.92 | 845 MiB |
| 4K30 | 615 / 15 | 0.203 / 8.968 | 12.96 / 17.38 | 25.77 / 29.33 | 1919 MiB |

Worker time starts when the app worker takes the request and ends when its
picture is ready. Request-to-composition includes worker queuing, UI delivery,
rendering and the complete offscreen UI GPU submission. Footprint includes
portable copying, project opening, the replay trace and report generation; it
is not an isolated decoder-memory measurement. Device terminal padding is
reported separately from starvation and is not concealed or counted as media.

The clock-lag samples compare the displayed Edit frame with the latest device
estimate admitted by that UI frame before GPU completion. They are not an
acoustic sync measurement or physical display latency. No native window,
listening, physical scanout, memory-pressure stress, looping or multiple
Generated-provider transitions are qualified by these two ordinary playthroughs.
Previous bounded decoder/loop tests remain separate evidence.

## Measurement correction and checks

The first runs already covered every picture without audio faults, but timing
telemetry discarded a submitted picture when the same UI pass requested the
next decode. It lost 2 composition samples at 4K and 724 at 1080p. The harness
now keeps the pending decode and submitted picture separately. A new request
cannot cancel the submitted timing; an actual replacement submission can.
Composition completes only the submitted ticket and retains the next request.

Two regression tests fail against the archived old implementation and pass
against the fixed implementation in isolated pure-module runs. All 14 telemetry
tests pass in the release app test build, as do the six harness option tests.
Strict workspace/all-target Clippy with `ui-harness` and rustfmt pass. The
ordinary Original and Sound playback replays retain simulated delivery and
pass with the final binary. No additional agent was used; the implementing
agent reviewed the complete diff, authority boundaries, bounds and measurements.

Initial traces, missing timings, the rejected short fixture, final traces,
commands, logs, source snapshots, fixture/model receipts, library hashes and
resource observations are retained in the
[evidence manifest](../../tools/media-qualification/evidence/2026-10-11-generated-app-playback/manifest.json).
Temporary fixture packages and copied executables remain under
`/tmp/deadpan-generated-playback-performance-20261011`.

Executed final app SHA-256:
`9a4d7324657bd18bd5e48a7848dbe84f252cf7f32557d9db9c1b42ddc14778d6`.
