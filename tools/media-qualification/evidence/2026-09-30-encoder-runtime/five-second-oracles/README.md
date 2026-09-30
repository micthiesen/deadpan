# Scratch-only five-second reference experiment

These are capacity extensions for one full-project qualification recording. They are **not the original four-second admission** and are not repository or production changes.

Retained originals live in `originals/`. The modified copies raise only:

- maximum whole-asset duration from four to five seconds;
- maximum PCM fixture extent from 192,000 to 240,000 stereo sample frames;
- corresponding diagnostic descriptions.

All format, exact PTS, trim, record-count, hash, fidelity, peak, RMS and no-realignment rules remain unchanged. The AVFoundation reader uses the same five-second capacity. Never truncate the movie, raw PCM or reference, rebase observed timestamps, align to events, or hide physical padding when using these copies.

## Prepared artifacts

- `edits.json`: pinned source SHA-256 values and exact unique replacement anchors.
- `originals/`: complete source snapshots preserved before editing.
- `diffs/` and `manifest.json`: full source changes and original/modified/diff hashes.
- `prepare.py`: bounded, reproducible source validation and preparation. It fails if repository sources changed or retained output differs. It never writes outside this directory.
- `test_five_second_extension.py`: pure equivalence and negative checks; no decoding or subprocesses.

The parent executed preparation and all eight pure checks successfully, then
used the modified reader for the complete native comparison. Journals and
results are retained in the containing evidence directory. To reproduce in
scratch space, first remove the saved source files' `.txt` suffixes, then run:

```sh
python3 /tmp/deadpan-runtime-binding-20260930/five-second-oracles/prepare.py
python3 /tmp/deadpan-runtime-binding-20260930/five-second-oracles/test_five_second_extension.py
```

Place this directory before the repository oracle directory on the fresh process's import path. Record the imported modules' resolved paths and hashes. Compile the retained scratch `avfoundation_probe.m` with the existing reader build command, producing a separately named scratch binary. Do not replace the normal reader or alter its original source.

For the project evidence, require the full 128 frames, the full 205,005 authored stereo samples, the original exact absolute sample PTS, start zero and end 205,005. Keep complete decoded physical samples and all spans, including any observed padding. Retain the original failed four-second report separately, then record the five-second extension manifest and reader binary/source hashes with the new result. A passing file comparison still requires all unchanged picture and PCM fidelity checks.
