# Split sound owner evidence

Base: `743b349d`. This increment preserves attached sounds on eligible surviving
fragments produced by interior pauses, partial deletion and range moves.

`implementation.patch` and `identity.json` record code and exact source/binary
hashes. `commands.json` records commands, UTC, exit codes, elapsed time, source
diff identities and executed test-binary hashes. Compressed logs retain failed
experiments as well as final checks. `test-counts.json` counts the full
core/plan/audio/playback/store run, without double-counting focused reruns.

`baseline-final-result.json.gz` records the prior qualified binary's atomic
refusal and the final release binary accepting the same request against an
independent copy of the earlier qualified fixture. `baseline-result.json.gz`
retains the earlier debug comparison. No shared tree was reset for comparison.

`project.zip` contains the final real-media CLI project, exact requests and
managed media. `split-sounds.mp4` and `release-export-result.json` retain decoded
picture/audio verification. `debug-export.json` retains the earlier run.

Run `python3 check-evidence.py` for artifact integrity; add `--source` from the
repository to compare the recorded files with the current checkout. A later
intentional source change can make that additional comparison fail.

No GUI, device output, listening, independent-agent review or newly completed
DP requirement/release gate is claimed. See the qualification and self-review.
