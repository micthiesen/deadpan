# Source projection reuse evidence, 2026-10-12

See [qualification](../../../../docs/qualification/source-projection-reuse-2026-10-12.md). The primary agent implemented and self-reviewed alone, without live GUI, device output or listening.

`commands.json`, compressed logs, `identity.json` and `implementation.patch` retain successful/failed checks and source/binary identities against `0c445fc3`. `checks.py` runs the sequential Cargo checks with `run.py` and the qualified FFmpeg prefix. `test-counts.json` distinguishes full debug coverage from focused release checks.

`first-witness.patch`, applied to a separate copy of parent `0c445fc3` with its own Cargo target directory, reproduces the temporary diagnostic that showed repeated preparation of the same two nested stages through count 64. That diagnostic was removed. The failed witness log is retained. Unit-test setup errors and the first real-media fixture's structural guard/256-sample API refusal are also preserved; their corrected tests pass.

The real-media project uses the existing qualified WAV and Original MP4. Small 256-frame authored-bus windows are bit-identical between old/new binaries and three cold subreads. An initial baseline script incorrectly expected the old binary to fail that small read; `baseline-debug.log.gz` retains that rejected assumption. The corrected comparison is in `baseline-debug2.log.gz` and the result files.

Full Render is the actual real-media reproducer: the old binary fails with `render_audio_failed` / `prepared stages per read`, with cleanup confirmed; the new debug and final release binaries render the same revision successfully. `qualify-release.py`, retained render stdout/stderr and verification reports record the result. Both emitted-file reports verify all 120 pictures, four audio windows, 20 compared blocks and zero signal offset using unchanged thresholds. `nested-eq.mp4` is the final release output; `project.zip` retains the source package and authored command requests.

The separate short partial-Repeat AAC fidelity failure remains explicit in `release-export-result.json.gz`; it is not resolved or reclassified by this cache fix.

The first release media checks overlapped the final app build, which replaced the CLI with a different build artifact. The evidence hash check caught this. `before-app-*` reports and corresponding command receipts retain those successful earlier checks; the final baseline and Render checks were rerun after all Cargo work ended, and the checker requires their hashes to match the final binaries.

Run `python3 check-evidence.py` for hash, report and archive verification; from the exact source revision, add `--source` to check source hashes.
