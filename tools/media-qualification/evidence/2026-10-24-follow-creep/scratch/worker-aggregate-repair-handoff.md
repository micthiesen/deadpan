# Aggregate test repair, executed evidence

Only changed source file: crates/deadpan-core/tests/framing/follow_aggregate.rs.
No production, audio budget, formatter, Git, UI or media runtime changes.

The old full timed-Hold capture failed first at its independent audio binding capture complexity budget. This refusal remains explicitly asserted by the patch test; it is not exact-cap capture acceptance.

Tests now split:
- raw_patch_enforces_exact_aggregate_cap_and_retained_result_union: actual document/RestoreSnapshot exact 100,000 records; inverse identity; raw patch acceptance; patch inventory exactly 100,000 plus retained unchanged fixed Follow causes result 100,001 framing refusal; patch own inventory 100,001 refuses; original document bytes remain unchanged.
- raw_slice_reader_enforces_exact_aggregate_cap_without_claiming_capture_admission: starts from an actual valid small CapturedEditSlice. Keeps its eight-frame physical Hold and audio binding, adds one zero-duration part containing empty framed Sequences. The supported raw reader admits 100,000 records, roundtrips identically, rejects 100,001 records with framing refusal, and leaves original slice/document bytes unchanged. The expanded forest truthfully fails validate_capture against the original small source. This proves standalone reader validation, not capture provenance or capture preallocation.

Focused command permitted by Root:
CARGO_TARGET_DIR=/tmp/deadpan-follow-creep-2026-10-24/target cargo test --locked -p deadpan-core --test framing follow_boundaries::aggregate

Attempt 1: 1 passed (all patch cases), 1 failed (raw slice physical-owner invariant), 21 filtered, 14.46s. Logs retained. Cargo failed; exact Cargo exit was not captured because the wrapper assigned zsh's readonly status variable after Cargo finished; wrapper exit 1. Terminal 18572 complete.
Attempt 2: exit 0, 2 passed, 0 failed, 21 filtered, 14.91s. Logs retained. Terminal 88756 complete. No active terminal process.

Files frozen for Root formatting, strict checks and independent review. Previous worker-aggregate-handoff.md describes the superseded failing approach and remains preserved as historical evidence.
