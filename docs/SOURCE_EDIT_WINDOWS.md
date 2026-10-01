# Exact selected Source time

Core schema 39/database 48 retain an optional `SourceNode.edit_window`. It records
the exact selected editorial interval before whole-frame enclosure. Picture and
audio mappings continue to determine rendering. [Atomic Slip](SOURCE_SLIP.md)
checks this window against current mappings and stored source qualification.
The field alone supplies no handle authority or native mode.

## Coordinates and admission

`SourceEditWindow` has checked, private rational `start` and `end` values. It is a
half-open interval in the physical Source owner's local project-frame clock,
after the independent audio offset. Construction requires
`0 <= start < end <= i64::MAX`; document validation also requires
`end <= source.duration`. Fractional endpoints remain exact.

`None`, omitted in JSON, means that no common selected interval is declared.
It does not mean `[0, source.duration)`. Generic and historical Sources retain
that absence. A single-stream Source may declare a window without becoming
linked. A present window does not prove that independently authored media maps
share a clock or match a qualified source receipt.

Keep three different intervals:

- The whole-frame allocation controls structural duration.
- The exact editorial window records selected time.
- Each stream's selected support controls which measured media is rendered.

For example, selected audio may end at local 0.4 while the adjacent first picture
begins at 0.8 in a one-frame allocation. Holding that picture does not extend the
selected audio to 0.8 or 1.0. An empty audio selection remains linked and dormant;
an editorial window itself must be positive.

## Construction and existing edits

| Path | Window |
| --- | --- |
| Full import or whole-Original reuse | `[0, exact enabled A/V union extent)` before ceil |
| Original moment | `[0, exact selected picture duration)` before ceil |
| Temporary catalog audio range | `[0, selected sample count / source rate * project rate)` |
| Generic or historical Source | Absent unless explicitly authored |

Complete media contexts, affine mappings, endpoint holds and sample offsets are
unchanged. A full import may include audio lead/tail; a picture-led moment keeps
its picture selection even when audio is dormant or absent.

Splits, copies, captured slices, moves and occurrence isolation retain the
complete physical owner's window. A Partition changes the visible view without
shrinking that owner. Camera, gain, marks and sound edits preserve it.

A real `SetSourceVideoMapping` change clears the window because a generic
one-stream edit cannot reconstruct common selected intent. The same applies to
`SetSourceAudioMapping` when either mapping or independent offset changes.
Value-identical assignments retain it. Occurrence edits use the same rule on
their isolated copy; other occurrences retain their own state. Inverse patches
restore the exact prior value or absence, including through durable Undo/Redo.

`prepend_owner_frames` returns the exact checked interval translated by a
nonnegative prefix. It does not enlarge the Source. A future atomic edit must
also update mappings, duration, retained audio clocks, effect clocks and marks.
Tail-only context growth preserves the interval. A crop view's intersection with
the retained interval can be empty if it contains only endpoint padding; that
does not permit inventing a positive selected interval.

## Future Trim boundary

The resolver must verify the window's relationship to qualified full media
context and the current affine maps before exposing handles. It must preserve
playback rate: materialize `FitBeat` against the old duration before growth, or
reject growth without mutation.

Audio support used for intersection includes its independent offset. Convert a
physical-local intersection back to mapping coordinates by subtracting that exact
offset before storing `SourceAudioMapping::SelectedPlacement.selection`. Applying
the offset a second time moves selected audio. Rounded picture padding cannot
make unselected audio audible.

This metadata does not itself change picture selection, PCM, mark resolution,
sample clocks or structural duration. Those behaviors remain with their existing
owners. Missing intent, incoherent maps and endpoint-only views require explicit
resolver policy, not an inferred full-duration selection.

## Format and evidence

Current documents and patches retain the window. Supported historical Source
wrappers upgrade with `None` and reject projection of a present window. Their
strict grammars reject the new field even when null or escaped. Frozen audio
layouts and contexts do not contain editorial intent and need no new field;
the context schema remains 5.

Single-Original baseline validation permits an absent window only when every
other Source field exactly matches the measured full Original. A present window
must also match that full interval. This preserves historical baselines without
rewriting their intent or relaxing media, timing or protected-history checks.
Fresh initialization always records the exact window.

Unused development databases 39 through 47 are refused without writes or
migration. Existing frozen adapters for databases 1 through 38 remain supported.
See [qualification](qualification/source-slip-2026-10-01.md) for actual checks;
no requirement or gate is completed by this model addition.
