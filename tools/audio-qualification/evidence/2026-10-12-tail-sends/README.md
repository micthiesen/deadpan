# Selected tail-send evidence

See the [qualification](../../../../docs/qualification/tail-sends-2026-10-12.md).
The primary agent worked alone. No live GUI or audio device was opened.

`identity.json` pins changed source and the final executables. Command receipts
include actual test executable hashes; replay receipts pin the app they ran.
`implementation.patch` includes new source files. Logs are compressed verbatim.

The original `suites` run completed all 583 audio tests before a core test's
obsolete future-version assertion failed. `suites-rest` completed 1555 core,
planning and playback tests before a store assertion of schema 77 failed.
`store-tests` completes all 668 store checks after both stale store assertions
were corrected. `app-tests2` passes 1121 tests with nine existing opt-in ignores.
The final doc-test targets, Clippy, formatting and whitespace checks pass.
These results total 3927 passing tests without counting repeats.

The first export fixture's expected amplitude was wrong. `wav-oracle.json`
records an independent s16 decode: 0.75 source peak and 0.3758904252 at -6 dB.
The corrected 0.37..0.39 bound allows the reconstruction-filter overshoot.
`export-final` retains the successful real H.264/AAC run, full emitted-file
report, output bytes, receipt and authoritative package. No timing, decoder or
signal verifier tolerance changed.

`replay1` failed because its button was below the inspector viewport. `replay2`
passed before image review exposed a weak text-existence assertion. Final
replay checks require visible paint after real scroll input and cover stale
tail capture as well. Its three named checkpoints are the retained PNGs.
Regression replays cover existing Hold effects, audio treatments and EQ.
The reports preserve scroll, screenshot-capacity and layout-retry warnings.

`run.py`, `finish-checks.py`, `post-checks.py`, `replays.py` and `retain.py`
record the machine-specific commands and scratch layout. The partial suite
runs and subsequent narrowed runs intentionally avoid repeating passing suites.
For a fresh checkout, use a new scratch/output directory and the repository's
qualified FFmpeg prefix. Run the app with `--ui-check --scenario tail-sends
--mode visual --output NEW_DIRECTORY`; this uses offscreen Metal.

Run `python3 check-evidence.py --source` here to check hashes, results and
current source. Omit `--source` to validate historical evidence after code moves.
