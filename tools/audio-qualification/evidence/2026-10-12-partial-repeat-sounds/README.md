# Partial Repeat attached-sound evidence, 2026-10-12

See [qualification](../../../../docs/qualification/partial-repeat-sounds-2026-10-12.md).
The primary agent worked alone and self-reviewed. No live GUI, device audio or listening ran.

`commands.json` and compressed logs retain successful and failed runs, source-diff and binary hashes. `identity.json` anchors the final source and release binaries to parent `414b9d6b`; `implementation.patch` retains the code/test diff. `checks.py` reruns the bounded check sequence through `run.py` with the qualified FFmpeg prefix.

The real-media package and `partial-repeat-sounds.mp4` retain the public Render result after partial Repeat, count/gap changes and durable Undo/Redo. Both export reports deliberately show `passed: false`: all 13 pictures and zero-offset audio timing pass, but one of 33 compared AAC blocks fails the unchanged 1.5 dB fidelity threshold. This is negative export evidence, not full export qualification. The regression asserts that specific diagnostic failure.

`baseline-final.py`, request and compressed result compare the same request on prior qualified and final release binaries: old refuses without document change; new accepts and retains four sound owners. Executables are not embedded; their hashes are recorded.

`audio-nested.log.gz` and `nested-trace.log.gz` retain the pre-Repeat EQ/Preserve preparation-budget failure. Apply `nested-eq-reproduction.patch` to a separate copy of this source and run `cargo test --locked -p deadpan-audio --test audio_definition partial_repeat_keeps_complete_nested_preserve_context -- --nocapture` with the qualified prefix to reproduce it. Use a separate Cargo target directory for that source comparison. The dry nested-Preserve test passes. EQ plus nested Preserve is not qualified by this increment.

Run `python3 check-evidence.py` to check retained hashes, reports and archive integrity. From the exact source revision, add `--source` to verify source hashes too.
