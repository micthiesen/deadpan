# Aggregate Follow qualification additions

Frozen test files:
- crates/deadpan-core/tests/framing/follow_boundaries.rs: child declaration only.
- crates/deadpan-core/tests/framing/follow_aggregate.rs: one sequential real-limit test and fixture helpers.

Test filter: raw_slice_and_patch_enforce_exact_aggregate_cap_and_retained_result_union

The fixture retains one unchanged fixed Follow and inserts 49,999 animated Follow Holds plus one fixed Follow Hold under one ordinary Sequence, producing exactly 100,000 framing records. It enters through ProjectDocument::from_json, actual RestoreSnapshot command, and actual reversible DocumentPatch. No production limits are lowered. JSON byte lengths remain checked against the real 64 MiB limit.

Coverage:
- Exact-cap valid document and inverse identity.
- Actual exact-cap CapturedEditSlice::capture, to_json/from_json, validate_capture and value equality.
- Raw slice forest with one extra fixed Follow (100,001 records): from_json rejects with framing refusal; original slice serialization and base document bytes remain unchanged.
- Exact-cap result from raw stored patch roundtrip.
- Raw patch with exactly 100,000 after-records plus one retained unchanged Follow: result union has 100,001 records, apply rejects with framing refusal, base bytes remain unchanged. This is a result-validation boundary, not a preallocation claim.
- Raw patch with 100,001 after-records: patch inventory validation rejects with framing refusal; base bytes remain unchanged.

This batch adds no Wrap or SetRetime command cases. Aggregate ingress was the assigned priority; existing earlier tests retain raw Retime, Repeat/isolation/Explode, Hold, copy/crop/Trim/Roll/Slip coverage.

No Cargo, formatter, product runtime or Git command was executed by this worker. Root owns compilation, formatting, actual test execution and independent review. Files frozen pending Root checks.
