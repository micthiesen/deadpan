# Full-Original baseline compatibility review

Reviewed the current diff in `crates/deadpan-store/src/single_source.rs` read-only on 2026-10-01. No correctness findings.

`matches_full_original` keeps exact Source equality for explicit windows. It allows historical absence only by clearing the receipt-derived expected clone's `edit_window`, then comparing the entire Source. Wrong explicit intervals, changed duration/mappings, asset identity, link relation and independent audio offset remain unequal. It neither rewrites historical JSON nor loosens profile, receipt, presentation-basis or history-floor validation.

The focused regression proves exact and absent windows pass, a valid but shortened explicit window fails, and absence does not conceal duration, mapping, sample-offset or audio-asset changes. Together with the two migration witnesses and existing single-source integration tests, this covers the intended policy boundary.

As expected for an optional field, the validator cannot distinguish migrated absence from an otherwise identical current document with absent editorial metadata. All measured media fields still require exact equality; fresh initialization continues to derive Some(window).

Root has already identified moving the test module below `check_history_floor` before lint. Tests are running under root ownership; this review did not execute Cargo, native processes or tests and makes no test-pass claim.
