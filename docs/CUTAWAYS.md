# Cutaways

A cutaway shows another moment of the Original over part of a beat while the
beat's own sound continues (specification §8.2 "Reaction cutaway", §17.3 and
the §31 worked edit, step 7). It is a picture attachment, not new time.

## Model

Only a Source or Hold beat hosts cutaways, because its local clock is its
content's clock. `BeatNode.cutaways` holds up to 16 sorted, disjoint
[`Cutaway`](../crates/deadpan-core/src/cutaway.rs) records:

| Field | Meaning |
| --- | --- |
| `range` | Half-open project frames in the host beat's own output clock. |
| `asset`, `selection` | The Original video and the exact source interval shown at its natural rate. |
| `fit` | After a short selection runs out: `hold` (default) the final picture, `loop` it, or `gap` to show the host again. |
| `removed` | A video-only delete: the range shows the project background instead, and `asset`/`selection` record the removed Original pictures ([role edits](ROLE_EDITS.md#role-only-deletes)). Omitted when false. |

Validation requires a registered non-still video asset, a selection inside its
video, nonnegative nonempty ranges and the count bound. A cutaway longer than
its host is clipped to the host's duration; a Hold that is shortened and then
lengthened again shows the clipped part again. `SetCutaways { node, cutaways }`
replaces a beat's list as one reversible edit and changes no timing, sound or
retained clock, so it is admitted alongside beat-owned sounds.

Because the attachment lives on its host node, every structural operation that
moves, copies or deletes the host carries it. Split and Trim contraction keep
the full Source behind unity Partitions, so the fragments show the same
pictures and refining a fragment again changes nothing; occurrence isolation
and slice copies clone the host with its cutaways and their assets; deleting
the host removes them. Source Trim and Roll shift them with the content when
the physical Source gains a prefix, like framing and gain. The field is additive within core schema 46.

## Picture and sound

`RenderPlan::picture` checks the visited beat's cutaways before descending: a
picture inside a range becomes a `Picture::Source` of the cutaway's selection
with the adjacent-hold endpoint policy, so admission, decoding, caching and
export are unchanged. The host's own framing and its ancestors' framing still
apply; the host's captured Hold geometry does not. Audio plans never read
cutaways, so the host's sound, policies and sample clocks are untouched.
`RenderPlan::provider_picture` samples without cutaways; word, pause and shot
projections use it, so motions follow the heard Original under a cutaway.

## Commands

`:cutaway [register=r] [fit=hold|loop|gap] [audio=keep]` places the copied
Original moment (the selected register, or `register=`) over the Edit range,
which must lie inside the selected beat, or over the whole beat without a
range. A selected Split fragment places the cutaway on its Source, in the
Source's clock; a group, Repeat or speed change is refused with guidance to
open it and select a source or pause beat. Other `audio=` values are refused because a cutaway never changes
sound. An overlap with an existing cutaway is refused; `:cutaway clear`
removes the cutaways overlapping the range. The inspector lists the beat's
cutaway ranges. While recording a macro, a whole-beat `:cutaway` records the
semantic `SetCutaway { register, fit }`, which resolves the register's Original
moment and the selected beat when replayed; `:gag are-we-done` uses the same
instruction over its tail pause. A ranged cutaway refuses while recording.
[Captions](CAPTIONS.md) follow the same host and lifecycle rules. J- and L-cuts
([role edits](ROLE_EDITS.md#j-and-l-cuts)) place a picture-keeping cutaway over
the stretch their Roll moves.

## Tests

- Core: natural-rate pictures, hold/loop/gap fits, prefix shift and wire form.
- Core: only Source and Hold beats host cutaways.
- Plan: pictures inside and outside a range over a Hold, the held final
  picture selecting the last selected frame, host framing still applied,
  every picture unchanged across an actual Split through the range and a
  second Split inside a fragment, and the provider picture ignoring it.
- Replay (`cutaway`): copy Original pictures 90..96 into register `r`, select
  Edit 30..40, type `:cutaway register=r audio=keep`, check pictures
  29/90/95/95/40 at frames 29/30/35/39/40, unchanged duration, the inspector
  row and Undo.

## Remaining

Picture-in-picture overlays, z-order between overlapping attachments, a
timeline indication of cutaway ranges on beat cards, listing a fragment's
Source cutaways in its inspector, ranges spanning several beats, cutaways
over groups, and occurrence-specific cutaways inside one Repeat play.
