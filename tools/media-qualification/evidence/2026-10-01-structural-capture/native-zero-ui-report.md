# Empty-group placement UI

Implemented in the shared checkout, limited to `preview/splice.rs`, its children, and a new `preview/harness/splice/structural_capture.rs` replay plus its declaration/call.

Behavior:

- An exact zero-duration Child opens Place slice with a structural source card, retained historical path/label, exact source boundary and bounded nested group names. The card says no pictures or audio are included. It scrolls within its allotted height.
- No source endpoint work is scheduled, accepted or rendered for this source. In/Out source focus returns no copied-picture work and retains the accepted destination image/caption. Ordinary zero insertion continues using the unchanged committed destination picture identity.
- In/Out refinement, Move, Replace and destination frame stepping reject zero source content with explicit reasons. `j/k` retains exact child slots even when several seams share a frame. Initial selection prefers the captured selected child's slot when its boundary matches the cursor. Footer text identifies adjacent children.
- Positive whole-Child Move remains available through the existing exact core movement path. Only zero-duration groups reject Move.
- `Prepared.empty_slot` is checked against the current zero proposal's exact slot before acceptance. Zero placement remains commit-ready without endpoint textures and has no temporal range graphic.
- An entirely zero-time destination shows an empty-edit state, schedules no frame zero, and refuses audition before constructing a playback window. A nonempty destination can still audition its real surrounding context.

Added meaningful checks:

- Pure slot test proves exact selected-empty identity among equal-time boundaries and the empty root slot.
- The production replay creates three adjacent empty groups beside the real qualified Original, captures the nested middle group through `y`, checks the card and absent endpoint state/work, rejects source refinement/Move/Replace/audio, preserves accepted destination picture/caption, steps equal-time slots, cancels unchanged, and commits one selectable zero group without a Visual range. One Undo restores all authored fields.
- A second replay segment deletes the Original, copies an empty group in the resulting zero-time document, checks truthful empty destination rendering/no picture work/no audition, commits and Undoes the structural copy, and restores the entire fixture.
- Replay capture candidates are included for the nonempty and empty destination states. No captures have been run or inspected by this agent.

Scoped Rust 1.97.1 rustfmt and `git diff --check` pass. Cargo, actual replay/UI execution and independent review remain owned by root. No compiled or runtime success is claimed here. The root owns `Captured` metadata accessors and `Prepared.empty_slot`; this implementation consumes those changes.

Review follow-up: zero-content restrictions apply only when entering Move. An existing Move always permits `m` and the enabled Copy instead button to return to Copy. The production replay exercises positive Child capture, successful range refinement/Move, restoration of original bounds at the child's current position, no-op rejection with Copy still enabled, successful return to Copy, and history-neutral cancel. Formatting and whitespace checks pass; execution remains pending with root.

Evidence correction: the initial positive whole-Child Move restriction was removed. Root traced `preflight_capture`/`selected_children` and the existing `co_located_empty_slots_reorder_ownership_without_retiming_the_bus` core test, which proves a positive range moves its child while excluding zero-duration siblings at both endpoints. The earlier restriction was unnecessary; the zero-duration Move restriction remains.

Replay fixture correction after `visual-place-first/report.json`: the first run timed out waiting for a no-op after restoring full bounds because slot 0 was before an existing empty sibling, making the operation a valid ownership reorder. `whole_child_move_toggle` now derives the Source's exact pre-edit root slot, reaches it through production `d` and `j/k` keys, and asserts the slot before refining. Restoring the full extent then targets its actual original position. The valid refined Move and unready Move-to-Copy checks remain. Only this replay file changed in the correction; scoped formatting and whitespace checks pass. No replay was run by this agent.

Replay visibility correction after `visual-place-second`: the source card was visibly rendered in screenshot 137, but `d.rect` searched only AccessKit labels while egui Label nodes exposed their text through `value`. Positive source-card and empty-destination text checks now call the existing `paint_text`, which requires actual text paint to fit its clip and viewport. Endpoint absence checks inspect both AccessKit `label` and `value` prefixes for first/last included widgets, alongside the retained empty endpoint state and source-work guards. Apply/Cancel checks remain control queries. Scoped formatting and whitespace checks pass; the replay file is frozen for root verification, with no execution by this agent.
