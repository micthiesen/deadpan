# Repeat selections

Repeat wraps linked picture and sound in an ordinary Sequence. The result stays
structural: a whole child remains the literal child of a new Repeat; a range
becomes a neutral Sequence of retained fragments inside a Repeat. It adds one
Undo entry. Independent root sounds are moved once and are not duplicated.

## Keyboard and commands

| Input in Your edit | Result |
| --- | --- |
| `rr` / `3rr` | Wrap the selected beat in two / three total plays. |
| `rl` / `3rl` | Repeat the next frame two / three times. |
| `r3l` | Repeat the next three frames twice. |
| `rj`, `rk`, `rgg`, `rG` | Repeat from the cursor to a beat or group boundary. |
| Visual `r` / `3r` | Repeat the nonempty selected range two / three times. |
| `:wrap-repeat 3` | Wrap the captured range, or the selected beat when no range exists. |
| `:repeat 3` | Change an existing Repeat's total plays; otherwise wrap the selected beat. Clear Visual selection first. |

A leading count is total plays. A motion count is distance. Supplying both
explicit counts refuses the complete binding. Empty intervals, missing selected
beats and zero-duration children refuse without falling back to another target.
Configured motion paths, logical/physical keyboard modes and native text/IME
ownership use the shared router. Pending hints show the exact path and next keys.

The first Repeat prefix captures the project, revision, group, pane, cursor and
selection, including absence. Returning to an invalidated context does not
restore it. Commands capture their target when command entry opens. Wrapping
preserves both the register bank and a pending one-shot register choice.

Rapid explicit whole-beat wraps retain the existing bounded queue of 16 pending
intents. A checked completion must name a fresh wrapper around the exact previous
child with the requested total plays. Only a pending whole-beat terminal can
continue onto that wrapper; a motion terminal still requires its original
captured context. Navigation, cancellation, failure or another action cancels
waiting wraps without undoing an already submitted edit.

## Shared authoring path

`ProjectDocument::repeat_selection` resolves the ordinary Sequence interval and
the exact identity budget. `RepeatSelection` receives separate wrapper, optional
group and Split identities. Generic structural endpoint splitting retains
Sequence, Repeat and Retime contexts behind transparent partitions. It does not
flatten unrelated structures or discard framing owner clocks.

Before splitting, the command captures unbound audio clocks and the old suffix
entry. Existing lattices, resumes and reanchors survive. A new enclosing Repeat
does not rewrite an established intrinsic lattice; see
[owned audio bindings](OWNED_AUDIO_BINDINGS.md). Marks and existing silence
permissions gain the new wrapper's first occurrence. Those permissions do not
automatically authorize added plays. Root sound insertion occurs at the old
selection end for precisely the extra authored frames.

`SetRepeatPlays` changes the count of an ordinary Sequence child while preserving
its gap recipe and surviving stable iteration identities. It handles variable
play and gap overrides, captures suffix entries and transforms root sounds once
at the shared old/new tail. Historical `WrapRepeat` and `SetRepeat` commands keep
their previous behavior for history replay.

## Macros and dot

`SemanticInstruction::Repeat { selector, plays }` uses the shared staged planner
and exact identity allocator. Its selectors are SelectedBeat, VisualSelection,
and typed frame, beat or group-boundary motion. Recording stores the unresolved
selector and total plays; a counted named run resolves each step against its
staged context and saves authored changes as one Compound transaction.

A committed explicit wrap or single Apply proves its effective intent before
workspace refresh. Dot resolves that intent at a fresh context. A current Visual
range takes precedence; empty ranges refuse, and a saved Visual selector needs a
new range. Dot preserves register intent for Repeat. Exact retries retain their
receipt without reinstalling an older candidate. Named Run and the `:repeat`
setter do not install a repeatable edit. Count setters cannot be recorded yet.

This does not implement temporal-occurrence navigation, text or role selectors,
all Section 8 operations, or the full product acceptance gates.

See [qualification and retained evidence](qualification/repeat-operator-2026-10-03.md)
for actual PCM, picture-plan, persistence, keyboard and native checks.
