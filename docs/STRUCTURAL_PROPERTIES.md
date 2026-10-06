# Structural edit properties

[`structural_properties.rs`](../crates/deadpan-core/tests/structural_properties.rs)
runs random sequences of structural commands through the ordinary
`apply_with_result` path. Each case generates a root Sequence of 2–6 beats
(linked-audio Sources, silent Holds and small groups at 30000/1001 fps) and
1–20 abstract operations. Each operation resolves against the current tree,
so later steps act on Repeats, Retimes, fragments and groups made earlier.

Covered commands: Split, DeleteRipple, DeleteChildren, DeleteRange, InsertTime,
SpliceSlice and SpliceSliceAt with range and child captures, ReplaceSlice,
MoveRange, GroupSelection by range and child, Ungroup, RepeatSelection by child
and range, SetRepeatPlays, SetRepeatGaps, IsolateGap, WrapRetime, SetRetime,
SetHoldDuration, EditScoped and EditOccurrence renames, SetMark,
[Explode](EXPLODE_DUPLICATE.md), Duplicate of a child and of a range, and
EditScopedMany renames of several plays. A
companion test fails if any of these families never commits, so a resolver
change cannot silently stop exercising one.

## Invariants after every committed step

- The result validates and is unchanged by a JSON round trip.
- The forward patch applied to the input equals the result. The inverse patch
  applied to the result equals the input.
- `duration_delta` equals the actual duration change. It also equals the change
  promised by the command family: zero for time-neutral edits and moves; minus
  the removed time for deletions; plus the inserted or pasted time; and
  `plays × child + (plays − 1) × gap` for Repeat changes without overrides.
- Marks keep their label, bias and loss policy. Unresolved marks never become
  bound. `KeepUnresolved` marks never disappear. Time-neutral edits, moves,
  pastes and duplicates lose no bound mark unless the edit removes a node the
  mark is owned by or hosted in, as Ungroup removes its wrapper.
- Time-neutral edits (Split, Group, Ungroup, IsolateGap, Explode, scoped and
  multi-play renames, and marks) keep every retained audio clock. Each
  surviving unrepeated physical owner resolves the same lattice and resume through `AudioBindingState::resolve`,
  ignoring work counters and birth indexes. This is the core-level proxy for an
  unchanged plan sample mapping, because `deadpan-plan` is not a dependency of
  the core crate.
- A refused command leaves its input unchanged.

After the whole sequence, applying every inverse patch in reverse order
restores the generated document. Applying the forward patches again reaches the
final document.

## Bounds

Debug builds run 64 cases and release builds 512, with no failure persistence
files. Both finish in seconds. A 6,000-case release run found no failures.
These tests do not decode media, and they do not cover Repeat play overrides
or nested occurrence paths in the clock comparison. Decoded PCM equivalence
belongs to the plan and audio crates; the decoded explode check is in
`crates/deadpan-audio/tests/composite_insert/explode.rs`.
