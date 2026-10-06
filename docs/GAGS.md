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

## Changing parameters after insertion

The pinned group label is the stored form of an inserted gag's recipe:
`GagRecipe::from_label` parses it back and accepts it only when the recipe
writes exactly that label again, so a renamed group, an unknown version or an
edited label names no recipe. Any group whose label is exactly such a label is
treated as that gag, even if the label was typed by hand; its parts must still
match (below). Lengths, plays, gains, seeds and registers round-trip exactly.
The label writes a creep scale and a zoom step to three decimals: values with
at most three decimals (the `:gag` defaults and typical input) come back
exactly, a finer value as written there, and a zoom step is re-quantized to
the framing grid it was stored on. Selecting such a group shows it in the inspector as a
**Gag** with its recipe, version and parameters, and **Change parameters…**
(Enter in the inspector) opens `:gag-set` pre-filled with the exact current
values.

`:gag-set key=value …` takes the `:gag` grammar for that recipe; parameters not
given keep their pinned values (`seed=` alone keeps the variation percentage).
The gag is the one selected when `:` opened; a selection change before Enter
refuses. It is one semantic `SetGag { recipe, parameters }` instruction and one
Undo, naming only the changed parameters, so `.` and macros change just those
on another gag of the same recipe and keep its other values. The planner
enters the group and first checks every part is exactly what the recipe made:
the pause lengths, the Long Answer's creep, the Repeat's plays, default gap,
each later gap Hold and escalation, room tone then silence, the reaction tail's
kind and length and the single whole-pause reaction cutaway, with no framing,
gain, captions or cutaways added by hand. Any difference refuses; edit the
parts directly then. It then edits each part whose parameter changed through
the ordinary leaves and renames the group to the new label.

| Recipe | Parameter edits |
| --- | --- |
| The Long Answer | `pause` sets the pause length (`SetHoldDuration`); `creep` replaces its creep with the new end scale (`SetFraming`). |
| The Escalator | `plays`, `gain-step` and `zoom-step` set the Repeat's plays and escalation (`SetRepeat`). |
| One More Time | `plays`, `gap`, `shorten` and `vary`/`seed` set plays and the complete recomputed gap ladder (`SetRepeat`). |
| Nothing Happens | `tone` and `silence` set the two pause lengths; `register` replaces the room tone from the new Original moment (`SetRoomTone`). |
| Are We Done? | `pause` sets the pause length and rings the reverb tail through it; the reaction cutaway is cleared and placed again over the whole pause from `register` (or the same register). |
| The Non-Sequitur | Refused: its register content is already pasted; paste the new content instead. |

A different recipe's parameters, unchanged parameters, a group that is not an
inserted gag and a gag whose parts were changed by hand (another part count or
kind) refuse without an edit; edit the parts directly then. Macros record
`SetGag`, and `.` sets the same complete parameters on another inserted gag of
the same recipe. Evidence: `every_recipe_label_parses_back_to_exactly_its_recipe`,
`setting_a_long_answers_parameters_edits_its_pause_and_creep_and_relabels_it`
(the edited creep equals a fresh insertion's), `setting_one_more_time_changes_its_plays_and_gap_ladder`,
`a_gag_set_refuses_other_recipes_unchanged_values_and_reshaped_parts`, the
Are We Done? case in `are_we_done_hangs_a_reverb_tail_under_a_reaction_cutaway_in_one_group`
(each one compound with an exact inverse),
`gag_set_changes_only_the_given_parameters_and_prefills_exact_values`,
`an_inserted_gag_shows_its_recipe_and_offers_its_exact_parameters`, and the
`creative-dot` replay (the inspector's Gag row, `:gag-set plays=4` on one
One More Time and `.` on another).

## Inspecting an expansion

`:gag-inspect NAME [parameters]` takes exactly the `:gag` grammar, resolves the
recipe at the project rate with the current Visual range deciding its content,
and lists its ordinary steps in Help (RECIPE EXPANSION), one line each with the
resolved pause lengths (milliseconds also as frames), gap ladder, escalation and
the pinned group label. Nothing is applied and history is unchanged; `:gag`
with the same parameters applies the same expansion.

## Seeded variation

One More Time accepts `vary=20%` (1 to 50) and `seed=N`. Each gap `k` moves by
at most that percentage of itself, by `GagVariation::vary`, a fixed SplitMix64
draw from the seed, rounded once to a whole unit and kept positive. The seed is
pinned in the recipe (and its macro wire) and in the group label (`varied ±20%
(seed 7)`); the resolved gaps are stored as the gap Holds' exact durations, so
playback, export and Undo never draw again. Without `seed=` the app draws one
once at authoring time and pins it; `:gag-inspect` without `seed=` names the
seed it drew for the preview and says to apply with that `seed=` to get exactly
those gaps, since applying without one draws a new seed. Evidence:
`seeded_variation_resolves_fixed_bounded_gaps_and_pins_its_seed`, the
`recipe-library` replay (inspected gaps equal applied gaps) and the
`one-more-time-varied` preview/export fixture.

## Local recipes

`:recipe-save a` keeps the selected group (for example a gag after changing its
parts) as project register a, through the ordinary whole-group copy (`"ayag`):
its exact beats, timing, framing, cutaways, captions and gain travel with it, and
the bank persists across reopen. `:recipe a` inserts a fresh copy at the cursor
(`"ap`) as one Undo, and `:recipe-inspect a` outlines it in Help before reuse:
each part's root with kind, label and length, and a group's direct children with
their attachments. Both are recordable. A saved group is fixed content that
lives in this project, because it shows this project's Original. A saved copy
of an inserted gag keeps its pinned label, so `:gag-set` still changes the
inserted copy's parameters; a group whose parts were changed by hand has no
recipe parameters left to expose and is edited directly.

## Presets shared across projects

`:gag-save NAME` keeps the selected inserted gag's recipe and exact parameters
(parsed from its pinned label) as a named preset in
`Application Support/Deadpan/gag-presets.json`, shared by every project. A
preset holds no media or project identity. `:gag NAME [key=value …]` inserts it
as the concrete recipe it names, with any given parameters changed by the
`:gag-set` grammar, so macros record and `.` repeats that portable recipe;
`:gag-presets` lists them in Help. Names are 1 to 32 lowercase letters, digits
or hyphens and never a built-in name; the file holds at most 128 presets in
256 KiB. A save holds an exclusive lock for its read-change-write, writes a
uniquely named temporary file and renames it over the old one, so concurrent
saves keep each other's presets. One entry this Deadpan cannot read is
skipped with a warning and kept in the file; a file of another version is
reported and never overwritten. Replays and tests use a private file;
headless and worker paths never read it. Evidence:
`presets_round_trip_in_one_bounded_file_and_refuse_bad_names`,
`edge_and_gag_set_commands_parse_their_choices` and the `creative-dot` replay
(save, list, insert in the same project with the private file, and `:gag-set`
on a `:recipe a` copy).

## Remaining

Fixed-content local recipes cannot move between projects (each shows its own
Original); structured recipe storage independent of the group label, and seeded
framing variation, remain open.
