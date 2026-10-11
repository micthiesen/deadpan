# Stereo balance evidence, 2026-10-11

See [qualification](../../../../docs/qualification/stereo-balance-2026-10-11.md).

`source-pins.json` identifies the complete changed executable/test source and
routing snapshot against the base revision. `binary-pins.json` identifies the
release app, CLI and reused media helpers. The actual movie and independently
decoded AAC PCM are retained beside the full export report. No alignment was
applied to the independent channel measurements.

Logs retain initial failures as well as passing final checks. The initial
context-version and treatment-record expectations were corrected; the app
compile caught a missing balance description. Reference regeneration exits
with failure deliberately, requiring a reviewed rerun. The full app run has
one unrelated take-test receipt race and nine existing opt-in tests skipped;
`takes-bound` passes after binding that helper to its request ticket.

The two compressed replay reports retain every semantic check, input step,
warning and skipped scope. Four reviewed offscreen PNGs are selected from their
named checkpoints. Full replay directories, source snapshots and executables
remain under `/tmp/deadpan-stereo-balance-20261011` on the qualification Mac.
`manifest.json` hashes every retained evidence file other than itself.

`check.py` validates this evidence and the current source identities. Scripts
are retained as run; fixture and scratch paths document this run and are not
portable prerequisites for normal application use.
