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
| `,v` | Trim mode. |
| `+/-` | Change selected/current beat's audio gain by ±3 dB. |

## Camera and trim

Camera: numbered target selection; `h/j/k/l` move center 1%; uppercase moves 5%; `+/-` change scale by a 1.05 multiplier; `f` picks a target; `r` resets preview. Enter commits; Escape cancels.

Trim: Tab cycles in/out/slip/roll; `h/l` adjusts frames; Shift adjusts ten frames; `r` toggles applicable ripple/overwrite policy. Enter commits; Escape cancels.

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
:render
```

`:hold` inserts time; `:hold-provider` changes an existing Hold. `:repeat` sets parameters on an already selected Repeat; explicit `wrap-repeat` creates intentional nesting. AI completion only creates a candidate: audition and accept to change the committed picture.

`:` opens commands; `?` opens searchable help; `Cmd-E` renders with automatic output settings. Text fields retain native text-editing behavior; Normal-mode shortcuts do not intercept typed captions, filenames, or IME composition.
