# Retained viewer pixel verification

This corrects an erroneous visual assessment of two prior PNGs. Their saved
pixels contain the picture; no application renderer change was needed.
`archived-pixels.json` records direct RGBA counts, viewer bounds and hashes of
the archived images/reports. `inspect_archived.py` repeats that read-only check
with Pillow and never changes a capture.

The placement replay now verifies the actual composed fixture picture and
caption after inspector scrolling at minimum/default sizes. It counts opaque
red, green and blue pixels inside the viewer only; other UI colors cannot
satisfy it. This is a fixture-specific assertion, not a universal picture test.

Run `python3 check-evidence.py --source` to validate hashes, successful checks,
replay assertions and current source. Without `--source`, it validates the
retained evidence after the application has moved on. The primary agent alone
implemented and self-reviewed this change. No live GUI, audio device, listening
or physical display evidence is claimed. No requirement or gate is closed.
