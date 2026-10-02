# Keyboard compatibility

Deadpan reserves Kestrel's global shortcuts. The current whole-Original reuse
binding is **`,i`**, with **`:insert`** retained. `⌘Return` belongs to Kestrel's
Ghostty launcher. Reuse runs once, rejects a count, does not repeat while held,
and does not activate inside text entry or IME composition.

The comma prefix remains usable while browsing the Original so `,i` can copy it
into Your edit. Other edit operators keep the Original non-destructive. Native
text editing retains its own selection, clipboard, undo, and redo behavior.

Catalog placement uses `,s` with no count or key-repeat activation. In Placed
sounds and its inspector, `h/l`, `j/k`, Enter, `+/-` and `dd` act on the event.
Logical Plus accepts no modifier or Shift; Minus accepts no modifier so the
physical fallback for an underscore cannot change gain. Command, Control and
Option variants remain reserved. The `sound-placement` production replay covers
pane entry, native command text and captured targets alongside this routing audit.

The room-tone sheet adds no modified global shortcuts. Its production router
uses plain Enter, Escape and Space, plus Shift+Space. Text fields keep Space;
focused buttons keep native Enter/Space activation. Tab and Shift+Tab stay
inside the modal. Composition and held activation keys cannot apply a draft.
The same router is included in the reservation audit and `room-tone` replay.

Beat gain uses the same logical `+` / `-` keys for counted 3 dB steps, with
Placed sounds taking precedence. Camera keeps its scale keys; Original and
catalog Sound focus cannot edit a retained beat. `:gain VALUE`, `:gain-mute`
and bare `:gain` use the existing native command field and its captured target.
The Gain draft adds no modified shortcut. Plain Enter/Space on its heading
apply/audition; native text fields and focused buttons retain their own input.
Escape cancels outside active composition or popup ownership. Tab and Shift+Tab
use native traversal inside the draft and reveal each newly focused control.
At the boundary, Cancel wraps forward to the heading and the heading wraps
backward to Cancel; surrounding workspace controls remain disabled. The focused
`gain` production replay passes complete populated circuits in both directions
at 960×640 and 1280×820, including actual paint and hit clipping. These injected
events do not qualify physical keyboard layouts or OS IME delivery.

Normal workspace Tab continues to cycle panes. The saved Hold inspector's gain
controls are separately checked through AccessKit Focus events with strict
paint and complete hit-target clips. Native CUA also verified gain boundary
wrapping, keyboard envelope/key editing, field reveal, literal shortcut text and
Escape cancellation without changing the saved project.

## Marks and jumps

Normal mode uses `m` then one ASCII letter to save, and `'` then one letter to
jump. Shift selects a separate uppercase letter. The logical Quote symbol may
require Shift or Option on a keyboard layout; Command and Control stay reserved.
Prefixes have no timeout, show the valid next keys and reject counts. A held
movement key cannot complete a pending mark. Native text and IME retain input.

`Ctrl O` / `Ctrl I` traverse the bounded mark-jump trail; `Cmd O` / `Cmd I`
retain Open/Import. The same Control keys work in the Marks modal, whose text
field and buttons use native Tab and activation. Composition and held activation
cannot save, remove, close or jump. `:mark a`, `:jump a`, `:unmark a`, `:marks`,
`:jump-back` and `:jump-forward` share the production router. See
[mark navigation](MARK_NAVIGATION.md) for capture, expiration and clock rules.
The marks qualification passed 16,368 cases over the same 62 Kestrel globals.

## Source Slip preview

**`:slip +5f`** opens a stopped-picture draft for the selected eligible Source or
neutral Partition in Your edit. Its captured target includes missing/ineligible
entry context; later selection cannot supply one. Clear any Visual Edit selection
first, including an empty range. The amount is one signed or unsigned whole-frame
integer with an ASCII `f` suffix.

On the preview heading/background, `h/l` changes material by one project frame
and Shift changes ten. Left/Right inspects one delivered picture, Shift ten;
`i/o` inspects first/last and `b` compares Before/Proposed at the same Edit frame.
Enter applies once only after the exact current Proposed picture is submitted
at the current viewer size; Escape cancels. Before, zero movement, pending
pictures and failures cannot apply. The actual Edit/Original cursors and selected
beat stay fixed during inspection. A nudge away from a clamp starts at its applied
handle, and batched movement keys retain every step.

Tab and Shift+Tab use native control traversal. Amount text, focused buttons and
IME retain their input; Enter in the amount field does not apply. Composition
owns Enter/Escape. Held movement may repeat, while Apply, Cancel, `b` and `i/o`
do not. Command, Control and Option variants remain reserved. The modal excludes
ordinary editing, transport and Render. It adds no modified global shortcut.
`:slip` remains a separate stopped-picture preview; `,v` opens the combined
Trim mode described below.

The production `slip` replay passes the exact picture gate, clamps,
comparison, batched input, text/IME focus, late replies, cancel, one commit and
Undo/Redo. All 17,360 reservation cases over the same 62 globals pass, with no
conflicts or live Kestrel registry drift. Rendered and native evidence is retained
in [native Slip qualification](qualification/native-slip-2026-10-01.md).
See [Source Slip](SOURCE_SLIP.md#native-stopped-picture-preview) for the full
capture, picture and saved-receipt contract.

## Combined Trim

**`,v`** or bare **`:trim`** opens the selected eligible Source or neutral unity
Source Partition in an ordinary Sequence in Your edit. Clear any active or
retained Edit Visual range first, including an empty range. Original, catalog
Sounds and Placed sounds cannot open Trim. The target and literal right neighbor
are captured at prefix/command entry before audition stops; late replies cannot
replace that context. Roll requires an eligible literal right neighbor.

The comma hint lists `v Trim`. Activation rejects counts and held repeats;
plain `v` retains its Visual selection behavior. Bare `:trim` starts on In with
four zero amounts and Ripple policy. The parameter form is:

```text
:trim edge=out delta=-3f mode=ripple
```

Supply `edge=in|out|slip|roll`, `delta=<whole frames>f` and
`mode=ripple|overwrite` exactly once each, in any order. Amounts accept an
optional ASCII sign, as in `-3f`, `+5f` or `7f`; the other three values start at
zero. Missing, duplicate or unknown arguments are rejected.

On the Trim heading/background, Tab cycles In → Out → Slip → Roll; Shift-Tab
reverses. Switching controls preserves every amount. `h/l` changes the active
value by −1/+1 project frame, or −10/+10 with Shift. `r` toggles the complete
draft's Ripple/Overwrite policy, preserving all values or reporting a refusal.
`i/o` selects In/Out; inside Slip it chooses the inspected edge without leaving
Slip. `b` compares Before/Proposed. `e` focuses the native amount field. Space
auditions, pauses or resumes, and Shift-Space restarts the junction's context
loop. The outgoing/incoming pair stays fixed during audio.

Enter applies one nonzero edit only after all input is acknowledged and the
current Proposed pair has been submitted at the current viewer size. Escape
restores entry context before saving starts. Only `h/l` repeats while held;
Tab, compare, policy, inspection, transport and activation keys require a fresh
press. Trim has no count prefix. Native fields and buttons keep Tab and
activation. Enter in the amount field accepts text and returns to Trim controls
without applying on the same event. Plain Escape cancels from a field or button
outside composition; IME retains Enter/Escape. Command, Control and Option
chords remain unclaimed. Trim excludes ordinary editing, history, other editing
drafts and Render. See [the native contract](COMBINED_TRIM.md#native-trim).

The production reservation audit includes Trim's text, background, composition
and held-key combinations. Its current expected coverage is 296 routing cases
for each of 62 reservations, or 18,352 total. This is an expected count, not a
passing-run claim. The help update checked source and fixture hashes only;
compilation, audit execution, replay and native interaction remain separate
qualification work. The local Kestrel source still matches the checked fixture:
SHA-256 `368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.

## Place slice

**`:splice`** opens a local linked Original or Edit placement draft in Your edit.
`i/o` selects included In/exclusive Out, `d` selects the destination, and `f`
inspects its picture. `h/l` or Left/Right adjusts frames with counts; `j/k`
chooses a Sequence slot. `b` compares Before/Proposed, Space auditions or pauses,
and Shift+Space loops both joins. Enter commits once; Escape cancels. The draft
keeps the copied range and saved editor cursors intact until commit.

For a fresh edited copy, `m` selects Move or returns to Copy. `s` inspects its
removal site and `f` its insertion site; Shift+Space loops only the selected
site's bounded context. Site inspection does not change the prepared command.
`r` leaves Move and always uses Copy for replacement. The insertion destination
is retained independently through operation switches. Plain `m/s` follow the
same native focus, repeat and IME ownership rules as the other draft keys.
The [native Move qualification](qualification/native-move-2026-09-30.md) retains
the passing 11,904-case routing audit, replayed held-key/focus/composition cases
and separate native keyboard check. Local Kestrel source SHA-256 is
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.

In Your edit, `v` starts a temporal range and a second `v` finishes it; Escape
clears it. `y` copies a nonempty range from either Original or Your edit; without
an Edit selection it copies the exact selected beat, including an empty group.
Visual `d`, whole-beat `dd` and captured `:delete` update the session register
only after saving the cut. A failed cut keeps the previous copy. No shortcut
chords change. Empty groups paste at explicit slots; `j/k` retains equal-time
sibling ordering in Place slice. Their In/Out, Replace and Move controls explain
why time-based operations are unavailable. An Edit
copy retains its source revision through later edits and Undo. The range stays
independent of the copied register. In Place slice,
`r` explicitly toggles **Replace selection** for the captured range. In/Out
refinement changes the inserted source; the removed range stays fixed. `d/j/k`
therefore leave replacement's destination unchanged. Fast `p/P` replaces a
selected Edit range in one command. Text fields, IME and focused native buttons
retain ownership of `r` as they do the other draft keys.

These keys add no modified global shortcut. The production router preserves
composition and focused native buttons. The `place-slice` replay checks exact
endpoint/destination pictures, both joins, loop pause/resume, stale captures,
cancel/commit/undo and actual picture/text clips at 960×640 and 1280×820.
Linked insertion at ordinary Sequence seams and direct Source/Hold interiors
is available; counted `j/k` also works from an interior. The remaining
[slice placement modes](SLICE_PLACEMENT.md) are still required.

## Visual deletion

In Your edit, plain `d` cuts the active or finished nonempty Visual range once.
An empty selection produces guidance and never falls back to whole-beat deletion.
Without a selection, `dd` retains its whole-beat meaning. Counts and held `d`
activation cannot repeat a range cut. Native text, composition and focused
controls retain their input. Original, catalog sounds and placed sounds retain
their separate routes.

`:delete` captures the range or beat when command entry opens, including an
empty or missing target. Later selection or cursor changes cannot supply a
different target; changed sessions, revisions or groups reject it. The command
footer shows the captured interval and linked picture/sound scope.

The `delete-range` production replay checks both selection directions, finished
and empty selections, captured targets, native focus, synthetic IME, exact
decoded joins, minimum-window paint clips and one-transaction Undo. These checks
do not qualify physical IME or keyboard layouts.

## Render

**`⌘E`** and **`:render`** open the native Render flow for the full committed
edit. The shortcut uses exactly Command+E, clears pending prefixes, and does
not repeat while held. Command+Shift+E, Command+Option+E, Control+Command+E and
plain E do not dispatch Render. Native text fields and IME composition retain
their input. The Render button remains available when a preview owns a field;
the final field text is consumed before the preview proposal is captured.

An unsaved Camera, Gain or Room tone preview opens a decision with **Commit
preview and render**, **Discard preview and render**, and **Keep editing**.
Escape chooses Keep editing outside active composition. Tab navigation and
native button activation remain available. Cancelling the destination picker
preserves the preview and creates no render job. Choosing Commit for a changed
preview saves one typed edit, then starts Render from that exact commit receipt. A later render
admission failure still reports the saved revision. A changed project session
or revision rejects the captured choice instead of retargeting it.

The production route lives in `Bindings::key` and the native command parser;
the Camera, Gain and Room tone routers preserve their text and composition
rules around it. The reservation audit therefore exercises the same route.
The local Kestrel `Shortcuts.swift` was inspected for this change: its SHA-256
is `368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`, matching
the checked fixture. It reserves Option+E for Ungroup and has no Command+E
reservation. This source comparison is not a new routing-test or physical
keyboard qualification result. The `render` production replay covers the
shortcut, command entry, preview decisions, stale captures and export flow;
its current run results are recorded separately.

**`:renders`** and the **Renders** header control open saved render history.
Native Tab / Shift Tab moves between controls, Enter activates, and Escape
returns to the editor outside composition. The browser owns ordinary editing
keys while open. It pages through saved edits, attempts and destinations on the
project owner thread. Choosing a recovery action captures the historical job,
encoding attempt or destination before opening any picker. Later edits preserve
that historical target; changing projects rejects it. Cancelling a picker
returns to the browser without creating an attempt or committing a preview.

Automatic Render uses the current SDR picture and audio path and rejects
unsupported content. Native and headless recovery support retained-movie retry,
fresh encoding of a saved edit and destination reconciliation, including through
an already-open native owner. Full mastering and HDR output remain open.
See [headless Render](HEADLESS.md#automatic-render).

## Automated audit

The UI feedback runner includes a shortcut audit. The standalone routing tests
also run without a native window:

```sh
cargo test --locked -p deadpan-app navigation::
```

[The audit](../crates/deadpan-app/src/navigation/shortcut_audit.rs) sends each of
the 62 exact global Kestrel bindings through the actual `Bindings::key`, Camera,
text-action, inspector, room-tone, Gain, Marks, Place slice, Slip and Trim routers,
including `Bindings::key_with_selection` for empty and nonempty Edit selections.
The prior Slip integration passed 280 routing cases per reservation, or 17,360 total,
including pending prefixes, counts and overflow, text/IME and repeat/focus
combinations. Its execution and matching live-source digest are retained in the
[native Slip record](qualification/native-slip-2026-10-01.md).
The added Trim cases bring the expected count to 18,352; see the separate
[Trim qualification status](#combined-trim) above.
The earlier corrected workspace, room-tone, gain and retime runs each passed all 5,456
cases with no conflicts or live-source drift; all 298 app/harness tests also
pass. The [native-gain qualification](qualification/native-gain-2026-09-28.md) retains
source identities, native CUA evidence and remaining physical-input limits.
A reservation fails if it dispatches an action or leaves an editor prefix pending.
This catches shortcuts that seem harmless because they only start a count.

The checked [reservation fixture](../crates/deadpan-app/src/navigation/kestrel-reserved.tsv)
comes from evaluating Kestrel's `ShortcutRegistry.definitions`, including loop
expansion, exact modifier masks, and app scopes. There is no second application
keymap to keep synchronized: the audit calls the same routers used by the UI.
Positive-control tests demonstrate failures for the former `⌘Return` insertion
and Option-number count behavior.

The runner can compare a supplied local `Shortcuts.swift` with the fixture's
SHA-256. A changed source fails the check as **registry drift**, with both hashes
in the report. It does not infer that new Swift definitions are safe. Without a
live source, the report covers the checked registry snapshot only. See
[the UI feedback workflow](UI_FEEDBACK.md) for runner arguments and report files.

## Reserved global keys

Option is `⌥`, Command is `⌘`, and Hyper is Control+Option+Shift+Command. Modifier
combinations are exact; for example Kestrel's Option+H does not reserve plain H.

| Keys | Kestrel use |
|---|---|
| `⌥H/J/K/L` | Window focus |
| `⇧⌥H/J/K/L` | Swap tiles |
| `⌃⌥H/J/K/L` | Move tiles/groups |
| `Hyper H/J/K/L` | Resize split |
| `⌥N/P`, `⌥Space` | Cycle windows or tiled/floating focus |
| `⌥1–5`, `⇧⌥1–5` | Switch Desktop; send and follow |
| `Hyper 1–5`, `Hyper 0` | Set layout share; balance |
| `⌘⌥H/L`, `⇧⌘⌥H/L` | Relative Desktop; send and follow |
| `⌥\`, `⌥=`, `⌥A/D/T/S/E` | Layout structure |
| `Hyper Backspace/T/F/R/D/A` | Layout undo and window actions |
| `⇧⌥Space` | Toggle floating |
| `⌘Return` | Raise Ghostty |
| `⌃⌥Return`, `⌃⌘Return`, `⌥⌘Return`, `⇧⌥Return` | Communications, Mail, Codex, Calendar |
| `Hyper I/N/U`, `⌃⌘W` | Kestrel help, Notes, update, display sleep |

Kestrel's `⌘N` entry is scoped to Ghostty, so Deadpan retains its native New
shortcut. Unknown app scopes or physical keycodes in a refreshed fixture fail
validation until reviewed. Option-number inputs cannot become Deadpan counts;
Shift-only logical digits still work on keyboard layouts that require Shift.
Logical punctuation such as `,`, `:`, and `?` retains layout-aware routing.

Kestrel intercepts physical key positions, while egui routes logical keys. This
audit uses the registry's ANSI positions and verifies Deadpan's input semantics;
it does not prove OS event delivery for every input source. Physical non-US
layout delivery, real IME events, and global event-tap behavior still require
the small native acceptance check. Changing to physical shortcut routing inside
Deadpan would break its existing logical-symbol contract.

## Refresh after a Kestrel change

Use the explicit path to a trusted local Kestrel `Sources/Kestrel/Shortcuts.swift`:

```sh
python3 crates/deadpan-app/src/navigation/export-kestrel-reservations.py \
  /path/to/kestrel/Sources/Kestrel/Shortcuts.swift \
  > /tmp/deadpan-kestrel-reserved-new.tsv
diff -u crates/deadpan-app/src/navigation/kestrel-reserved.tsv \
  /tmp/deadpan-kestrel-reserved-new.tsv
```

The exporter evaluates that Swift source with data-only dependency shims, using
the macOS developer Swift toolchain. Run it only on trusted registry source. It
does not start Kestrel, install bindings, or modify dotfiles. The Rust audit
itself only hashes the supplied source and never executes it.

Review the changed definitions, replace the fixture, update the expected binding
count if needed, and rerun the audit. Unknown source grammar is not parsed or
silently skipped; a registry API change makes the exporter fail until its shims
are deliberately updated. Exporter tooling is for development and is not an
end-user application requirement.

The 2026-09-27 refresh reviewed Kestrel commit
`dc9245c09db3c101a2419c364174449b6e4415ae`. It changed only the Ghostty New-window
help description. Re-evaluating the live registry produced identical reservation
rows; only the source digest changed. The failed pre-refresh audit remains in the
[moment-paste qualification](qualification/moment-paste-2026-09-27.md).

## Interaction direction

Keep plain Vim motions and the comma leader for editor actions. Put common keys
beside their controls and show valid next keys while a prefix is pending. Use
native Command shortcuts for lifecycle and history where Kestrel leaves them
available. Preserve a clear focused-pane cue separately from selected content.

Original Visual selection and range reuse now have the `original-moment` replay;
its current native/visual evidence limits remain explicit in qualification.
General Visual replacement and targets still need their own replay scenarios,
visible selection scope, reversible previews, and measured feedback. Trim's
implemented keys and controls are described above; their presence does not
establish replay or native qualification. Specification bindings alone do not
establish implemented capabilities. Any new modifier binding must pass this
audit and its real interaction scenario before being added to help or keycaps.
