# Sequence group navigation

`Enter` opens the selected ordinary Sequence group. `Backspace` returns to its
parent and selects the group just left. The beat header shows clickable
breadcrumbs, the parent key, focus, child count and group duration. `:enter` and
`:parent` provide command equivalents. Original context remains non-destructive.

`j/k` selects direct children at the current depth. `h/l` and `gg/G` use the
current group's bounds. Card coordinates and the edit cursor remain absolute
project frames; the footer shows both group-relative and edit positions.
Entering a group clamps an outside cursor to its bounds. Returning to a parent
keeps the cursor. Empty groups remain navigable and accept whole-Original reuse.

The inspector can split, repeat, delete, resize an existing Hold and open Camera
on the current direct child. Inspector Enter retains its parameter action;
normal Enter opens a Sequence. Text fields, IME, menus, dialogs, help and Camera
keep their input ownership. Navigation clears pending operators and counts.

Deletion submits one `DeleteRipple` command and retains the sample entry of
every following audio owner, including later siblings outside the current group.
Empty groups remain deletable without changing time. One Undo restores the full
authored document. See [retained deletion clocks](AUDIO_REANCHORS.md#ripple-deletion).

## Command ownership

`SequenceScope` is an ephemeral path of direct Sequence children. It is not
authored state and introduces no core or database schema. The project service
validates its full path, writer session, document revision and direct target.
Whole-Original reuse retains its captured scope through cached and asynchronous
preparation. A committed edit restores that scope before selecting its result;
navigation while work prepares cannot silently retarget it. History reconciles
an invalid path to its nearest surviving parent without moving the cursor to a
different selected beat. Camera drafts also retain and validate their scope.

InsertTime still resolves the absolute boundary through the core. It may descend
into a deeper ordinary Sequence. The UI selects the visible child enclosing the
new Hold until that group is entered. At a viewed group's first or last boundary,
an insertion that belongs to its parent is refused with a Backspace instruction.
The user must return to the owning group before inserting there.

Space auditions the full edit from the absolute cursor, including while browsing
a group. The footer explicitly reports a cursor outside that group. An explicit
pause or terminal playback update returns to the nearest containing scope and
selects the beat at the heard position. Opening a command or stopping for an
inspector action preserves the captured target and scope instead. See
[audition](PLAYBACK.md) for sample clocks and remaining audio qualification.

## Verification and remaining work

Service tests cover exact nested extents, empty groups, rejected Repeat/Retime
descent, pause seams, direct membership, stale requests, history and cached and
asynchronous reuse. Router and selection tests cover prefixes, input ownership
and absolute coordinates. CPU layout tests draw the production breadcrumb header
at compact and large sizes, including a long path. The contributed
[UI harness](UI_FEEDBACK.md) extends `nested-pause` through navigation, Hold
duration, history, Camera and endpoint refusal using production inputs.

Repeat plays, gaps and Retime descendants still require occurrence-aware
navigation. Group creation/ungroup controls, ranges, semantic operators,
general cursor splicing and the full editing workflow remain required. This
increment does not make any full-product requirement or gate complete.
The [qualification record](qualification/group-navigation-2026-09-26.md) separates
completed checks from the environment's unavailable GPU replay.
