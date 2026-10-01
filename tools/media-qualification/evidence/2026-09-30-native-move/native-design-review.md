# Native Move integration design review

## Judgment

**Proceed after adding the boundary-reparenting comparison branch and fixing the remaining comparison/receipt contract details below.** No need for new decoder, DSP, picture identity or general transaction infrastructure. The design's current-revision authority and reuse of opaque edited-view admission fit the existing architecture.

Reviewed `/tmp/deadpan-atomic-move-20260930/native-followup-design.md`, specification §9.7, the native edited-slice handoff, and current service, store, preview, picture and transport code. No implementation, Cargo, tests or UI actions were performed.

## Required design correction

### Boundary reparenting must compare the same global position

The unconditional removal/insertion maps at design lines 72-81 are wrong for meaningful moves with `destination_before == source_before.start/end`. Core admits these because ownership can change while temporal order remains unchanged (`move_range.rs::Plan::new`, lines 127-175). They are distinct from exact no-ops.

Example: root contains A(a:10 frames), then B(b:10 frames). Move a from A's `[0,10)` into B slot zero at original `d=10`. A becomes empty and a remains at global `[0,10)`, now receiving B's live framing/gain. Under the proposed insertion comparison, proposed frame 5 falls inside `[0,10)` and collapses to saved destination boundary 10. It shows b instead of comparing a's changed treatment at frame 5. Removal comparison similarly invents a temporal hole.

**Correction:** add an ownership-only case when `d == a || d == b` and core reports `is_noop == false`. Compare identical global frame/sample positions. Show source and destination parent names and the unchanged interval, with wording such as “Move between groups; timing unchanged.” Keep both scope addresses visible, but do not draw a removed time gap or inserted time block. Exact no-ops remain uncommittable in the native UI.

## Preferred bounded comparison semantics

Use **site-relative time comparison**, not a universal same-sample correspondence. The root sound bus remains in global time while structural source voices move, so no single full-bus map preserves every voice's original sample. Physical-owner traversal just to flip Before/Proposed would add scope without a product requirement.

Keep `JoinSite::Removal` and `JoinSite::Insertion` as local inspection state. Switching site changes the inspection cursor/window and transport ticket, not the proposal or its immutable snapshot. Source refinement, destination changes and Copy/Move/Replace changes still advance the proposal change identity.

For genuine temporal relocation, retain the design's paired affected intervals:

- Removal: saved `[a,b)` and proposed `[removal_join,removal_join)`.
- Insertion: saved `[d,d)` and proposed `inserted_after`.

Generalize the existing local interval mapper. For from `[f0,f1)` and to `[t0,t1)`:

- Before the interval, preserve signed offset from its start: `t0 + (x-f0)`.
- After the interval, preserve offset from its end: `t1 + (x-f1)`.
- A changed interior with an empty counterpart maps to that counterpart's join.
- An exact empty seam maps to the counterpart's start. Make this explicit before the suffix branch.

For audio, first convert each complete absolute boundary with `B(frame)`, then apply the same checked integer rule. Preserve paused/playing state and the heard offset defined by this local rule. Do not claim physical source-sample identity or bit-identical bus alignment.

A useful NTSC witness makes that distinction explicit: at 30000/1001, move `[2,4)` to `d=6`; `B(2)=3203`, `B(4)=6406`, `B(6)=9610`. An insertion-prefix comparison of saved sample 9510 maps to 6306 by site-relative time. A particular displaced Source's retained sample can instead correspond to 6307. This is not a product defect under the chosen site-relative contract. Both views must still render their own canonical PCM exactly.

### Nearby sites and outside-window cursors

Do not apply one local site's offset map across the other site's changed interval. Cap the flank between the two sites:

| Move direction | Removal context cap | Insertion context cap |
| --- | --- | --- |
| `d < a` | Prefix stops at old `d` / proposed `d+L` | Suffix stops at old `a` / proposed `b` |
| `d > b` | Suffix stops at old `d` / proposed `d-L` | Prefix stops at old `b` / proposed `a` |

Use these absolute bounds independently in each revision, with the configured lead/follow and project endpoints as additional caps. They can differ by one sample after rounding. Clamp a mapped sample to the actual counterpart window if its extra terminal allocation has no counterpart. Label a shortened flank, for example “Context ends at other move join.” The affected interval itself remains fully inspectable.

If unrestricted h/l inspection has left the active comparison window, pressing Compare resets to that site's counterpart boundary and gives brief “Comparison reset to insertion/removal join” feedback. Do not map it to distant moved material. Shift-Space starts the active site's bounded window. A terminal join remains an exact cursor boundary `N`; display the last included picture `N-1` separately, as the existing transport `Position` contract already permits.

These rules resolve the design's outside-window TODO without adding another mode or control. Tests should include distant sites, one-frame separation, overlapping default lead/follow, both terminal boundaries, both directions and ownership-only reparenting.

## Receipt and selection integration

The range cannot live only in the draft/request. The current successful `receive_splice` closes the draft immediately (`preview/splice.rs:443-464`), including when the receipt arrives before its workspace. The current `CommittedEdit` contains one selected node and cursor but no range (`project.rs:254`). The later generic completion path rebuilds scope and calls `reconcile_edit_range`, which clears an old revision's selection (`preview.rs:614-704`, `preview/edit_range.rs::Selection::reconcile`).

**Preferred integration:** carry an optional exact range-selection result in the retained `CommittedEdit` receipt, with its destination parent/scope and first moved child. Use the generic once-only completion path to install it after the matching workspace and scope identity have been reconciled. Match receipt project/session/revision before consuming it; a newer workspace must not accept an older range. Keep the saved receipt available after closing the draft. Duplicate updates must not reselect after subsequent navigation. Failed refresh must preserve the old visible selection and saved/Reopen message.

The existing real delayed-delivery harness at `preview/harness/splice/edited.rs:178-240` is the direct regression template. Extend it to assert the entire moved range, not only `selected_beat`, then add a duplicate delivery after user navigation. Also test receipt-before-workspace, workspace/receipt together, failed refresh and session switch.

Resolve the first child and contiguous forest against the admitted final document on the service. Do not wrap moved roots or keep using `plan.node_duration(Prepared.node) == Prepared.range.duration()`; that check appears in both `finish_splice_preview` and `receive_splice` and assumes copied content has one imported root.

## Authority and presentation hotspots

- Add an explicit Copy/Move operation to `project::splice::Proposal`. Service ingress must check Original+Move, Replace+Move and source/current revision mismatch independently of disabled UI controls. Preserve copied-source preparation before destination failure.
- Keep the insertion destination independently from the fixed replacement interval when toggling Copy/Move/Replace. Choosing Move from Replace restores the retained insertion target; it must not reinterpret the replacement start as a newly chosen destination.
- Build one retained `Command::MoveRange` from the current source parent/refined range and explicit destination. Core preflight supplies Split IDs, clocks, result interval and no-op. Reuse that request for preview and commit.
- For an exact no-op, retain usable source endpoints and display “Already at this position; no change.” Disable native commit. Minimal typed result metadata plus this reason is enough; do not allocate an artificial wrapper or force the no-op through the existing `Prepared.node` assumption.
- `ProjectStore::preview_edit_slice` currently obtains its capture revision through `command_slice`, which admits only the three copy commands. Add a narrow Move branch using its validated source revision and the existing `prepare_command` transaction. Do not relax `view_edit_slice` capture validation or Original proposal admission.
- Existing `Snapshot::proposed_edit_slice`, `Work::EditedProposed`, `slice_view::admit_edited` and session liveness already serve an arbitrary genuinely admitted edited document. Keep their seals and cache keys. A new “Move picture” worker path is unnecessary.
- Active-site controls describe requested inspection state. The displayed picture's coordinate/caption must still come from accepted presentation, especially while switching sites or revisions with an older GPU target retained.
- `preview/splice/controls.rs` has fixed picture reservations and an unwrapped heading row. The second local timeline and operation controls need measured layout at 960x640; adding widgets without replacing those assumptions can crowd the picture. Keep logical-key routing and native button/IME ownership through `navigation/splice.rs`, update contextual help, `navigation/shortcut_audit.rs`, compatibility docs and the replay together.

## Suggested narrow write ownership

| Owner | Files and responsibility |
| --- | --- |
| Contract/service/admission | `project/splice.rs`, `project.rs` receipt type, `project/service/splice.rs`, `project/service/splice/request.rs`, a focused comparison-data helper if needed, `deadpan-store/src/slice_preview.rs`, and corresponding project/store tests. Finalize shared types first. |
| Native interaction/result application | `preview/splice.rs`, `preview/splice/controls.rs`, `preview/edit_range.rs`, the small completion section of `preview.rs`, `navigation/splice.rs`, shortcut audit/help, and `preview/harness/splice` Move replay. Own local-site mapping, receipt application order and measured layout together. |
| Media regression coverage | Existing worker/playback edited-proposal tests and new service preview-versus-commit frame/PCM tests. Start with no production decoder, DSP, worker or presentation changes; flag any necessary boundary change to the contract owner. |

Root retains gate execution, docs integration and native bundle/window ownership. The reserved Cursor QA app and Space remain untouched.
