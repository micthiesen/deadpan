# Event prefix cache and loop/Tail verification

One treated event with EQ and both continuous returns exhausted the unchanged
plan-work budget. Per-request prefix tiles now share automation and source work
across overlapping EQ and wet-effect histories.

The new playback regression uses registered 44.1 kHz audio, the store, canonical
limiter, production loop feeder and callback with a simulated device. Eight
generations compare 147,456 stereo frames and 576 monotonic delivery reports to
the complete canonical render. The Tail regression uses a separate whole-input
convolution/automation oracle for root and attached sounds and shuffled seeks.

`commands.json` retains successful and failed attempts and executed test binary
hashes. `implementation.patch` and `identity.json` bind the final source.
`test-counts.json` summarizes the complete audio/playback suite. The emitted
H.264/AAC file, package and report are the real treated-event export fixture.

Run `python3 check-evidence.py --source` to verify evidence against this source,
or omit `--source` after later development. `checks.py` and `run.py` reproduce
the ordered validation with the qualified FFmpeg prefix.

The primary agent implemented and self-reviewed this work alone. No live GUI,
physical device, listening check, persistent filtered cache or realtime latency
qualification is claimed. Partial sound-owner interval routes remain open.
