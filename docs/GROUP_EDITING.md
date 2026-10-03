# Named groups

Group a selected beat or Visual range with `,g`, then enter a JSON-quoted name:

```text
:group name="the uncomfortable answer"
```

The result is an ordinary editable Sequence. Enter opens its children and
Backspace returns to the parent. `:ungroup` promotes a selected neutral
Sequence's children into its parent. Each operation has one Undo.

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
selection, including no empty Visual selection. It refuses a group with framing,
audio treatments or authored edge policy rather than discarding that behavior.
After success, the first promoted child is selected, including an empty child.
For an empty group, selection goes to the following sibling, then the previous
sibling if there is no following one. The cursor remains at the old group start.

Group also accepts [Visual group objects](EDITED_SLICES.md#group-objects):
`vig` retains exact child roots and `vag` retains the group as an owned unit.
Empty endpoint children remain selected; no-child contents refuse grouping.
The [group-object qualification record](qualification/group-objects-2026-10-03.md)
tracks that later keyboard, register, semantic and Place integration.

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
not complete temporal edits inside Repeat/Retime occurrences, distribution of
group framing or treatments during Ungroup, saved gag recipes, or the full
visual slice placement workflow. Those requirements and all release gates
remain open.
