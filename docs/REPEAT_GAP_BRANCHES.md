# Editable Repeat gaps

Core 23/database 29 retain a sparse `gap_overrides` map beside play overrides.
Each entry maps a Repeat and the stable preceding play ID to an independently
owned ordinary subtree. The shared default gap remains a Hold recipe. This is
the structural prerequisite for inserting Original moments or extra time inside
one selected gap; general cursor splice and native gap controls remain open.

`SetGapOverride` replaces one gap with a supplied fresh subtree. Its meaning is
replacement, so it does not claim the identity of the old default-gap content.
`ClearGapOverride` removes the independent branch and exposes the current default.
An empty Sequence branch explicitly suppresses one gap. Removing that entry
would instead expose the default and is a different operation.

`IsolateGap` materializes a currently rendered default gap as an ordinary Hold
without changing duration or existing mark coordinates. The command supplies the
new node and timing identities; the current document supplies the recipe. Its
occurrence form first isolates outer repeated ancestors. The new Hold retains
the current captured picture context and gap-edge choices. Surrounding Repeat
framing and effects remain on the Repeat, outside the independent branch.

The branch follows its preceding stable play through reorder. A final play owns
its branch but renders no trailing gap. Growth or reorder can expose it again;
retiring its play removes the branch, and undo restores it. Changing or removing
the default gap leaves all explicit branches intact. Project-coordinate lookup
rejects dormant and zero-duration branches. Their owned definitions remain
available for editing and retained-context reads. A dormant Hold is not an
effective generation target.

The existing `InstancePath` is sufficient: a unique target node and its ordered
Repeat instances distinguish a play subtree from its separately owned gap.
Compact layout compilation merges sparse play and gap changes into segments,
never expanding the play count. Gap branches can contain Source, Sequence,
Hold, Repeat and Retime structures; a gap branch may have zero duration while a
Repeat's play child must remain positive.

## Audio clocks and policy

An isolated Hold owns its current raw recipe. Its retained lattice and phase
terms may refer to the old Repeat-gap clock; they do not refer to a historical
raw Hold recipe. The current node lives in the ordinary Node binding map. Each
old own-gap argument is closed to the selected preceding play, independently of
outer Repeat arguments. Outer scope arguments still follow copied ownership.

A gap that did not exist in a captured layout uses the canonical gap-definition
PointCeil clock. Materialization closes that dispatch and discards an enclosing
reanchor window which the birth had already excluded. Surviving gaps keep their
old origin-based RoundEven or Preserve-input PointCeil placement. Current Hold
duration, audio policy and source remain live; changes to the Repeat's default
do not alter this independent recipe.

Frozen layouts retain the sparse branches too. A dormant branch retains its
affine clock and meaningful support, with an empty visible allocation. Current
root, point, picture, policy and audio-definition walks descend an active branch
at the preceding play's end. They must not read the default gap there. Ordinary
Repeat-gap selectors continue to identify default recipes, with no fabricated
per-gap node identity.

Core schemas through 22 reject gap-branch fields, including explicit empty or
null fields inside retained layouts, and reject the new commands. The frozen
core-22 adapter also retains its old requirement that a binding's owner map and
reference recipe kind agree. Audio contexts use schema 3; schemas 1 and 2 retain
their closed layout vocabulary. Database migration replays the complete earlier
history and preserves a pre-migration backup.

[Qualification](qualification/gap-branches-2026-09-26.md) records independent
review, decoded PCM, compact geometry, actual old-binary migrations and current
CLI history checks, with raw results and remaining verification limits.

See [the splice design](STRUCTURAL_SPLICE_DESIGN.md),
[gap timing ownership](GAP_AUDIO_BINDINGS.md), and
[compact reanchors](AUDIO_REANCHORS.md). None of these primitives alone completes
the arbitrary-boundary insertion or full keyboard editing requirements.
