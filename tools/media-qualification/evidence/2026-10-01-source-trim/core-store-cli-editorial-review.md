# Core, store and CLI editorial-edge review

Independent read-only review by native_mark_ui_review, 2026-10-01. No runtime execution by the reviewer.

The review first identified the missing Slip-on-Partition fade. After the shared seam helper and persistence tests were added, re-review found no actionable defects in those paths. The helper marks requested target sides, follows ordinary Sequence ancestors to positive-duration neighbors, stops at positive silence and preserves existing markers. Slip marks both sides after rejecting zero; Trim marks only its changed side after installing the final structural target. Store tests include both incident markers and an explicit Hard choice across reopen and two Undo/Redo steps. CLI tests check the marker after commit and its restoration through Undo/Redo.

Runtime evidence is recorded separately. This review does not qualify the still-changing plan/audio edge traversal.
