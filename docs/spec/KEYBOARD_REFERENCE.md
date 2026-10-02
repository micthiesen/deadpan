# Deadpan — proposed keyboard reference

This is the specified default keymap, not a claim that an application has been implemented. Full scope and edge semantics are in `DEADPAN_SPEC.md`.

V1 starts with one full Original video already on the timeline. **Original** is
the non-destructive Source context; **Your edit** is Sequence context. Reuse
moments from that same video and add external audio-only effects. New native
projects live in Documents/Deadpan. Undo stops at the full-original baseline.
Common actions show their keys in the UI; pending input shows exact prefix and
valid next keys, with focused pane distinct from selected content.

## Navigate

| Key | Action |
|---|---|
| `h/l` | Back/forward one frame; counts accepted. |
| `j/k` | Next/previous beat at the current structural depth. |
| `w/b/e` | Next word / previous word / word end. |
| `W/B` | Next/previous sentence. |
| `]c/[c` | Next/previous edit boundary. |
| `]s/[s` | Next/previous source shot. |
| `]p/[p` | Next/previous silence interval. |
| `gg/G` | Start/end. |
| `/`, `n/N` | Search; next/previous match. |
| `m` + letter, `'` + letter | Set/jump to mark. |
| `Space` | Play/pause. |
| `Shift-Space` | Audition-loop selection with context. |
| `Enter/Backspace` | Drill into group / return to parent. |
| `Tab/Shift-Tab` | Cycle visible panes; focus remains distinct from selection. |

## Select and edit

`v` starts a time selection. Operators accept objects or motions: `d` delete, `y` yank, `r` repeat. `dd/yy/rr` act on a whole beat. `3riw` repeats a word **three times total**, not four.

Objects: `iw/aw` word; `is/as` sentence; `ip/ap` pause; `ib/ab` beat; `ig/ag` group; `iS/aS` shot. `i` is tight; `a` includes defined handles or owned attachments. Audio-only sample edits are explicitly selected through `:select role=audio`.

`p/P` paste after/before a beat; replace a Visual selection. `s` splits. `x` deletes one frame. `u` undoes; `Ctrl-r` redoes. `.` repeats the last semantic edit. `"` plus a letter selects a register. `q` plus a letter records a macro; `q` stops; `@` plus a letter runs it.

In Original context, mark or yank a moment for reuse; `d` and `r` never change
original bytes or create a replacement timeline. Return to Your edit to change
the existing sequence. Picture registers and cutaways refer to the same Original;
external media contributes sound only.

## Shape the moment

| Key | Action |
|---|---|
| `,h` | Insert 0.5 seconds of freeze + silence. |
| `,i` | Reuse the full Original after the selected beat (`:insert`). |
| `,s` | Place the selected catalog sound at the retained Edit cursor (`:sound-place`). |
| `3,h` | Insert 1.5 seconds. |
| `,a` | Insert same hold and request an AI candidate. |
| `,z` | 1.35× punch-in. |
| `,c` | Creep toward 1.35× over selection. |
| `,m` | Mute selected sound. |
| `,r` | Reaction cutaway picker using moments from the Original. |
| `,e` | Escalating-repeat recipe. |
| `,b` | Bleep selected interval. |
| `,t` | Reverb tail. |
| `,g` | Group as gag. |
| `,f` | Camera mode. |
| `,v` | Open Trim for an eligible Source beat or neutral Source fragment in Your edit; no count or held activation. |
| `+/-` | Change selected/current beat's audio gain by ±3 dB. |

## Placed sounds

Placed sounds: `:sounds` focuses the event list; `j/k` select, `h/l` move by exact
project frames, Enter opens a fine 48 kHz sample position, `+/-` change gain by
3 dB, and `dd` removes the event. These keys retain event scope in its inspector.
Catalog sound selection and the picture beat remain separate. Commands include
`:sound-at 137`, `:sound-gain -3`, `:sound-edges soft|hard` and `:sound-delete`.
At an explicitly identified silent pause, `:sound-allow` permits this sound
through that occurrence; `:sound-silence` restores its suppression. The inspector
shows the retained Edit cursor and exact issuer. Command entry captures that scope.
Placement never changes picture duration; unsupported overflow or routed movement
reports a reason without changing history. This is the current root-event subset;
full host ownership, send/tail policies and creative treatments remain required.
Use explicit `:sound-delete` in command entry; `:delete` remains a picture-beat
command and is rejected in sound context.

## Camera and trim

Camera: numbered target selection; `h/j/k/l` move center 1%; uppercase moves 5%; `+/-` change scale by a 1.05 multiplier; `f` picks a target; `r` resets preview. Enter commits; Escape cancels.

### Native Trim

`,v` or `:trim` opens the selected eligible Source or neutral unity Source
Partition in an ordinary Sequence in Your edit. Clear active and retained Edit
Visual ranges first, including an empty range. Original, catalog Sounds and
Placed sounds cannot open Trim. Bare `:trim` starts on In with four zero amounts
and Ripple policy. The separate `:slip +5f` preview remains supported.

On the Trim heading/background:

| Key | Action |
| --- | --- |
| `Tab` / `Shift-Tab` | Cycle In, Out, Slip and Roll forward/backward without clearing values. |
| `h/l` | Adjust the active amount by −1/+1 project frame; Shift changes −10/+10. |
| `r` | Toggle Ripple/Overwrite for all values, or show why the change is refused. |
| `i/o` | Select In/Out; within Slip choose its inspected edge and keep Slip active. |
| `b` | Compare Before/Proposed. |
| `e` | Focus the native whole-frame amount field. |
| `Space` | Audition, pause or resume the junction context. |
| `Shift-Space` | Restart a loop of that context. |
| `Enter` | Apply once after accepted input and the current nonzero Proposed pair are ready. |
| `Escape` | Cancel and restore entry context before saving starts. |

Only `h/l` repeats while held; Trim has no counts. Native fields and buttons
keep Tab and activation. Field Enter accepts text without applying on that event;
IME retains Enter/Escape. Outside composition, plain Escape cancels even from a
field or button. Command, Control and Option chords remain reserved. Roll needs
an eligible literal right neighbor. Finish or cancel Trim before editing, history
or Render. See [the native contract](../COMBINED_TRIM.md#native-trim) and
[qualification status](../KEYBINDING_COMPATIBILITY.md#combined-trim).

Tab through native controls to reach feedback. While its focus ring is visible,
Up/Down scroll, Page Up/Down page, and Home/End reach its ends without changing
the draft. Tab leaves feedback and Escape cancels Trim.

## Commands

```text
:hold 1.5s video=ai audio=silence
:hold-provider ai
:repeat 3 gap=120ms gain-step=3dB zoom-step=0.08
:zoom 1.35 target=face:2 curve=step
:creep from=1 to=1.4 target=current
:retime 0.75 pitch=preserve
:cutaway register=r audio=keep
:gain +6dB
:trim
:trim edge=out delta=-3f mode=ripple
:render
```

`:hold` inserts time; `:hold-provider` changes an existing Hold. `:repeat` sets parameters on an already selected Repeat; explicit `wrap-repeat` creates intentional nesting. AI completion only creates a candidate: audition and accept to change the committed picture.

The parameterized `:trim` requires `edge=in|out|slip|roll`,
`delta=<whole frames>f` and `mode=ripple|overwrite` exactly once each, in any
order. Only the selected amount is initialized; the other three remain zero.
Signed or unsigned ASCII integers such as `-3f`, `+5f` and `7f` are accepted.
Missing, duplicate or unknown arguments fail.

`:` opens commands; `?` opens searchable help; `Cmd-E` renders with automatic output settings. Text fields retain native text-editing behavior; Normal-mode shortcuts do not intercept typed captions, filenames, or IME composition.
