# Audio confined to the middle of a video

`middle-audio.mp4` remuxes Deadpan's existing synthetic CFR/B-frame video with
synthetic 44.1 kHz mono audio offset by one second and encoded as AAC. Both
inputs are generated test media owned by this repository; no external footage
is included.

Run `python3 generate.py` with FFmpeg and ffprobe on PATH in a fresh fixture
copy. The command refuses to overwrite the output. `provenance.json` retains
input/output hashes, the exact command, FFmpeg build details and ffprobe records.
The output SHA-256 is
`0542a0f4ef9768b126bceb5954ff5757ba8b4289d6c2df8ea5b7bc0b240c93f3`.

Video spans `[0,120120)` ticks at 1/30000. Qualified audio spans
`[43076,88217)` ticks at 1/44100. Raw AAC decode blocks reach 89156 because of
terminal padding; the qualified audio index retains the measured terminal
duration. Video ordinal ranges `[0,10)` and `[80,90)` therefore have no audio
overlap on either side, while the full Original retains its audio stream.

The storage regression verifies that both silent slices retain a linked Source
and empty audio selection through preview, commit, close/reopen, Undo and Redo.
