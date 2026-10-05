# Gag recipes

A gag is a named recipe that expands to ordinary, editable beats
(specification §8.4 and the §31 worked edit, step 8). It is not a special clip:
after applying it, every part can be changed directly, and ungrouping detaches
it from its recipe.

## Model

[`GagRecipe`](../crates/deadpan-core/src/semantic/gags.rs) is a semantic
instruction carrying its recipe version and parameters. The core planner
expands it on the staged document into ordinary instructions and finishes with
a named Group whose label pins the recipe, version and parameters, for
example `The Long Answer · v1 · pause 1500ms, creep to 1.350×`. The whole
expansion is one compound transaction and one Undo. A recipe version that is
not available refuses instead of expanding differently, so upgrading a recipe
never changes existing projects or recorded macros.

| Recipe | Expansion |
| --- | --- |
| `long-answer` (`pause=1.5s creep=1.35`) | Silent freeze pause at the cursor, a smoothstep creep from 1× to the scale on it, grouped. |
| `escalator` (`plays=3 gain-step=3dB zoom-step=0.08`) | The Visual range or selected beat as an escalating Repeat ([Repeat escalation](REPEAT_ESCALATION.md)), grouped. |
| `non-sequitur` (`register=r`) | The register's content pasted at the cursor (a hard cut to it and straight back), grouped. |
| `one-more-time` (`plays=3 gap=500ms shorten=200ms`) | The Visual range or selected beat as a Repeat whose silent freeze gaps get shorter: `gap - k·shorten` after play `k + 1`, each resolved once from its exact value. The first gap is the Repeat's default gap; later gaps are independent gap Holds ([editable Repeat gaps](REPEAT_GAP_BRANCHES.md)). Gap and shorten share a unit; a gap that would reach zero refuses. Grouped. |
| `are-we-done` (`register=r pause=1.5s`) | A freeze pause at the cursor whose sound is the reverb tail of what is heard just before it, ringing for the whole pause, with a reaction cutaway from the Original moment in the register over it (`InsertPause`, `Tail`, `SetCutaway`). Grouped. See [reversed pauses and tails](REVERSE_AND_TAILS.md). |
| `nothing-happens` (`register=r tone=1s silence=1s`) | Two silent freeze pauses at the cursor holding the same picture: the first loops room tone from the exact audio of the Original moment in the register, the second is true silence. Grouped over both pauses. |

One More Time adds the general `SetRepeat { plays, gaps, escalation }`
instruction (also `:repeat N gap= gain-step= zoom-step=`, below) and Nothing
Happens adds `SetRoomTone { register }`. Room tone is the picture selection of
the copied moment mapped through its exact audio placement, In rounded up and
Out rounded down, which equals converting measured picture PTS straight to
source samples as the native room-tone sheet does; store admission rechecks
the qualified asset and exact sample endpoints. Each gap freeze holds the last
picture of the Repeat's first play, with composition below the Repeat captured
and the Repeat's own framing and escalation left live (`PauseSite::RepeatGap`).

Recipes rest on two semantic instructions that macros can also record:
`InsertPause { length }` (frames or exact milliseconds, rounded once to the
project rate) and `SetFraming { framing }` on the selected beat. The Long
Answer refuses a boundary whose pause would land inside a nested group, since
it frames and groups that pause; open the group first. The planner
asks the host for a pause's frozen picture through a resolver
(`deadpan_cli::pause::pause_provider`, shared with the native `,h`), and
refuses a pause at a boundary that belongs to an enclosing group.

## Commands

`:gag long-answer|escalator|non-sequitur|one-more-time|nothing-happens|are-we-done
[parameters]` applies a recipe at the Edit cursor or selected beat.
`:repeat [N] gap=120ms [gap-step=-40ms] [gain-step=3dB] [zoom-step=0.08]`
changes the selected Repeat's plays, gaps and escalation together, or wraps a
plain selected beat first, as one recorded `SetRepeat` and one Undo. `gap=`
defines every gap of the Repeat: `gap=500ms` after a ladder resets all gaps to
500 ms, and `gap=0` removes them all. `gap-step` makes each later gap shorter
(`-`) or longer as its own independent gap Hold, at most 64 gaps (longer
ladders refuse). Restating the current gaps makes no edit. The inspector lists
every gap. Macro recording records the recipe instruction,
and `,h`, `:hold … video=black`, `,z`, `,c`, `:zoom` and `:creep` record as
pauses and framing instead of refusing (ranged framing is refused while recording).
Recorded framing stores the applied absolute poses and replaces the replayed
beat's framing, as the native keys do; framing a single Repeat play is not
recordable yet.

## Tests

- Core: recipe expansion, labels, version refusal and wire form; the Long
  Answer as instructions and as a gag, each one compound with an exact
  inverse; an interior pause split and millisecond rounding; framing needing a
  selected beat.
- Replay (`gags`): `:gag long-answer pause=12f creep=1.5` at frame 30 checks
  the pinned label, the framed pause, +12 frames, the message and Undo; a
  macro recording `,h` replays a pause at a new cursor.

- Core (One More Time and Nothing Happens): gap ladders with independent gap
  Holds, the shrinking-to-zero refusal, room tone from an Original moment with
  exact inward rounding, the frozen bank recording the register read, and one
  compound with an exact inverse; `SetRepeat` wraps or sets plays, gaps and
  escalation together and authors nothing when nothing changes. The
  `SetRepeatGaps` command keeps stable plays, moves only the suffix with its
  retained sampling clocks, transforms root sounds once and retires replaced
  default-gap permissions.
- Replay (`recipes`): both gags through the command line, the `:repeat` gap
  ladder and its inspector row, each with one Undo.
- Core (Are We Done?): one group holding a pause with a whole-pause reverb
  tail and a full-pause reaction cutaway, with an exact inverse.
- Export: the `one-more-time` fixture runs the recipe through the headless
  semantic path; `are-we-done` does too; `nothing-happens` exports its ordinary parts
  ([preview/export verification](PREVIEW_EXPORT_VERIFICATION.md)).

## Remaining

Saving a modified group as a local
recipe and inspecting an expansion before applying it, a gag-aware inspector
(the label is currently shown as a Sequence label), and seeded variation.
