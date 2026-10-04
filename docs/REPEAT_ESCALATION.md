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
selection the command explains that a beat must be wrapped first. `gap=` is
refused: changing a gap's duration is not supported yet. The inspector shows
"Gain per play" and "Zoom per play".

## Tests

- Core: step arithmetic, bounds, grid rounding and wire form
  (`repeat_escalation` tests); one reversible command, unchanged timing and
  refused growth (`escalation_is_one_reversible_parameter_change…`).
- Picture plan: per-play and gap scale layers inside the Repeat's framing
  (`escalation_scales_each_later_play…`).
- Audio: real decoded PCM gains per play and gap at arbitrary query
  boundaries (`repeat_escalation_adds_its_step…`).
- Replay: the `editing` scenario types the §31 command, checks the stored
  steps, the third play's 1.16 scale, unchanged duration, the inspector rows,
  the refused mismatched count and Undo.

## Remaining

Per-play speed and gap progression ("One More Time"), escalation toward a
selected target rather than the canvas center, recording escalation in macros
and dot-repeat, an `,e` recipe binding, and setting count and steps in one
undoable command.
