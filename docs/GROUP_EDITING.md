# Named groups

Group a selected beat or Visual range with `,g`, then enter a JSON-quoted name:

```text
:group name="the uncomfortable answer"
```

The result is an ordinary editable Sequence. Enter opens its children and
Backspace returns to the parent. `:ungroup` promotes a selected Sequence's
children into its parent, moving the group's own framing, gain and edge choices
onto them. Each operation has one Undo.

## Selection and keyboard behavior

Group captures the current ordinary Sequence scope, selected direct child and
typed Visual selection when command entry opens. A later reply or navigation
cannot supply a missing target or change that captured selection. A current
Visual selection takes precedence over the selected beat. An empty Time range
refuses instead of falling back to the beat.

Without Visual selection, Group uses the explicitly selected child, including
an empty Sequence. The cursor alone cannot supply that child. The new group is
selected and the cursor moves to its start; Visual selection clears. The label
preserves spaces, Unicode and JSON escapes, including an empty label, with a
1024-byte UTF-8 limit and no NUL. Malformed or additional arguments refuse.

`,g` has no count. Its pending prefix teaches the next key, and native text and
IME composition remain text after command entry opens. `group.create` and
`group.ungroup` can be remapped through the existing keymap. Ungroup has no
default key path; use `:ungroup`.

Ungroup requires an explicitly selected direct-child Sequence with no Visual
selection, including no empty Visual selection. The group's own behavior moves
onto its promoted children so every picture and audio sample is unchanged (see
[effect distribution](#effect-distribution)).
After success, the first promoted child is selected, including an empty child.
For an empty group, selection goes to the following sibling, then the previous
sibling if there is no following one. The cursor remains at the old group start.

Group also accepts [Visual group objects](EDITED_SLICES.md#group-objects):
`vig` retains exact child roots and `vag` retains the group as an owned unit.
Empty endpoint children remain selected; no-child contents refuse grouping.

## Effect distribution

`Command::Ungroup { node, wrappers }` plans each promoted child once, in the
group's exact clock, and applies everything in one reversible transaction.

- **Framing.** Framing composes provider to root. A child without framing takes
  the group's camera path with a `RetainedOutput` clock windowed to the child's
  own interval of that path, so animated curves continue across child
  boundaries unchanged. A child with its own framing cannot hold two composed
  operations, so it keeps its framing inside a new single-child Sequence that
  carries the group's windowed path.
- **Clip gain.** Gain is applied per contribution as one exact Q32 dB sum over
  the owner chain. The group's trim and mute move unchanged; its envelopes and
  mute ranges translate into each child's clock (`ClipGain::windowed`), keeping
  keys that begin in an earlier child at negative local times so curves keep
  their exact shape. A child whose recipe is clip gain alone absorbs the sum
  (`ClipGain::followed_by`); a child with saturation, EQ, balance or sends gets
  a wrapper carrying the group's gain after its own stages.
- **Edges.** Editorial edges and `node_start`/`node_end` policies belong to the
  group's outer boundaries, so they move to the first and last children with
  positive duration. Coincident owners produce one fade and any Hard suppresses
  it, so a union is exact. A transparent fragment that could not carry a Hard
  side without marking it gets a wrapper. An empty group's edge choices act on
  the seam between its neighbors and refuse.
- **Refusals.** Saturation shapes the children's combined mix, EQ filters it
  continuously across their cuts, balance applies to the mixed sum and sends
  route the group's whole output; none has a per-child equivalent, so Ungroup
  refuses before any change The message names the
  effect and the way forward, for example: "ungroup cannot keep this group's
  saturation, which acts on its children's combined mix and has no per-child
  equivalent; remove the group's saturation first, or keep the group". The
  other effects read `EQ`, `stereo balance` and `effect sends` in the same
  sentence.
- **Captions and cutaways.** A Sequence never carries them; validation keeps
  them on Source and Hold hosts. `:caption` or `:cutaway` on a selected group
  (or an Edit range inside it) gives each host beneath the group the exact
  intersection of the range in its own clock
  ([`ProjectDocument::caption_over` and `cutaway_over`](../crates/deadpan-core/src/group_attachments.rs)),
  descending through nested ordinary Sequences and unity Partitions as
  `cutaway_host` does, one `SetCaptions`/`SetCutaways` leaf per host in one
  Apply and one Undo. Every picture center lies in one child, so each frame
  shows the same caption text, placement and reveal. A cutaway share starts
  its selection where the whole cutaway would be at that frame; a `hold` share
  past the selection holds the last picture, a `gap` share past it is
  dropped, and a `loop` or `bounce` that would repeat inside a later beat
  refuses ("use fit=hold or fit=gap, or place it inside one beat").
  `cutaway_over` keeps a video-only delete's `removed` flag on every share. A Repeat or speed change
  inside the range refuses, for example "a caption over this group reaches
  "Echo", a Repeat, whose plays would each need it; place the caption
  inside or around that beat instead": its content plays at several times or
  another rate, and a caption there would count that inner Repeat's plays for
  `reveal`. Ungroup therefore carries every caption and cutaway unchanged.
- **Inside Repeats.** `OccurrenceEdit::Ungroup { wrappers }` ungroups the
  group in one explicit play after isolating that occurrence, with the same
  distribution and fresh wrapper identities. In the native inspector,
  `:ungroup` with This play at every Repeat level issues it through the
  semantic planner; with All plays at every level it ungroups the shared
  definition with a plain `Ungroup`, which every play that follows the
  definition shows (overridden plays keep their own copies). A mix of the two
  refuses. An All plays scoped context admits only Ungroup and named calls.

`ProjectDocument::ungroup_requirements` reports how many fresh wrapper
identities the command needs; the semantic planner allocates them, and a
wrong count refuses before any change. Wrappers take the group's label. Marks,
retained audio clocks, lineage and root sounds are unchanged: the move touches
only node effects and Sequence structure. Inside a retained independent sound
scope, a treated group still refuses as below. Evidence: core
`ungroup_reports_removed_mark_host_and_moves_group_owned_effects`,
`duration_changes_and_internal_insertions_retain_fixed_keys_and_hidden_suffix`
(composed gain equality at 71 exact positions, merge, wrapper and refusals) and
`ungrouping_a_marked_sequence_moves_its_editorial_edge_to_the_child`; plan
`ungrouping_a_framed_group_wraps_children_that_have_their_own_framing`; audio
`ungrouping_a_treated_group_keeps_the_authored_bus_bit_identical` (bit-identical
authored bus through absorb, merge, saturated-child and fragment-wrapper paths);
golden-render `ungrouping_a_framed_gained_group_renders_exactly_like_the_group`
(every exported I420 frame and PCM block hashes identically); and the `groups`
replay, which ungroups a creeping, gained group over a zoomed fragment with real
keys and compares composed pictures. Captions, cutaways and Ungroup inside
Repeats: core `group_attachments` tests (per-frame caption and cutaway
picture equality for every fit, the Repeat refusal), planner `scoped_ungroup`
tests (one-play and All plays Ungroup with wrappers, replay and exact Undo,
the named saturation refusal, `:caption` on a group in one Apply); plan
`ungrouping_a_captioned_group_inside_a_repeat_keeps_every_picture` (picture,
camera poses and captions equal on every frame after one-play and All plays
Ungroup); audio
`ungrouping_a_gained_group_inside_a_repeat_keeps_every_play_bit_identical`
(decoded authored bus, including a saturated child's wrapper); golden-render
`ungrouping_a_captioned_group_inside_a_repeat_renders_exactly_like_the_group`;
and the `groups` replay, which captions a group, ungroups it, then ungroups a
gained group in play 2 and in All plays with real keys.

## Exact structural edits

`GroupSelection` takes `parent`, a `SliceCaptureSelection`, `label`, exact
`GroupSelectionIdentities { group, split }`, and a timing identity tied to the
new revision. `ProjectDocument::group_selection` preflights the range and number
of Split identities. Whole-child grouping adds only the wrapper. Range grouping
splits partial endpoints using their complete owner contexts and retained audio
clocks. Empty siblings at the two range boundaries remain outside; empty
siblings strictly inside belong to the group.

Grouping preserves duration, source mappings, descendant ownership, framing and
independent root sounds. Split-remapped silent-Hold permissions remain attached
to their exact issuers. Grouping and neutral Ungroup do not ripple the root sound
bus or allocate a new clock when no split requires it.

Inside a retained independent sound scope, an aligned unary group can be
ungrouped while every affected historical reference still identifies it as an
introduced neutral wrapper. Later label and descendant audio edits are retained.
The removal preserves the old scope and all sound clocks; a genuine later
capture of the group, or group-owned processing, keeps Ungroup unavailable.
See [sound ownership](OWNED_SOUND_VOICES.md#neutral-grouping) for the exact scope.

Marks inside grouped contents keep their existing exact transforms. A mark
hosted on a removed group becomes explicitly unresolved under the existing
removed-host rule. The operation does not invent a new mark host. The headless
Macro response includes sorted `mark_changes` with full `before` and `after`
logical mark records, including fragments and unresolved reasons. It compares
the entry document with the final plan, so intermediate losses are not reported
as final losses. Dry-run and commit use the same comparison.

## Macros and dot

`SemanticInstruction::Group { selector, label }` and `Ungroup` use the shared
staged planner and Compound transaction path. Native recording appends only
the exact successful instruction. A saved Macro can group, navigate and ungroup
against each preceding staged result; a failure commits none of it. A complete
run has one Undo.

Dot retains a Group's label and selector, resolving them at the current target.
A current Visual selection overrides the retained selector. A saved Visual
selector requires a new admissible Time or Object selection. Ungroup dot
resolves a new explicitly selected Sequence and refuses every Visual selection. Neither operation consumes the
pending register choice or changes the copy bank.

A successful individual Apply proves dot intent before workspace refresh.
Named Macro Run does not choose a new dot action. Exact retries retain the
existing receipt without resurrecting an older action.

## Remaining scope

This increment supports structural edits in ordinary Sequence scopes. It does
not complete temporal edits inside Repeat/Retime occurrences, treated groups
inside retained independent sound scopes, saved gag recipes, or the full visual slice placement
workflow. Those requirements and all release gates remain open.
