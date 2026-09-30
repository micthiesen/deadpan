# Slice placement evidence, 2026-09-30

See [qualification](../../../../docs/qualification/slice-placement-2026-09-30.md)
and [the implementation contract](../../../../docs/SLICE_PLACEMENT.md).

`final-summary.json` summarizes test and replay counts plus audio preparation
timings. Command JSON records include the base commit, tracked diff digest and
source manifest digest. `source-*.json.gz` includes untracked source files;
decompress before comparing its SHA-256 with the command report.

`replays/*.json.gz` retains full production input, semantic, paint and assertion
reports. `captures/` contains the final 960×640 and 1280×820 slice workspace
images. These are offscreen Metal captures, not native desktop screenshots.
The replay's audio delivery is simulated; its source decode, GPU submission,
service proposal and SQLite commit are real.

`test-workspace.log` records 2,592 passing tests. `test-ui-final.log` records
358 passing UI-feature app/headless tests. `replay-final` passes 215 slice checks;
`replay-original-moment` passes 16 existing copy/paste checks. The Kestrel audit
contains 6,448 reserved-chord checks within one reported audit assertion.

`audio-debug.log` and `audio-release.log` are JSON measurements from the
read-only `qualify_audio_stream` example. All eight blocks' PCM and limiter-gain
hashes match across profiles. Debug refills exceed the 170.667 ms buffer budget;
release refills take 21.21–31.12 ms on this fixture. Preparation timings do not
prove device delivery or acoustic quality.

`native/` retains actual UI observations and consistent SQLite backup snapshots.
The earlier debug runs reported `Starved`; preserve those observations even
though the release loop, pause/resume and focused-button checks pass.
`release-database-check.json` confirms all row hashes/counts in 20 tables are
unchanged and the writer lock is released. No local native screenshot API was available,
so those images were inspected inline and are not represented by invented paths.

Initial compilation, layout, silent-fixture and replay-expectation failures are
retained alongside their corrected runs. The first diagnostic compile used a
hex-format trait absent from the pinned SHA-256 dependency; explicit byte
formatting fixed it. The first release bundle command refused to overwrite the
existing debug QA bundle; a separate release QA bundle was then created.

`collect.py.txt` records evidence collection. `manifest.json` hashes retained
files. Temporary media packages and external libraries remain in the reported
scratch paths; they are not a self-contained release or distribution fixture.
