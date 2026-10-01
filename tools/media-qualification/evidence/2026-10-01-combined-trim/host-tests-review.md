# Independent review of combined Trim store tests

Read-only static review of the five tests in `crates/deadpan-store/tests/source_registration/combined_trim.rs` and the three narrowly exposed Roll helpers. No Cargo, test, decoder or native execution. Capacity tests are separate geometry/preflight evidence and are not part of this report.

## Findings

### 1. Tie the mixed success case to the committed structure

At `combined_trim.rs:120–124`, the saved candidate is checked for total duration, unchanged assets and consistency with its own forward/inverse patches. Literal A/B output ranges are checked only on the independent pure resolution. Swapped retained A/B crops or incorrect crop allocations can preserve the expected physical Source values, assets, total duration and valid inverse while these assertions never compare final child/output geometry. Current host `validate_owner` does reject a revision-only or all-silence candidate; those are not bypass witnesses.

Add literal assertions on the committed root's children and durations, then the retained physical Sources' selection/mapping or committed occurrence ranges. Expected children are `[a-crop,b-crop]` with durations `[4,2]` for Ripple; `[trim-filler-0,a-crop,b-crop]` with durations `[1,4,1]` for Overwrite. Check the filler is silent and the two crop providers retain the intended Source IDs. A's final allocation is `[1,5)`; B's is `[1,3)` for Ripple and `[2,3)` for Overwrite. This is a test-oracle gap, not an identified production failure. The separate retired-B test already checks real node removal.

### 2. The nonempty-redo reused-revision control is masked by invalid timing

At `combined_trim.rs:250`, the `used` case changes `request.new_revision` to `saved` while `resources.timing.allocation` remains `rejected`. Core `source_trim_edit/apply.rs::validate_resources` rejects that mismatch before store `ensure_unused_revision` runs. The generic `is_err()` assertions therefore do not prove the claimed reused-revision path.

Set the timing allocation to `saved` alongside the result revision and assert error code `RevisionReused` for preview and commit. The independent zero test already reaches a genuine `RevisionReused` path because its resources are empty.

## Checks supported by the source

- The shared fixture registers two distinct qualified Originals, verifies different qualification/content identities, derives measured moments at ordinals 10..13, and confirms the generic profile. Root ID and request APIs match the current public definitions.
- I=O=S=R=1 has the asserted literal total and A/B output geometry under both policies. O=2/R=1 Overwrite consumes B, retains duration 6, and the retirement test checks its physical Source is absent.
- Receipt corruption is isolated to one side; the other receipt is resolved successfully before each attempt. Missing rows hit missing qualification, `X'00'` hits the retained receipt-identity check, the altered 32-byte Original SHA remains a structurally valid ownership record and reaches binding comparison, and the 64-digit asset hash remains valid core syntax and reaches receipt/asset comparison.
- The corruption loop uses an already-open store, so failures exercise operation admission rather than cold package-open validation. Both preview paths and commit must return `SourceRegistration`; exact saved rows must remain unchanged, then the original receipt, ownership and revision JSON are restored and compared to the clean inventory.
- Zero validates A while ignoring unused diagnostic B media. The test proves this separately for each B receipt/ownership/asset fault. It does not claim nonzero pure-Slip unused-B coverage.
- The inventory covers authored revisions, request/edit history, head/cursor/workflow, redo, receipts and Original records. The refusal fixture explicitly starts with one redo entry and proves redo remains usable. It does not claim byte-identical SQLite/WAL files or every auxiliary table.
- Exact inverse, one durable history entry, never-reused Undo/Redo revisions, immutable historical reads, reopen and final store validation are explicitly checked. No decoded PCM/picture evidence is inferred from these store assertions.
- Helper changes only widen `ready`, `stored` and `same_content` to `pub(super)`; no helper behavior changed. No public API/type mismatch was found by reading.

The two findings above were sent to root for correction. Runtime qualification remains root-owned.
