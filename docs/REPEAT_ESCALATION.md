# Repeat escalation

An escalating Repeat makes each play louder and closer than the one before,
as one editable structure (specification §8.1 "Escalation", §8.2 "Escalating
crop" and the §31 worked edit, step 3). The plays are not copied: the Repeat
stores two steps and every consumer derives each play's values from them.

## Model

`NodeKind::Repeat` has an optional
[`RepeatEscalation`](../crates/deadpan-core/src/repeat_escalation.rs):

| Field | Meaning |
| --- | --- |
| `gain_step` | Exact millidecibels added per play after the first. |
| `zoom.step` | Centered picture scale per play after the first, on the 2⁻³² framing grid. |
| `zoom.progression` | `add` (default): play `k` has scale `1 + k · step`. `multiply`: `step^k`. |
| `zoom.target` | Optional saved attention target the zoom moves toward; absent means the picture center. |

Play `k` is the 0-based position in the Repeat's current play order, so the
first play is always unchanged and reordering plays reorders the progression.
A configured gap belongs to the play before it and shares that play's values.
Document validation checks the last play: its gain must stay within
-96 dB..=+24 dB and its scale within 1/64..=64, so growing the play count past
those bounds is refused with the escalation named in the error. Escalation is
part of the Repeat node, so copies, deletion, occurrence isolation and history
carry it like any other node field.

`SetRepeatEscalation { node, escalation }` replaces it as one reversible edit.
It changes no timing, play, gap or retained clock and preserves sound clocks.
The field is additive within core schema 46; earlier binaries reject it as an
unknown field rather than ignoring it.

## Picture and sound

The picture plan adds one centered scale layer for plays after the first,
inside the Repeat's own framing and outside its contents' framing
([`RenderPlan::picture`](../crates/deadpan-plan/src/plan.rs)). The layer is
marked `escalation`; Camera never selects it, while a pause captured inside the
Repeat keeps it in its frozen view.

With `zoom.target`, each later play's layer centers on where that target is at
the picture's source time, seen through the contents' framing, exactly as a
Follow does. Unlike a Follow it keeps the view inside the picture: at scale `s`
both centers are clamped to `[1/(2s), 1 - 1/(2s)]` before the one quantization
to the framing grid, so a subject near the edge pulls the zoom toward it without
showing background. Where the picture is not the target's asset, outside the
target's span, or below a pause's captured geometry, the layer stays centered.
A scale below 1 is not clamped. Document validation refuses a target the
project does not have, so deleting a target needs the escalation changed first.
Explode refuses an escalation with a target, because per-play static framing
cannot express a moving subject and a Follow does not clamp.

Audio owner spans record the Repeat's play
([`AudioOwnerClock::escalation_millidecibels`](../crates/deadpan-plan/src/audio_owners.rs)),
and the authored bus adds the step like an authored gain factor before mixing
and the limiter, for the Original and for beat-owned sounds
([`authored_gain`](../crates/deadpan-audio/src/authored_gain.rs)). Preview and
export share both paths.

## Commands

With a Repeat selected, `:repeat 3 gain-step=3dB zoom-step=0.08` sets both
steps; `progression=multiply` compounds the zoom. Omitted steps keep their
values, `gain-step=0dB` or `zoom-step=0` removes one, and a decimal zoom step
is rounded once to the framing grid. The count, when given, must match the
Repeat's plays: changing the count and the steps together would need two
undo steps, so a different count is refused with guidance. On a non-Repeat
selection the command explains that a beat must be wrapped first. `gap=120ms`
sets every gap, `gap=0` removes them all, and `gap-step=-40ms` makes each later
gap shorter (see `crates/deadpan-app/src/navigation/escalation.rs`). The whole
command is one semantic `SetRepeat { plays, gaps, escalation }`, so count, gaps
and steps change in one undoable, macro-recorded step ([gags](GAGS.md)).
`zoom-target=current` aims the zoom at the saved target on the displayed
picture (one target must cover it, as for `:zoom target=current`), resolved
once at command entry and recorded as that target's id; `zoom-target=center`
returns it to the middle. A zoom-step change keeps the current target. The
inspector shows "Gain per play", "Zoom per play" and "Zoom toward".

`,e` applies the escalating-repeat recipe: three plays over the Visual range or
the selected beat, +3 dB and +0.08 scale per play, through the semantic
`Repeat` instruction's optional `escalation`, so the wrap and its escalation
are one Undo, macros record it and dot-repeat reapplies it.

## Tests

- Core: step arithmetic, bounds, grid rounding and wire form
  (`repeat_escalation` tests); one reversible command, unchanged timing and
  refused growth (`escalation_is_one_reversible_parameter_change…`).
- Picture plan: per-play and gap scale layers inside the Repeat's framing
  (`escalation_scales_each_later_play…`), and a target zoom clamped inside the
  picture at 2× and free at 3× (`an_escalating_zoom_moves_toward_its_target…`).
- App: `zoom-target=` parsing, merging that keeps the target through a step
  change, and the inspector row (`unnamed_parameters_keep_their_current_values`).
- Audio: real decoded PCM gains per play and gap at arbitrary query
  boundaries (`repeat_escalation_adds_its_step…`).
- Replay: the `editing` scenario types the §31 command, checks the stored
  steps, the third play's 1.16 scale, unchanged duration, the inspector rows,
  the refused mismatched count and Undo.

## Remaining

Per-play speed progression, exploding a target escalation, and choosing among
several targets on one picture by name from `:repeat` (use Camera's picker to
make one target current). Gap progression, macro recording and setting count,
gaps and steps in one undoable command are implemented through `SetRepeat`.
