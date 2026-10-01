# Atomic Source Slip

`SlipSource { parent, node, delta_frames }` changes which Original material a
beat uses while keeping its output position and duration. Positive deltas select
later material. Core schema 39/database 48 include this command and the exact
[editorial window](SOURCE_EDIT_WINDOWS.md) it requires.

## Scope

The target is a direct child of an explicit ordinary Sequence, under ordinary
Sequence ancestors. It may be a Source or one neutral unity Partition containing
a Source. Source-owned framing and audio treatments are retained.

The Source must retain a full qualified video span, an explicit mapping with
adjacent endpoint holds, and a positive editorial window. Enabled audio must
retain the same asset's full measured span and its linked affine clock. Audio
may be intentionally absent. An independent sample offset remains unchanged.

The visible intersection of the editorial window and the selected allocation
must lie within measured picture support. Audio coverage does not constrain
handles: late audio, early endings and dormant linked audio remain valid.

The initial command rejects FitBeat mappings, missing or incoherent editorial
windows, cropped source spans, independently mapped streams, treated or nested
Partitions, authored retimes, Repeat occurrences and audio-only picture lead or
tail. It does not infer or silently promote these shapes.

## Exact transformation

For each stream, convert its retained endpoints to Original seconds. Its mapping
has slope `alpha = mapped_frames / original_seconds` and intercept
`beta = mapping_start - alpha * original_start`. Linked streams must have equal
positive slopes and equal intercepts before the independent audio offset.
Signed source origins and non-natural common rates remain exact.

Let full picture support be `[v0,v1)` and the visible editorial interval be
`[a,b)`. The exact allowed delta is `[v0-a,v1-b]`. The lower bound is rounded
upward and the upper bound downward to whole frames. The requested move is
clamped within those bounds. Invalid entry states and overflow fail without
changing the document.

Both enabled full mapping starts move by minus the applied delta. Full spans,
mapped extents, Source duration, editorial window and Partition allocation stay
fixed. Selected picture support is the editorial window intersected with the
shifted picture support. Selected audio support is the intersection with shifted
effective audio support, converted back by subtracting the independent offset.
An empty intersection retains audio with equal selection endpoints at the nearest
support boundary, so a later Slip can make it audible again.

The operation changes both maps in one reversible transaction. Existing audio
bindings, sample grids, resumes, effect clocks, gain/mute keys and root sounds
stay fixed. Changed source content invalidates its current audio lineage through
the ordinary command path. Local, Occurrence and Sequence marks keep their
coordinates. Source marks retain their immutable PTS and resolve at the changed
output position or report `OutsideMapping` in that occurrence.

## Store and headless use

The store checks the current asset contract, retained qualification receipt,
Original ownership and complete stream spans in the same transaction that
prepares the edit. Ordinary editing does not reopen or hash Original bytes.
Playback and export retain their own fresh media validation.

`ProjectStore::preview_source_slip` returns exact and whole-frame handle limits,
requested/applied deltas, a limiting boundary and an optional edit. It checks
project, revision, new revision identity and stored admission even for an applied
zero. A preview reserves nothing and supplies no authority for a later commit.

The shared headless command path exposes that report as `source_slip` on dry run,
alongside `edit`. An applied zero returns `edit: null`; it creates no history.
A raw authored zero Slip fails with `InvalidCommand`. Nonzero commits recheck
the captured revision and admission, then create one ordinary durable undo entry.

```json
{"protocol":1,"project_id":"project","expected_revision":"current",
 "new_revision":"next","command":{"command":"slip_source",
 "parent":"sequence","node":"beat","delta_frames":5}}
```

Use `deadpan-cli command PROJECT --json REQUEST --dry-run` to inspect the result,
then submit that captured request without `--dry-run` to commit. The same command
is available through `deadpan-app --headless` and the live project writer.

## Remaining work

Native `:slip` and Trim mode are not connected by this increment. Full Trim also
requires In/Out, adjacent Roll, ripple/overwrite policy, physical Source growth,
nested occurrence editing and boundary-frame/waveform previews. Audio-only
lead/tail needs an index-derived adjacent-picture interval before it can be
admitted. Exact windows and stored receipts alone do not qualify those cases.
