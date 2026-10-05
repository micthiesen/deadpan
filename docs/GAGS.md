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

Recipes rest on two semantic instructions that macros can also record:
`InsertPause { length }` (frames or exact milliseconds, rounded once to the
project rate) and `SetFraming { framing }` on the selected beat. The Long
Answer refuses a boundary whose pause would land inside a nested group, since
it frames and groups that pause; open the group first. The planner
asks the host for a pause's frozen picture through a resolver
(`deadpan_cli::pause::pause_provider`, shared with the native `,h`), and
refuses a pause at a boundary that belongs to an enclosing group.

## Commands

`:gag long-answer|escalator|non-sequitur [parameters]` applies a recipe at the
Edit cursor or selected beat. Macro recording records the recipe instruction,
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

## Remaining

"One More Time" (gap progression), "Are We Done?" (cutaway with a tail),
"Nothing Happens" (room tone cut to silence), saving a modified group as a
local recipe and inspecting an expansion before applying it, a gag-aware
inspector (the label is currently shown as a Sequence label), and seeded
variation.
