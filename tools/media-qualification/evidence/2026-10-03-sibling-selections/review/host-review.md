# Host review: exact sibling forest foundation

Read-only review of the current dirty checkout on 2026-10-03. The core capture patch was still outside this diff, so this reviews the host changes on their intended Children contract. No build or test was run.

## Finding

**Low: zero-time Children copies are still presented as one empty group in native register and placement UI.** crates/deadpan-app/src/preview/copied.rs:37-42 uses duration alone and child_label().unwrap_or("Group"), so a two-child all-empty forest becomes “Copied empty group ‘Group’.” The new forest card in preview/splice/empty.rs says “Empty group contents,” but preview/splice/controls.rs:85 and :306 still say “Insert empty group” and “empty group.” Branch on SliceCaptureSelection::Children in the shared copy label and placement controls, and say “empty group contents” or “empty beats” consistently. Add a direct copy and restored-register label check for an all-empty Children payload.

## Checked paths with no further finding

- crates/deadpan-store/src/registers.rs pairs DeleteChildren with the same parent, first and last IDs as the slice after validate_capture checks the historical payload. A range or a different equal-time child cannot authorize that cut. The new store test covers mismatched commands, reopen, Undo/Redo and an all-empty forest.
- crates/deadpan-app/src/project/service/cut_slice.rs computes the successor at the exclusive end slot, then the preceding child if there is no successor. This preserves exact empty neighbors. The immutable request, bank write and last_cut receipt follow the existing atomic cut path. Exact retries compare the whole CaptureRequest and return the saved receipt; refresh failure keeps the copy and reopening message.
- crates/deadpan-app/src/project/service/edit_slice.rs and registers.rs treat Children as a forest without inventing a child label, and historical restore derives scope and bounds from the saved parent. preview/splice/empty.rs admits a zero-time Children source only at its full original range. Service preview and fast paste then use the existing slot-only zero-duration SpliceSlice path; endpoint refinement, replacement and Move remain refused.
- The exhaustive host uses of SliceCaptureSelection need no additional Children match arm for this foundation. The direct native copy/delete producers still emit Child or Range; semantic text-object production and its receipt/scope changes are the next integration, as requested.
