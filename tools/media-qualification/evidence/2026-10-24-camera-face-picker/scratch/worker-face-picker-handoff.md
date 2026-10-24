# Camera detected-face picker implementation

Worker production/focused-test ownership:
- crates/deadpan-app/src/navigation/camera.rs
- crates/deadpan-app/src/preview/camera.rs
- crates/deadpan-app/src/preview/camera/face_picker.rs (new)
- crates/deadpan-app/src/preview/targets.rs

Root additionally changed comment-only project/targets.rs and project/service/targets/faces.rs plus native harness faces.rs and faces/camera_picker.rs. These are not worker edits.

Behavior:
- f opens a fresh picker and submits exactly one bounded existing DetectFaces request for Camera's indexed Original picture. Proposals append after saved targets and five fixed points; saved/fixed positions do not change as the reply arrives.
- Nine entries per page. Tab/Shift+Tab cycle pages; pointer page buttons are separate full-width rows. Current-page digits only are selectable and appear on saved/fixed/proposal overlays.
- A reply must match detection ticket, session, revision, asset and indexed PTS, live picker, current scope/cursor/beat and the exact installed Original picture ticket. Close/reopen, Reset, ordinary target selection, session/revision/scope change or Escape invalidate it. Refused/superseded requests finish pending status truthfully; other entries stay usable.
- Found zero/unavailable/failed/cancelled statuses remain visible. Proposal rectangles use the same source-to-canvas projection as existing target rectangles.
- Selecting one proposal uses fresh_face_target and face_target, including exact measured aspect and current shot span, and submits Operation::Save only. No sibling proposals or framing are saved. Draft, Follow and selected target are untouched during the save.
- Before submitting Save, validate the complete prospective framing. A refused save clears only deferred acceptance and pending digit, keeping picker/draft/Follow retryable. Installation waits for Camera's existing verified own-save rebase and matching saved ID.
- After save, center the draft and explicitly switch an existing Follow target while preserving evaluated scale, authored bases, fallback, normalization clock and optional transition. Escape restores entry framing while keeping explicitly saved target. Enter is the separate framing edit.
- Existing macro policy refuses proposal detection/acceptance during recording. Existing source-only target Save continuation supports scoped Camera without changing Core representation.

Harness contract:
- picker_page is 1-based; picker_pages count; faces_status text; faces_pending boolean.
- picker entries retain number (only visible page), label and available and add face (1-based proposal identity or null).
- draft_pose is structured FramingPose, old pose debug string preserved.

Verification:
- Worker final command (exact prefix and all receipt hashes in worker-face-picker-freeze.json): cargo test --locked -p deadpan-app --features ui-harness camera::
- 34 passed, 0 failed, 1276 filtered, 0.03s, exit 0. Terminal 82290 completed.
- Earlier preview Camera tests: 17 passed, exit 0; no-run compile exit0 after fixing first compile's borrow/visibility errors.
- Known linker warning: __eh_frame section too large for compact unwind table. No compiler warnings from this code observed.
- New focused cases: all 133 entries reachable/no async renumbering; exact async job fields and close/reopen; refusal/supersession clears pending; save refusal preserves prior pose/Follow and retry; selected face preserves Follow bases/clock/transition; page key clears pending digit and preserves picker routing.
- Native UI, actual detector, Undo/Redo and export were not executed by this worker. Root owns native harness, broader tests, strict checks and independent review.

All owned source frozen pending Root formatting/review. No formatter or Git command executed. No live Cargo sessions remain.
