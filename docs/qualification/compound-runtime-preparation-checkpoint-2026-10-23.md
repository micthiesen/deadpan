# Compound runtime preparation checkpoint, 2026-10-23

This checkpoint records an uncommitted Store/CLI/app change that prepares the
host runtime bank before durable authored writes. The Store runs a fallible
host callback inside its existing immediate transaction. The callback prepares
the runtime bank from the prospective result; the bank is returned to the app
only after the authored transaction commits. If host preparation or a later
write fails, the transaction rolls back and the host result is discarded. The
existing infallible APIs delegate through the same guard helper. No schema
change is involved.

Two independent reviews reported no defects and completed before the executed
tests. Formatting completed successfully. Focused Store coverage passed all
five tests, including authoritative refusal before host preparation, rollback
on host or write failure, retry, and equivalence among prepared commit,
preview, history, and reopening.

The first attempted native test command selected the library target, where the
new test does not live. It exited 0 after selecting zero tests; this is retained
as a failed qualification attempt and is not counted as a pass. The later broad
run explicitly selected and passed
`transactional_runtime_preparation_rejects_bad_capture_before_saving_the_bank`.
The broad archive run passed 3,505 tests with 740 skipped. The full UI archive
run passed 1,296 tests with 11 skipped. Its same new native case passed. Both
archive runs passed their recorded source, input, and executable guards.

The full UI frozen source manifest identifies the six retained Rust source
copies, and each copy's SHA-256 matches that manifest. The broad and UI build,
list, run, helper, binary, input, and source manifests and full compressed test
logs are retained in
[the evidence directory](../../tools/media-qualification/evidence/2026-10-23-compound-preparation-checkpoint/).

At this checkpoint, strict Clippy was still in progress; formatting completed
successfully. The pending strict check is not claimed as passing. Native/media
qualification, release build and performance qualification also remain pending.
No runtime latency or
performance gain is claimed. The focused failure receipt and zero-selection
output are retained alongside the successful rerun evidence.

The inventory verifies 69 payload files /2,669,841 bytes; its SHA-256 is
`0e5aba7d33e5b4b14cde4a1cdbc1e8d3fc523028ba5f0c6e75f5e23c1e3a59ec`.
