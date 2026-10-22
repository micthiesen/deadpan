# Native fixed-window Hold duration replay plan

Read/draft only until root baseline go. No repository edits or runtime claims.
Base ed33550d. Ownership: new scoped_plays/fixed_hold_duration.rs and optional
child modules, scoped_plays.rs registration, hold_duration/refusals.rs only.

Use the existing Driver, real keyboard/pointer widgets and project service.
Root owns Cargo, replay execution and screenshots. No browser/server/worktree.

Fixture: copy the qualified current project, then public Command transactions
remove its copied edit and insert a Retime with ordinary Sequence input:
Lead Hold2, selected RoomTone Hold8, Suffix Hold10. Read the RoomTone source
from the copied qualified Original. Register no fake media. Keep the immutable
Original profile/catalog and original baseline history. Attach one short,
qualified Original-audio BeatSound to the fixed output, independent +37 samples.
The final fixture has no Repeat, derived clock or accepted candidate.

A: fixed Edit Retime output20, input[0,20), FollowSpeed.
Selected Hold exact interval[2,10), cursor5. After every successful resize,
continuation selects the same Hold at visible start2, with same provider,
fixed authored stage output/map, unchanged output event and 20-frame output.
Enter prefill8f, submit10f; pointer Change duration submit9f; explicit command
submit10f; no-op10f changes no history or cursor. Undo/Redo compare full authored
content apart from revision. Record a semantic SetHoldDuration10f macro after
undo to8f; undo duration; call on a currently selected eligible Hold for a real
8→10 change; undo and dot for another real8→10 change. Saved macro contains
length intent only. Capture previous macro/register data rather than assuming
a register is empty. Preserve unrelated copied content and Original cursor.

B: output10, input[3,18), same20-frame body.
Hold intrinsic start maps−2/3, crop-visible exact interval[0,14/3).
Its sampled frame centers give frames0..5; an interior cursor2 must continue
at visible start0 after Enter/pointer or explicit duration commits. Growing8→10
changes visible end to6 without changing output10/input[3,18). A shrink to5
would leave body17<input end18, and must fail atomically: no new revision,
fixed map/output/provider/event/cursors/register bank/selection unchanged.
Assertions derive exact boundaries from literal geometry, not observed plan.

Delayed receipts: hold_project_updates=true, submit a real resize, wait for
service durable completion, then keyboard/pointer navigate away and back to the
same visible Hold before admitting receipt. Navigation generation changes even
when final target matches. Release updates; saved duration remains durable but
cannot reset newer cursor or inspector continuation. Repeat while recording:
instruction count stays0 while pending; navigation abandons recording, preserves
the previously saved register and cannot append the delayed intent.

Refusal update: current no-Repeat Retime→Hold case is newly eligible. Replace
only that case with Repeat→fixed Retime→Hold, choose explicit This play, assert
one real Repeat authority plus Retime ancestry, and retain explicit rejection.
Keep All plays, mixed, several-play and root-sound This play cases unchanged.

Always restore prior main workspace, Original cursor, ordinary selection,
inspector scroll baseline and feedback hold flag through existing cleanup.
Retain private package only on cleanup failure. Screenshots are root-owned.
