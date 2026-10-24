# Whole-Hold native harness handoff

Ownership: new preview/harness/hold_follow_creep.rs and one registration line
in preview/harness.rs, preview/harness/scenarios.rs, ui_harness.rs. All four
paths frozen in native-harness-freeze.json. Root owns compilation/runtime.
No Cargo, product runtime, Git write, fixture mutation or other source edit.

Scenario hold-follow-creep starts a fresh ordinary counter project. It creates
a center-only point at source(.58,.5), applies Source zoom2 following that point,
then inserts a real12-frame pause at Edit40. It checks the actual ordinary Hold,
retained CapturedFraming, Freeze of indexed Original39 and total132.

The Hold's second point is authored at source(.68,.5), so captured2x/.58
geometry must produce live(.7,.5), distinct from fallback(.5,.5). It checks
Camera before framing, committed plan, separate target/framing, then a whole
Hold creep1.1->2.2 linear (factor2). It verifies every local frame using an
independent integer half-even Q32 oracle for scale and grid-quantized center,
exact local frame centers/duration, Freeze timestamp, retained picture context,
unchanged recipe/other nodes/both cursors/total132.

Native recording stores Zoom Keep intent. Undo/Redo, unchanged Camera Enter,
Cancel after a scale adjustment, named macro replay and direct Apply->Undo->dot
must preserve the exact animated recipe. The clipping negative corrects the
point from.68 to.98 at its first source moment, then checks fallback(.5,.5)
with scale animation retained in Camera and plan. Undo restores actual live.7
and ends with the resolved Follow for Root's media export.

Only formatting was executed (exit0). No behavioral pass is claimed.
First native setup may reveal UI selection/rounding assumptions; it refuses
with actual state rather than forcing fixture state. If pause insertion did
not select the actual Hold, it uses one production J then asserts the selection.
