# Preserve context while admitting aligned grouping

Manual journal-scope narrowing is now refuted. Forty single-placement cases,
forty paired EQ/event ablation cases and four JSON-admitted exterior crops pass,
but two placements drop retained Top input sample 22422 and change 1,899 canonical
PCM channel samples. Production remains unchanged. This is a failed test-only
representation proposal, not a defect in an admitted production grouping edit.

Preferred implementation: explicit per-reference introduced live ordinary
Sequence IDs. Keep existing timing tables, journal scopes, historical owners,
chronology and every old context alias unchanged. A fresh group has no historical
alias in a reference captured before it existed. A later capture includes it
normally. Do not infer this relation from matching duration, shape or lineage.

Root owns shared SoundClockReference serialization/builders, all call sites,
reducer admission/token, capture/copy/rename/later-edit lifecycle, Cargo, formatter,
Git, evidence and progress artifact. The wrapper-proof worker owns scratch-only
audio_reference.rs overload and focused test drafts; the audio worker owns
scratch-only ablation/crop probes. Repository Rust stays frozen during each run.

The first admission remains aligned GroupSelection of one complete Source/Hold
child, with Source/Hold sound owners
and ordinary Sequence placement paths, zero endpoint Splits, no Repeat map or
Retime/Repeat/Partition ancestry. Prepare the complete journal update against
the exact project/revision/selector. Unsupported cases retain the existing
atomic refusal. No partial updates, owner duration changes or root-owned sound
transport become eligible incidentally.

Required lifecycle: carry group relations through later references and exact
copy renaming; narrow/filter only through an already-proven selected subtree.
Copying the new group alone must retain its LIVE journal scope. Older references
narrow the historical scope to the proven owner and retain the fresh group as an
explicit unpaired unary live-scope root; the new capture reference pairs the
genuine historical group. Do not use the superseded owner-only live-scope fallback
or invent historical group aliases. Ungroup removing a recorded wrapper stays refused
until an explicit retirement transform exists. Preserve every existing guard.

Qualification must distinguish existing Top EQ from subsequently applied EQ on
the fresh group, and must retain full PCM, raw signed input, suppression, exact
phase/clip history, durable Undo/Redo/reopen/copy and source admission. Full
scope narrowing and derived layouts that invent historical group aliases are
not the chosen implementation.

The v1 proof prototype passes seven focused tests in a separate source copy.
The v2 unary-root extension remains a draft. The reducer, planner, metadata
lifecycle and new-group mixed-history terminal support are not implemented or
verified. Keep the production guard until those obligations are complete.
