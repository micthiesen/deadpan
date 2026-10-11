# Closely spaced Generated providers, 2026-10-11

The preview worker now prepares the next accepted Generated provider while a
different Generated provider is playing. Two half-second accepted Holds with
only one Original picture between them play all 180 pictures at 1080p60 in
both final runs. Previously, admission waited for that single Original picture:
the second entry took 190 ms on the worker and lost 11 following pictures.
DP-16 remains partial.

## Fixture and behavior

The fixture is a verified portable copy of the 1080p60
[real Generated export](real-generated-export-2026-10-11.md) project. Its
two-second Original contains test-pattern video and stereo chirps. The first
accepted LTX Hold occupies Edit frames 60–89. A production `macro` InsertPause
at frame 91 inserts a second 500 ms Hold; real local generation with seed 815
takes 70.54 seconds, reaches Ready, and is explicitly accepted. The first
accepted Hold remains unchanged. The result has two different sampled masters,
60 Generated pictures and 120 Original pictures in exactly three seconds.

Original frame 60 appears alone at Edit frame 90. Each Hold therefore has
independent Original conditioning boundaries. This fixture does not bypass
the normal reconciliation of neighboring Generated providers and does not
claim direct adjacency or mutually dependent accepted conditioning.

Preparation keeps one job and skips the currently serving Generated artifact.
The scan resets when that artifact changes, in addition to the existing
session, revision, generation and loop-window scope. It still examines at most
1,024 new plan frames per request within the four-second/captured-loop horizon.
An idle Generated decoder is released before a different admission starts.
The Original and its return companion remain available while Generated plays.

The playback bound is now one retained Original, its one companion, one
retained Generated decoder and one upcoming Generated admission: at most four
playback decoders. Full asset/artifact/color/session keys and store liveness
still govern adoption and every read. Cancellation and stopping release the
extra resources on the worker, outside the mailbox lock. Offline readers keep
their existing single-decoder policy.

## Measurement

The existing `generated-playback` replay uses the actual app worker, real
device audio at 5% monitor gain, proxies disabled and complete offscreen Metal
UI submissions at a requested 120 Hz. It opens a fresh private portable copy
each time. This is the M5 Max / 128 GiB / macOS 26.5.2 reference Mac, Rust
1.97.1 and qualified FFmpeg 8.0.3 libraries. No compiler or inference ran
alongside the measured playthroughs. File caches are warm.

| Run | Missing pictures | Second entry worker ms | Second entry request-to-composition ms | Composition p95 / max ms | Peak footprint MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Previous binary 1 | 13 | 189.689 | 202.730 | 20.200 / 202.730 | 770.0 |
| Previous binary 2, after both fixed runs | 12 | 183.371 | 190.139 | 18.930 / 190.139 | 769.1 |
| Fixed 1 | 0 | 0.156 | 9.219 | 14.589 / 22.218 | 767.8 |
| Fixed 2 | 0 | 0.150 | 10.060 | 14.519 / 20.372 | 768.5 |

Both final runs preserve all 180 worker/composition timing pairs, use no proxy
pictures, reach the terminal audio boundary, and report zero underruns or
device faults. The sampled heard-clock lag stays at most one frame, versus
12 at the old second entry. The old run also lost frames 64 and 69 earlier in
the first Hold; that separate variation is retained, not attributed to the
second-provider admission fix. The repeated old-binary run after both fixed
runs again loses two earlier pictures (63 and 71) and ten immediately after
the second entry, without audio faults.

Footprint is for the whole replay, including copying, opening and reporting;
it does not isolate decoder allocation. This sequential Original return needs
no companion job, so it does not measure the worst-case four-decoder footprint.
The viewer is 1280×820 points at 1×; source decoding is full resolution. No
native window, physical scanout, acoustic synchronization, looping with two
providers, or longer/wider Generated masters is qualified by these runs.

## Verification and evidence

All five real-media Generated tests pass. The new regression proves that the
successor is fully admitted during the first Hold, stays prepared across the
single Original picture, is adopted with its first picture already decoded,
and returns all 30 successor pictures byte-for-byte equal to an independent
reader. It also checks Original reuse, old-provider release and cancellation
of another successor admission on stop. Existing tests cover repeated entry,
the Original companion, stale-session rejection and the actual preview worker.
All 45 ordinary worker tests, strict workspace/all-target Clippy with
`ui-harness`, the release app build and rustfmt pass.

The implementing agent reviewed the complete diff, bounds, key/authority
checks, job lifetime, cancellation and test assertions. No additional agent was
used. The [evidence manifest](../../tools/media-qualification/evidence/2026-10-11-generated-transitions/manifest.json)
retains commands, failed baseline, final traces, generation/acceptance receipts,
fixture and executable pins, source snapshots and checks. Packages and copied
executables remain under `/tmp/deadpan-generated-transitions-20261011`.

Executed final app SHA-256:
`1a6f80dcbe7bfa296539068a96d734fa20058ef82d8fd1a1f8478e6b7ce685fe`.
