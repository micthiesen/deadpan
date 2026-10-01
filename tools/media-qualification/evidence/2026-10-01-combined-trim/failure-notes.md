# Combined Trim authoring failures

The initial recorder invocation failed before starting Cargo because its new
output directory did not exist. Created the task directory and retried. No
compiler or tests ran in that attempt.

`combined-core-first` failed to compile before any tests ran. A glob export
exposed the internal reducer `apply`, making the public command function
ambiguous. Two ExactFrameRange constructors return DocumentError rather than
TimeError. The correction exports the public DTOs explicitly and propagates
those two errors directly. No timing or structural semantics changed.

`combined-core-second` compiled and passed every surrounding target plus the
17 original combined command tests. Its two new capacity fixtures failed during
ProjectDocument parsing, before Trim. AudioBindingState validation spends work
on historical bindings and then every current node under MAX_DOCUMENT_NODES;
a bound exactly-100k-node document cannot pass that existing independent budget.
The static review's claimed successful bound-capacity witness was unreachable.
No runtime limit was changed.

The corrected positive fixture is unbound: exact-capacity interactive geometry
and structural preflight omit the retired B wrapper, then actual authoring still
refuses the later capture resource limit atomically. It does not claim a fully
admitted exact-capacity edit. The negative fixture is also valid/unbound; its
99997 nodes and four required temporary Split nodes exceed 100000 even though
the final 99998 nodes would fit. It rejects at that explicit temporary budget.
All 19 combined command tests pass after these test-only corrections.

Independent store-test review added committed wrapper identities/allocations,
physical Source windows and linked map movement assertions. It also aligned the
used-revision case's timing allocation so it reaches RevisionReused rather than
failing an earlier timing mismatch. Root's CLI review uses a necessarily captured
Ripple timing role and validates the typed malformed-intent request before the
wrong-allocation check, avoiding accidental JSON-shape-only failure.

`combined-store-first` passed four of five tests. The mixed-success oracle
incorrectly expected positive integral In/Roll to narrow the physical Source's
selected window start. The window retains earlier selected context behind its
Partition; only the allocation starts at 1 (or final overwritten B at 2).
The correction requires the retained old window start, keeps literal committed
wrapper/child/allocation assertions, and checks Out growth plus linked Slip map
movement. No production code or tolerance changed.

`combined-plan-first` failed to compile before running its two tests. The
padding-tail test borrowed a selected frame from a temporary VFR index and
used that borrow afterward. A named local index now owns it through the
assertion. No production code or expected picture coordinate changed.

The complete workspace then passed 3,366 unit/integration tests and both
documentation tests on source inventory 9561470566396a8230900d62f66fdc5404f500f33e30ae95e3bf3e8f6453981a.
Strict Clippy found `unnecessary_sort_by` in overwrite's final child ordering.
The comparator now uses `sort_by_key` with the identical three-element key;
both forms are stable sorts. No timing, ordering key, budget or test expectation
changed. The corrected source receives a fresh strict lint run and focused
combined-core regression check; the original full-workspace evidence is retained.
