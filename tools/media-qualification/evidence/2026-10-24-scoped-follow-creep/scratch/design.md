# Live Follow creep in explicit Repeat-play scopes

Base fb38159bae82f2c08e7dfd96f112c24d6571b8e2.
All five ordinary owner kinds are now qualified. Extend the existing scoped
framing transaction to retain a fixed Follow during whole-owner Keep creep.
Do not invent another transaction, clock, payload or scope representation.

Use every existing validate_scoped_context guard: Your edit, no Visual or
duration selection, captured root directly under its ordinary Sequence parent,
all enclosing Repeat branches explicitly Play, exact target/presentation match,
and the target inside the captured root. Keep Default, missing, stale, mismatched,
out-of-root and Original contexts refused. Every already-valid explicit scoped
framing context is the intended surface, including nested explicit Plays and
existing supported Retime traversal. Do not add arbitrary owner-kind restrictions.
If an existing path has a real unresolved clock constraint, report the concrete
case before changing this contract.

Scale belongs to the same complete selected owner output clock already used
by scoped fixed-center creep. Target centers use the emitted provider's exact
Original SourcePoint. Use Source/Hold/Sequence/Repeat/Retime as actual owner
kinds; descendants remain unrestricted. Keep both scale bases, fallback,
quantization, target identity, timing, pitch, source/captured geometry, audio
support, unrelated plays and gap recipes intact. Existing animated paths still
require explicit replacement. Full-length Visual ranges remain ranges.

Rename the internal Rust builder marker OrdinaryFollowOwner to WholeFollowOwner
and Shot.ordinary_owner to Shot.whole_owner, because an admitted whole owner may
now be scoped. This is not a wire/schema change or new authority capability.
Ordinary call sites keep their guards; scoped call sites must validate their
captured authority before supplying the marker.

Core/CLI worker owns the builder API/tests, semantic planner and scoped tests,
the CLI unit field rename, new integration, registration and old scoped-negative
reconciliations. Plan owns a new scoped test module and registration only.
Native owns a new replay, its registrations and older Repeat/Retime replay
explicit-Play negatives. Root owns Camera integration and registry help.
Writers share the checkout, preserve concurrent changes and foreign
fuzz/Cargo.lock/.claude, and never commit, push, create worktrees or run whole-tree
writers. Scratch stays here. Keep source snapshots as .rs.txt.

Camera must use the command's captured scoped framing authority and exact
captures_scope match. Do not make its ordinary resolver accept Repeat contents.
Keep existing fixed-center, face and other scoped framing routes unchanged.
All writes still pass through the shared semantic planner and EditScoped.

Preferred decisive native witness:
1. Author point .58 at Original0, correct it to .68 at12, retain Original[0,24).
2. Wrap at2/3 Tape speed into36frames and author a fixed live Follow on the Retime.
3. Wrap three plays with4-frame gaps:116frames. Wrap two plays with8-frame gap:
   240frames,384384 canonical48k samples if actual semantics agree.
4. Choose outer Play2 and inner Play2 explicitly. Creep only that whole Retime
   from1.1to2.2. Its expected output is global[164,200); provider coordinates are
   (2j+1)/3 for local j0..35, decoded ordinal=floor, correction boundary local18.
5. Independently check all output frames, clone/address continuity, unchanged
   other play/gap views and audio support, Camera, Undo/Redo, named macro and dot
   on fresh concrete plays. Restore one clear final selected branch for export.
These numbers are a proposal until actual production commands/assertions pass.

Core/CLI tests should cover all five owner kinds and nested all-Play authority,
staged re-resolution, dry-run, exact isolation/history/registers and refusals.
Plan tests should stress fractional/retained clocks, clipping and target
fallback independently without copying production pose math.
Do not compare physical clone IDs as if isolation were forbidden: verify the
expected identity mapping separately and compare unchanged rendered/audio
semantics outside the selected owner. Preserve complete exact Undo.

Core/CLI owns the first Cargo lane; Plan waits for its release. Root owns final
integration/review/broad checks/release/native/media/evidence/progress/Git.
Retain every failed fixture and exact old source. No runtime pass is inferred
from a compiled or statically reviewed replay.
