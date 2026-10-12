# Repeat Split clock evidence, 2026-10-12

See [qualification](../../../../docs/qualification/repeat-split-clocks-2026-10-12.md). The primary agent implemented and self-reviewed this increment alone, without live GUI, device output or listening.

`identity.json`, `implementation.patch`, command receipts and compressed logs retain source and binary identities against `1e8baec9`. `first-witness.patch` records the original red regression and temporary alias trace; tracing is absent from the final implementation. The original core regression fails at the external-Repeat guard. `old-baseline-result.json.gz` additionally shows the previous release refusing the new command during stored-history replay, with the new binary proving the authored snapshot unchanged before and after. That is a history-replay witness, not an old-binary media comparison.

All four new PCM variants cover dry/treated Source and nested Preserve, initial plays, later births, gaps, whole-block copy/recopy and deletion of the original Repeat. Copy comparisons use each concrete play's origin-based boundaries. The first oracle incorrectly concatenated a moved multi-play sample interval and compared across one-sample differences in allocation. Its failures are retained in `audio-second.log.gz`; the corrected per-play oracle keeps the existing exact dry and 1e-6 treated bounds.

The first audio compile attempted `.len()` on a returned iterator. The first store fixture requested pause insertion inside Repeat, which the structural layer still refuses. The corrected store test exercises RepeatSelection and includes forced history-write rollback, preview/commit equality, writable/read-only reopen and fresh-revision Undo/Redo. These setup failures are retained; no unsupported structural guard was removed for them.

`debug-project.zip` and `release-project.zip` retain real qualified WAV/MP4 inputs, project history and exact authored requests. The CLI repeats a partial range inside an existing Repeat, increases its play count, adds silent gaps and performs durable Undo/Redo. Debug and release emitted-file reports verify all 17 pictures and 44 audio blocks with zero measured offset and unchanged fidelity thresholds.

`qualify-final.py` runs after all Cargo work ends. Five nonzero 256-pair authored windows from the saved qualified debug executable match the final release executable bit for bit. The final executable renders and verifies the same committed project without changing its authored document. Full reports and commands are retained in `final-result.json.gz`, and `repeat-split.mp4` is that final movie. `debug-repeat-split.mp4` retains the earlier debug output. The initial baseline script incorrectly expected the old executable to read a history containing the newly supported command; that failed attempt is retained separately.

`checks.py` describes the sequential Cargo checks; `test-counts.json` distinguishes full debug suite coverage from focused release regressions. `self-review.txt` records the alias, cohort, processing-scope and resource-bound review. No requirement or release gate is newly complete.

Run `python3 check-evidence.py` to verify file hashes, reports and archives. From the exact source revision, add `--source` to check source hashes.
