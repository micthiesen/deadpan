# Partial attached-sound copy evidence

Base source: `b041989ea2edcc071bb20bbe9376afed1d3312d5`.
The implementation patch and per-file SHA-256 values identify this increment
without relying on an uncommitted checkout label. Command receipts record exact
arguments, UTC times, exit codes, logs and executed test-binary hashes. Final app
and CLI hashes are in `identity.json`.

The broad core/plan/audio/playback/store run passed 2,859 tests, zero ignored,
across 146 result groups. The final focused release run covers seven partial-copy
and boundary cases; the store persistence test and public Render/verification
fixture are separate release commands. Strict workspace/all-target Clippy uses
the `ui-harness` feature and denies every warning.

The export is an authored seven-frame edit made through CLI commands from real
fixture media: attached stereo PCM with event and shared-owner EQ, partial paste,
recopy in its explicit group, then deletion of the original owner. The Original's
audio is moved outside the beat to isolate the attachment's timing oracle.
Four pre/post-edit canonical PCM windows compare within 1e-6. The emitted movie,
verification report and zipped fixture directory (project, media and command
requests) are retained. This is no physical audio or listening qualification.

All development failures are retained, including the shared-EQ boundary loss,
new-wrapper context mapping, restored owner-splitting guard, store API typo,
nominally clipped old EQ oracle, unsupported partial Preserve endpoints, export
fixture scope/isolation corrections and a test-helper Clippy import failure.
See `review.md`; the mixed Original/attachment cut comparison remains unqualified.

`run.py` and `checks.py` show the serial local commands and qualified FFmpeg
prefix. `retain.py` records the evidence from the task scratch directory.
Run `python3 check-evidence.py` here to verify all retained hashes, outcomes,
test totals and export results. Add `--source` to compare the checkout with the
retained implementation identity. No independent reviewer, live GUI or audio
device was used. No DP requirement or release gate is newly complete.
